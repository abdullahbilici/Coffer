use tauri::State;

use super::{
    required_name,
    sealing::{open, seal_inline},
};
use crate::{
    db::projects::{self, Project},
    error::AppResult,
    state::AppState,
    time::now_millis,
};

#[tauri::command]
pub fn get_project(state: State<AppState>, id: i64) -> AppResult<Project> {
    state.session().require_entered()?;
    projects::get(&state.db(), id)
}

#[tauri::command]
pub fn create_project(state: State<AppState>, name: String) -> AppResult<Project> {
    state.session().require_unlocked()?;
    let name = required_name(&name, "Project name")?;
    let conn = state.db();
    let id = projects::insert(&conn, &name, "", now_millis())?;
    projects::get(&conn, id)
}

#[tauri::command]
pub fn rename_project(state: State<AppState>, id: i64, name: String) -> AppResult<()> {
    state.session().require_unlocked()?;
    let name = required_name(&name, "Project name")?;
    projects::rename(&state.db(), id, &name, now_millis())
}

#[tauri::command]
pub fn update_project_readme(state: State<AppState>, id: i64, readme: String) -> AppResult<()> {
    state.session().require_unlocked()?;
    let conn = state.db();
    match seal_inline(&state, &readme)? {
        Some((template, secrets)) => {
            projects::update_readme(&conn, id, &template, Some(&secrets), now_millis())
        }
        None => projects::update_readme(&conn, id, &readme, None, now_millis()),
    }
}

#[tauri::command]
pub fn reveal_project_readme(state: State<AppState>, id: i64) -> AppResult<String> {
    state.session().require_entered()?;
    let (template, sealed) = projects::readme_secrets(&state.db(), id)?;
    open(&state, Some(&template), &sealed)
}

#[tauri::command]
pub fn delete_project(state: State<AppState>, id: i64) -> AppResult<()> {
    state.session().require_unlocked()?;
    projects::delete(&state.db(), id)
}
