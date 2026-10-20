use serde::Serialize;
use tauri::State;

use crate::{
    db::{
        folders::{self, Folder},
        items::{self, ItemSummary},
        projects::{self, ProjectSummary},
    },
    error::AppResult,
    state::AppState,
};

/// Everything the sidebar needs, loaded in one call. Contains no item bodies.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tree {
    projects: Vec<ProjectSummary>,
    folders: Vec<Folder>,
    items: Vec<ItemSummary>,
}

#[tauri::command]
pub fn get_tree(state: State<AppState>) -> AppResult<Tree> {
    state.session().require_entered()?;
    let conn = state.db();
    Ok(Tree {
        projects: projects::list(&conn)?,
        folders: folders::list(&conn)?,
        items: items::list(&conn)?,
    })
}
