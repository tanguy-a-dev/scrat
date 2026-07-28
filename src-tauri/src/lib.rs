mod accounts;
mod categories;
mod db;
mod import;
mod settings;
mod transactions;

use db::DbState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
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
            accounts::set_default_account,
            categories::list_categories,
            categories::create_category,
            categories::rename_category,
            categories::move_category,
            categories::delete_category,
            categories::set_default_category,
            transactions::list_transactions,
            transactions::create_transaction,
            transactions::delete_transaction,
            transactions::set_transaction_category,
            transactions::delete_transactions_in_range,
            transactions::suggest_account_for_source,
            transactions::suggest_category_for_source,
            import::preview_csv_import,
            import::commit_csv_import,
            settings::get_currency,
            settings::set_currency,
            settings::export_database,
            settings::import_database,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
