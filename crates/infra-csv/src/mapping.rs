//! Which column means what, and how to read a file once that's decided.
//!
//! This module is deliberately split in two halves that never call each
//! other:
//!
//! - [`detect_mapping`] *guesses* a [`ColumnMapping`] from the file.
//! - [`apply_mapping`] *reads* the file through a mapping, with no guessing
//!   left in it at all.
//!
//! The split is the whole point. Detection is a pile of heuristics over
//! adversarial input and will sometimes be wrong — a bank invents a column
//! layout nobody anticipated, or a file is genuinely ambiguous (is
//! `03/04/2026` March or April?). When that happens the user corrects the
//! mapping in the import dialog and the corrected one goes back through
//! `apply_mapping`, the exact same code path a detected mapping takes. There
//! is no second, hand-mapped import path to keep in sync.

use std::collections::HashSet;

use scrat_domain::transaction::OperationKind;

use crate::detection::{
    self, ColumnStats, column_values, detect_date_format, detect_header, fold, measure_columns,
    parse_amount_cell, parse_date_cell_with, parse_rows,
};
use crate::operation_kind;

/// Header labels naming the transaction's own date, best first.
///
/// A real export routinely carries three of them — Caisse d'Épargne ships
/// "Date de comptabilisation", "Date operation" *and* "Date de valeur" — and
/// they mean different things. The value date is when interest starts
/// counting, which can be days after the money actually moved, so it is the
/// one date that is *not* what a ledger wants; see
/// [`DEPRIORITIZED_DATE_HEADERS`].
const DATE_HEADERS: &[&str] = &[
    "date de comptabilisation",
    "date comptabilisation",
    "booking date",
    "date d'operation",
    "date de l'operation",
    "date operation",
    "operation date",
    "transaction date",
    "date de transaction",
    "date",
];

/// Date labels that name a date other than the transaction's own. Checked
/// *before* [`DATE_HEADERS`], because every one of these also contains the
/// bare word "date" and would otherwise match it.
const DEPRIORITIZED_DATE_HEADERS: &[&str] = &["date de valeur", "date valeur", "value date"];

/// Header labels naming a single signed amount column.
const SINGLE_AMOUNT_HEADERS: &[&str] = &[
    "montant",
    "amount",
    "somme",
    "valeur operation",
    "operation value",
];

/// Header labels naming the money-out half of a two-column amount.
const DEBIT_HEADERS: &[&str] = &[
    "debit",
    "retrait",
    "sortie",
    "depense",
    "withdrawal",
    "money out",
    "paid out",
];

/// Header labels naming the money-in half of a two-column amount.
const CREDIT_HEADERS: &[&str] = &[
    "credit",
    "versement",
    "entree",
    "recette",
    "deposit",
    "money in",
    "paid in",
];

/// Header labels naming the row's own text, best first.
///
/// Exactly one is chosen even when a file offers several (Caisse d'Épargne
/// has "Libelle simplifie", "Libelle operation" *and* "Informations
/// complementaires"). Concatenating them buries the merchant name in
/// reference numbers, and the description is matched verbatim against
/// history to auto-categorize an imported row — so a short, stable string is
/// worth more here than a complete one. The user can point this at a
/// different column in the import dialog if the bank's "simplified" label
/// turns out to be the useless one.
const DESCRIPTION_HEADERS: &[&str] = &[
    "description",
    "libelle simplifie",
    "libelle operation",
    "libelle",
    "designation",
    "intitule",
    "narrative",
    "beneficiaire",
    "payee",
    "merchant",
    "motif",
    "memo",
    "informations complementaires",
    "details",
];

/// Where a row's amount comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmountSource {
    /// One column carrying a signed amount.
    Single(usize),
    /// Two columns, at most one populated per row: `debit` is money out,
    /// `credit` money in. Banks disagree about whether the debit column
    /// carries its own minus sign (Caisse d'Épargne writes `-9,99`, plenty
    /// of others write `9,99`), so [`apply_mapping`] takes the magnitude and
    /// applies the sign the *column* implies — correct either way.
    DebitCredit { debit: usize, credit: usize },
}

/// A complete answer to "what does this file's grid mean" — everything
/// detection guessed, in one editable value.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnMapping {
    pub delimiter: char,
    pub has_header: bool,
    pub column_count: usize,
    pub date_column: Option<usize>,
    /// The one chrono format the date column is read with. Pinned for the
    /// whole column rather than re-sniffed per cell, so a file can't switch
    /// interpretation halfway down and silently produce a mix of March and
    /// April dates.
    pub date_format: String,
    pub amount: Option<AmountSource>,
    /// Columns whose text is joined, in the order given, to form the row's
    /// description.
    ///
    /// Always an explicit list — there is deliberately no "everything not
    /// otherwise used" mode. A bank export surrounds the description with
    /// columns that are also text (the instrument, a category hint, a
    /// reference, a flag), and sweeping them all in buries the merchant name
    /// in noise, which then poisons the exact-description match used to
    /// auto-categorize an imported row. Which columns are in play is
    /// something the user can see and correct; "whatever's left" is not.
    pub description_columns: Vec<usize>,
    pub category_column: Option<usize>,
    pub subcategory_column: Option<usize>,
    pub currency_column: Option<usize>,
    pub account_column: Option<usize>,
    pub operation_kind_column: Option<usize>,
}

/// A CSV split into its header (if it has one) and its data rows, before any
/// meaning has been assigned to the columns.
#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub delimiter: char,
    pub header: Option<Vec<String>>,
    pub rows: Vec<Vec<String>>,
    pub column_count: usize,
}

impl ParsedFile {
    /// Moves the first row in or out of the header position.
    ///
    /// A mapping that came from the user (or from a remembered one) is
    /// authoritative about whether the file has a header — whether
    /// `detect_header` got it right is one of the things the user is there
    /// to correct, so its opinion must not survive the correction.
    pub fn set_has_header(&mut self, has_header: bool) {
        match (has_header, self.header.is_some()) {
            (true, false) if !self.rows.is_empty() => self.header = Some(self.rows.remove(0)),
            (false, true) => {
                if let Some(header) = self.header.take() {
                    self.rows.insert(0, header);
                }
            }
            _ => {}
        }
    }
}

/// Decodes, sniffs the delimiter, splits into rows, and lifts off a header
/// row if the file has one.
pub fn parse_file(bytes: &[u8]) -> ParsedFile {
    let text = detection::decode_bytes(bytes);
    let delimiter = detection::sniff_delimiter(&text);
    let mut rows = parse_rows(&text, delimiter);
    let header = if detect_header(&rows) && !rows.is_empty() {
        Some(rows.remove(0))
    } else {
        None
    };
    let column_count = rows
        .iter()
        .map(|r| r.len())
        .chain(header.iter().map(|h| h.len()))
        .max()
        .unwrap_or(0);
    ParsedFile {
        delimiter,
        header,
        rows,
        column_count,
    }
}

/// A stable identifier for "files that look like this one", so a mapping the
/// user corrected once can be offered again the next time they export from
/// the same bank.
///
/// Keyed on the header row where there is one — that's what actually
/// identifies a bank's export format, and it survives the file covering a
/// different month with different data underneath.
///
/// A headerless file has no such name, so it's identified by its *shape*
/// instead: the delimiter plus one character per column saying whether it
/// holds dates, numbers, text, or nothing. That is weaker, and two banks
/// could in principle collide on it. What keeps that safe is that a
/// remembered mapping is never applied silently — the dialog shows the
/// mapping and the resulting rows before anything is imported, so a wrong
/// match is visible and correctable exactly like a wrong guess.
///
/// The `h1:`/`s1:` prefixes version the scheme, so changing how signatures
/// are built retires the old rows instead of matching them wrongly.
pub fn file_signature(file: &ParsedFile) -> String {
    match &file.header {
        Some(header) => {
            let cells: Vec<String> = (0..file.column_count)
                .map(|i| header.get(i).map(|c| fold(c.trim())).unwrap_or_default())
                .collect();
            format!("h1:{}", cells.join("\u{1f}"))
        }
        None => {
            let shape: String = measure_columns(&file.rows, file.column_count)
                .iter()
                .map(|s| {
                    if s.coverage == 0.0 {
                        '-'
                    } else if s.date_rate >= 0.8 {
                        'd'
                    } else if s.amount_rate >= 0.8 {
                        'n'
                    } else {
                        't'
                    }
                })
                .collect();
            format!("s1:{}:{}", file.delimiter, shape)
        }
    }
}

/// One column as the import dialog needs to show it: what the file calls it,
/// and enough real values to recognize it by when it calls it nothing.
#[derive(Debug, Clone)]
pub struct ColumnSummary {
    pub index: usize,
    pub header: Option<String>,
    pub samples: Vec<String>,
}

pub fn summarize_columns(file: &ParsedFile) -> Vec<ColumnSummary> {
    (0..file.column_count)
        .map(|index| ColumnSummary {
            index,
            header: file
                .header
                .as_ref()
                .and_then(|h| h.get(index))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            samples: column_values(&file.rows, index)
                .into_iter()
                .take(3)
                .map(|s| s.trim().to_string())
                .collect(),
        })
        .collect()
}

/// Index of the first keyword in `keywords` that the (folded) header cell
/// contains. Lower is a better match.
fn keyword_rank(header_cell: &str, keywords: &[&str]) -> Option<usize> {
    let folded = fold(header_cell.trim());
    keywords.iter().position(|k| folded.contains(k))
}

/// Finds the column whose header best matches `keywords` — **by keyword
/// priority, not by column position**.
///
/// The distinction is load-bearing. Scanning columns left-to-right and
/// taking the first that matches *any* keyword let Caisse d'Épargne's
/// "Libelle operation" (column 2) claim the operation-type field ahead of
/// its actual "Type operation" (column 5), which both mislabeled every row
/// and stripped the real description out of the file. Ranking by keyword
/// means the most specific label wins wherever it sits.
fn find_labeled_column(
    header: &[String],
    keywords: &[&str],
    exclude: &HashSet<usize>,
) -> Option<usize> {
    (0..keywords.len()).find_map(|rank| {
        header.iter().enumerate().position(|(i, cell)| {
            !exclude.contains(&i) && keyword_rank(cell, &keywords[rank..=rank]).is_some()
        })
    })
}

/// A column that parses as dates is not money, whatever else it looks like.
fn looks_like_dates(stats: &ColumnStats) -> bool {
    stats.date_rate >= 0.8 && stats.coverage > 0.0
}

/// Does this column's *content* back up a header that claims it holds money?
fn corroborates_as_amount(stats: &ColumnStats) -> bool {
    stats.amount_rate >= 0.8 && stats.amount_plausibility > 0.0 && !looks_like_dates(stats)
}

fn is_empty_column(stats: &ColumnStats) -> bool {
    stats.coverage == 0.0
}

/// Picks the highest-scoring candidate, keeping the **earliest** column on a
/// tie.
///
/// The tie-break is not cosmetic. Caisse d'Épargne's three date columns all
/// parse at 100%, and `Iterator::max_by` returns the *last* maximum — which
/// silently handed every row the value date (identical on every row of a
/// statement) instead of the date the money moved.
fn best_scoring(candidates: &[usize], score: impl Fn(usize) -> f64) -> Option<usize> {
    candidates
        .iter()
        .fold(None, |best: Option<(usize, f64)>, &c| {
            let s = score(c);
            match best {
                Some((_, best_score)) if best_score >= s => best,
                _ => Some((c, s)),
            }
        })
        .filter(|&(_, s)| s > 0.0)
        .map(|(c, _)| c)
}

fn date_name_bonus(header: Option<&Vec<String>>, col: usize) -> f64 {
    let Some(cell) = header.and_then(|h| h.get(col)) else {
        return 0.0;
    };
    if keyword_rank(cell, DEPRIORITIZED_DATE_HEADERS).is_some() {
        return 0.15;
    }
    match keyword_rank(cell, DATE_HEADERS) {
        Some(rank) => 1.0 - rank as f64 * 0.02,
        None => 0.0,
    }
}

fn name_bonus(header: Option<&Vec<String>>, col: usize, keywords: &[&str]) -> f64 {
    header
        .and_then(|h| h.get(col))
        .and_then(|cell| keyword_rank(cell, keywords))
        .map_or(0.0, |rank| 1.0 - rank as f64 * 0.02)
}

fn detect_date_column(
    header: Option<&Vec<String>>,
    stats: &[ColumnStats],
    reserved: &HashSet<usize>,
) -> Option<usize> {
    let candidates: Vec<usize> = (0..stats.len())
        .filter(|c| !reserved.contains(c) && stats[*c].date_rate >= 0.5)
        .collect();
    best_scoring(&candidates, |c| {
        stats[c].date_rate * stats[c].coverage * (1.0 + date_name_bonus(header, c))
    })
}

/// True when the two columns behave like a debit/credit pair: rarely both
/// populated on the same row, and between them covering most of the file.
fn behaves_like_debit_credit(rows: &[Vec<String>], debit: usize, credit: usize) -> bool {
    if rows.is_empty() {
        return false;
    }
    let amount_at = |row: &Vec<String>, col: usize| {
        row.get(col)
            .and_then(|c| parse_amount_cell(c))
            .filter(|&v| v != 0)
    };
    let mut both = 0;
    let mut either = 0;
    for row in rows {
        match (amount_at(row, debit), amount_at(row, credit)) {
            (Some(_), Some(_)) => {
                both += 1;
                either += 1;
            }
            (Some(_), None) | (None, Some(_)) => either += 1,
            (None, None) => {}
        }
    }
    let total = rows.len() as f64;
    either as f64 / total >= 0.8 && both as f64 / total <= 0.05
}

/// Assigns which of two columns is the money-out one. A column carrying
/// negative values is unambiguously the debit side; when neither is signed
/// (a bank writing both halves as bare positives), the left column is taken
/// as debit — the order every statement prints them in.
fn order_debit_credit(rows: &[Vec<String>], a: usize, b: usize) -> (usize, usize) {
    let negatives = |col: usize| {
        rows.iter()
            .filter(|r| {
                r.get(col)
                    .and_then(|c| parse_amount_cell(c))
                    .is_some_and(|v| v < 0)
            })
            .count()
    };
    if negatives(b) > negatives(a) {
        (b, a)
    } else {
        (a, b)
    }
}

fn detect_amount(
    header: Option<&Vec<String>>,
    rows: &[Vec<String>],
    stats: &[ColumnStats],
    reserved: &HashSet<usize>,
) -> Option<AmountSource> {
    let usable: Vec<usize> = (0..stats.len())
        .filter(|c| !reserved.contains(c) && !looks_like_dates(&stats[*c]))
        .collect();

    // 1. The bank labeled a debit/credit pair. Accept it when neither half
    //    contradicts its label — but tolerate one half being *entirely
    //    blank*, which is the normal shape of a statement covering a period
    //    with no income (or no spending) at all. Demanding that both halves
    //    corroborate would reject exactly those files.
    if let Some(header) = header {
        let excluded: HashSet<usize> = (0..stats.len()).filter(|c| !usable.contains(c)).collect();
        let debit = find_labeled_column(header, DEBIT_HEADERS, &excluded);
        let credit = find_labeled_column(header, CREDIT_HEADERS, &excluded);
        if let (Some(debit), Some(credit)) = (debit, credit) {
            let (ds, cs) = (&stats[debit], &stats[credit]);
            let acceptable = |s: &ColumnStats| corroborates_as_amount(s) || is_empty_column(s);
            if debit != credit
                && acceptable(ds)
                && acceptable(cs)
                && (corroborates_as_amount(ds) || corroborates_as_amount(cs))
            {
                return Some(AmountSource::DebitCredit { debit, credit });
            }
        }
    }

    // 2. The bank labeled a single signed amount column.
    if let Some(header) = header {
        let named = best_scoring(&usable, |c| {
            let bonus = name_bonus(Some(header), c, SINGLE_AMOUNT_HEADERS);
            if bonus > 0.0 && corroborates_as_amount(&stats[c]) {
                bonus
            } else {
                0.0
            }
        });
        if let Some(c) = named {
            return Some(AmountSource::Single(c));
        }
    }

    let scored: Vec<usize> = usable
        .iter()
        .copied()
        .filter(|c| stats[*c].amount_rate >= 0.5 && stats[*c].amount_plausibility > 0.0)
        .collect();

    // 3. No header, or one that named nothing: recognize a debit/credit pair
    //    by behavior instead. Only pairs where neither column would do on
    //    its own are considered, so a plain signed-amount file never gets
    //    split across two columns.
    let pair = scored
        .iter()
        .enumerate()
        .flat_map(|(i, &a)| scored[i + 1..].iter().map(move |&b| (a, b)))
        .filter(|&(a, b)| {
            stats[a].coverage < 0.9
                && stats[b].coverage < 0.9
                && behaves_like_debit_credit(rows, a, b)
        })
        .max_by(|&(a1, b1), &(a2, b2)| {
            let score = |a: usize, b: usize| stats[a].coverage + stats[b].coverage;
            score(a1, b1).partial_cmp(&score(a2, b2)).unwrap()
        });
    if let Some((a, b)) = pair {
        let (debit, credit) = order_debit_credit(rows, a, b);
        return Some(AmountSource::DebitCredit { debit, credit });
    }

    // 4. Fall back to whichever single column looks most like money.
    best_scoring(&scored, |c| {
        stats[c].amount_rate * stats[c].coverage * stats[c].amount_plausibility
    })
    .map(AmountSource::Single)
}

/// Picks the description columns from content, for a file whose header names
/// none (or has no header at all).
///
/// Takes *every* column scoring at least half the best one, not just the
/// winner, because a real export genuinely splits the description across
/// columns: LCL writes card purchases in one column and transfers in the
/// next, so either alone loses half the file's descriptions. Everything
/// well below the best is the noise `description_likelihood` exists to
/// reject — the instrument, the category hint, the reference, the flag.
fn detect_description_columns(stats: &[ColumnStats], reserved: &HashSet<usize>) -> Vec<usize> {
    /// Relative, not absolute: what counts as "wordy" depends on how wordy
    /// this particular bank's descriptions are.
    const SHARE_OF_BEST: f64 = 0.5;

    let candidates: Vec<usize> = (0..stats.len()).filter(|c| !reserved.contains(c)).collect();
    let best = candidates
        .iter()
        .map(|&c| stats[c].description_likelihood)
        .fold(0.0, f64::max);
    if best <= 0.0 {
        return Vec::new();
    }
    candidates
        .into_iter()
        .filter(|&c| stats[c].description_likelihood >= best * SHARE_OF_BEST)
        .collect()
}

/// Finds an unlabeled column holding the payment instrument, for a file
/// whose header doesn't name one.
///
/// This exists because the description is no longer "everything left over".
/// A headerless export like LCL puts `Carte` / `Virement` in a column of its
/// own; that column used to reach `operation_kind` by accident, swept into
/// the description text and read back out of it. Now that the description is
/// only the columns that are actually prose, the instrument has to be
/// recognized as the column it is.
///
/// Deliberately run *after* the description columns are chosen and excluded:
/// description text routinely names an instrument too (`CB SOME STORE`,
/// `PRLV SEPA SOME BILL`), so on its own this test would happily claim the
/// description itself.
fn detect_operation_kind_column(
    rows: &[Vec<String>],
    column_count: usize,
    exclude: &HashSet<usize>,
) -> Option<usize> {
    /// An instrument column is a closed vocabulary of short labels. Both
    /// bounds are what keep prose out.
    const MAX_DISTINCT: usize = 8;
    const MAX_LEN: usize = 24;
    const MIN_RECOGNIZED: f64 = 0.8;

    (0..column_count)
        .filter(|c| !exclude.contains(c))
        .filter_map(|c| {
            let values = column_values(rows, c);
            if values.is_empty() {
                return None;
            }
            let distinct: HashSet<&str> = values.iter().map(|v| v.trim()).collect();
            if distinct.len() > MAX_DISTINCT || distinct.iter().any(|v| v.chars().count() > MAX_LEN)
            {
                return None;
            }
            // `Other` means "named something this vocabulary doesn't know",
            // which any text column would score — only a genuine hit counts.
            let recognized = values
                .iter()
                .filter(|v| {
                    matches!(operation_kind::from_label(v), Some(k) if k != OperationKind::Other)
                })
                .count() as f64
                / values.len() as f64;
            (recognized >= MIN_RECOGNIZED).then_some((c, values.len()))
        })
        // Prefer the column that labels the most rows.
        .max_by_key(|&(_, populated)| populated)
        .map(|(c, _)| c)
}

/// The full guess. Named fields are resolved first because they constrain
/// what's left: a column the header calls "Categorie" is not a candidate for
/// the amount, however its contents happen to parse.
pub fn detect_mapping(file: &ParsedFile) -> ColumnMapping {
    let stats = measure_columns(&file.rows, file.column_count);
    let header = file.header.as_ref();
    let none = HashSet::new();

    let (
        subcategory_column,
        category_column,
        currency_column,
        account_column,
        operation_kind_column,
        description_column,
    ) = match header {
        Some(h) => {
            let subcategory = find_labeled_column(
                h,
                &["subcateg", "sous-categ", "sous categ", "souscateg"],
                &none,
            );
            let mut taken: HashSet<usize> = subcategory.into_iter().collect();
            let category = find_labeled_column(h, &["categ"], &taken);
            taken.extend(category);
            let currency = find_labeled_column(h, &["currency", "devise"], &taken);
            taken.extend(currency);
            let account = find_labeled_column(h, &["account", "compte"], &taken);
            taken.extend(account);
            let operation_kind = find_operation_kind_column(h, &taken);
            taken.extend(operation_kind);
            let description = find_labeled_column(h, DESCRIPTION_HEADERS, &taken);
            (
                subcategory,
                category,
                currency,
                account,
                operation_kind,
                description,
            )
        }
        None => (None, None, None, None, None, None),
    };

    let reserved: HashSet<usize> = [
        subcategory_column,
        category_column,
        currency_column,
        account_column,
        operation_kind_column,
        description_column,
    ]
    .into_iter()
    .flatten()
    .collect();

    let date_column = detect_date_column(header, &stats, &reserved);
    let mut claimed = reserved.clone();
    claimed.extend(date_column);
    let amount = detect_amount(header, &file.rows, &stats, &claimed);
    match amount {
        Some(AmountSource::Single(c)) => {
            claimed.insert(c);
        }
        Some(AmountSource::DebitCredit { debit, credit }) => {
            claimed.extend([debit, credit]);
        }
        None => {}
    }

    // A header that names the description settles it; otherwise the columns
    // that read as prose are found by content.
    let description_columns = match description_column {
        Some(c) => vec![c],
        None => detect_description_columns(&stats, &claimed),
    };
    claimed.extend(description_columns.iter().copied());

    // Only now, with the description accounted for, can an unlabeled
    // instrument column be told apart from the description itself.
    let operation_kind_column = operation_kind_column
        .or_else(|| detect_operation_kind_column(&file.rows, file.column_count, &claimed));

    let date_format = date_column
        .map(|c| detect_date_format(&column_values(&file.rows, c)))
        .unwrap_or("%Y-%m-%d")
        .to_string();

    ColumnMapping {
        delimiter: file.delimiter,
        has_header: file.header.is_some(),
        column_count: file.column_count,
        date_column,
        date_format,
        amount,
        description_columns,
        category_column,
        subcategory_column,
        currency_column,
        account_column,
        operation_kind_column,
    }
}

/// Finds a header cell naming the operation-type column, e.g. the French
/// "Type opération" / "Nature de l'opération" or an English "Transaction
/// type".
///
/// Matched on the *pair* of words rather than a bare "type", which on its
/// own is far too eager — "Type de compte" (account type) and "Type de
/// carte" both contain it and mean something else entirely. A bare `type`
/// header is accepted only as an exact cell match, where there's nothing
/// else it could be qualifying.
///
/// Note what is deliberately *absent*: "Libelle operation". It reads like a
/// type column and is not one — it's the raw description text, and claiming
/// it here both mislabeled every row and stripped the description out of the
/// file entirely.
fn find_operation_kind_column(header: &[String], exclude: &HashSet<usize>) -> Option<usize> {
    find_labeled_column(
        header,
        &[
            "type operation",
            "type d'operation",
            "typeoperation",
            "operation type",
            "transaction type",
            "type de transaction",
            "nature",
            "payment method",
            "mode de paiement",
        ],
        exclude,
    )
    .or_else(|| {
        header.iter().enumerate().position(|(i, cell)| {
            !exclude.contains(&i) && matches!(fold(cell.trim()).as_str(), "type" | "operation")
        })
    })
}

#[derive(Debug, Clone)]
pub struct ParsedRow {
    pub date: Option<chrono::NaiveDate>,
    pub amount_minor_units: Option<i64>,
    pub description: String,
    /// The raw text of the file's own Category column, if the mapping names
    /// one — used to file the row under a matching category (creating it if
    /// needed) instead of the fallback category chosen for the whole import;
    /// see `commit_csv_import`.
    pub csv_category: Option<String>,
    /// The raw text of the file's own Subcategory column, if the mapping
    /// names one — nests under `csv_category` when both are applied,
    /// mirroring the app's own CSV export format.
    pub csv_subcategory: Option<String>,
    /// How the money moved. Taken from the mapped operation-type column when
    /// there is one, and otherwise read out of the description text —
    /// falling back to [`OperationKind::Card`], which is both the commonest
    /// instrument and the likeliest meaning of a file that never says.
    pub operation_kind: OperationKind,
    /// True when this row looks like a bank's opening/closing balance line
    /// rather than a real transaction — see `is_boundary_balance_row`.
    /// Doesn't affect `is_valid` (the row still parses as a perfectly good
    /// date+amount); it only changes the default checked state the import
    /// UI starts the row at, since the heuristic can misfire and the user
    /// can always re-check the row.
    pub is_likely_balance_row: bool,
    pub raw: Vec<String>,
}

impl ParsedRow {
    /// A row usable for import needs both a date and a non-zero amount —
    /// `Transaction` itself rejects zero amounts, and a missing date can't
    /// be defaulted sensibly.
    pub fn is_valid(&self) -> bool {
        self.date.is_some() && matches!(self.amount_minor_units, Some(a) if a != 0)
    }
}

#[derive(Debug, Clone)]
pub struct ImportPreview {
    pub rows: Vec<ParsedRow>,
    pub mapping: ColumnMapping,
    pub columns: Vec<ColumnSummary>,
    /// Fraction of rows the mapping actually yielded a date for — measured
    /// from the parsed result, not from the score of the column that won
    /// detection. The old confidence was the latter, which is how a file
    /// where *nothing* was importable still reported 100%.
    pub date_confidence: f64,
    /// Fraction of rows the mapping actually yielded a non-zero amount for.
    pub amount_confidence: f64,
}

/// The most common field count across `rows` — the shape real transaction
/// rows share, used by [`is_boundary_balance_row`] as the baseline a
/// boundary row is compared against.
fn modal_row_len(rows: &[Vec<String>]) -> usize {
    let mut frequency = std::collections::HashMap::new();
    for r in rows {
        *frequency.entry(r.len()).or_insert(0) += 1;
    }
    frequency
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(len, _)| len)
        .unwrap_or(0)
}

/// True if row `idx` is the file's first or last data row and is
/// structurally thinner than the rest. Most banks bookend an export with
/// one row per boundary carrying the balance as of the start/end date — just
/// a date, an amount, and a reference/account code — missing the
/// transaction-type, counterparty, and reference columns a real transaction
/// row has, so it parses into fewer fields than the modal row.
///
/// Only the very first and very last row are ever considered: a short row
/// in the middle of the file is far more likely to be a genuine transaction
/// with some blank trailing fields than a balance line, and banks only ever
/// bookend the whole file, never the middle of it.
fn is_boundary_balance_row(rows: &[Vec<String>], idx: usize, modal_len: usize) -> bool {
    rows.len() >= 3 && (idx == 0 || idx == rows.len() - 1) && rows[idx].len() < modal_len
}

fn amount_for_row(raw: &[String], source: Option<AmountSource>) -> Option<i64> {
    let cell = |col: usize| {
        raw.get(col)
            .and_then(|c| parse_amount_cell(c))
            .filter(|&v| v != 0)
    };
    match source? {
        AmountSource::Single(c) => raw.get(c).and_then(|c| parse_amount_cell(c)),
        AmountSource::DebitCredit { debit, credit } => match (cell(debit), cell(credit)) {
            (Some(d), None) => Some(-d.abs()),
            (None, Some(c)) => Some(c.abs()),
            // Both halves populated is not a shape any bank intends, but
            // netting them is the only reading that doesn't discard money.
            (Some(d), Some(c)) => Some(c.abs() - d.abs()),
            (None, None) => None,
        },
    }
}

/// Reads `file` through `mapping`. No heuristics run here — every decision
/// was already made (or corrected by the user) in the mapping.
pub fn apply_mapping(file: &ParsedFile, mapping: &ColumnMapping) -> ImportPreview {
    let rows = &file.rows;
    let modal_len = modal_row_len(rows);
    let description_columns = &mapping.description_columns;

    let parsed_rows: Vec<ParsedRow> = rows
        .iter()
        .enumerate()
        .map(|(i, raw)| {
            let cell_text = |col: Option<usize>| {
                col.and_then(|c| raw.get(c))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            };
            let description = description_columns
                .iter()
                .filter_map(|&c| raw.get(c))
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .collect::<Vec<_>>()
                .join(" ");

            // The instrument column is the bank's own answer, so it wins —
            // whether the header named it or `detect_operation_kind_column`
            // found it unlabeled. A blank cell falls back to reading the
            // description text, which is where plenty of exports put the
            // instrument inline ("CB SOME STORE", "PRLV SEPA UTILITY").
            let operation_kind = cell_text(mapping.operation_kind_column)
                .and_then(|label| operation_kind::from_label(&label))
                .unwrap_or_else(|| operation_kind::from_description(&description));

            ParsedRow {
                date: mapping
                    .date_column
                    .and_then(|c| raw.get(c))
                    .and_then(|c| parse_date_cell_with(c, &mapping.date_format)),
                amount_minor_units: amount_for_row(raw, mapping.amount),
                operation_kind,
                description,
                csv_category: cell_text(mapping.category_column),
                csv_subcategory: cell_text(mapping.subcategory_column),
                is_likely_balance_row: is_boundary_balance_row(rows, i, modal_len),
                raw: raw.clone(),
            }
        })
        .collect();

    let total = parsed_rows.len().max(1) as f64;
    ImportPreview {
        date_confidence: parsed_rows.iter().filter(|r| r.date.is_some()).count() as f64 / total,
        amount_confidence: parsed_rows
            .iter()
            .filter(|r| matches!(r.amount_minor_units, Some(a) if a != 0))
            .count() as f64
            / total,
        columns: summarize_columns(file),
        mapping: mapping.clone(),
        rows: parsed_rows,
    }
}

/// Detect a mapping and immediately read the file through it — what the
/// import dialog calls the first time it sees a file.
pub fn build_preview(bytes: &[u8]) -> ImportPreview {
    let file = parse_file(bytes);
    let mapping = detect_mapping(&file);
    apply_mapping(&file, &mapping)
}

/// Read a file through a mapping the caller already has — what the import
/// dialog calls after the user corrects a column, and what a remembered
/// mapping is replayed through.
pub fn preview_with_mapping(bytes: &[u8], mapping: &ColumnMapping) -> ImportPreview {
    let mut file = parse_file(bytes);
    file.set_has_header(mapping.has_header);
    apply_mapping(&file, mapping)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    /// The Caisse d'Épargne shape, fabricated but structurally exact: three
    /// date columns, a separate Debit and Credit, a constant "Pointage"
    /// flag column, and a "Libelle operation" that is description text
    /// rather than an operation type. Every one of those broke detection.
    const CAISSE_SHAPE: &str = "\
Date de comptabilisation;Libelle simplifie;Libelle operation;Reference;Informations complementaires;Type operation;Categorie;Sous categorie;Debit;Credit;Date operation;Date de valeur;Pointage operation
31/07/2026;PayPal;PRLV PayPal Europe;A0001;A0001-;Prelevement;Shopping;Shopping - autre;-9,99;;31/07/2026;04/08/2026;0
30/07/2026;ACME SARL;VIR SEPA ACME SARL;A0002;Virement ACME-0036;Virement recu;Rentree d'argent;Virement recu;;+2521,14;30/07/2026;04/08/2026;0
29/07/2026;NAVIGO;NAVIGO;;FR 75PARIS-;Carte bancaire;Transports;Transports en commun;-2,55;;29/07/2026;04/08/2026;0
28/07/2026;BAKERY;BAKERY;;FR PARIS-;Carte bancaire;Alimentation;Alimentation - autre;-3,40;;28/07/2026;04/08/2026;0
";

    fn preview(text: &str) -> ImportPreview {
        build_preview(text.as_bytes())
    }

    #[test]
    fn caisse_shape_maps_every_field_to_the_column_the_header_names() {
        let file = parse_file(CAISSE_SHAPE.as_bytes());
        let mapping = detect_mapping(&file);

        assert_eq!(mapping.date_column, Some(0), "booking date, not value date");
        assert_eq!(
            mapping.amount,
            Some(AmountSource::DebitCredit {
                debit: 8,
                credit: 9
            })
        );
        assert_eq!(mapping.description_columns, vec![1]);
        assert_eq!(
            mapping.operation_kind_column,
            Some(5),
            "not 'Libelle operation'"
        );
        assert_eq!(mapping.category_column, Some(6));
        assert_eq!(mapping.subcategory_column, Some(7));
        assert_eq!(mapping.date_format, "%d/%m/%Y");
    }

    /// The regression that motivated all of this: every row of this file used
    /// to come out with `amount == 0` (the constant "Pointage operation"
    /// column won detection), so nothing at all was importable.
    #[test]
    fn caisse_shape_yields_a_usable_amount_for_every_row() {
        let preview = preview(CAISSE_SHAPE);

        assert_eq!(preview.rows.len(), 4);
        assert!(preview.rows.iter().all(|r| r.is_valid()));
        assert_eq!(
            preview
                .rows
                .iter()
                .map(|r| r.amount_minor_units.unwrap())
                .collect::<Vec<_>>(),
            vec![-999, 252_114, -255, -340],
        );
        assert_eq!(preview.amount_confidence, 1.0);
        assert_eq!(preview.date_confidence, 1.0);
    }

    /// A credit row is income. Reading only the Debit column would drop it
    /// silently, which is the whole reason `AmountSource` has two shapes.
    #[test]
    fn a_credit_only_row_is_positive_and_a_debit_only_row_is_negative() {
        let preview = preview(CAISSE_SHAPE);
        assert_eq!(preview.rows[0].amount_minor_units, Some(-999));
        assert_eq!(preview.rows[1].amount_minor_units, Some(252_114));
    }

    #[test]
    fn caisse_shape_takes_the_booking_date_not_the_value_date() {
        let preview = preview(CAISSE_SHAPE);
        assert_eq!(
            preview.rows.iter().map(|r| r.date).collect::<Vec<_>>(),
            vec![
                NaiveDate::from_ymd_opt(2026, 7, 31),
                NaiveDate::from_ymd_opt(2026, 7, 30),
                NaiveDate::from_ymd_opt(2026, 7, 29),
                NaiveDate::from_ymd_opt(2026, 7, 28),
            ],
        );
    }

    /// Reference numbers, the unused date columns and the raw operation
    /// label all used to be concatenated into the description.
    #[test]
    fn caisse_shape_description_is_the_merchant_alone() {
        let preview = preview(CAISSE_SHAPE);
        assert_eq!(
            preview
                .rows
                .iter()
                .map(|r| r.description.as_str())
                .collect::<Vec<_>>(),
            vec!["PayPal", "ACME SARL", "NAVIGO", "BAKERY"],
        );
    }

    #[test]
    fn caisse_shape_reads_the_operation_kind_from_the_type_column() {
        let preview = preview(CAISSE_SHAPE);
        assert_eq!(
            preview
                .rows
                .iter()
                .map(|r| r.operation_kind)
                .collect::<Vec<_>>(),
            vec![
                OperationKind::DirectDebit,
                OperationKind::BankTransfer,
                OperationKind::Card,
                OperationKind::Card,
            ],
        );
    }

    /// A statement covering a period with no income at all leaves the whole
    /// Credit column blank. The pair must still be recognized — demanding
    /// that both halves corroborate would reject exactly these files.
    #[test]
    fn a_debit_credit_pair_is_recognized_when_one_half_is_entirely_blank() {
        let text = "\
Date;Libelle;Debit;Credit
01/07/2026;STORE A;-12,00;
02/07/2026;STORE B;-3,40;
03/07/2026;STORE C;-49,00;
";
        let file = parse_file(text.as_bytes());
        let mapping = detect_mapping(&file);
        assert_eq!(
            mapping.amount,
            Some(AmountSource::DebitCredit {
                debit: 2,
                credit: 3
            })
        );
        let preview = apply_mapping(&file, &mapping);
        assert!(preview.rows.iter().all(|r| r.amount_minor_units < Some(0)));
    }

    /// Some banks write both halves as bare positives and let the column
    /// carry the meaning. The debit column's magnitude must still come out
    /// negative.
    #[test]
    fn an_unsigned_debit_column_still_produces_a_negative_amount() {
        let text = "\
Date;Libelle;Debit;Credit
01/07/2026;STORE A;12,00;
02/07/2026;EMPLOYER;;2500,00
03/07/2026;STORE C;49,00;
";
        let preview = preview(text);
        assert_eq!(
            preview
                .rows
                .iter()
                .map(|r| r.amount_minor_units.unwrap())
                .collect::<Vec<_>>(),
            vec![-1_200, 250_000, -4_900],
        );
    }

    /// Without a header, the pair is recognized from behavior: two numeric
    /// columns that are never both populated and together cover the file.
    #[test]
    fn a_headerless_debit_credit_pair_is_recognized_from_its_shape() {
        let text = "\
01/07/2026;STORE A;12,00;
02/07/2026;EMPLOYER;;2500,00
03/07/2026;STORE C;49,00;
04/07/2026;STORE D;7,25;
05/07/2026;REFUND;;18,40
";
        let file = parse_file(text.as_bytes());
        let mapping = detect_mapping(&file);
        assert_eq!(
            mapping.amount,
            Some(AmountSource::DebitCredit {
                debit: 2,
                credit: 3
            })
        );
    }

    /// …but an ordinary single signed-amount column must never be split
    /// across two columns just because some other column also holds numbers.
    #[test]
    fn a_single_signed_amount_column_is_not_mistaken_for_a_debit_credit_pair() {
        let text = "01/07/2026;10,00;STORE A\n02/07/2026;-5,00;STORE B\n03/07/2026;-6,00;STORE C\n";
        let file = parse_file(text.as_bytes());
        assert_eq!(detect_mapping(&file).amount, Some(AmountSource::Single(1)));
    }

    /// The false positive that recognizing debit/credit pairs by behavior
    /// introduced, and that rejecting text in [`parse_amount_cell`] closed.
    ///
    /// This headerless shape has a sparse reference column and a description
    /// column whose text embeds a date and a card number. Both used to
    /// "parse" as amounts, and together they satisfied every structural test
    /// for a debit/credit pair — so the real amount column (1) lost, and the
    /// file's descriptions were read as money.
    #[test]
    fn a_description_column_embedding_digits_is_not_half_of_a_debit_credit_pair() {
        let text = "\
01/05/2026;841,76;;00122 021115Z
01/05/2026;-35;Carte;;CB  SOME STORE  30/06/26;;0;Divers
01/05/2026;2291,1;Virement;;;VIREMENT ENTREPRISE;;
01/05/2026;-0,07;;;INTERETS DEBITEURS AU 30 06 26;;;
01/05/2026;-4;Carte;;CB  FLOWERS CITY  30/06/26;;0;Divers
05/06/2026;2890,27;;00122 021115Z
";
        let file = parse_file(text.as_bytes());
        let mapping = detect_mapping(&file);

        assert_eq!(mapping.amount, Some(AmountSource::Single(1)));
        let preview = apply_mapping(&file, &mapping);
        assert!(preview.rows.iter().all(|r| r.is_valid()));
        assert_eq!(preview.rows[0].amount_minor_units, Some(84_176));
        assert_eq!(preview.rows[1].amount_minor_units, Some(-3_500));
    }

    #[test]
    fn a_constant_flag_column_never_wins_the_amount() {
        // Column 3 is `0` on every row: perfectly parseable, fully
        // populated, and completely meaningless as money.
        let text = "\
Date;Libelle;Montant;Pointage
01/07/2026;STORE A;-12,00;0
02/07/2026;STORE B;-3,40;0
03/07/2026;STORE C;-49,00;0
";
        let file = parse_file(text.as_bytes());
        assert_eq!(detect_mapping(&file).amount, Some(AmountSource::Single(2)));
    }

    /// The headerless LCL shape, which already worked and must keep working:
    /// ragged rows, a description that shifts column by transaction type,
    /// and balance lines bookending the file.
    #[test]
    fn headerless_ragged_export_still_concatenates_remaining_columns() {
        let text = "\
01/07/2026;500,00;;ACC REF 12345
01/07/2026;-35,00;Carte;;CB SOME STORE   30/06/26;;0;Divers
01/07/2026;120,50;Virement;;;INCOMING WAGES;;
01/07/2026;-19,99;Prelevement;;PRLV SEPA SOME BILL;;;
05/07/2026;565,51;;ACC REF 12345
";
        let preview = preview(text);

        assert_eq!(preview.rows.len(), 5);
        assert!(preview.rows.iter().all(|r| r.is_valid()));
        assert_eq!(preview.rows[1].amount_minor_units, Some(-3_500));
        assert!(preview.rows[1].description.contains("CB SOME STORE"));
        assert!(preview.rows[2].description.contains("INCOMING WAGES"));
        assert_eq!(preview.rows[1].operation_kind, OperationKind::Card);
        assert_eq!(preview.rows[2].operation_kind, OperationKind::BankTransfer);
        assert_eq!(preview.rows[3].operation_kind, OperationKind::DirectDebit);
        assert!(preview.rows[0].is_likely_balance_row);
        assert!(preview.rows[4].is_likely_balance_row);
        assert!(!preview.rows[1].is_likely_balance_row);
    }

    /// The full LCL column layout, described by its owner as: 1 date,
    /// 2 amount, 3 transaction type, 4 reference id, 5 description,
    /// 6 transfer description, 7 unknown flag, 8 category.
    ///
    /// Only 5 and 6 are the description. Treating the description as
    /// "everything not otherwise claimed" pulled in the instrument, the
    /// reference, the flag and the category too, producing
    /// "Carte CB SOME STORE 30/06/26 0 Divers" where the merchant name is —
    /// and, because imported rows are auto-categorized by matching that text
    /// verbatim against history, making the match far less likely to land.
    #[test]
    fn a_headerless_export_picks_only_the_prose_columns_as_the_description() {
        let text = "\
01/05/2026;841,76;;00122 021115Z
01/05/2026;-35;Carte;;CB  PLACEMINUTE COM  30/06/26;;0;Divers
01/05/2026;2291,1;Virement;;;VIREMENT ENTREPRISE;;
01/05/2026;-0,07;;;INTERETS DEBITEURS AU 30 06 26;;;
01/05/2026;-4;Carte;;CB  FLOWERS CITY    30/06/26;;0;Divers
01/05/2026;-30,91;Carte;;CB  STEAMGAMES.COM   29/06/26;;0;Divers
02/05/2026;-19,99;Virement;;PRLV SEPA ORANGE MOBILE;;;
03/05/2026;-25,5;Carte;;CB  SERVICE NAVIGO   02/07/26;;0;Divers
05/06/2026;2890,27;;00122 021115Z
";
        let file = parse_file(text.as_bytes());
        let mapping = detect_mapping(&file);

        // Columns 5 and 6 as the user counts them; 4 and 5 zero-indexed.
        assert_eq!(mapping.description_columns, vec![4, 5]);
        // The instrument column is recognized as such rather than swept into
        // the description — which is the only reason dropping it from the
        // description doesn't cost the operation kind.
        assert_eq!(mapping.operation_kind_column, Some(2));

        let preview = apply_mapping(&file, &mapping);
        assert_eq!(preview.rows[1].description, "CB  PLACEMINUTE COM  30/06/26");
        assert_eq!(preview.rows[2].description, "VIREMENT ENTREPRISE");
        assert_eq!(
            preview.rows[3].description,
            "INTERETS DEBITEURS AU 30 06 26"
        );
        assert_eq!(preview.rows[1].operation_kind, OperationKind::Card);
        assert_eq!(preview.rows[2].operation_kind, OperationKind::BankTransfer);
        assert!(preview.rows.iter().all(|r| r.is_valid()));
    }

    #[test]
    fn build_preview_does_not_flag_a_short_row_in_the_middle_of_the_file() {
        let text = "\
01/07/2026;-35,00;Carte;;CB SOME STORE   30/06/26;;0;Divers
02/07/2026;10,00;;
03/07/2026;-19,99;Virement;;PRLV SEPA SOME BILL;;;
";
        let preview = preview(text);
        assert_eq!(preview.rows.len(), 3);
        assert!(!preview.rows[1].is_likely_balance_row);
    }

    #[test]
    fn build_preview_does_not_flag_boundary_rows_in_a_very_short_file() {
        let text = "\
01/07/2026;500,00;;ACC REF 12345
01/07/2026;-35,00;Carte;;CB SOME STORE   30/06/26;;0;Divers
";
        let preview = preview(text);
        assert_eq!(preview.rows.len(), 2);
        assert!(preview.rows.iter().all(|r| !r.is_likely_balance_row));
    }

    /// The app's own export format must round-trip: Currency and Account
    /// never belong in the description, and Subcategory is its own field.
    #[test]
    fn the_apps_own_export_format_round_trips() {
        let text = "\
Date;Amount;Currency;Description;Category;Subcategory;Account
2026-08-04;-12,25;EUR;HEMA GARERER CHA;Home;;LCL
2026-08-04;-20,97;EUR;LES SUPER HEROS;Education;Books;LCL
2026-08-04;-60,80;EUR;SC-SUSHI SASHI;Food & Drinks;;LCL
";
        let preview = preview(text);

        assert_eq!(preview.rows.len(), 3);
        assert_eq!(preview.rows[0].description, "HEMA GARERER CHA");
        assert_eq!(preview.rows[0].csv_category, Some("Home".to_string()));
        assert_eq!(preview.rows[0].csv_subcategory, None);
        assert_eq!(preview.rows[1].csv_subcategory, Some("Books".to_string()));
        assert_eq!(
            preview.rows[2].csv_category,
            Some("Food & Drinks".to_string())
        );
        assert!(preview.rows.iter().all(|r| r.is_valid()));
    }

    #[test]
    fn build_preview_has_no_csv_category_without_a_matching_header() {
        let text = "01/07/2026;10,00;STORE A\n02/07/2026;-5,00;STORE B\n";
        let preview = preview(text);
        assert!(preview.rows.iter().all(|r| r.csv_category.is_none()));
    }

    /// "Type de compte" is the account's type, not the operation's — reading
    /// it as one would label every row from a column that says nothing about
    /// how the money moved.
    #[test]
    fn an_account_type_column_is_not_mistaken_for_the_operation_type() {
        let text = "\
Date;Amount;Type de compte;Description
01/07/2026;-35,00;Compte courant;VIREMENT SOMEONE
02/07/2026;-12,00;Compte courant;SOME STORE
";
        let preview = preview(text);
        assert_eq!(preview.rows[0].operation_kind, OperationKind::BankTransfer);
        assert_eq!(preview.rows[1].operation_kind, OperationKind::Card);
    }

    #[test]
    fn a_bare_type_header_is_accepted() {
        let text = "\
Date;Amount;Type;Description
01/07/2026;-35,00;Virement;SOMEONE
02/07/2026;-12,00;Carte;SOME STORE
";
        let preview = preview(text);
        assert_eq!(preview.rows[0].operation_kind, OperationKind::BankTransfer);
        assert_eq!(preview.rows[1].operation_kind, OperationKind::Card);
        assert_eq!(preview.rows[0].description, "SOMEONE");
    }

    #[test]
    fn a_blank_type_cell_falls_back_to_the_description_for_that_row_only() {
        let text = "\
Date;Amount;Type opération;Description
01/07/2026;-35,00;Chèque;RENT
02/07/2026;120,50;;VIR SALARY
03/07/2026;-12,00;;SOME STORE
";
        let preview = preview(text);
        assert_eq!(preview.rows[0].operation_kind, OperationKind::Check);
        assert_eq!(preview.rows[1].operation_kind, OperationKind::BankTransfer);
        assert_eq!(preview.rows[2].operation_kind, OperationKind::Card);
    }

    #[test]
    fn build_preview_decodes_utf8_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"01/07/2026;10,00;STORE A\n");
        let preview = build_preview(&bytes);
        assert_eq!(preview.rows.len(), 1);
        assert_eq!(preview.rows[0].date, NaiveDate::from_ymd_opt(2026, 7, 1));
    }

    /// The correction path: a mapping the user edited is read exactly as
    /// given.
    ///
    /// Note what does *not* happen — re-pointing the amount at column 2
    /// leaves column 1 out of the description rather than sweeping it back
    /// in. The description is an explicit list of columns, so the only thing
    /// that changes it is changing it; a field quietly gaining a column
    /// because some other field let go of one is the "everything unused"
    /// behavior this replaced.
    #[test]
    fn a_corrected_mapping_is_applied_verbatim() {
        let text = "01/07/2026;10,00;-42,50;STORE A\n02/07/2026;-5,00;-7,25;STORE B\n";
        let file = parse_file(text.as_bytes());
        let detected = detect_mapping(&file);
        assert_eq!(detected.amount, Some(AmountSource::Single(1)));
        assert_eq!(detected.description_columns, vec![3]);

        let corrected = ColumnMapping {
            amount: Some(AmountSource::Single(2)),
            ..detected
        };
        let preview = apply_mapping(&file, &corrected);

        assert_eq!(preview.rows[0].amount_minor_units, Some(-4_250));
        assert_eq!(preview.rows[1].amount_minor_units, Some(-725));
        assert_eq!(preview.rows[0].description, "STORE A");

        let widened = ColumnMapping {
            description_columns: vec![1, 3],
            ..corrected
        };
        assert_eq!(
            apply_mapping(&file, &widened).rows[0].description,
            "10,00 STORE A"
        );
    }

    /// The genuinely undecidable case: every date in the column is valid
    /// both ways. Detection picks day-first; the user overriding it must
    /// actually change how the column reads.
    #[test]
    fn a_user_supplied_date_format_overrides_the_detected_one() {
        let text = "03/04/2026;-10,00;STORE A\n05/06/2026;-20,00;STORE B\n";
        let file = parse_file(text.as_bytes());
        let detected = detect_mapping(&file);
        assert_eq!(detected.date_format, "%d/%m/%Y");
        assert_eq!(
            apply_mapping(&file, &detected).rows[0].date,
            NaiveDate::from_ymd_opt(2026, 4, 3)
        );

        let corrected = ColumnMapping {
            date_format: "%m/%d/%Y".to_string(),
            ..detected
        };
        assert_eq!(
            apply_mapping(&file, &corrected).rows[0].date,
            NaiveDate::from_ymd_opt(2026, 3, 4)
        );
    }

    /// A remembered mapping is authoritative about the header, so replaying
    /// one must not re-run `detect_header` and disagree with itself.
    #[test]
    fn preview_with_mapping_honors_the_mappings_own_header_flag() {
        let text = "Date;Amount;Description\n01/07/2026;10,00;STORE A\n02/07/2026;-5,00;STORE B\n";
        let detected = detect_mapping(&parse_file(text.as_bytes()));
        assert!(detected.has_header);

        let headerless = ColumnMapping {
            has_header: false,
            ..detected.clone()
        };
        let preview = preview_with_mapping(text.as_bytes(), &headerless);
        assert_eq!(preview.rows.len(), 3, "the header row is now a data row");
        assert!(!preview.rows[0].is_valid());

        let preview = preview_with_mapping(text.as_bytes(), &detected);
        assert_eq!(preview.rows.len(), 2);
    }

    #[test]
    fn columns_are_summarized_with_their_header_and_sample_values() {
        let file = parse_file(CAISSE_SHAPE.as_bytes());
        let columns = summarize_columns(&file);
        assert_eq!(columns.len(), 13);
        assert_eq!(columns[8].header.as_deref(), Some("Debit"));
        assert_eq!(columns[8].samples, vec!["-9,99", "-2,55", "-3,40"]);
        assert_eq!(
            columns[0].header.as_deref(),
            Some("Date de comptabilisation")
        );
    }

    /// The point of the signature: the *same bank, different month* must
    /// come out identical, so last month's corrected mapping is found again.
    #[test]
    fn the_signature_survives_the_data_underneath_the_header_changing() {
        let january = parse_file(CAISSE_SHAPE.as_bytes());
        let february = parse_file(
            "\
Date de comptabilisation;Libelle simplifie;Libelle operation;Reference;Informations complementaires;Type operation;Categorie;Sous categorie;Debit;Credit;Date operation;Date de valeur;Pointage operation
14/02/2026;CINEMA;CB CINEMA;B0009;B0009-;Carte bancaire;Loisirs;Sorties;-11,00;;14/02/2026;16/02/2026;0
12/02/2026;PHARMACIE;CB PHARMACIE;B0010;B0010-;Carte bancaire;Sante;Pharmacie;-7,80;;12/02/2026;16/02/2026;0
10/02/2026;EMPLOYER;VIR SEPA EMPLOYER;B0011;Virement EMPLOYER;Virement recu;Rentree d'argent;Salaire;;+3100,00;10/02/2026;16/02/2026;0
"
            .as_bytes(),
        );
        assert_eq!(file_signature(&january), file_signature(&february));
    }

    /// …and a different bank must not, or one bank's mapping would be
    /// silently applied to another's file.
    #[test]
    fn a_different_header_produces_a_different_signature() {
        let caisse = parse_file(CAISSE_SHAPE.as_bytes());
        let other = parse_file(
            "Date;Amount;Description;Category\n01/07/2026;-10,00;STORE A;Food\n02/07/2026;-5,00;STORE B;Food\n"
                .as_bytes(),
        );
        assert_ne!(file_signature(&caisse), file_signature(&other));
    }

    /// Casing and stray whitespace in the header are formatting, not
    /// identity — a bank that capitalizes differently one month is the same
    /// bank.
    #[test]
    fn the_signature_ignores_header_casing_and_padding() {
        let a = parse_file(
            "Date;Amount;Description\n01/07/2026;-10,00;A\n02/07/2026;-5,00;B\n".as_bytes(),
        );
        let b = parse_file(
            "  DATE ;amount;  Description\n01/07/2026;-10,00;A\n02/07/2026;-5,00;B\n".as_bytes(),
        );
        assert_eq!(file_signature(&a), file_signature(&b));
    }

    /// A headerless file has no name to go on, so it's identified by column
    /// shape. Two files of the same layout must still match.
    #[test]
    fn a_headerless_file_is_identified_by_its_column_shape() {
        let a = parse_file("01/07/2026;10,00;STORE A\n02/07/2026;-5,00;STORE B\n".as_bytes());
        let b = parse_file("14/09/2026;-3,20;BAKERY\n15/09/2026;-8,00;CINEMA\n".as_bytes());
        assert_eq!(file_signature(&a), file_signature(&b));
        assert!(file_signature(&a).starts_with("s1:;:"));
    }

    /// An empty or unparseable file must report no confidence rather than
    /// dividing by zero.
    #[test]
    fn an_empty_file_yields_an_empty_preview() {
        let preview = build_preview(b"");
        assert!(preview.rows.is_empty());
        assert_eq!(preview.date_confidence, 0.0);
        assert_eq!(preview.amount_confidence, 0.0);
    }
}
