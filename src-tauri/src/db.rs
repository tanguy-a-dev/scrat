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
