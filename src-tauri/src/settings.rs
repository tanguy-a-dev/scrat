use std::path::PathBuf;

use scrat_application::category_service::CategoryService;
use scrat_domain::language::Language;
use scrat_domain::money::Currency;
use scrat_infra_sqlite::{Connection, SqliteCategoryRepository};
use tauri::{AppHandle, State};

use crate::accounts::app_currency;
use crate::db::{DbState, db_path};
use crate::errors::{AppError, codes};

#[tauri::command]
pub fn get_currency(state: State<DbState>) -> Result<String, AppError> {
    let guard = state.0.lock().unwrap();
    let conn = guard.as_ref().ok_or_else(AppError::db_locked)?;
    Ok(app_currency(conn).code().to_string())
}

/// Changing this only relabels amounts going forward — the app stores no
/// per-transaction currency, so past amounts are never re-converted.
#[tauri::command]
pub fn set_currency(state: State<DbState>, code: String) -> Result<(), AppError> {
    let currency = Currency::new(&code)?;
    let guard = state.0.lock().unwrap();
    let conn = guard.as_ref().ok_or_else(AppError::db_locked)?;
    Ok(scrat_infra_sqlite::set_currency_code(
        conn,
        currency.code(),
    )?)
}

/// The app-wide interface language (set via Settings > Language); falls back
/// to the app default when nothing has been chosen, or when the stored value
/// is one this build doesn't translate into.
pub(crate) fn app_language(conn: &Connection) -> Language {
    scrat_infra_sqlite::get_language(conn)
        .ok()
        .flatten()
        .and_then(|code| Language::parse(&code).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_language(state: State<DbState>) -> Result<String, AppError> {
    let guard = state.0.lock().unwrap();
    let conn = guard.as_ref().ok_or_else(AppError::db_locked)?;
    Ok(app_language(conn).as_str().to_string())
}

/// Switching the language also relabels the categories the app itself seeded,
/// but only those still carrying the name the app gave them — see
/// `CategoryService::relabel_seeded_categories` for why that check is the
/// whole policy. Categories the user created or renamed are never touched.
///
/// The relabel runs before the setting is written, so a failure mid-rename
/// leaves the database claiming the language whose names it still mostly
/// holds, rather than claiming one it was only part-way to.
#[tauri::command]
pub fn set_language(state: State<DbState>, language: String) -> Result<u32, AppError> {
    let next = Language::parse(&language)
        .map_err(|_| AppError::new(codes::UNSUPPORTED_LANGUAGE).with("value", language))?;
    let guard = state.0.lock().unwrap();
    let conn = guard.as_ref().ok_or_else(AppError::db_locked)?;
    let current = app_language(conn);

    let repo = SqliteCategoryRepository::new(conn);
    let service = CategoryService::new(&repo);
    let relabelled = service.relabel_seeded_categories(current, next)?;

    scrat_infra_sqlite::set_language(conn, next.as_str())?;
    Ok(relabelled as u32)
}

/// The only intervals the settings UI offers. `0` means "never" — the
/// frontend idle timer treats it as disabled. Validated here too so a stray
/// value (a bug, a hand-edited settings row) can't arm a lock interval
/// nobody chose.
const AUTO_LOCK_OPTIONS_MINUTES: [u32; 4] = [0, 1, 10, 60];
const DEFAULT_AUTO_LOCK_MINUTES: u32 = 10;

#[tauri::command]
pub fn get_auto_lock_minutes(state: State<DbState>) -> Result<u32, AppError> {
    let guard = state.0.lock().unwrap();
    let conn = guard.as_ref().ok_or_else(AppError::db_locked)?;
    match scrat_infra_sqlite::get_auto_lock_minutes(conn)? {
        Some(value) => value
            .parse::<u32>()
            .map_err(|_| AppError::new(codes::AUTO_LOCK_STORED_INVALID)),
        None => Ok(DEFAULT_AUTO_LOCK_MINUTES),
    }
}

#[tauri::command]
pub fn set_auto_lock_minutes(state: State<DbState>, minutes: u32) -> Result<(), AppError> {
    if !AUTO_LOCK_OPTIONS_MINUTES.contains(&minutes) {
        return Err(AppError::new(codes::AUTO_LOCK_INVALID));
    }
    let guard = state.0.lock().unwrap();
    let conn = guard.as_ref().ok_or_else(AppError::db_locked)?;
    Ok(scrat_infra_sqlite::set_auto_lock_minutes(
        conn,
        &minutes.to_string(),
    )?)
}

/// Copies the already-encrypted SQLCipher file as-is — the exported copy
/// stays encrypted, so exporting never weakens the data's protection.
#[tauri::command]
pub fn export_database(app: AppHandle, destination: String) -> Result<(), AppError> {
    let path = db_path(&app)?;
    if !scrat_infra_sqlite::database_exists(&path) {
        return Err(AppError::new(codes::NOTHING_TO_EXPORT));
    }
    std::fs::copy(&path, &destination)
        .map_err(|e| AppError::new(codes::FILESYSTEM_ERROR).with("detail", e))?;
    Ok(())
}

/// Replaces the current encrypted database with the one at `source`, keyed
/// by `passphrase` — permanently discards whatever is currently loaded.
///
/// Safety ordering matters here: the passphrase is validated against
/// `source` and the file is copied into a temp file *before* the current
/// database is touched at all, so a bad file or wrong passphrase never costs
/// the user their existing data. Only once that copy has succeeded do we
/// drop the live connection and atomically rename the temp file over the
/// real database path — same-directory rename is atomic, so a failure there
/// leaves the original database file exactly as it was.
#[tauri::command]
pub fn import_database(
    app: AppHandle,
    state: State<DbState>,
    source: String,
    passphrase: String,
) -> Result<(), AppError> {
    let source_path = PathBuf::from(&source);
    if !scrat_infra_sqlite::database_exists(&source_path) {
        return Err(AppError::new(codes::IMPORT_FILE_MISSING));
    }

    // Validate before touching the current database at all.
    drop(scrat_infra_sqlite::unlock_existing(
        &source_path,
        &passphrase,
    )?);

    let dest_path = db_path(&app)?;
    let temp_path = dest_path.with_extension("importing.tmp");
    std::fs::copy(&source_path, &temp_path)
        .map_err(|e| AppError::new(codes::FILESYSTEM_ERROR).with("detail", e))?;

    let mut guard = state.0.lock().unwrap();
    *guard = None; // release the current connection before replacing its file
    if let Err(e) = std::fs::rename(&temp_path, &dest_path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(AppError::new(codes::IMPORT_FINALIZE_FAILED).with("detail", e));
    }
    match scrat_infra_sqlite::unlock_existing(&dest_path, &passphrase) {
        Ok(conn) => {
            *guard = Some(conn);
            Ok(())
        }
        Err(e) => Err(AppError::new(codes::IMPORT_REOPEN_FAILED).with("detail", e)),
    }
}

/// Permanently deletes the encrypted database file — there is no undo and no
/// backup is made. The frontend is expected to have gotten explicit user
/// confirmation before calling this.
#[tauri::command]
pub fn delete_database(app: AppHandle, state: State<DbState>) -> Result<(), AppError> {
    let path = db_path(&app)?;
    let mut guard = state.0.lock().unwrap();
    *guard = None; // release the connection before deleting its file
    if scrat_infra_sqlite::database_exists(&path) {
        std::fs::remove_file(&path)
            .map_err(|e| AppError::new(codes::FILESYSTEM_ERROR).with("detail", e))?;
    }
    for suffix in ["db-wal", "db-shm", "db-journal"] {
        let sidecar = path.with_extension(suffix);
        if sidecar.exists() {
            let _ = std::fs::remove_file(&sidecar);
        }
    }
    Ok(())
}
