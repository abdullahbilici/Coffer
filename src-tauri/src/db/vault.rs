//! The whole vault as stored rows (still sealed), for backups and restores.

use std::collections::HashMap;

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

pub struct TodoRow {
    pub id: i64,
    pub project_id: Option<i64>,
    pub item_id: Option<i64>,
    pub title: String,
    /// With `secrets`, a `` `!0!` ``-marker template.
    pub description: String,
    pub secrets: Option<SecureBody>,
    pub due_date: Option<String>,
    pub done: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct PaymentRow {
    pub id: i64,
    pub project_id: Option<i64>,
    pub name: String,
    pub currency: String,
    pub cycle: String,
    pub start_date: String,
    pub cancel_date: Option<String>,
    /// (effective date, amount), oldest first.
    pub prices: Vec<(String, i64)>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Default)]
pub struct VaultRows {
    pub projects: Vec<ProjectRow>,
    pub folders: Vec<FolderRow>,
    pub items: Vec<ItemRow>,
    pub todos: Vec<TodoRow>,
    pub payments: Vec<PaymentRow>,
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

    let todos = conn
        .prepare(
            "SELECT id, project_id, item_id, title, description, nonce, ciphertext, due_date, done,
             created_at, updated_at FROM todos ORDER BY id",
        )?
        .query_map([], |row| {
            Ok(TodoRow {
                id: row.get("id")?,
                project_id: row.get("project_id")?,
                item_id: row.get("item_id")?,
                title: row.get("title")?,
                description: row.get("description")?,
                secrets: sealed(row.get("nonce")?, row.get("ciphertext")?),
                due_date: row.get("due_date")?,
                done: row.get("done")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
            })
        })?
        .collect::<Result<_, _>>()?;

    let mut prices: HashMap<i64, Vec<(String, i64)>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT payment_id, effective_date, amount FROM payment_prices ORDER BY effective_date",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        prices
            .entry(row.get("payment_id")?)
            .or_default()
            .push((row.get("effective_date")?, row.get("amount")?));
    }
    let payments = conn
        .prepare(
            "SELECT id, project_id, name, currency, cycle, start_date, cancel_date, created_at,
             updated_at FROM payments ORDER BY id",
        )?
        .query_map([], |row| {
            Ok(PaymentRow {
                id: row.get("id")?,
                project_id: row.get("project_id")?,
                name: row.get("name")?,
                currency: row.get("currency")?,
                cycle: row.get("cycle")?,
                start_date: row.get("start_date")?,
                cancel_date: row.get("cancel_date")?,
                prices: Vec::new(),
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
            })
        })?
        .map(|row| {
            let mut payment = row?;
            payment.prices = prices.remove(&payment.id).unwrap_or_default();
            Ok(payment)
        })
        .collect::<Result<_, rusqlite::Error>>()?;

    Ok(VaultRows {
        projects,
        folders,
        items,
        todos,
        payments,
    })
}

/// Replaces every project, folder, item, todo and payment with `rows`, keeping their ids, in one
/// transaction.
pub fn replace(conn: &Connection, rows: &VaultRows) -> AppResult<()> {
    let tx = conn.unchecked_transaction()?;
    // Folders may reference parents inserted after them; check references at commit instead.
    tx.pragma_update(None, "defer_foreign_keys", true)?;
    // General todos and personal payments belong to no project, so the cascade misses them.
    tx.execute("DELETE FROM todos", [])?;
    tx.execute("DELETE FROM payments", [])?;
    // Cascades to folders, items, tags and project todos and payments.
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
    for t in &rows.todos {
        let secrets = t.secrets.as_ref();
        tx.execute(
            "INSERT INTO todos (id, project_id, item_id, title, description, nonce, ciphertext,
             due_date, done, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                t.id,
                t.project_id,
                t.item_id,
                t.title,
                t.description,
                secrets.map(|s| &s.nonce),
                secrets.map(|s| &s.ciphertext),
                t.due_date,
                t.done,
                t.created_at,
                t.updated_at
            ],
        )?;
    }
    let mut price_stmt = tx.prepare(
        "INSERT INTO payment_prices (payment_id, effective_date, amount) VALUES (?1, ?2, ?3)",
    )?;
    for p in &rows.payments {
        tx.execute(
            "INSERT INTO payments (id, project_id, name, currency, cycle, start_date, cancel_date,
             created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                p.id,
                p.project_id,
                p.name,
                p.currency,
                p.cycle,
                p.start_date,
                p.cancel_date,
                p.created_at,
                p.updated_at
            ],
        )?;
        for (from, amount) in &p.prices {
            price_stmt.execute(params![p.id, from, amount])?;
        }
    }
    drop(price_stmt);
    tx.commit()?;
    Ok(())
}
