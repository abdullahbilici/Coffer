use tauri::State;

use crate::{
    db::pins::{self, PinRef},
    error::AppResult,
    state::AppState,
};

#[tauri::command]
pub fn set_pinned(state: State<AppState>, pin: PinRef, pinned: bool) -> AppResult<()> {
    state.session().require_unlocked()?;
    let conn = state.db();
    if pinned {
        pins::pin(&conn, pin)
    } else {
        pins::unpin(&conn, pin)
    }
}

#[tauri::command]
pub fn reorder_pins(state: State<AppState>, order: Vec<PinRef>) -> AppResult<()> {
    state.session().require_unlocked()?;
    pins::reorder(&state.db(), &order)
}
