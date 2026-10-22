use serde::Deserialize;
use tauri::State;

use super::{
    required_name,
    sealing::{open, seal_inline},
    valid_date,
};
use crate::{
    db::todos::{self, Todo, TodoRecord},
    error::AppResult,
    state::AppState,
    time::now_millis,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTodoInput {
    /// `None` creates a new todo.
    id: Option<i64>,
    project_id: Option<i64>,
    item_id: Option<i64>,
    title: String,
    description: String,
    due_date: Option<String>,
}

#[tauri::command]
pub fn reveal_todo(state: State<AppState>, id: i64) -> AppResult<String> {
    state.session().require_entered()?;
    let (template, sealed) = todos::description_secrets(&state.db(), id)?;
    open(&state, Some(&template), &sealed)
}

#[tauri::command]
pub fn save_todo(state: State<AppState>, input: SaveTodoInput) -> AppResult<Todo> {
    state.session().require_unlocked()?;
    let title = required_name(&input.title, "Title")?;
    let due_date = input.due_date.as_deref().map(valid_date).transpose()?;
    let conn = state.db();

    let sealed = seal_inline(&state, &input.description)?;
    let (description, secrets) = match &sealed {
        Some((template, secrets)) => (template.as_str(), Some(secrets)),
        None => (input.description.as_str(), None),
    };
    let record = TodoRecord {
        project_id: input.project_id,
        item_id: input.item_id,
        title: &title,
        description,
        secrets,
        due_date,
    };
    let now = now_millis();
    let id = match input.id {
        Some(id) => {
            todos::update(&conn, id, &record, now)?;
            id
        }
        None => todos::insert(&conn, &record, now)?,
    };
    todos::get(&conn, id)
}

#[tauri::command]
pub fn set_todo_done(state: State<AppState>, id: i64, done: bool) -> AppResult<()> {
    state.session().require_unlocked()?;
    todos::set_done(&state.db(), id, done, now_millis())
}

#[tauri::command]
pub fn delete_todo(state: State<AppState>, id: i64) -> AppResult<()> {
    state.session().require_unlocked()?;
    todos::delete(&state.db(), id)
}
