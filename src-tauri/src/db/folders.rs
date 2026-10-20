use rusqlite::{Connection, params};
use serde::Serialize;

use super::expect_changed;
use crate::error::AppResult;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: i64,
    pub project_id: i64,
    /// `None` for folders at the project root.
    pub parent_id: Option<i64>,
    pub name: String,
}

pub fn list(conn: &Connection) -> AppResult<Vec<Folder>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, parent_id, name FROM folders ORDER BY name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Folder {
            id: row.get(0)?,
            project_id: row.get(1)?,
            parent_id: row.get(2)?,
            name: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn project_of(conn: &Connection, id: i64) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT project_id FROM folders WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?)
}

pub fn insert(
    conn: &Connection,
    project_id: i64,
    parent_id: Option<i64>,
    name: &str,
    now: i64,
) -> AppResult<i64> {
    conn.execute(
        "INSERT INTO folders (project_id, parent_id, name, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![project_id, parent_id, name, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn rename(conn: &Connection, id: i64, name: &str) -> AppResult<()> {
    let changed = conn.execute(
        "UPDATE folders SET name = ?2 WHERE id = ?1",
        params![id, name],
    )?;
    expect_changed(changed)
}

/// Deletes the folder with all nested folders and items.
pub fn delete(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM folders WHERE id = ?1", [id])?;
    Ok(())
}
