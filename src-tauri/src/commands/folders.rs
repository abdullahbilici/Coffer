use tauri::State;

use super::{ensure_folder_in_project, required_name};
use crate::{db::folders, error::AppResult, state::AppState, time::now_millis};

#[tauri::command]
pub fn create_folder(
    state: State<AppState>,
    project_id: i64,
    parent_id: Option<i64>,
    name: String,
) -> AppResult<i64> {
    state.session().require_entered()?;
    let name = required_name(&name, "Folder name")?;
    let conn = state.db();
    ensure_folder_in_project(&conn, parent_id, project_id)?;
    folders::insert(&conn, project_id, parent_id, &name, now_millis())
}

#[tauri::command]
pub fn rename_folder(state: State<AppState>, id: i64, name: String) -> AppResult<()> {
    state.session().require_entered()?;
    let name = required_name(&name, "Folder name")?;
    folders::rename(&state.db(), id, &name)
}

#[tauri::command]
pub fn delete_folder(state: State<AppState>, id: i64) -> AppResult<()> {
    state.session().require_entered()?;
    folders::delete(&state.db(), id)
}
