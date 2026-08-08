mod accounts;
mod categories;
mod db;
mod errors;
mod import;
mod settings;
mod transactions;
mod transfer_rules;

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
            db::change_passphrase,
            db::lock_db,
            accounts::list_accounts,
            accounts::create_account,
            accounts::rename_account,
            accounts::establish_opening_balance,
            accounts::add_description_pattern,
            accounts::remove_description_pattern,
            accounts::delete_account,
            accounts::set_default_account,
            accounts::reorder_accounts,
            categories::list_categories,
            categories::create_category,
            categories::rename_category,
            categories::set_category_icon,
            categories::move_category,
            categories::delete_category,
            categories::get_rent_category,
            categories::set_rent_category,
            transactions::list_transactions,
            transactions::list_transactions_page,
            transactions::count_transactions,
            transactions::create_transaction,
            transactions::delete_transaction,
            transactions::delete_transactions,
            transactions::set_transaction_category,
            transactions::set_transactions_category,
            transactions::suggest_account_for_description,
            transactions::suggest_category_for_description,
            transactions::export_transactions_csv,
            transactions::list_recurring_charges,
            transactions::reconcile_account,
            transfer_rules::list_transfer_rules,
            transfer_rules::create_transfer_rule,
            transfer_rules::delete_transfer_rule,
            transfer_rules::apply_transfer_rules,
            import::preview_csv_import,
            import::check_duplicate_transactions,
            import::commit_csv_import,
            settings::get_currency,
            settings::set_currency,
            settings::get_language,
            settings::set_language,
            settings::get_auto_lock_minutes,
            settings::set_auto_lock_minutes,
            settings::export_database,
            settings::import_database,
            settings::delete_database,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
