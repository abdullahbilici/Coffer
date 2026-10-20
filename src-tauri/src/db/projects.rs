use rusqlite::{Connection, params};
use serde::Serialize;

use super::{SecureBody, expect_changed};
use crate::error::AppResult;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: i64,
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: i64,
    pub name: String,
    /// Markdown overview. With inline secrets this is the `` `!0!` ``-marker template;
    /// the full text goes through `reveal_project_readme`.
    pub readme: String,
    pub has_secrets: bool,
    pub updated_at: i64,
}

pub fn list(conn: &Connection) -> AppResult<Vec<ProjectSummary>> {
    let mut stmt = conn.prepare("SELECT id, name FROM projects ORDER BY name COLLATE NOCASE")?;
    let rows = stmt.query_map([], |row| {
        Ok(ProjectSummary {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Project> {
    Ok(conn.query_row(
        "SELECT id, name, readme, updated_at, readme_ciphertext IS NOT NULL FROM projects WHERE id = ?1",
        [id],
        |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                readme: row.get(2)?,
                has_secrets: row.get(4)?,
                updated_at: row.get(3)?,
            })
        },
    )?)
}

pub fn insert(conn: &Connection, name: &str, readme: &str, now: i64) -> AppResult<i64> {
    conn.execute(
        "INSERT INTO projects (name, readme, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
        params![name, readme, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn rename(conn: &Connection, id: i64, name: &str, now: i64) -> AppResult<()> {
    let changed = conn.execute(
        "UPDATE projects SET name = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, name, now],
    )?;
    expect_changed(changed)
}

/// Stores the overview; `secrets` is `Some` when `readme` is a template with secret markers.
pub fn update_readme(
    conn: &Connection,
    id: i64,
    readme: &str,
    secrets: Option<&SecureBody>,
    now: i64,
) -> AppResult<()> {
    let changed = conn.execute(
        "UPDATE projects SET readme = ?2, readme_nonce = ?3, readme_ciphertext = ?4, updated_at = ?5
         WHERE id = ?1",
        params![
            id,
            readme,
            secrets.map(|s| &s.nonce),
            secrets.map(|s| &s.ciphertext),
            now
        ],
    )?;
    expect_changed(changed)
}

/// The overview template and its sealed secrets.
pub fn readme_secrets(conn: &Connection, id: i64) -> AppResult<(String, SecureBody)> {
    Ok(conn.query_row(
        "SELECT readme, readme_nonce, readme_ciphertext FROM projects
         WHERE id = ?1 AND readme_ciphertext IS NOT NULL",
        [id],
        |row| {
            Ok((
                row.get(0)?,
                SecureBody {
                    nonce: row.get(1)?,
                    ciphertext: row.get(2)?,
                },
            ))
        },
    )?)
}

/// Deletes the project with all of its folders and items.
pub fn delete(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM projects WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn readme_secrets_round_trip_through_storage() {
        let conn = open_in_memory();
        let id = insert(&conn, "Acme", "", 0).unwrap();
        let sealed = SecureBody {
            nonce: vec![1; 12],
            ciphertext: vec![2; 32],
        };

        update_readme(&conn, id, "pass: `!0!`", Some(&sealed), 1).unwrap();
        assert!(get(&conn, id).unwrap().has_secrets);
        let (template, stored) = readme_secrets(&conn, id).unwrap();
        assert_eq!(template, "pass: `!0!`");
        assert_eq!(stored.ciphertext, sealed.ciphertext);

        update_readme(&conn, id, "no secrets", None, 2).unwrap();
        assert!(!get(&conn, id).unwrap().has_secrets);
    }
}
