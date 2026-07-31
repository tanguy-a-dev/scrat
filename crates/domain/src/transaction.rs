use chrono::NaiveDate;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::account::AccountId;
use crate::category::CategoryId;
use crate::money::Money;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TransactionError {
    #[error("transaction amount cannot be zero")]
    ZeroAmount,
    #[error("source cannot be empty")]
    EmptySource,
    #[error("source cannot be longer than {0} characters")]
    SourceTooLong(usize),
    #[error("invalid id: {0}")]
    InvalidId(String),
    #[error("a transfer must belong to a transfer group")]
    TransferWithoutGroup,
    #[error("only a transfer can belong to a transfer group")]
    GroupWithoutTransferRole,
    #[error("unknown transaction role: {0}")]
    UnknownRole(String),
}

const MAX_SOURCE_LEN: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionId(Uuid);

impl TransactionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(raw: &str) -> Result<Self, TransactionError> {
        Uuid::parse_str(raw)
            .map(Self)
            .map_err(|_| TransactionError::InvalidId(raw.to_string()))
    }

    pub fn as_string(&self) -> String {
        self.0.to_string()
    }
}

impl Default for TransactionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceText(String);

impl SourceText {
    pub fn new(raw: &str) -> Result<Self, TransactionError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(TransactionError::EmptySource);
        }
        if trimmed.chars().count() > MAX_SOURCE_LEN {
            return Err(TransactionError::SourceTooLong(MAX_SOURCE_LEN));
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A stable fingerprint of (account, date, amount, normalized source). Not
/// enforced unique — identical transactions are allowed — kept only as a
/// candidate key for a future "find likely duplicates" review feature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupKey(String);

impl DedupKey {
    pub fn compute(
        account_id: AccountId,
        date: NaiveDate,
        amount_minor_units: i64,
        source: &str,
    ) -> Self {
        let normalized_source = source
            .trim()
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        let mut hasher = Sha256::new();
        hasher.update(account_id.as_string());
        hasher.update("|");
        hasher.update(date.format("%Y-%m-%d").to_string());
        hasher.update("|");
        hasher.update(amount_minor_units.to_string());
        hasher.update("|");
        hasher.update(normalized_source);

        Self(format!("{:x}", hasher.finalize()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionKind {
    Expense,
    Income,
}

/// Identifies the two legs of a single transfer — the outflow on the source
/// account and the mirrored inflow on the counterpart. Both legs carry the
/// same value, which is what lets deleting either one take the other with
/// it instead of leaving the counterpart account silently overstated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransferGroupId(Uuid);

impl TransferGroupId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(raw: &str) -> Result<Self, TransactionError> {
        Uuid::parse_str(raw)
            .map(Self)
            .map_err(|_| TransactionError::InvalidId(raw.to_string()))
    }

    pub fn as_string(&self) -> String {
        self.0.to_string()
    }
}

impl Default for TransferGroupId {
    fn default() -> Self {
        Self::new()
    }
}

/// What a transaction *means*, as opposed to which way its amount points.
///
/// Only [`TransactionRole::Normal`] rows are real income or spending. The
/// other two move or correct money that was already yours, so counting them
/// would inflate both sides of every report: a transfer would show up as an
/// expense on the source account and income on the destination, and a
/// reconciliation delta would read as earnings the user never received.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionRole {
    /// Ordinary income or spending — the only role that belongs in totals.
    Normal,
    /// One leg of a movement between two of the user's own accounts.
    Transfer,
    /// A correction bringing an account whose statements can't be imported
    /// back in line with the balance the user actually observes.
    Adjustment,
}

impl TransactionRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Transfer => "transfer",
            Self::Adjustment => "adjustment",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, TransactionError> {
        match raw {
            "normal" => Ok(Self::Normal),
            "transfer" => Ok(Self::Transfer),
            "adjustment" => Ok(Self::Adjustment),
            other => Err(TransactionError::UnknownRole(other.to_string())),
        }
    }

    /// Whether this role should be summed into income and expense reporting.
    /// Note that it is *not* excluded from account balances — the money
    /// really did move, so every role counts there.
    pub fn counts_toward_income_and_expenses(&self) -> bool {
        matches!(self, Self::Normal)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    id: TransactionId,
    date: NaiveDate,
    amount: Money,
    source: SourceText,
    category_id: CategoryId,
    account_id: AccountId,
    dedup_key: DedupKey,
    role: TransactionRole,
    transfer_group_id: Option<TransferGroupId>,
}

impl Transaction {
    pub fn new(
        id: TransactionId,
        date: NaiveDate,
        amount: Money,
        source: SourceText,
        category_id: CategoryId,
        account_id: AccountId,
    ) -> Result<Self, TransactionError> {
        Self::new_with_role(
            id,
            date,
            amount,
            source,
            category_id,
            account_id,
            TransactionRole::Normal,
            None,
        )
    }

    /// Full constructor. Prefer [`Transaction::new`] for ordinary income and
    /// spending; this exists for transfer legs, reconciliation adjustments,
    /// and rehydrating a stored row.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_role(
        id: TransactionId,
        date: NaiveDate,
        amount: Money,
        source: SourceText,
        category_id: CategoryId,
        account_id: AccountId,
        role: TransactionRole,
        transfer_group_id: Option<TransferGroupId>,
    ) -> Result<Self, TransactionError> {
        if amount.minor_units() == 0 {
            return Err(TransactionError::ZeroAmount);
        }
        match (role, transfer_group_id) {
            (TransactionRole::Transfer, None) => {
                return Err(TransactionError::TransferWithoutGroup)
            }
            (TransactionRole::Normal | TransactionRole::Adjustment, Some(_)) => {
                return Err(TransactionError::GroupWithoutTransferRole)
            }
            _ => {}
        }
        let dedup_key = DedupKey::compute(account_id, date, amount.minor_units(), source.as_str());
        Ok(Self {
            id,
            date,
            amount,
            source,
            category_id,
            account_id,
            dedup_key,
            role,
            transfer_group_id,
        })
    }

    pub fn id(&self) -> TransactionId {
        self.id
    }

    pub fn date(&self) -> NaiveDate {
        self.date
    }

    pub fn amount(&self) -> &Money {
        &self.amount
    }

    pub fn source(&self) -> &SourceText {
        &self.source
    }

    pub fn category_id(&self) -> CategoryId {
        self.category_id
    }

    pub fn account_id(&self) -> AccountId {
        self.account_id
    }

    pub fn dedup_key(&self) -> &DedupKey {
        &self.dedup_key
    }

    pub fn role(&self) -> TransactionRole {
        self.role
    }

    pub fn transfer_group_id(&self) -> Option<TransferGroupId> {
        self.transfer_group_id
    }

    /// Builds this transaction's counterpart leg: the same movement seen
    /// from the other account, so the amount flips sign while the date and
    /// source text stay put. Using the source account's date rather than
    /// guessing at a settlement lag keeps the pair internally consistent,
    /// at the cost of the counterpart's balance being up to a day early.
    pub fn mirrored_onto(
        &self,
        account_id: AccountId,
        group_id: TransferGroupId,
    ) -> Result<Self, TransactionError> {
        Self::new_with_role(
            TransactionId::new(),
            self.date,
            self.amount.negated(),
            self.source.clone(),
            self.category_id,
            account_id,
            TransactionRole::Transfer,
            Some(group_id),
        )
    }

    pub fn kind(&self) -> TransactionKind {
        if self.amount.is_negative() {
            TransactionKind::Expense
        } else {
            TransactionKind::Income
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;

    fn money(minor_units: i64) -> Money {
        Money::from_minor_units(minor_units, Currency::new("USD").unwrap())
    }

    fn make_transaction(amount_minor_units: i64) -> Result<Transaction, TransactionError> {
        Transaction::new(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            money(amount_minor_units),
            SourceText::new("Whole Foods").unwrap(),
            CategoryId::new(),
            AccountId::new(),
        )
    }

    #[test]
    fn transaction_kind_is_expense_when_amount_negative() {
        let transaction = make_transaction(-500).unwrap();
        assert_eq!(transaction.kind(), TransactionKind::Expense);
    }

    #[test]
    fn transaction_kind_is_income_when_amount_positive() {
        let transaction = make_transaction(500).unwrap();
        assert_eq!(transaction.kind(), TransactionKind::Income);
    }

    #[test]
    fn transaction_rejects_zero_amount() {
        let result = make_transaction(0);
        assert_eq!(result, Err(TransactionError::ZeroAmount));
    }

    #[test]
    fn new_defaults_to_a_normal_transaction_with_no_transfer_group() {
        let transaction = make_transaction(-500).unwrap();
        assert_eq!(transaction.role(), TransactionRole::Normal);
        assert_eq!(transaction.transfer_group_id(), None);
    }

    #[test]
    fn only_normal_transactions_count_toward_income_and_expenses() {
        assert!(TransactionRole::Normal.counts_toward_income_and_expenses());
        assert!(!TransactionRole::Transfer.counts_toward_income_and_expenses());
        assert!(!TransactionRole::Adjustment.counts_toward_income_and_expenses());
    }

    #[test]
    fn role_round_trips_through_its_stored_string() {
        for role in [
            TransactionRole::Normal,
            TransactionRole::Transfer,
            TransactionRole::Adjustment,
        ] {
            assert_eq!(TransactionRole::parse(role.as_str()), Ok(role));
        }
    }

    #[test]
    fn role_parse_rejects_unknown_text() {
        assert_eq!(
            TransactionRole::parse("teleport"),
            Err(TransactionError::UnknownRole("teleport".to_string()))
        );
    }

    /// A transfer leg with no group id is a leg with no counterpart — the
    /// exact shape that would overstate the other account's balance forever.
    #[test]
    fn transfer_without_a_group_is_rejected() {
        let result = Transaction::new_with_role(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            money(-500),
            SourceText::new("Virement N26").unwrap(),
            CategoryId::new(),
            AccountId::new(),
            TransactionRole::Transfer,
            None,
        );
        assert_eq!(result, Err(TransactionError::TransferWithoutGroup));
    }

    #[test]
    fn non_transfer_carrying_a_group_is_rejected() {
        let result = Transaction::new_with_role(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            money(-500),
            SourceText::new("Whole Foods").unwrap(),
            CategoryId::new(),
            AccountId::new(),
            TransactionRole::Normal,
            Some(TransferGroupId::new()),
        );
        assert_eq!(result, Err(TransactionError::GroupWithoutTransferRole));
    }

    #[test]
    fn mirrored_leg_flips_the_amount_and_keeps_date_source_and_group() {
        let group_id = TransferGroupId::new();
        let counterpart_account = AccountId::new();
        let outflow = Transaction::new_with_role(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            money(-25_000),
            SourceText::new("Virement N26").unwrap(),
            CategoryId::new(),
            AccountId::new(),
            TransactionRole::Transfer,
            Some(group_id),
        )
        .unwrap();

        let inflow = outflow
            .mirrored_onto(counterpart_account, group_id)
            .unwrap();

        assert_eq!(inflow.amount().minor_units(), 25_000);
        assert_eq!(inflow.date(), outflow.date());
        assert_eq!(inflow.source(), outflow.source());
        assert_eq!(inflow.account_id(), counterpart_account);
        assert_eq!(inflow.transfer_group_id(), Some(group_id));
        assert_eq!(inflow.role(), TransactionRole::Transfer);
        assert_ne!(inflow.id(), outflow.id());
    }

    /// The two legs sum to zero, which is the whole point: a transfer moves
    /// money without creating or destroying any.
    #[test]
    fn a_transfer_pair_nets_to_zero() {
        let group_id = TransferGroupId::new();
        let outflow = Transaction::new_with_role(
            TransactionId::new(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            money(-25_000),
            SourceText::new("Virement N26").unwrap(),
            CategoryId::new(),
            AccountId::new(),
            TransactionRole::Transfer,
            Some(group_id),
        )
        .unwrap();
        let inflow = outflow.mirrored_onto(AccountId::new(), group_id).unwrap();

        assert_eq!(
            outflow.amount().minor_units() + inflow.amount().minor_units(),
            0
        );
    }

    #[test]
    fn dedup_key_is_stable_for_same_inputs() {
        let account_id = AccountId::new();
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();

        let a = DedupKey::compute(account_id, date, -500, "Whole Foods");
        let b = DedupKey::compute(account_id, date, -500, "Whole Foods");

        assert_eq!(a, b);
    }

    #[test]
    fn dedup_key_normalizes_source_case_and_whitespace() {
        let account_id = AccountId::new();
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();

        let a = DedupKey::compute(account_id, date, -500, "Whole   Foods");
        let b = DedupKey::compute(account_id, date, -500, "  whole foods  ");

        assert_eq!(a, b);
    }

    #[test]
    fn dedup_key_differs_for_different_source_text() {
        let account_id = AccountId::new();
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();

        let a = DedupKey::compute(account_id, date, -500, "Whole Foods");
        let b = DedupKey::compute(account_id, date, -500, "Trader Joe's");

        assert_ne!(a, b);
    }
}
