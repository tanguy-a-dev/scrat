use chrono::NaiveDate;
use thiserror::Error;

use crate::account::{Account, AccountId};
use crate::category::{Category, CategoryId};
use crate::transaction::{Transaction, TransactionId};

#[derive(Debug, Error)]
#[error("{0}")]
pub struct RepositoryError(pub String);

/// Port for persisting accounts. Implemented by an infra-* adapter (SQLite
/// in production, an in-memory fake in application-layer tests) — the
/// application layer depends only on this trait, never on a concrete
/// storage technology.
pub trait AccountRepository {
    fn insert(&self, account: &Account) -> Result<(), RepositoryError>;
    fn update(&self, account: &Account) -> Result<(), RepositoryError>;
    fn delete(&self, id: AccountId) -> Result<(), RepositoryError>;
    fn find_by_id(&self, id: AccountId) -> Result<Option<Account>, RepositoryError>;
    fn list_all(&self) -> Result<Vec<Account>, RepositoryError>;
    /// Count of transactions referencing this account — used to decide
    /// whether a delete must fall back to archiving instead.
    fn transaction_count(&self, id: AccountId) -> Result<u64, RepositoryError>;
    /// Sum of all transaction amounts (minor units) posted to this account.
    /// Zero when there are none.
    fn sum_transactions_minor_units(&self, id: AccountId) -> Result<i64, RepositoryError>;
}

/// Port for persisting categories (a self-referential tree via `parent_id`).
pub trait CategoryRepository {
    fn insert(&self, category: &Category) -> Result<(), RepositoryError>;
    fn update(&self, category: &Category) -> Result<(), RepositoryError>;
    fn delete(&self, id: CategoryId) -> Result<(), RepositoryError>;
    fn find_by_id(&self, id: CategoryId) -> Result<Option<Category>, RepositoryError>;
    fn list_all(&self) -> Result<Vec<Category>, RepositoryError>;
    /// Re-parents every category whose `parent_id` is `from` to `to`
    /// (`None` promotes them to root level). A no-op when there are none.
    fn reassign_children(
        &self,
        from: CategoryId,
        to: Option<CategoryId>,
    ) -> Result<(), RepositoryError>;
    /// Moves every transaction referencing `from` to `to`. `to` must be a
    /// real category — a transaction's category can never be null.
    fn reassign_transactions(
        &self,
        from: CategoryId,
        to: CategoryId,
    ) -> Result<(), RepositoryError>;
    /// Count of transactions referencing this category — used to decide
    /// whether a delete requires an explicit reassignment target.
    fn transaction_count(&self, id: CategoryId) -> Result<u64, RepositoryError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted,
    DuplicateSkipped,
}

/// Port for persisting transactions (the ledger).
pub trait TransactionRepository {
    /// Inserts the transaction. A repeat of the same (account, date, amount,
    /// normalized source) is rejected by the `dedup_key` unique constraint —
    /// this is a hard error, appropriate for interactive single-entry where
    /// a duplicate likely means a mistake. Bulk CSV import should use
    /// [`insert_or_skip`](Self::insert_or_skip) instead.
    fn insert(&self, transaction: &Transaction) -> Result<(), RepositoryError>;
    /// Like `insert`, but a duplicate `dedup_key` is reported as
    /// `DuplicateSkipped` rather than an error — makes re-importing the same
    /// CSV (or an overlapping date range) idempotent.
    fn insert_or_skip(&self, transaction: &Transaction) -> Result<InsertOutcome, RepositoryError>;
    fn delete(&self, id: TransactionId) -> Result<(), RepositoryError>;
    /// Recategorizes an existing transaction — used when the user corrects
    /// or updates a transaction's category after the fact.
    fn update_category(
        &self,
        id: TransactionId,
        category_id: CategoryId,
    ) -> Result<(), RepositoryError>;
    fn list_in_range(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<Transaction>, RepositoryError>;
    /// Every transaction, unfiltered. Not `list_in_range(NaiveDate::MIN,
    /// NaiveDate::MAX)` — those extremes format with a leading sign
    /// (`+262142-12-31`) that sorts before ordinary years in a TEXT
    /// comparison, so that combination silently matches nothing.
    fn list_all(&self) -> Result<Vec<Transaction>, RepositoryError>;
}
