use std::path::PathBuf;
use std::sync::Mutex;

use scrat_infra_sqlite::{Connection, DbError};
use tauri::{AppHandle, Manager, State};

pub struct DbState(pub Mutex<Option<Connection>>);

impl Default for DbState {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

pub(crate) fn db_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("could not resolve app data directory: {e}"))?;
    Ok(dir.join("scrat.db"))
}

pub(crate) fn describe(err: DbError) -> String {
    match err {
        DbError::InvalidPassphrase => "incorrect passphrase".to_string(),
        DbError::EmptyPassphrase => "passphrase cannot be empty".to_string(),
        DbError::AlreadyExists(_) => "a database already exists".to_string(),
        DbError::Sqlite(e) => format!("database error: {e}"),
        DbError::Io(e) => format!("filesystem error: {e}"),
        DbError::Repository(e) => format!("database error: {e}"),
    }
}

/// The shortest passphrase that may key the database.
///
/// Enforced here rather than only in the form that collects it: this is the
/// single value protecting every byte the app stores, and a UI check is only
/// as good as the caller that happens to be running. Both entry points that
/// set a passphrase — [`create_db_with_passphrase`] and
/// [`change_passphrase`] — go through [`check_passphrase_length`].
const MIN_PASSPHRASE_LENGTH: usize = 8;

fn check_passphrase_length(passphrase: &str) -> Result<(), String> {
    if passphrase.chars().count() < MIN_PASSPHRASE_LENGTH {
        return Err(format!(
            "passphrase must be at least {MIN_PASSPHRASE_LENGTH} characters"
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn is_db_initialized(app: AppHandle) -> Result<bool, String> {
    Ok(scrat_infra_sqlite::database_exists(&db_path(&app)?))
}

#[tauri::command]
pub fn create_db_with_passphrase(
    app: AppHandle,
    state: State<DbState>,
    passphrase: String,
) -> Result<(), String> {
    check_passphrase_length(&passphrase)?;
    let conn = scrat_infra_sqlite::create_new(&db_path(&app)?, &passphrase).map_err(describe)?;
    *state.0.lock().unwrap() = Some(conn);
    Ok(())
}

#[tauri::command]
pub fn unlock_db(app: AppHandle, state: State<DbState>, passphrase: String) -> Result<(), String> {
    let conn =
        scrat_infra_sqlite::unlock_existing(&db_path(&app)?, &passphrase).map_err(describe)?;
    *state.0.lock().unwrap() = Some(conn);
    Ok(())
}

/// Changes the passphrase protecting the database, re-encrypting it in
/// place. `current_passphrase` must be correct — checked by independently
/// unlocking the file with it (mirroring [`unlock_db`]'s canary check)
/// *before* the live connection is touched — so a wrong guess never risks
/// the already-open database. The frontend is expected to have already
/// collected the new passphrase twice and confirmed both entries match.
#[tauri::command]
pub fn change_passphrase(
    app: AppHandle,
    state: State<DbState>,
    current_passphrase: String,
    new_passphrase: String,
) -> Result<(), String> {
    check_passphrase_length(&new_passphrase)?;
    drop(
        scrat_infra_sqlite::unlock_existing(&db_path(&app)?, &current_passphrase)
            .map_err(describe)?,
    );

    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    scrat_infra_sqlite::rekey(conn, &new_passphrase).map_err(describe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passphrase_shorter_than_the_minimum_is_rejected() {
        assert!(check_passphrase_length("").is_err());
        assert!(check_passphrase_length("short").is_err());
        assert!(check_passphrase_length("1234567").is_err());
    }

    #[test]
    fn passphrase_at_or_above_the_minimum_is_accepted() {
        assert!(check_passphrase_length("12345678").is_ok());
        assert!(check_passphrase_length("a much longer passphrase").is_ok());
    }

    /// Counted in characters, not bytes — a passphrase of eight non-ASCII
    /// characters is eight characters long, however many bytes UTF-8 spends
    /// on it. Measuring `len()` would silently accept a shorter one.
    #[test]
    fn passphrase_length_counts_characters_not_bytes() {
        assert!(check_passphrase_length("éàüñ").is_err());
        assert!(check_passphrase_length("éàüñéàüñ").is_ok());
    }
}
