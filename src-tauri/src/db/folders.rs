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

/// The folder and every folder nested below it.
pub fn subtree(conn: &Connection, id: i64) -> AppResult<Vec<i64>> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE sub(id) AS (
             SELECT id FROM folders WHERE id = ?1
             UNION ALL
             SELECT f.id FROM folders f JOIN sub ON f.parent_id = sub.id
         )
         SELECT id FROM sub",
    )?;
    let rows = stmt.query_map([id], |row| row.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Moves a folder under `parent_id` (or the project root). When the project changes, every
/// nested folder and item moves with it. The caller rejects moves into the folder's own subtree.
pub fn move_to(
    conn: &Connection,
    id: i64,
    project_id: i64,
    parent_id: Option<i64>,
) -> AppResult<()> {
    let tx = conn.unchecked_transaction()?;
    let changed = tx.execute(
        "UPDATE folders SET project_id = ?2, parent_id = ?3 WHERE id = ?1",
        params![id, project_id, parent_id],
    )?;
    expect_changed(changed)?;
    let subtree = "WITH RECURSIVE sub(id) AS (
             SELECT ?1
             UNION ALL
             SELECT f.id FROM folders f JOIN sub ON f.parent_id = sub.id
         )";
    tx.execute(
        &format!("{subtree} UPDATE folders SET project_id = ?2 WHERE id IN (SELECT id FROM sub)"),
        params![id, project_id],
    )?;
    tx.execute(
        &format!(
            "{subtree} UPDATE items SET project_id = ?2 WHERE folder_id IN (SELECT id FROM sub)"
        ),
        params![id, project_id],
    )?;
    tx.commit()?;
    Ok(())
}

/// Deletes the folder with all nested folders and items.
pub fn delete(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM folders WHERE id = ?1", [id])?;
    Ok(())
}
