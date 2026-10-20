use tauri::State;

use super::{ensure_folder_in_project, required_name};
use crate::{
    db::folders,
    error::{AppError, AppResult},
    state::AppState,
    time::now_millis,
};

#[tauri::command]
pub fn create_folder(
    state: State<AppState>,
    project_id: i64,
    parent_id: Option<i64>,
    name: String,
) -> AppResult<i64> {
    state.session().require_unlocked()?;
    let name = required_name(&name, "Folder name")?;
    let conn = state.db();
    ensure_folder_in_project(&conn, parent_id, project_id)?;
    folders::insert(&conn, project_id, parent_id, &name, now_millis())
}

#[tauri::command]
pub fn rename_folder(state: State<AppState>, id: i64, name: String) -> AppResult<()> {
    state.session().require_unlocked()?;
    let name = required_name(&name, "Folder name")?;
    folders::rename(&state.db(), id, &name)
}

#[tauri::command]
pub fn move_folder(
    state: State<AppState>,
    id: i64,
    project_id: i64,
    parent_id: Option<i64>,
) -> AppResult<()> {
    state.session().require_unlocked()?;
    let conn = state.db();
    ensure_folder_in_project(&conn, parent_id, project_id)?;
    if let Some(parent_id) = parent_id
        && folders::subtree(&conn, id)?.contains(&parent_id)
    {
        return Err(AppError::Invalid(
            "A folder can't move into itself".to_string(),
        ));
    }
    folders::move_to(&conn, id, project_id, parent_id)
}

#[tauri::command]
pub fn delete_folder(state: State<AppState>, id: i64) -> AppResult<()> {
    state.session().require_unlocked()?;
    folders::delete(&state.db(), id)
}
