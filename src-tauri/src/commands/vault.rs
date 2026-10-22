use serde::Serialize;
use tauri::State;

use crate::{
    db::{
        folders::{self, Folder},
        items::{self, ItemSummary},
        payments::{self, Payment},
        pins::{self, PinRef},
        projects::{self, ProjectSummary},
        todos::{self, Todo},
    },
    error::AppResult,
    state::AppState,
};

/// Everything the sidebar and agenda need, loaded in one call. Contains no item bodies; todo
/// descriptions come as templates, like overviews.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tree {
    projects: Vec<ProjectSummary>,
    folders: Vec<Folder>,
    items: Vec<ItemSummary>,
    /// Pinned projects, folders and items in the user's order.
    pins: Vec<PinRef>,
    todos: Vec<Todo>,
    payments: Vec<Payment>,
}

#[tauri::command]
pub fn get_tree(state: State<AppState>) -> AppResult<Tree> {
    state.session().require_entered()?;
    let conn = state.db();
    Ok(Tree {
        projects: projects::list(&conn)?,
        folders: folders::list(&conn)?,
        items: items::list(&conn)?,
        pins: pins::list(&conn)?,
        todos: todos::list(&conn)?,
        payments: payments::list(&conn)?,
    })
}
