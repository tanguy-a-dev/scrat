use scrat_domain::money::Currency;
use tauri::{AppHandle, State};

use crate::accounts::app_currency;
use crate::db::{db_path, DbState};

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
