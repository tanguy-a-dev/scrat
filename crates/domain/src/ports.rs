use thiserror::Error;

use crate::account::{Account, AccountId};

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
