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

/// A stable fingerprint of (account, date, amount, normalized source), used
/// to make re-importing the same CSV rows a no-op (`INSERT ... ON CONFLICT
/// DO NOTHING` in the infra layer keys off this).
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    id: TransactionId,
    date: NaiveDate,
    amount: Money,
    source: SourceText,
    category_id: CategoryId,
    account_id: AccountId,
    dedup_key: DedupKey,
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
        if amount.minor_units() == 0 {
            return Err(TransactionError::ZeroAmount);
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
