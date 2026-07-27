mod accounts;
mod categories;
mod db;

use db::DbState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(DbState::default())
        .invoke_handler(tauri::generate_handler![
            db::is_db_initialized,
            db::create_db_with_passphrase,
            db::unlock_db,
            accounts::list_accounts,
            accounts::create_account,
            accounts::rename_account,
            accounts::set_opening_balance,
            accounts::add_source_pattern,
            accounts::remove_source_pattern,
            accounts::archive_account,
            accounts::activate_account,
            accounts::delete_account,
            categories::list_categories,
            categories::create_category,
            categories::rename_category,
            categories::move_category,
            categories::delete_category,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
