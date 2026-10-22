use rusqlite::{Connection, params};
use serde::Serialize;

use super::{SecureBody, expect_changed};
use crate::error::AppResult;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub id: i64,
    /// `None` for a general todo.
    pub project_id: Option<i64>,
    /// An optional linked note.
    pub item_id: Option<i64>,
    pub title: String,
    /// Markdown. With inline secrets this is the `` `!0!` ``-marker template; the full text
    /// goes through `reveal_todo`.
    pub description: String,
    pub has_secrets: bool,
    /// `YYYY-MM-DD`.
    pub due_date: Option<String>,
    pub done: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct TodoRecord<'a> {
    pub project_id: Option<i64>,
    pub item_id: Option<i64>,
    pub title: &'a str,
    /// Plain text or a marker template.
    pub description: &'a str,
    pub secrets: Option<&'a SecureBody>,
    pub due_date: Option<&'a str>,
}

const COLUMNS: &str = "id, project_id, item_id, title, description, due_date, done, created_at,
     updated_at, ciphertext IS NOT NULL AS has_secrets";

fn to_todo(row: &rusqlite::Row) -> rusqlite::Result<Todo> {
    Ok(Todo {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        item_id: row.get("item_id")?,
        title: row.get("title")?,
        description: row.get("description")?,
        has_secrets: row.get("has_secrets")?,
        due_date: row.get("due_date")?,
        done: row.get("done")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

/// Todos by due date (undated last), then by creation.
pub fn list(conn: &Connection) -> AppResult<Vec<Todo>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM todos ORDER BY due_date IS NULL, due_date, created_at"
    ))?;
    let rows = stmt.query_map([], to_todo)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Todo> {
    Ok(conn.query_row(
        &format!("SELECT {COLUMNS} FROM todos WHERE id = ?1"),
        [id],
        to_todo,
    )?)
}

pub fn insert(conn: &Connection, record: &TodoRecord, now: i64) -> AppResult<i64> {
    conn.execute(
        "INSERT INTO todos (project_id, item_id, title, description, nonce, ciphertext, due_date,
         created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![
            record.project_id,
            record.item_id,
            record.title,
            record.description,
            record.secrets.map(|s| &s.nonce),
            record.secrets.map(|s| &s.ciphertext),
            record.due_date,
            now
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update(conn: &Connection, id: i64, record: &TodoRecord, now: i64) -> AppResult<()> {
    let changed = conn.execute(
        "UPDATE todos SET project_id = ?2, item_id = ?3, title = ?4, description = ?5, nonce = ?6,
         ciphertext = ?7, due_date = ?8, updated_at = ?9 WHERE id = ?1",
        params![
            id,
            record.project_id,
            record.item_id,
            record.title,
            record.description,
            record.secrets.map(|s| &s.nonce),
            record.secrets.map(|s| &s.ciphertext),
            record.due_date,
            now
        ],
    )?;
    expect_changed(changed)
}

pub fn set_done(conn: &Connection, id: i64, done: bool, now: i64) -> AppResult<()> {
    let changed = conn.execute(
        "UPDATE todos SET done = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, done, now],
    )?;
    expect_changed(changed)
}

pub fn delete(conn: &Connection, id: i64) -> AppResult<()> {
    expect_changed(conn.execute("DELETE FROM todos WHERE id = ?1", [id])?)
}

/// The template and sealed secrets of a todo description that has any.
pub fn description_secrets(conn: &Connection, id: i64) -> AppResult<(String, SecureBody)> {
    Ok(conn.query_row(
        "SELECT description, nonce, ciphertext FROM todos WHERE id = ?1 AND ciphertext IS NOT NULL",
        [id],
        |row| {
            Ok((
                row.get("description")?,
                SecureBody {
                    nonce: row.get("nonce")?,
                    ciphertext: row.get("ciphertext")?,
                },
            ))
        },
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        items::{self, Body, ItemRecord},
        open_in_memory, projects,
    };

    fn record<'a>(project_id: Option<i64>, title: &'a str, due: Option<&'a str>) -> TodoRecord<'a> {
        TodoRecord {
            project_id,
            item_id: None,
            title,
            description: "",
            secrets: None,
            due_date: due,
        }
    }

    #[test]
    fn lists_by_due_date_and_follows_project_and_note_deletes() {
        let conn = open_in_memory();
        let project = projects::insert(&conn, "Acme", "", 0).unwrap();
        let body = Body::Plain(String::new());
        let note = items::insert(
            &conn,
            &ItemRecord {
                project_id: project,
                folder_id: None,
                title: "Note",
                body: &body,
                tags: &[],
            },
            0,
        )
        .unwrap();

        insert(&conn, &record(None, "Undated", None), 0).unwrap();
        insert(&conn, &record(None, "Later", Some("2026-12-01")), 0).unwrap();
        let linked = TodoRecord {
            item_id: Some(note),
            ..record(Some(project), "Sooner", Some("2026-11-01"))
        };
        let id = insert(&conn, &linked, 0).unwrap();

        let titles: Vec<String> = list(&conn).unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(titles, ["Sooner", "Later", "Undated"]);

        set_done(&conn, id, true, 1).unwrap();
        items::delete(&conn, note).unwrap();
        let todo = get(&conn, id).unwrap();
        assert!(todo.done);
        assert_eq!(todo.item_id, None);

        projects::delete(&conn, project).unwrap();
        assert_eq!(list(&conn).unwrap().len(), 2);
    }
}
