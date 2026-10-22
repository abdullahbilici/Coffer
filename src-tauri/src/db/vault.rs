//! The whole vault as stored rows (still sealed), for backups and restores.

use rusqlite::{Connection, params};

use super::{
    SecureBody,
    items::{Body, all_tags, body_columns},
};
use crate::error::{AppError, AppResult};

pub struct ProjectRow {
    pub id: i64,
    pub name: String,
    /// With `readme_secrets`, a `` `!0!` ``-marker template.
    pub readme: String,
    pub readme_secrets: Option<SecureBody>,
    pub pin_order: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct FolderRow {
    pub id: i64,
    pub project_id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub pin_order: Option<i64>,
    pub created_at: i64,
}

pub struct ItemRow {
    pub id: i64,
    pub project_id: i64,
    pub folder_id: Option<i64>,
    pub title: String,
    pub body: Body,
    pub tags: Vec<String>,
    pub pin_order: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Default)]
pub struct VaultRows {
    pub projects: Vec<ProjectRow>,
    pub folders: Vec<FolderRow>,
    pub items: Vec<ItemRow>,
}

fn sealed(nonce: Option<Vec<u8>>, ciphertext: Option<Vec<u8>>) -> Option<SecureBody> {
    Some(SecureBody {
        nonce: nonce?,
        ciphertext: ciphertext?,
    })
}

fn corrupted() -> AppError {
    AppError::Crypto("Corrupted note".to_string())
}

pub fn is_empty(conn: &Connection) -> AppResult<bool> {
    Ok(conn.query_row(
        "SELECT NOT EXISTS (SELECT 1 FROM projects) AS empty",
        [],
        |row| row.get("empty"),
    )?)
}

pub fn dump(conn: &Connection) -> AppResult<VaultRows> {
    let projects = conn
        .prepare(
            "SELECT id, name, readme, readme_nonce, readme_ciphertext, pin_order, created_at,
             updated_at FROM projects ORDER BY id",
        )?
        .query_map([], |row| {
            Ok(ProjectRow {
                id: row.get("id")?,
                name: row.get("name")?,
                readme: row.get("readme")?,
                readme_secrets: sealed(row.get("readme_nonce")?, row.get("readme_ciphertext")?),
                pin_order: row.get("pin_order")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
            })
        })?
        .collect::<Result<_, _>>()?;

    let folders = conn
        .prepare(
            "SELECT id, project_id, parent_id, name, pin_order, created_at
             FROM folders ORDER BY id",
        )?
        .query_map([], |row| {
            Ok(FolderRow {
                id: row.get("id")?,
                project_id: row.get("project_id")?,
                parent_id: row.get("parent_id")?,
                name: row.get("name")?,
                pin_order: row.get("pin_order")?,
                created_at: row.get("created_at")?,
            })
        })?
        .collect::<Result<_, _>>()?;

    let mut tags = all_tags(conn)?;
    let mut stmt = conn.prepare(
        "SELECT id, project_id, folder_id, title, secure, body, nonce, ciphertext, pin_order,
         created_at, updated_at FROM items ORDER BY id",
    )?;
    let mut rows = stmt.query([])?;
    let mut items = Vec::new();
    while let Some(row) = rows.next()? {
        let id: i64 = row.get("id")?;
        let secure: bool = row.get("secure")?;
        let text: Option<String> = row.get("body")?;
        let secrets = sealed(row.get("nonce")?, row.get("ciphertext")?);
        let body = match (secure, text, secrets) {
            (true, _, Some(sealed)) => Body::Secure(sealed),
            (false, Some(template), Some(secrets)) => Body::Inline { template, secrets },
            (false, Some(text), None) => Body::Plain(text),
            _ => return Err(corrupted()),
        };
        items.push(ItemRow {
            id,
            project_id: row.get("project_id")?,
            folder_id: row.get("folder_id")?,
            title: row.get("title")?,
            body,
            tags: tags.remove(&id).unwrap_or_default(),
            pin_order: row.get("pin_order")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        });
    }

    Ok(VaultRows {
        projects,
        folders,
        items,
    })
}

/// Replaces every project, folder and item with `rows`, keeping their ids, in one transaction.
pub fn replace(conn: &Connection, rows: &VaultRows) -> AppResult<()> {
    let tx = conn.unchecked_transaction()?;
    // Folders may reference parents inserted after them; check references at commit instead.
    tx.pragma_update(None, "defer_foreign_keys", true)?;
    // Cascades to folders, items and tags.
    tx.execute("DELETE FROM projects", [])?;

    for p in &rows.projects {
        let secrets = p.readme_secrets.as_ref();
        tx.execute(
            "INSERT INTO projects (id, name, readme, readme_nonce, readme_ciphertext, pin_order,
             created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                p.id,
                p.name,
                p.readme,
                secrets.map(|s| &s.nonce),
                secrets.map(|s| &s.ciphertext),
                p.pin_order,
                p.created_at,
                p.updated_at
            ],
        )?;
    }
    for f in &rows.folders {
        tx.execute(
            "INSERT INTO folders (id, project_id, parent_id, name, pin_order, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                f.id,
                f.project_id,
                f.parent_id,
                f.name,
                f.pin_order,
                f.created_at
            ],
        )?;
    }
    let mut tag_stmt = tx.prepare("INSERT INTO item_tags (item_id, tag) VALUES (?1, ?2)")?;
    for i in &rows.items {
        let (secure, text, nonce, ciphertext) = body_columns(&i.body);
        tx.execute(
            "INSERT INTO items (id, project_id, folder_id, title, secure, body, nonce, ciphertext,
             pin_order, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                i.id,
                i.project_id,
                i.folder_id,
                i.title,
                secure,
                text,
                nonce,
                ciphertext,
                i.pin_order,
                i.created_at,
                i.updated_at
            ],
        )?;
        for tag in &i.tags {
            tag_stmt.execute(params![i.id, tag])?;
        }
    }
    drop(tag_stmt);
    tx.commit()?;
    Ok(())
}
