use tauri::{AppHandle, State, WebviewWindow};

use crate::{capture, db::settings, error::AppResult, state::AppState};

/// Stores and registers the shortcut that opens quick capture (`None` switches it off). A
/// shortcut that can't be registered isn't stored, and the old one stays.
#[tauri::command]
pub fn set_capture_shortcut(
    app: AppHandle,
    state: State<AppState>,
    shortcut: Option<String>,
) -> AppResult<()> {
    let conn = state.db();
    state.session().require_unlocked()?;
    let old = settings::capture_shortcut(&conn)?;
    if let Err(e) = capture::register(&app, shortcut.as_deref()) {
        let _ = capture::register(&app, old.as_deref());
        return Err(e);
    }
    settings::set_capture_shortcut(&conn, shortcut.as_deref())
}

/// The panel hides itself once its exit has played.
#[tauri::command]
pub fn hide_capture(window: WebviewWindow) {
    let _ = window.hide();
}

/// Gives the panel back the keyboard, e.g. after a Touch ID prompt took it.
#[tauri::command]
pub fn focus_capture(app: AppHandle, window: WebviewWindow) {
    if window.is_visible().unwrap_or(false) {
        capture::show(&app, &window);
    }
}

#[tauri::command]
pub fn fit_capture(window: WebviewWindow, height: f64) {
    capture::fit(&window, height);
}

#[tauri::command]
pub fn open_in_coffer(app: AppHandle, conversation_id: i64) -> AppResult<()> {
    capture::open_in_main(&app, conversation_id)
}
