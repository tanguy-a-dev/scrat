//! Scrat SQLCipher-backed persistence adapter: implements the repository
//! ports defined in `scrat-domain`.

mod account_repository;
mod category_repository;
mod connection;
mod migrations;
mod transaction_repository;

pub use account_repository::SqliteAccountRepository;
pub use category_repository::SqliteCategoryRepository;
pub use connection::{create_new, database_exists, unlock_existing, DbError};
pub use rusqlite::Connection;
pub use transaction_repository::SqliteTransactionRepository;
