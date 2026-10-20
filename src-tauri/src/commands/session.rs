use std::sync::Arc;

use tauri::State;

use crate::{
    auth, crypto,
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
    tauri::async_runtime::spawn_blocking(|| auth::authenticate("unlock your Coffer"))
        .await
        .map_err(|e| AppError::Auth(e.to_string()))??;
    let key = crypto::load_or_create_key()?;

    #[cfg(debug_assertions)]
    crate::seed::run_if_requested(&state.db(), &key)?;

    let info = state.session().open_window(key);
    session::schedule_expiry(Arc::clone(&state.session));
    Ok(info)
}

#[tauri::command]
pub fn lock(state: State<AppState>) -> SessionInfo {
    state.session().close_window()
}
