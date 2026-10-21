mod auth;
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

use commands::{folders, items, pins, projects, session as session_cmd, vault};
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let conn = db::open(&dir.join("coffer.db"))?;
            app.manage(AppState::new(conn));
            lock_events::observe(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            session_cmd::session_info,
            session_cmd::unlock,
            session_cmd::lock,
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
