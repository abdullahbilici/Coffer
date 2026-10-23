use std::sync::Arc;

use tauri::State;
use zeroize::Zeroizing;

use crate::{
    clipboard,
    db::settings::{self, Settings},
    error::AppResult,
    mcp,
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

#[tauri::command]
pub fn set_clipboard_concealed(state: State<AppState>, on: bool) -> AppResult<()> {
    let conn = state.db();
    state.session().require_unlocked()?;
    settings::set_clipboard_concealed(&conn, on)
}

/// Copies a secret the frontend already shows; kept out of clipboard history and cleared
/// later as the settings say.
#[tauri::command]
pub fn copy_secret(state: State<AppState>, text: Zeroizing<String>) -> AppResult<()> {
    state.session().require_entered()?;
    let settings = settings::load(&state.db())?;
    clipboard::copy_secret(
        &text,
        settings.clipboard_concealed,
        settings.clipboard_clear(),
    );
    Ok(())
}

#[tauri::command]
pub fn set_mcp_access(state: State<AppState>, on: bool) -> AppResult<()> {
    let conn = state.db();
    state.session().require_unlocked()?;
    settings::set_mcp_access(&conn, on)
}

/// The command that registers this binary with Claude Code.
#[tauri::command]
pub fn mcp_command(state: State<AppState>) -> AppResult<String> {
    state.session().require_entered()?;
    Ok(mcp::register_command(&std::env::current_exe()?))
}
