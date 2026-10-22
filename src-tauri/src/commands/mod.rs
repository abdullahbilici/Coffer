//! Tauri command handlers. Keep them thin: validate input, take locks, delegate to `db`.

pub mod backup;
pub mod folders;
pub mod items;
pub mod pins;
pub mod projects;
pub mod sealing;
pub mod session;
pub mod settings;
pub mod vault;

use rusqlite::Connection;

use crate::{
    db,
    error::{AppError, AppResult},
};

const MAX_NAME_LEN: usize = 200;

/// Trims a user-provided name and rejects empty or oversized values.
fn required_name(value: &str, label: &str) -> AppResult<String> {
    let name = value.trim();
    if name.is_empty() {
        return Err(AppError::Invalid(format!("{label} is required")));
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(AppError::Invalid(format!(
            "{label} must be at most {MAX_NAME_LEN} characters"
        )));
    }
    Ok(name.to_string())
}

/// Rejects a folder that doesn't belong to `project_id`. `None` means the project root.
fn ensure_folder_in_project(
    conn: &Connection,
    folder_id: Option<i64>,
    project_id: i64,
) -> AppResult<()> {
    if let Some(folder_id) = folder_id
        && db::folders::project_of(conn, folder_id)? != project_id
    {
        return Err(AppError::Invalid(
            "Folder belongs to another project".to_string(),
        ));
    }
    Ok(())
}
