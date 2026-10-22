use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::{
    auth,
    backup::{self, Trigger},
    crypto,
    db::settings,
    error::{AppError, AppResult},
    lock_events::SESSION_CHANGED,
    session::{self, SessionInfo},
    state::AppState,
};

#[tauri::command]
pub fn session_info(state: State<AppState>) -> SessionInfo {
    state.session().info()
}

#[tauri::command]
pub async fn unlock(state: State<'_, AppState>) -> AppResult<SessionInfo> {
    authenticate("unlock your Coffer").await?;

    #[cfg(debug_assertions)]
    crate::seed::run_if_requested(&state.db(), &crypto::load_or_create_key()?)?;

    open_window(&state)
}

#[tauri::command]
pub fn lock(app: AppHandle, state: State<AppState>) -> SessionInfo {
    // Failures are recorded in the settings and shown in the Backup section.
    let _ = backup::run(&app, Trigger::Flush);
    state.session().close_window()
}

/// Asks for Touch ID (or the account password). `reason` completes "Coffer is trying to …".
pub(super) async fn authenticate(reason: &'static str) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || auth::authenticate(reason))
        .await
        .map_err(|e| AppError::Auth(e.to_string()))?
}

/// Opens (or restarts) the unlock window with the configured length. Authenticate first.
fn open_window(state: &AppState) -> AppResult<SessionInfo> {
    let key = crypto::load_or_create_key()?;
    let length = settings::load(&state.db())?.unlock_window();
    let info = state.session().open_window(key, length);
    session::schedule_expiry(Arc::clone(&state.session), length);
    Ok(info)
}

/// Opens the unlock window after a Touch ID prompt the command itself asked for, and tells
/// the frontend, which didn't call `unlock`.
pub(super) fn open_window_after_auth(app: &AppHandle, state: &AppState) -> AppResult<()> {
    let info = open_window(state)?;
    let _ = app.emit(SESSION_CHANGED, info);
    Ok(())
}
