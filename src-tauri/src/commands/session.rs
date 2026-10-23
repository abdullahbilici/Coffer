use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::{
    auth,
    backup::{self, Trigger},
    crypto::{self, VaultKey},
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

/// Both windows follow the session: the one that asked gets it back, the other one hears it.
#[tauri::command]
pub async fn unlock(app: AppHandle, state: State<'_, AppState>) -> AppResult<SessionInfo> {
    let key = authenticated_key("unlock your Coffer").await?;

    #[cfg(debug_assertions)]
    crate::seed::run_if_requested(&state.db(), &key)?;

    let info = open_window(&state, key)?;
    let _ = app.emit(SESSION_CHANGED, &info);
    Ok(info)
}

#[tauri::command]
pub fn lock(app: AppHandle, state: State<AppState>) -> SessionInfo {
    // Failures are recorded in the settings and shown in the Backup section.
    let _ = backup::run(&app, Trigger::Flush);
    let info = state.session().close_window();
    let _ = app.emit(SESSION_CHANGED, &info);
    info
}

/// Touch ID (or the account password), then the vault key from the Keychain. Every prompt for
/// device owner auth goes through here, so `COFFER_DEV_INSECURE` covers them all. `reason`
/// completes "Coffer is trying to …".
pub async fn authenticated_key(reason: &'static str) -> AppResult<VaultKey> {
    #[cfg(debug_assertions)]
    if crate::dev::insecure() {
        return Ok(crate::dev::insecure_key());
    }
    tauri::async_runtime::spawn_blocking(move || auth::authenticate(reason))
        .await
        .map_err(|e| AppError::Auth(e.to_string()))??;
    crypto::load_or_create_key()
}

/// Opens (or restarts) the unlock window with the configured length.
fn open_window(state: &AppState, key: VaultKey) -> AppResult<SessionInfo> {
    let length = settings::load(&state.db())?.unlock_window();
    let info = state.session().open_window(key, length);
    session::schedule_expiry(Arc::clone(&state.session), length);
    Ok(info)
}

/// Opens the unlock window from a command that asked for auth itself, and tells the frontend,
/// which didn't call `unlock`.
pub(super) fn open_window_after_auth(
    app: &AppHandle,
    state: &AppState,
    key: VaultKey,
) -> AppResult<()> {
    let info = open_window(state, key)?;
    let _ = app.emit(SESSION_CHANGED, info);
    Ok(())
}
