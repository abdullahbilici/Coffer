use serde::Deserialize;
use tauri::State;

use super::{
    ensure_folder_in_project, required_name,
    sealing::{open, seal, seal_inline},
};
use crate::{
    db::items::{self, Body, Encrypted, Item, ItemRecord, ItemSummary},
    error::AppResult,
    state::AppState,
    time::now_millis,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveItemInput {
    /// `None` creates a new item.
    id: Option<i64>,
    project_id: i64,
    folder_id: Option<i64>,
    title: String,
    body: String,
    secure: bool,
    tags: Vec<String>,
}

#[tauri::command]
pub fn get_item(state: State<AppState>, id: i64) -> AppResult<Item> {
    state.session().require_entered()?;
    items::get(&state.db(), id)
}

#[tauri::command]
pub fn reveal_item(state: State<AppState>, id: i64) -> AppResult<String> {
    state.session().require_entered()?;
    let Encrypted { template, sealed } = items::encrypted(&state.db(), id)?;
    open(&state, template.as_deref(), &sealed)
}

#[tauri::command]
pub fn save_item(state: State<AppState>, input: SaveItemInput) -> AppResult<ItemSummary> {
    state.session().require_unlocked()?;
    let title = required_name(&input.title, "Title")?;
    let tags = items::normalize_tags(&input.tags);
    let conn = state.db();

    ensure_folder_in_project(&conn, input.folder_id, input.project_id)?;

    let body = if input.secure {
        Body::Secure(seal(&state, &input.body)?)
    } else {
        match seal_inline(&state, &input.body)? {
            Some((template, secrets)) => Body::Inline { template, secrets },
            None => Body::Plain(input.body),
        }
    };

    let record = ItemRecord {
        project_id: input.project_id,
        folder_id: input.folder_id,
        title: &title,
        body: &body,
        tags: &tags,
    };
    let now = now_millis();
    let id = match input.id {
        Some(id) => {
            items::update(&conn, id, &record, now)?;
            id
        }
        None => items::insert(&conn, &record, now)?,
    };
    items::summary(&conn, id)
}

#[tauri::command]
pub fn move_item(
    state: State<AppState>,
    id: i64,
    project_id: i64,
    folder_id: Option<i64>,
) -> AppResult<()> {
    state.session().require_unlocked()?;
    let conn = state.db();
    ensure_folder_in_project(&conn, folder_id, project_id)?;
    items::move_to(&conn, id, project_id, folder_id, now_millis())
}

#[tauri::command]
pub fn duplicate_item(state: State<AppState>, id: i64) -> AppResult<ItemSummary> {
    state.session().require_unlocked()?;
    let conn = state.db();
    let copy = items::duplicate(&conn, id, now_millis())?;
    items::summary(&conn, copy)
}

#[tauri::command]
pub fn delete_item(state: State<AppState>, id: i64) -> AppResult<()> {
    state.session().require_unlocked()?;
    items::delete(&state.db(), id)
}
