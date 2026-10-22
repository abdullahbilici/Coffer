mod auth;
mod backup;
mod clipboard;
mod commands;
mod crypto;
mod db;
mod error;
mod lock_events;
mod secrets;
#[cfg(debug_assertions)]
mod seed;
mod session;
mod state;
mod time;

use tauri::Manager;

use commands::{
    backup as backup_cmd, folders, items, pins, projects, session as session_cmd, settings, vault,
};
use state::AppState;

/// Dev builds keep their own vault file, so testing (and `npm run reseed`) never touches the
/// real one. Both share the Keychain key.
#[cfg(debug_assertions)]
const DB_FILE: &str = "coffer-dev.db";
#[cfg(not(debug_assertions))]
const DB_FILE: &str = "coffer.db";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let conn = db::open(&dir.join(DB_FILE))?;
            app.manage(AppState::new(conn));
            lock_events::observe(app.handle());
            backup::spawn_worker(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            session_cmd::session_info,
            session_cmd::unlock,
            session_cmd::lock,
            settings::get_settings,
            settings::set_unlock_window,
            settings::set_clipboard_clear,
            settings::clear_clipboard_later,
            backup_cmd::vault_is_empty,
            backup_cmd::default_backup_dir,
            backup_cmd::suggest_passphrase,
            backup_cmd::set_backup_passphrase,
            backup_cmd::back_up_now,
            backup_cmd::set_backup_dir,
            backup_cmd::set_backup_auto,
            backup_cmd::restore_backup,
            vault::get_tree,
            projects::get_project,
            projects::create_project,
            projects::rename_project,
            projects::update_project_readme,
            projects::reveal_project_readme,
            projects::delete_project,
            folders::create_folder,
            folders::rename_folder,
            folders::move_folder,
            folders::delete_folder,
            items::get_item,
            items::reveal_item,
            items::save_item,
            items::move_item,
            items::duplicate_item,
            items::delete_item,
            pins::set_pinned,
            pins::reorder_pins,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
