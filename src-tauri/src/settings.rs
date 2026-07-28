use std::path::PathBuf;

use scrat_domain::money::Currency;
use tauri::{AppHandle, State};

use crate::accounts::app_currency;
use crate::db::{db_path, describe, DbState};

#[tauri::command]
pub fn get_currency(state: State<DbState>) -> Result<String, String> {
    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    Ok(app_currency(conn).code().to_string())
}

/// Changing this only relabels amounts going forward — the app stores no
/// per-transaction currency, so past amounts are never re-converted.
#[tauri::command]
pub fn set_currency(state: State<DbState>, code: String) -> Result<(), String> {
    let currency = Currency::new(&code).map_err(|e| e.to_string())?;
    let guard = state.0.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| "database is locked".to_string())?;
    scrat_infra_sqlite::set_currency_code(conn, currency.code()).map_err(|e| e.to_string())
}

/// Copies the already-encrypted SQLCipher file as-is — the exported copy
/// stays encrypted, so exporting never weakens the data's protection.
#[tauri::command]
pub fn export_database(app: AppHandle, destination: String) -> Result<(), String> {
    let path = db_path(&app)?;
    if !scrat_infra_sqlite::database_exists(&path) {
        return Err("there is no database to export yet".to_string());
    }
    std::fs::copy(&path, &destination).map_err(|e| e.to_string())?;
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
) -> Result<(), String> {
    let source_path = PathBuf::from(&source);
    if !scrat_infra_sqlite::database_exists(&source_path) {
        return Err("the selected file does not exist".to_string());
    }

    // Validate before touching the current database at all.
    drop(scrat_infra_sqlite::unlock_existing(&source_path, &passphrase).map_err(describe)?);

    let dest_path = db_path(&app)?;
    let temp_path = dest_path.with_extension("importing.tmp");
    std::fs::copy(&source_path, &temp_path).map_err(|e| e.to_string())?;

    let mut guard = state.0.lock().unwrap();
    *guard = None; // release the current connection before replacing its file
    if let Err(e) = std::fs::rename(&temp_path, &dest_path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!(
            "could not finalize import: {e}. Your original database file was not modified — \
             reload the app and unlock it with your original passphrase."
        ));
    }
    match scrat_infra_sqlite::unlock_existing(&dest_path, &passphrase) {
        Ok(conn) => {
            *guard = Some(conn);
            Ok(())
        }
        Err(e) => Err(format!(
            "the database was replaced but could not be reopened ({}). Reload the app and \
             unlock it with the imported file's passphrase.",
            describe(e)
        )),
    }
}
