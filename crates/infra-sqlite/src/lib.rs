//! Scrat SQLCipher-backed persistence adapter: implements the repository
//! ports defined in `scrat-domain`.

mod account_repository;
mod category_repository;
mod connection;
mod migrations;
mod seed;
mod settings_repository;
mod transaction_repository;

pub use account_repository::SqliteAccountRepository;
pub use category_repository::SqliteCategoryRepository;
pub use connection::{create_new, database_exists, unlock_existing, DbError};
pub use rusqlite::Connection;
pub use settings_repository::{
    get_currency_code, get_default_account_id, get_default_category_id, set_currency_code,
    set_default_account_id, set_default_category_id,
};
pub use transaction_repository::SqliteTransactionRepository;
