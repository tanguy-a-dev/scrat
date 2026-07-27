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

fn describe(err: DbError) -> String {
    match err {
        DbError::InvalidPassphrase => "incorrect passphrase".to_string(),
        DbError::AlreadyExists(_) => "a database already exists".to_string(),
        DbError::Sqlite(e) => format!("database error: {e}"),
        DbError::Io(e) => format!("filesystem error: {e}"),
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
