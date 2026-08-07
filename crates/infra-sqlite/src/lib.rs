//! Scrat SQLCipher-backed persistence adapter: implements the repository
//! ports defined in `scrat-domain`.

mod account_repository;
mod category_repository;
mod connection;
mod csv_mapping_repository;
mod migrations;
mod seed;
mod settings_repository;
mod transaction_repository;
mod transfer_rule_repository;

pub use account_repository::SqliteAccountRepository;
pub use category_repository::SqliteCategoryRepository;
pub use connection::{DbError, create_new, database_exists, rekey, unlock_existing};
pub use csv_mapping_repository::{get_csv_mapping, save_csv_mapping};
pub use rusqlite::Connection;
pub use settings_repository::{
    get_auto_lock_minutes, get_currency_code, get_default_account_id, get_rent_category_id,
    set_auto_lock_minutes, set_currency_code, set_default_account_id, set_rent_category_id,
};
pub use transaction_repository::SqliteTransactionRepository;
pub use transfer_rule_repository::SqliteTransferRuleRepository;
