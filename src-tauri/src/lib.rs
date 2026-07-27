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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
