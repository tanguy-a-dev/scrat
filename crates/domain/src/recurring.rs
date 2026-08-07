//! Detection of recurring charges — subscriptions, rent, utilities, any
//! commitment that bills on a rhythm.
//!
//! Pure analysis over transactions already in hand: no I/O, no repository
//! access, no persistence. Nothing here is stored; a recurring charge is a
//! *conclusion drawn from the ledger*, recomputed on demand, never a record
//! the user has to maintain. That matters because the ledger is the only
//! thing that can be wrong, and it's already the thing they curate.
//!
//! The whole design leans one way on purpose: **a missed subscription is a
//! much cheaper mistake than an invented one.** A user who sees nine real
//! commitments and misses a tenth has still learned something true. A user
//! shown "weekly commitment: supermarket, €62/week" has been told a habit is
//! a contract, and now distrusts the whole panel. Every threshold below is
//! set with that asymmetry in mind.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use chrono::{Days, Months, NaiveDate};

use crate::category::CategoryId;
use crate::transaction::{Direction, Transaction};

/// Minimum number of distinct-day occurrences before a merchant is called
/// recurring. Two points produce a single interval, and a single interval
/// cannot disagree with itself — three is the shortest series that can be
/// checked for consistency rather than merely measured.
const MIN_OCCURRENCES: usize = 3;

/// Fraction of intervals that must match the candidate cadence. Deliberately
/// not all of them: a failed payment, a bank holiday, or a month the user
/// paused the service leaves one long gap, and rejecting an otherwise clean
/// series over it would drop exactly the subscriptions worth surfacing.
const MIN_CADENCE_RATIO: f64 = 0.6;

/// Fraction of occurrences whose amount must sit within
/// [`AMOUNT_TOLERANCE_PERCENT`] of the median.
///
/// This is the rule that separates a subscription from a habit, and it is the
/// single most important threshold in this module. Weekly groceries at the
/// same shop repeat every bit as regularly as a streaming service does —
/// cadence alone cannot tell them apart. What distinguishes them is that a
/// subscription costs the same thing every time and a habit never does.
const MIN_AMOUNT_STABILITY_RATIO: f64 = 0.75;

/// How far an individual charge may drift from the median and still count as
/// "the same amount". Wide enough for a metered utility bill or an FX-converted
/// charge, far too narrow for a supermarket trip.
const AMOUNT_TOLERANCE_PERCENT: i64 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cadence {
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}

impl Cadence {
    /// Longest-period-first is irrelevant here — the windows are disjoint, so
    /// at most one can match any interval.
    const ALL: [Cadence; 4] = [
        Cadence::Weekly,
        Cadence::Monthly,
        Cadence::Quarterly,
        Cadence::Yearly,
    ];

    /// Inclusive day-count window an interval must fall in to count as this
    /// cadence. Wide enough to absorb weekend drift and the 28-vs-31 day
    /// spread of calendar months, narrow enough that the windows never touch —
    /// a 14-day rhythm matches nothing, which is the honest answer rather than
    /// forcing it into "weekly" or "monthly".
    fn window(self) -> RangeInclusive<i64> {
        match self {
            Cadence::Weekly => 6..=8,
            Cadence::Monthly => 26..=35,
            Cadence::Quarterly => 82..=98,
            Cadence::Yearly => 350..=380,
        }
    }

    fn from_interval_days(days: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.window().contains(&days))
    }

    /// Nominal period length, used only for the overdue grace period — the
    /// cadence windows above, not this, decide what counts as a match.
    fn nominal_days(self) -> u64 {
        match self {
            Cadence::Weekly => 7,
            Cadence::Monthly => 30,
            Cadence::Quarterly => 91,
            Cadence::Yearly => 365,
        }
    }

    /// Charges per year, used to express any cadence as a monthly figure so
    /// commitments on different rhythms can be summed into one total.
    fn per_year(self) -> i64 {
        match self {
            Cadence::Weekly => 52,
            Cadence::Monthly => 12,
            Cadence::Quarterly => 4,
            Cadence::Yearly => 1,
        }
    }

    /// The next date this cadence would bill, given the last one it did.
    ///
    /// Monthly and longer step by calendar months rather than by a fixed day
    /// count, so a charge that bills on the 31st lands on the 30th (or the
    /// 28th) rather than sliding a day earlier every month.
    fn advance(self, from: NaiveDate) -> NaiveDate {
        let advanced = match self {
            Cadence::Weekly => from.checked_add_days(Days::new(7)),
            Cadence::Monthly => from.checked_add_months(Months::new(1)),
            Cadence::Quarterly => from.checked_add_months(Months::new(3)),
            Cadence::Yearly => from.checked_add_months(Months::new(12)),
        };
        // Only reachable within a few hundred years of chrono's ceiling; the
        // date is a display hint, so degrading to "no later than" beats
        // failing the whole scan.
        advanced.unwrap_or(from)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Cadence::Weekly => "weekly",
            Cadence::Monthly => "monthly",
            Cadence::Quarterly => "quarterly",
            Cadence::Yearly => "yearly",
        }
    }
}

/// One detected commitment. A read model, not an entity — it holds no
/// invariants of its own beyond what [`detect_recurring_charges`] guarantees,
/// and it is never persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurringCharge {
    /// The normalized key occurrences were grouped by. Kept for debugging and
    /// for stable identity across scans — not meant for display.
    pub merchant_key: String,
    /// The raw description text of the most recent occurrence: the form the user
    /// has actually seen on their statement, noise and all.
    pub label: String,
    pub cadence: Cadence,
    /// Median charge as a positive magnitude. It's a cost — the sign carries
    /// no information and would only invite a double negation somewhere.
    pub typical_amount_minor_units: i64,
    /// [`Self::typical_amount_minor_units`] restated as a per-month figure, so
    /// weekly, quarterly and yearly commitments can be summed with monthly
    /// ones into a single "committed per month".
    pub monthly_equivalent_minor_units: i64,
    pub occurrences: usize,
    pub first_seen: NaiveDate,
    pub last_seen: NaiveDate,
    pub next_expected: NaiveDate,
    /// Category of the most recent occurrence.
    pub category_id: CategoryId,
    /// False once the charge is overdue by more than half a period.
    ///
    /// Lapsed charges are reported rather than dropped, because "didn't I
    /// cancel that?" and "wait, why did that stop?" are both things worth
    /// knowing. They're excluded from any committed-per-month total, though —
    /// money that stopped leaving is not a commitment.
    pub is_active: bool,
}

/// Groups charges that came from the same merchant despite the per-line noise
/// banks attach to each one — posting dates, card sequence numbers, mandate
/// references.
///
/// Deliberately conservative. It strips what is unambiguously per-transaction
/// noise and nothing else; in particular there is **no stop-word list of bank
/// prefixes** ("PRLV", "SEPA", "CARD PAYMENT TO"). Those are locale-specific,
/// they'd need maintaining per bank and per country, and leaving them in costs
/// nothing as long as the same prefix appears on every occurrence of the same
/// charge — which is exactly when it matters. Stripping them, by contrast,
/// risks collapsing two unrelated merchants into one key.
///
/// So the failure mode this chooses is a missed subscription (the same charge
/// worded two different ways stays split) rather than an invented one.
///
/// One exception: the "cb" token (Carte Bancaire) is always dropped, the same
/// way [`crate::transaction`]'s category-matching normalization and
/// `normalize_description` in `transaction_service.rs` already drop it. Unlike
/// "PRLV"/"SEPA" it isn't part of the merchant's own name — it's a fixed
/// payment-instrument marker some banks toggle on and off over time, which
/// would otherwise split one subscription's history into two merchant keys
/// the moment the bank changes its formatting.
pub fn merchant_key(description: &str) -> String {
    description
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty() && !is_noise_token(token) && *token != "cb")
        .collect::<Vec<_>>()
        .join(" ")
}

/// A token is noise when it carries a digit and is either pure digits or long
/// enough to be a date or reference rather than part of a name.
///
/// The two-character floor is what keeps short digit-bearing brand names
/// groupable — "o2", "m6" survive, while "12jun26", "ref00918" and "4913" do
/// not. Without it every customer of a numerically-named telco would silently
/// have their subscriptions merged into one another.
fn is_noise_token(token: &str) -> bool {
    let mut has_digit = false;
    let mut all_digits = true;
    let mut len = 0;
    for c in token.chars() {
        len += 1;
        if c.is_ascii_digit() {
            has_digit = true;
        } else {
            all_digits = false;
        }
    }
    has_digit && (all_digits || len > 2)
}

/// Finds every recurring charge in `transactions`, most expensive per month
/// first.
///
/// `today` is the reference point for whether a charge is still active;
/// transactions dated after it are ignored, so a future-dated entry can't
/// make a lapsed subscription look live.
///
/// Only expenses are considered. Recurring *income* is a real thing (salary is
/// the most regular line in most ledgers), but folding it in here would make
/// "committed per month" mean two opposite things at once.
pub fn detect_recurring_charges(
    transactions: &[Transaction],
    today: NaiveDate,
) -> Vec<RecurringCharge> {
    // BTreeMap, not HashMap: iteration order feeds into the output order for
    // charges that tie on cost, and a scan that reshuffles its own results
    // between runs looks broken even when it isn't.
    let mut by_merchant: BTreeMap<String, Vec<&Transaction>> = BTreeMap::new();
    for transaction in transactions {
        if transaction.direction() != Direction::Expense || transaction.date() > today {
            continue;
        }
        let key = merchant_key(transaction.description().as_str());
        // A description that normalizes away entirely (all digits, all punctuation)
        // can't be identified, and grouping every such line together would
        // invent a merchant out of unrelated noise.
        if key.is_empty() {
            continue;
        }
        by_merchant.entry(key).or_default().push(transaction);
    }

    let mut charges: Vec<RecurringCharge> = by_merchant
        .into_iter()
        .filter_map(|(key, group)| analyze_merchant(key, &group, today))
        .collect();

    charges.sort_by(|a, b| {
        b.monthly_equivalent_minor_units
            .cmp(&a.monthly_equivalent_minor_units)
            .then_with(|| a.label.cmp(&b.label))
    });
    charges
}

/// Sum of the monthly-equivalent cost of every *active* charge.
pub fn monthly_commitment(charges: &[RecurringCharge]) -> i64 {
    charges.iter().filter(|c| c.is_active).fold(0i64, |sum, c| {
        sum.saturating_add(c.monthly_equivalent_minor_units)
    })
}

fn analyze_merchant(
    merchant_key: String,
    group: &[&Transaction],
    today: NaiveDate,
) -> Option<RecurringCharge> {
    // Collapse to one occurrence per day. Several charges from one merchant on
    // one day are one visit, and a zero-day interval between them would wreck
    // the cadence check outright — the median interval of a weekly series with
    // one double-charge day is 0, which matches no cadence at all.
    let mut by_day: BTreeMap<NaiveDate, i64> = BTreeMap::new();
    for transaction in group {
        let magnitude = transaction.amount().minor_units().saturating_abs();
        by_day
            .entry(transaction.date())
            .and_modify(|total| *total = total.saturating_add(magnitude))
            .or_insert(magnitude);
    }
    if by_day.len() < MIN_OCCURRENCES {
        return None;
    }

    let dates: Vec<NaiveDate> = by_day.keys().copied().collect();
    let amounts: Vec<i64> = by_day.values().copied().collect();

    let intervals: Vec<i64> = dates
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).num_days())
        .collect();
    let cadence = Cadence::from_interval_days(median(&intervals)?)?;

    let matching = intervals
        .iter()
        .filter(|days| cadence.window().contains(*days))
        .count();
    if (matching as f64) < intervals.len() as f64 * MIN_CADENCE_RATIO {
        return None;
    }

    let typical = median(&amounts)?;
    if typical <= 0 {
        return None;
    }
    // Checked throughout: these amounts are external input that reached the
    // ledger from a CSV, and a plausible-looking row can still hold an
    // implausible number.
    let tolerance = (typical.saturating_mul(AMOUNT_TOLERANCE_PERCENT) / 100).max(1);
    let stable = amounts
        .iter()
        .filter(|amount| amount.saturating_sub(typical).saturating_abs() <= tolerance)
        .count();
    if (stable as f64) < amounts.len() as f64 * MIN_AMOUNT_STABILITY_RATIO {
        return None;
    }

    let last_seen = *dates.last()?;
    let next_expected = cadence.advance(last_seen);
    let is_active = next_expected
        .checked_add_days(Days::new(cadence.nominal_days() / 2))
        .is_none_or(|overdue_at| today <= overdue_at);

    // The most recent occurrence supplies both the display label and the
    // category: if the user has recategorized this charge, or the merchant has
    // renamed itself, the newest line is the one they'd recognize.
    let newest = group.iter().max_by_key(|t| t.date())?;

    // i128 intermediate — a weekly charge multiplied by 52 is the one place
    // this arithmetic can leave i64, and saturating there would report a
    // nonsense total rather than an over-large one.
    let monthly_equivalent = ((typical as i128 * cadence.per_year() as i128) / 12)
        .clamp(i64::MIN as i128, i64::MAX as i128) as i64;

    Some(RecurringCharge {
        merchant_key,
        label: newest.description().as_str().to_string(),
        cadence,
        typical_amount_minor_units: typical,
        monthly_equivalent_minor_units: monthly_equivalent,
        occurrences: by_day.len(),
        first_seen: dates[0],
        last_seen,
        next_expected,
        category_id: newest.category_id(),
        is_active,
    })
}

/// Median of `values`, or `None` if empty.
///
/// The median rather than the mean throughout: one annual price rise, or one
/// month a utility ran hot, should not drag the "typical" figure to a value
/// that no individual charge was ever close to. Even counts take the mean of
/// the two middle values — these are minor units, so there is no fractional
/// cent worth preserving.
fn median(values: &[i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        Some(sorted[mid])
    } else {
        Some(((sorted[mid - 1] as i128 + sorted[mid] as i128) / 2) as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::AccountId;
    use crate::money::{Currency, Money};
    use crate::transaction::{Description, TransactionId};

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn expense(day: NaiveDate, minor_units: i64, description: &str) -> Transaction {
        Transaction::new(
            TransactionId::new(),
            day,
            Money::from_minor_units(minor_units, Currency::new("EUR").unwrap()),
            Description::new(description).unwrap(),
            CategoryId::new(),
            AccountId::new(),
        )
        .unwrap()
    }

    #[test]
    fn detects_a_stable_monthly_subscription() {
        let transactions = vec![
            expense(date(2026, 4, 12), -1349, "NETFLIX.COM"),
            expense(date(2026, 5, 12), -1349, "NETFLIX.COM"),
            expense(date(2026, 6, 12), -1349, "NETFLIX.COM"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 20));

        assert_eq!(charges.len(), 1);
        assert_eq!(charges[0].cadence, Cadence::Monthly);
        assert_eq!(charges[0].typical_amount_minor_units, 1349);
        assert_eq!(charges[0].monthly_equivalent_minor_units, 1349);
        assert_eq!(charges[0].occurrences, 3);
        assert_eq!(charges[0].next_expected, date(2026, 7, 12));
        assert!(charges[0].is_active);
    }

    /// The false positive this module exists to avoid. A weekly supermarket
    /// trip is every bit as regular as a subscription — only the amounts
    /// disagree, and that has to be enough to reject it.
    #[test]
    fn rejects_a_regular_habit_whose_amounts_vary() {
        let transactions = vec![
            expense(date(2026, 5, 2), -4210, "CARREFOUR CITY"),
            expense(date(2026, 5, 9), -7788, "CARREFOUR CITY"),
            expense(date(2026, 5, 16), -2395, "CARREFOUR CITY"),
            expense(date(2026, 5, 23), -9102, "CARREFOUR CITY"),
            expense(date(2026, 5, 30), -5560, "CARREFOUR CITY"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 1));

        assert!(charges.is_empty());
    }

    #[test]
    fn requires_at_least_three_occurrences() {
        let transactions = vec![
            expense(date(2026, 5, 12), -999, "SPOTIFY"),
            expense(date(2026, 6, 12), -999, "SPOTIFY"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 20));

        assert!(charges.is_empty());
    }

    /// Real bank lines carry a posting date and a mandate reference that change
    /// every month. Without normalization each occurrence is its own merchant
    /// and nothing is ever detected.
    #[test]
    fn groups_occurrences_despite_per_line_reference_noise() {
        let transactions = vec![
            expense(
                date(2026, 4, 3),
                -1099,
                "PRLV SEPA SPOTIFY AB 4451920 03/04",
            ),
            expense(
                date(2026, 5, 3),
                -1099,
                "PRLV SEPA SPOTIFY AB 4462281 03/05",
            ),
            expense(
                date(2026, 6, 3),
                -1099,
                "PRLV SEPA SPOTIFY AB 4478334 03/06",
            ),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 10));

        assert_eq!(charges.len(), 1);
        assert_eq!(charges[0].merchant_key, "prlv sepa spotify ab");
        // The label keeps the newest raw line, noise included — it's what the
        // user sees on their own statement.
        assert_eq!(charges[0].label, "PRLV SEPA SPOTIFY AB 4478334 03/06");
    }

    #[test]
    fn does_not_merge_two_different_merchants() {
        let transactions = vec![
            expense(date(2026, 4, 3), -1099, "SPOTIFY"),
            expense(date(2026, 5, 3), -1099, "SPOTIFY"),
            expense(date(2026, 6, 3), -1099, "SPOTIFY"),
            expense(date(2026, 4, 5), -1099, "NETFLIX"),
            expense(date(2026, 5, 5), -1099, "NETFLIX"),
            expense(date(2026, 6, 5), -1099, "NETFLIX"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 10));

        assert_eq!(charges.len(), 2);
    }

    /// A short digit-bearing brand name must survive normalization, or every
    /// customer of such a telco has their bills merged with unrelated noise.
    #[test]
    fn merchant_key_keeps_short_names_containing_digits() {
        assert_eq!(merchant_key("O2 MOBILE 12JUN26 REF00918"), "o2 mobile");
        assert_eq!(merchant_key("EDF 4913"), "edf");
    }

    #[test]
    fn merchant_key_is_empty_when_description_is_all_noise() {
        assert_eq!(merchant_key("00219 4913 / 2026-06-12"), "");
    }

    /// "CB" (Carte Bancaire) is a payment-instrument marker, not part of the
    /// merchant's name — some banks add or drop it over time, and without
    /// stripping it a subscription's history would split into two merchant
    /// keys the moment that happens. Dates are already dropped by the
    /// digit-noise rule since each `/`-separated component is all-digit.
    #[test]
    fn merchant_key_ignores_cb_prefix_and_dates() {
        assert_eq!(
            merchant_key("CB NETFLIX.COM 12/06/26"),
            merchant_key("NETFLIX.COM")
        );
    }

    /// A bank that starts prefixing every card payment with "CB" partway
    /// through the history must not split one subscription into two.
    #[test]
    fn groups_occurrences_despite_a_cb_prefix_appearing_partway_through() {
        let transactions = vec![
            expense(date(2026, 4, 12), -1349, "NETFLIX.COM 12/04/26"),
            expense(date(2026, 5, 12), -1349, "CB NETFLIX.COM 12/05/26"),
            expense(date(2026, 6, 12), -1349, "CB NETFLIX.COM 12/06/26"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 20));

        assert_eq!(charges.len(), 1);
        assert_eq!(charges[0].occurrences, 3);
    }

    /// A description that normalizes to nothing must be skipped, not pooled with
    /// every other such line into a phantom merchant.
    #[test]
    fn ignores_descriptions_that_normalize_to_nothing() {
        let transactions = vec![
            expense(date(2026, 4, 3), -1000, "00219 / 03-04"),
            expense(date(2026, 5, 3), -1000, "00220 / 03-05"),
            expense(date(2026, 6, 3), -1000, "00221 / 03-06"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 10));

        assert!(charges.is_empty());
    }

    #[test]
    fn weekly_cadence_is_restated_as_a_monthly_equivalent() {
        let transactions = vec![
            expense(date(2026, 5, 4), -1200, "GYM CLASS"),
            expense(date(2026, 5, 11), -1200, "GYM CLASS"),
            expense(date(2026, 5, 18), -1200, "GYM CLASS"),
            expense(date(2026, 5, 25), -1200, "GYM CLASS"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 5, 28));

        assert_eq!(charges[0].cadence, Cadence::Weekly);
        // 1200 × 52 / 12
        assert_eq!(charges[0].monthly_equivalent_minor_units, 5200);
    }

    #[test]
    fn yearly_cadence_is_restated_as_a_monthly_equivalent() {
        let transactions = vec![
            expense(date(2024, 3, 1), -12000, "DOMAIN RENEWAL"),
            expense(date(2025, 3, 1), -12000, "DOMAIN RENEWAL"),
            expense(date(2026, 3, 1), -12000, "DOMAIN RENEWAL"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 3, 10));

        assert_eq!(charges[0].cadence, Cadence::Yearly);
        assert_eq!(charges[0].monthly_equivalent_minor_units, 1000);
    }

    #[test]
    fn flags_a_charge_that_has_stopped_as_inactive() {
        let transactions = vec![
            expense(date(2025, 9, 12), -1349, "OLD STREAMING"),
            expense(date(2025, 10, 12), -1349, "OLD STREAMING"),
            expense(date(2025, 11, 12), -1349, "OLD STREAMING"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 1));

        assert_eq!(charges.len(), 1);
        assert!(!charges[0].is_active);
    }

    #[test]
    fn monthly_commitment_counts_only_active_charges() {
        let transactions = vec![
            expense(date(2026, 4, 12), -1000, "LIVE ONE"),
            expense(date(2026, 5, 12), -1000, "LIVE ONE"),
            expense(date(2026, 6, 12), -1000, "LIVE ONE"),
            expense(date(2025, 9, 3), -5000, "CANCELLED ONE"),
            expense(date(2025, 10, 3), -5000, "CANCELLED ONE"),
            expense(date(2025, 11, 3), -5000, "CANCELLED ONE"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 20));

        assert_eq!(charges.len(), 2);
        assert_eq!(monthly_commitment(&charges), 1000);
    }

    /// Two charges from one merchant on one day are one visit. Left uncollapsed
    /// they produce a zero-day interval, whose median matches no cadence — so
    /// this would silently suppress an otherwise clean series.
    #[test]
    fn collapses_several_charges_on_the_same_day_into_one_occurrence() {
        let transactions = vec![
            expense(date(2026, 4, 12), -600, "SPLIT BILLER"),
            expense(date(2026, 4, 12), -400, "SPLIT BILLER"),
            expense(date(2026, 5, 12), -1000, "SPLIT BILLER"),
            expense(date(2026, 6, 12), -1000, "SPLIT BILLER"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 20));

        assert_eq!(charges.len(), 1);
        assert_eq!(charges[0].occurrences, 3);
        assert_eq!(charges[0].typical_amount_minor_units, 1000);
    }

    #[test]
    fn tolerates_a_single_skipped_period() {
        let transactions = vec![
            expense(date(2026, 1, 8), -2500, "INSURANCE"),
            expense(date(2026, 2, 8), -2500, "INSURANCE"),
            // March missed — a failed payment, caught up in April.
            expense(date(2026, 4, 8), -2500, "INSURANCE"),
            expense(date(2026, 5, 8), -2500, "INSURANCE"),
            expense(date(2026, 6, 8), -2500, "INSURANCE"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 20));

        assert_eq!(charges.len(), 1);
        assert_eq!(charges[0].cadence, Cadence::Monthly);
    }

    #[test]
    fn ignores_income() {
        let transactions = vec![
            expense(date(2026, 4, 1), 250000, "SALARY"),
            expense(date(2026, 5, 1), 250000, "SALARY"),
            expense(date(2026, 6, 1), 250000, "SALARY"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 20));

        assert!(charges.is_empty());
    }

    /// A future-dated entry must not resurrect a subscription that stopped
    /// billing months ago.
    #[test]
    fn ignores_transactions_dated_after_today() {
        let transactions = vec![
            expense(date(2025, 9, 12), -1349, "OLD STREAMING"),
            expense(date(2025, 10, 12), -1349, "OLD STREAMING"),
            expense(date(2025, 11, 12), -1349, "OLD STREAMING"),
            expense(date(2026, 12, 12), -1349, "OLD STREAMING"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 1));

        assert_eq!(charges.len(), 1);
        assert_eq!(charges[0].last_seen, date(2025, 11, 12));
        assert!(!charges[0].is_active);
    }

    /// A rhythm that matches no window is reported as no rhythm at all, rather
    /// than being rounded into the nearest cadence.
    #[test]
    fn rejects_a_cadence_between_the_recognized_windows() {
        let transactions = vec![
            expense(date(2026, 5, 1), -1000, "FORTNIGHTLY"),
            expense(date(2026, 5, 15), -1000, "FORTNIGHTLY"),
            expense(date(2026, 5, 29), -1000, "FORTNIGHTLY"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 1));

        assert!(charges.is_empty());
    }

    /// A utility that drifts a little each month is still a commitment; the
    /// tolerance has to be wide enough to keep it.
    #[test]
    fn keeps_a_charge_that_drifts_within_tolerance() {
        let transactions = vec![
            expense(date(2026, 4, 15), -4800, "ENERGY BILL"),
            expense(date(2026, 5, 15), -5000, "ENERGY BILL"),
            expense(date(2026, 6, 15), -5200, "ENERGY BILL"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 20));

        assert_eq!(charges.len(), 1);
        assert_eq!(charges[0].typical_amount_minor_units, 5000);
    }

    #[test]
    fn orders_charges_by_monthly_cost_descending() {
        let transactions = vec![
            expense(date(2026, 4, 2), -900, "CHEAP"),
            expense(date(2026, 5, 2), -900, "CHEAP"),
            expense(date(2026, 6, 2), -900, "CHEAP"),
            expense(date(2026, 4, 4), -8900, "PRICEY"),
            expense(date(2026, 5, 4), -8900, "PRICEY"),
            expense(date(2026, 6, 4), -8900, "PRICEY"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 20));

        assert_eq!(charges[0].label, "PRICEY");
        assert_eq!(charges[1].label, "CHEAP");
    }

    /// A long account or reference number that survived into the amount column
    /// during import would otherwise dominate every total it touches. It can't
    /// reach the median with only one outlier, and the checked arithmetic must
    /// not panic on it either.
    #[test]
    fn survives_an_implausibly_large_amount_without_panicking() {
        let transactions = vec![
            expense(date(2026, 4, 12), -1000, "ODD BILLER"),
            expense(date(2026, 5, 12), -1000, "ODD BILLER"),
            expense(date(2026, 6, 12), i64::MIN, "ODD BILLER"),
        ];

        let charges = detect_recurring_charges(&transactions, date(2026, 6, 20));

        // The outlier fails the stability check, so nothing is reported — the
        // point is that it fails cleanly rather than overflowing.
        assert!(charges.is_empty());
    }
}
