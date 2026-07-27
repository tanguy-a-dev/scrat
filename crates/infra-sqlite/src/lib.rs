//! Scrat SQLCipher-backed persistence adapter: implements the repository
//! ports defined in `scrat-domain`.

mod connection;
mod migrations;

pub use connection::{create_new, database_exists, unlock_existing, DbError};
pub use rusqlite::Connection;
