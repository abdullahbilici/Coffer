use std::sync::Arc;

use tauri::State;

use crate::{
    auth,
    crypto::{self, VaultKey},
    error::{AppError, AppResult},
    session::{self, SessionInfo},
    state::AppState,
};

#[tauri::command]
pub fn session_info(state: State<AppState>) -> SessionInfo {
    state.session().info()
}

#[tauri::command]
pub async fn unlock(state: State<'_, AppState>) -> AppResult<SessionInfo> {
    let key = authenticated_key().await?;

    #[cfg(debug_assertions)]
    crate::seed::run_if_requested(&state.db(), &key)?;

    let info = state.session().open_window(key);
    session::schedule_expiry(Arc::clone(&state.session));
    Ok(info)
}

/// Touch ID (or the account password), then the vault key from the Keychain. Every prompt for
/// device owner auth goes through here, so `COFFER_DEV_INSECURE` covers them all.
pub async fn authenticated_key() -> AppResult<VaultKey> {
    #[cfg(debug_assertions)]
    if crate::dev::insecure() {
        return Ok(crate::dev::insecure_key());
    }
    tauri::async_runtime::spawn_blocking(|| auth::authenticate("unlock your Coffer"))
        .await
        .map_err(|e| AppError::Auth(e.to_string()))??;
    crypto::load_or_create_key()
}

#[tauri::command]
pub fn lock(state: State<AppState>) -> SessionInfo {
    state.session().close_window()
}
