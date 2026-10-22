use std::sync::Arc;

use tauri::State;

use crate::{
    clipboard,
    db::settings::{self, Settings},
    error::AppResult,
    session::{self, SessionInfo},
    state::AppState,
};

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> AppResult<Settings> {
    state.session().require_entered()?;
    settings::load(&state.db())
}

/// Stores the new length and starts the open window over with it.
#[tauri::command]
pub fn set_unlock_window(state: State<AppState>, secs: Option<u64>) -> AppResult<SessionInfo> {
    let conn = state.db();
    state.session().require_unlocked()?;
    settings::set_unlock_window(&conn, secs)?;
    let length = settings::load(&conn)?.unlock_window();
    let info = state.session().restart_window(length)?;
    session::schedule_expiry(Arc::clone(&state.session), length);
    Ok(info)
}

#[tauri::command]
pub fn set_clipboard_clear(state: State<AppState>, secs: Option<u64>) -> AppResult<()> {
    let conn = state.db();
    state.session().require_unlocked()?;
    settings::set_clipboard_clear(&conn, secs)
}

/// Called right after the frontend copied a secret.
#[tauri::command]
pub fn clear_clipboard_later(state: State<AppState>) -> AppResult<()> {
    state.session().require_entered()?;
    if let Some(delay) = settings::load(&state.db())?.clipboard_clear() {
        clipboard::clear_later(delay);
    }
    Ok(())
}
