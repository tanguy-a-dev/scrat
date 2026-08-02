use std::path::Path;

use rusqlite::Connection;
use scrat_domain::ports::RepositoryError;
use thiserror::Error;

use crate::migrations;
use crate::seed;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("a database already exists at {0}")]
    AlreadyExists(String),
    #[error("passphrase cannot be empty")]
    EmptyPassphrase,
    #[error("incorrect passphrase")]
    InvalidPassphrase,
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

pub fn database_exists(path: &Path) -> bool {
    path.exists()
}

/// Creates a brand-new encrypted database at `path` and keys it with
/// `passphrase`. Fails if a file already exists there — use
/// [`unlock_existing`] to open one.
pub fn create_new(path: &Path, passphrase: &str) -> Result<Connection, DbError> {
    if path.exists() {
        return Err(DbError::AlreadyExists(path.display().to_string()));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut conn = Connection::open(path)?;
    key_connection(&conn, passphrase)?;
    migrations::run(&mut conn)?;
    seed::seed_default_categories(&conn)?;
    Ok(conn)
}

/// Opens an existing encrypted database at `path` and keys it with
/// `passphrase`. Returns [`DbError::InvalidPassphrase`] if the passphrase is
/// wrong — SQLCipher only reveals this once we try to actually read the
/// (still-encrypted) file, hence the canary query below.
pub fn unlock_existing(path: &Path, passphrase: &str) -> Result<Connection, DbError> {
    let mut conn = Connection::open(path)?;
    key_connection(&conn, passphrase)?;
    verify_canary(&conn)?;
    migrations::run(&mut conn)?;
    Ok(conn)
}

// SQLCipher's `PRAGMA key` does not support bind parameters — the passphrase
// must be embedded as a quoted SQL string literal, so we escape single
// quotes ourselves to keep it a valid literal.
fn key_connection(conn: &Connection, passphrase: &str) -> Result<(), DbError> {
    if passphrase.trim().is_empty() {
        return Err(DbError::EmptyPassphrase);
    }
    let escaped = passphrase.replace('\'', "''");
    conn.execute_batch(&format!("PRAGMA key = '{escaped}';"))?;
    Ok(())
}

fn verify_canary(conn: &Connection) -> Result<(), DbError> {
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| {
        row.get::<_, i64>(0)
    })
    .map(|_| ())
    .map_err(|_| DbError::InvalidPassphrase)
}

/// Re-encrypts the database backing `conn` under `new_passphrase`, in place.
///
/// `PRAGMA rekey` does not itself check the connection's *current* key — it
/// just re-encrypts with whatever new key it's given — so the caller is
/// expected to have already confirmed the user's claimed current passphrase
/// is correct (e.g. via [`unlock_existing`] against the same file) before
/// calling this. The connection stays open and usable afterwards; there is
/// no need to reopen it.
pub fn rekey(conn: &Connection, new_passphrase: &str) -> Result<(), DbError> {
    if new_passphrase.trim().is_empty() {
        return Err(DbError::EmptyPassphrase);
    }
    let escaped = new_passphrase.replace('\'', "''");
    conn.execute_batch(&format!("PRAGMA rekey = '{escaped}';"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
        dir.path().join("scrat.db")
    }

    #[test]
    fn create_new_creates_file_and_applies_migrations() {
        let dir = tempfile::tempdir().unwrap();
        let path = db_path(&dir);

        let conn = create_new(&path, "correct horse").unwrap();

        assert!(path.exists());
        let table_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'transactions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 1);

        let applied_version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(applied_version, crate::migrations::latest_version());
    }

    #[test]
    fn create_new_rejects_empty_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let path = db_path(&dir);

        let result = create_new(&path, "");

        assert!(matches!(result, Err(DbError::EmptyPassphrase)));
    }

    #[test]
    fn unlock_existing_rejects_empty_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let path = db_path(&dir);
        create_new(&path, "correct horse").unwrap();

        let result = unlock_existing(&path, "");

        assert!(matches!(result, Err(DbError::EmptyPassphrase)));
    }

    #[test]
    fn create_new_fails_if_file_already_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = db_path(&dir);
        create_new(&path, "pw").unwrap();

        let result = create_new(&path, "pw");

        assert!(matches!(result, Err(DbError::AlreadyExists(_))));
    }

    #[test]
    fn unlock_existing_with_correct_passphrase_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let path = db_path(&dir);
        create_new(&path, "correct horse").unwrap();

        let result = unlock_existing(&path, "correct horse");

        assert!(result.is_ok());
    }

    #[test]
    fn opening_db_with_wrong_passphrase_fails_canary_query() {
        let dir = tempfile::tempdir().unwrap();
        let path = db_path(&dir);
        create_new(&path, "correct horse").unwrap();

        let result = unlock_existing(&path, "wrong passphrase");

        assert!(matches!(result, Err(DbError::InvalidPassphrase)));
    }

    #[test]
    fn unlock_existing_reapplies_migrations_idempotently() {
        let dir = tempfile::tempdir().unwrap();
        let path = db_path(&dir);
        create_new(&path, "pw").unwrap();
        drop(unlock_existing(&path, "pw").unwrap());

        let result = unlock_existing(&path, "pw");

        assert!(result.is_ok());
    }

    #[test]
    fn rekey_changes_the_passphrase_needed_to_unlock() {
        let dir = tempfile::tempdir().unwrap();
        let path = db_path(&dir);
        let conn = create_new(&path, "old passphrase").unwrap();

        rekey(&conn, "new passphrase").unwrap();
        drop(conn);

        assert!(matches!(
            unlock_existing(&path, "old passphrase"),
            Err(DbError::InvalidPassphrase)
        ));
        assert!(unlock_existing(&path, "new passphrase").is_ok());
    }

    #[test]
    fn rekey_leaves_the_connection_usable_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let path = db_path(&dir);
        let conn = create_new(&path, "old passphrase").unwrap();

        rekey(&conn, "new passphrase").unwrap();

        // The live connection should keep working under its new key without
        // needing to be reopened.
        let table_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'transactions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 1);
    }

    #[test]
    fn rekey_rejects_empty_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let path = db_path(&dir);
        let conn = create_new(&path, "old passphrase").unwrap();

        let result = rekey(&conn, "");

        assert!(matches!(result, Err(DbError::EmptyPassphrase)));
    }
}
