use thiserror::Error;

use crate::account::{Account, AccountId};
use crate::category::{Category, CategoryId};

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
    fn reassign_children(&self, from: CategoryId, to: Option<CategoryId>) -> Result<(), RepositoryError>;
    /// Moves every transaction referencing `from` to `to`. `to` must be a
    /// real category — a transaction's category can never be null.
    fn reassign_transactions(&self, from: CategoryId, to: CategoryId) -> Result<(), RepositoryError>;
    /// Count of transactions referencing this category — used to decide
    /// whether a delete requires an explicit reassignment target.
    fn transaction_count(&self, id: CategoryId) -> Result<u64, RepositoryError>;
}
