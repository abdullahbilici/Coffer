use std::collections::HashMap;

use rusqlite::{Connection, params};
use serde::Serialize;

pub use super::SecureBody;
use super::expect_changed;
use crate::{error::AppResult, secrets};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemSummary {
    pub id: i64,
    pub project_id: i64,
    pub folder_id: Option<i64>,
    pub title: String,
    /// The whole body is encrypted.
    pub secure: bool,
    /// A plain body with `` `!...!` `` spans encrypted separately (see `secrets`).
    pub has_secrets: bool,
    pub tags: Vec<String>,
    pub updated_at: i64,
    pub created_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    #[serde(flatten)]
    pub summary: ItemSummary,
    /// `None` for secure items. For items with inline secrets this is the template with
    /// `` `!0!` ``-style markers; the full text goes through `reveal_item`.
    pub body: Option<String>,
}

pub enum Body {
    Plain(String),
    Secure(SecureBody),
    /// Readable template plus the encrypted list of its secret spans.
    Inline {
        template: String,
        secrets: SecureBody,
    },
}

/// What `reveal_item` needs to rebuild the plaintext.
pub struct Encrypted {
    /// `Some` for inline secrets, `None` when the whole body is encrypted.
    pub template: Option<String>,
    pub sealed: SecureBody,
}

pub struct ItemRecord<'a> {
    pub project_id: i64,
    pub folder_id: Option<i64>,
    pub title: &'a str,
    pub body: &'a Body,
    pub tags: &'a [String],
}

/// Read by name in `to_summary`, so adding or reordering columns can't shift values.
const SUMMARY_COLUMNS: &str = "id, project_id, folder_id, title, secure, updated_at, created_at,
     secure = 0 AND ciphertext IS NOT NULL AS has_secrets";

fn to_summary(row: &rusqlite::Row) -> rusqlite::Result<ItemSummary> {
    Ok(ItemSummary {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        folder_id: row.get("folder_id")?,
        title: row.get("title")?,
        secure: row.get("secure")?,
        has_secrets: row.get("has_secrets")?,
        tags: Vec::new(),
        updated_at: row.get("updated_at")?,
        created_at: row.get("created_at")?,
    })
}

pub fn list(conn: &Connection) -> AppResult<Vec<ItemSummary>> {
    let mut tags = all_tags(conn)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {SUMMARY_COLUMNS} FROM items ORDER BY title COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map([], to_summary)?;
    rows.map(|row| {
        let mut item = row?;
        item.tags = tags.remove(&item.id).unwrap_or_default();
        Ok(item)
    })
    .collect()
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Item> {
    let (mut summary, body) = conn.query_row(
        &format!("SELECT {SUMMARY_COLUMNS}, body FROM items WHERE id = ?1"),
        [id],
        |row| Ok((to_summary(row)?, row.get::<_, Option<String>>("body")?)),
    )?;
    summary.tags = tags_of(conn, id)?;
    let body = if summary.secure { None } else { body };
    Ok(Item { summary, body })
}

pub fn summary(conn: &Connection, id: i64) -> AppResult<ItemSummary> {
    Ok(get(conn, id)?.summary)
}

pub fn encrypted(conn: &Connection, id: i64) -> AppResult<Encrypted> {
    Ok(conn.query_row(
        "SELECT secure, body, nonce, ciphertext FROM items WHERE id = ?1 AND ciphertext IS NOT NULL",
        [id],
        |row| {
            let secure: bool = row.get("secure")?;
            Ok(Encrypted {
                template: if secure { None } else { row.get("body")? },
                sealed: SecureBody {
                    nonce: row.get("nonce")?,
                    ciphertext: row.get("ciphertext")?,
                },
            })
        },
    )?)
}

/// (secure, body, nonce, ciphertext)
type BodyColumns<'a> = (bool, Option<&'a str>, Option<&'a [u8]>, Option<&'a [u8]>);

pub(super) fn body_columns(body: &Body) -> BodyColumns<'_> {
    match body {
        Body::Plain(text) => (false, Some(text.as_str()), None, None),
        Body::Secure(s) => (true, None, Some(&s.nonce), Some(&s.ciphertext)),
        Body::Inline { template, secrets } => (
            false,
            Some(template.as_str()),
            Some(&secrets.nonce),
            Some(&secrets.ciphertext),
        ),
    }
}

pub fn insert(conn: &Connection, record: &ItemRecord, now: i64) -> AppResult<i64> {
    let (secure, text, nonce, ciphertext) = body_columns(record.body);
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO items (project_id, folder_id, title, secure, body, nonce, ciphertext, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![record.project_id, record.folder_id, record.title, secure, text, nonce, ciphertext, now],
    )?;
    let id = tx.last_insert_rowid();
    replace_tags(&tx, id, record.tags)?;
    tx.commit()?;
    Ok(id)
}

pub fn update(conn: &Connection, id: i64, record: &ItemRecord, now: i64) -> AppResult<()> {
    let (secure, text, nonce, ciphertext) = body_columns(record.body);
    let tx = conn.unchecked_transaction()?;
    let changed = tx.execute(
        "UPDATE items SET project_id = ?2, folder_id = ?3, title = ?4, secure = ?5, body = ?6,
         nonce = ?7, ciphertext = ?8, updated_at = ?9 WHERE id = ?1",
        params![
            id,
            record.project_id,
            record.folder_id,
            record.title,
            secure,
            text,
            nonce,
            ciphertext,
            now
        ],
    )?;
    expect_changed(changed)?;
    replace_tags(&tx, id, record.tags)?;
    tx.commit()?;
    Ok(())
}

/// Copies an item, its encrypted columns and tags as-is. Sharing the nonce is safe because the
/// plaintext is identical; any later edit re-encrypts with a fresh nonce.
pub fn duplicate(conn: &Connection, id: i64, now: i64) -> AppResult<i64> {
    let tx = conn.unchecked_transaction()?;
    let changed = tx.execute(
        "INSERT INTO items (project_id, folder_id, title, secure, body, nonce, ciphertext, created_at, updated_at)
         SELECT project_id, folder_id, title || ' copy', secure, body, nonce, ciphertext, ?2, ?2
         FROM items WHERE id = ?1",
        params![id, now],
    )?;
    expect_changed(changed)?;
    let copy = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO item_tags (item_id, tag) SELECT ?2, tag FROM item_tags WHERE item_id = ?1",
        params![id, copy],
    )?;
    tx.commit()?;
    Ok(copy)
}

/// Moves an item to another folder or project root without touching its content.
pub fn move_to(
    conn: &Connection,
    id: i64,
    project_id: i64,
    folder_id: Option<i64>,
    now: i64,
) -> AppResult<()> {
    let changed = conn.execute(
        "UPDATE items SET project_id = ?2, folder_id = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, project_id, folder_id, now],
    )?;
    expect_changed(changed)
}

pub fn delete(conn: &Connection, id: i64) -> AppResult<()> {
    expect_changed(conn.execute("DELETE FROM items WHERE id = ?1", [id])?)
}

/// Ids of readable notes whose text contains `query`, case-insensitively. Secure bodies and the
/// secrets inside templates are never searched.
pub fn search_bodies(conn: &Connection, query: &str) -> AppResult<Vec<i64>> {
    let query = query.to_lowercase();
    let mut stmt = conn.prepare("SELECT id, body FROM items WHERE secure = 0 AND body != ''")?;
    let mut rows = stmt.query([])?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next()? {
        let body: String = row.get("body")?;
        if secrets::strip_markers(&body)
            .to_lowercase()
            .contains(&query)
        {
            ids.push(row.get("id")?);
        }
    }
    Ok(ids)
}

fn replace_tags(conn: &Connection, id: i64, tags: &[String]) -> AppResult<()> {
    conn.execute("DELETE FROM item_tags WHERE item_id = ?1", [id])?;
    let mut stmt = conn.prepare("INSERT INTO item_tags (item_id, tag) VALUES (?1, ?2)")?;
    for tag in tags {
        stmt.execute(params![id, tag])?;
    }
    Ok(())
}

fn tags_of(conn: &Connection, id: i64) -> AppResult<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT tag FROM item_tags WHERE item_id = ?1 ORDER BY tag COLLATE NOCASE")?;
    let rows = stmt.query_map([id], |row| row.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub(super) fn all_tags(conn: &Connection) -> AppResult<HashMap<i64, Vec<String>>> {
    let mut stmt =
        conn.prepare("SELECT item_id, tag FROM item_tags ORDER BY tag COLLATE NOCASE")?;
    let mut rows = stmt.query([])?;
    let mut tags: HashMap<i64, Vec<String>> = HashMap::new();
    while let Some(row) = rows.next()? {
        tags.entry(row.get("item_id")?)
            .or_default()
            .push(row.get("tag")?);
    }
    Ok(tags)
}

/// Trims each `/`-separated segment, drops empty segments and duplicates.
/// `" infra / / ssh "` becomes `"infra/ssh"`.
pub fn normalize_tags(raw: &[String]) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in raw {
        let normalized = tag
            .split('/')
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
            .join("/");
        if !normalized.is_empty() && !tags.contains(&normalized) {
            tags.push(normalized);
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{folders, open_in_memory, projects};

    #[test]
    fn normalizes_tags() {
        let raw = [" infra / / ssh ", "infra/ssh", "", "/", "aws"].map(String::from);
        assert_eq!(normalize_tags(&raw), vec!["infra/ssh", "aws"]);
    }

    #[test]
    fn duplicates_items_and_cascades_folder_deletes() {
        let conn = open_in_memory();
        let project = projects::insert(&conn, "Acme", "", 0).unwrap();
        let parent = folders::insert(&conn, project, None, "Infra", 0).unwrap();
        let child = folders::insert(&conn, project, Some(parent), "AWS", 0).unwrap();
        let body = Body::Plain("hello".into());
        let tags = ["aws".to_string()];
        let record = ItemRecord {
            project_id: project,
            folder_id: Some(child),
            title: "Console",
            body: &body,
            tags: &tags,
        };
        insert(&conn, &record, 0).unwrap();

        let copy = duplicate(&conn, list(&conn).unwrap()[0].id, 1).unwrap();
        let copied = get(&conn, copy).unwrap();
        assert_eq!(copied.summary.title, "Console copy");
        assert_eq!(copied.summary.tags, vec!["aws"]);
        assert_eq!(copied.body.as_deref(), Some("hello"));

        // Moving the parent to another project carries the nested folder and item along.
        let other = projects::insert(&conn, "Other", "", 0).unwrap();
        folders::move_to(&conn, parent, other, None).unwrap();
        assert_eq!(
            folders::subtree(&conn, parent).unwrap(),
            vec![parent, child]
        );
        assert_eq!(folders::project_of(&conn, child).unwrap(), other);
        assert!(list(&conn).unwrap().iter().all(|i| i.project_id == other));

        folders::delete(&conn, parent).unwrap();

        assert!(list(&conn).unwrap().is_empty());
        assert!(all_tags(&conn).unwrap().is_empty());
    }

    #[test]
    fn searches_readable_bodies_only() {
        let conn = open_in_memory();
        let project = projects::insert(&conn, "Acme", "", 0).unwrap();
        let bodies = [
            Body::Plain("Host: DB.example.com".into()),
            Body::Inline {
                template: "pass: `!0!`".into(),
                secrets: SecureBody {
                    nonce: vec![0],
                    ciphertext: vec![0],
                },
            },
            Body::Secure(SecureBody {
                nonce: vec![0],
                ciphertext: vec![0],
            }),
        ];
        let ids: Vec<i64> = bodies
            .iter()
            .map(|body| {
                let record = ItemRecord {
                    project_id: project,
                    folder_id: None,
                    title: "Note",
                    body,
                    tags: &[],
                };
                insert(&conn, &record, 0).unwrap()
            })
            .collect();

        assert_eq!(search_bodies(&conn, "db.EXAMPLE").unwrap(), [ids[0]]);
        assert_eq!(search_bodies(&conn, "pass").unwrap(), [ids[1]]);
        assert!(search_bodies(&conn, "0").unwrap().is_empty());
    }
}
