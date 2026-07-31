use chrono::NaiveDate;
use thiserror::Error;

use crate::account::{Account, AccountId};
use crate::category::{Category, CategoryId};
use crate::transaction::{Transaction, TransactionId, TransferGroupId};
use crate::transfer_rule::{TransferRule, TransferRuleId};

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

/// Port for persisting transactions (the ledger).
pub trait TransactionRepository {
    /// Inserts the transaction. Two transactions with the same (account,
    /// date, amount, source) are both allowed — that's a legitimate
    /// coincidence (e.g. two identical coffees the same day), not a
    /// duplicate to reject. Re-importing the same CSV twice is the caller's
    /// responsibility to avoid; this port does no deduplication.
    fn insert(&self, transaction: &Transaction) -> Result<(), RepositoryError>;
    fn delete(&self, id: TransactionId) -> Result<(), RepositoryError>;
    fn find_by_id(&self, id: TransactionId) -> Result<Option<Transaction>, RepositoryError>;
    /// Deletes both legs of a transfer at once. Deleting only the leg the
    /// user is looking at would leave the counterpart account permanently
    /// overstated by the transfer amount, with nothing on screen to explain
    /// the gap — so removal is all-or-nothing, same as creation.
    fn delete_transfer_group(&self, group_id: TransferGroupId) -> Result<(), RepositoryError>;
    /// Recategorizes an existing transaction — used when the user corrects
    /// or updates a transaction's category after the fact.
    fn update_category(
        &self,
        id: TransactionId,
        category_id: CategoryId,
    ) -> Result<(), RepositoryError>;
    /// Deletes every listed transaction in one statement. Ids that don't
    /// exist are ignored rather than an error — the caller (bulk delete)
    /// expands transfer legs to their whole group and de-duplicates, so the
    /// same row can legitimately be named twice. Implementations must apply
    /// this atomically: a partial delete would leave the caller unable to
    /// tell the user what actually happened.
    fn delete_many(&self, ids: &[TransactionId]) -> Result<(), RepositoryError>;
    /// Recategorizes every listed transaction in one statement, atomically.
    fn update_category_many(
        &self,
        ids: &[TransactionId],
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
    /// Newest-first, `limit` transactions starting at `offset` — lets a
    /// caller walk the whole ledger in fixed-size batches instead of paying
    /// for `list_all` up front. Batch size is a count, not a calendar
    /// window, deliberately: a year of history can be one row or one
    /// hundred thousand depending on the user, so only a row count keeps
    /// each batch cheap.
    fn list_page(&self, offset: i64, limit: i64) -> Result<Vec<Transaction>, RepositoryError>;
    /// Counts transactions in `[start, end]`, optionally narrowed to a
    /// category and/or a case-insensitive source substring — the same two
    /// filters the transactions view applies. Pushed down to the query
    /// (rather than counting a `list_in_range` result in the caller) so a
    /// paginated view can show an accurate total for the current filters
    /// without first pulling every matching row across the wire.
    fn count_in_range(
        &self,
        start: NaiveDate,
        end: NaiveDate,
        category_id: Option<CategoryId>,
        source_contains: Option<&str>,
    ) -> Result<i64, RepositoryError>;
}

/// Port for persisting the rules that recognize an imported row as a
/// transfer to another of the user's own accounts.
pub trait TransferRuleRepository {
    fn insert(&self, rule: &TransferRule) -> Result<(), RepositoryError>;
    fn delete(&self, id: TransferRuleId) -> Result<(), RepositoryError>;
    fn list_all(&self) -> Result<Vec<TransferRule>, RepositoryError>;
}
