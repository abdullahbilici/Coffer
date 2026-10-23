//! Assistant conversations and their messages, stored sealed. Callers seal and open.

use rusqlite::{Connection, params};

use super::{Change, SecureBody, expect_changed};
use crate::error::AppResult;

pub struct ConversationRow {
    pub id: i64,
    pub title: SecureBody,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct MessageRow {
    pub id: i64,
    /// A sealed JSON turn.
    pub turn: SecureBody,
    pub created_at: i64,
}

/// Most recently active first.
pub fn list(conn: &Connection) -> AppResult<Vec<ConversationRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, title_nonce, title_ciphertext, created_at, updated_at FROM conversations
         ORDER BY updated_at DESC, id DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ConversationRow {
            id: row.get("id")?,
            title: SecureBody {
                nonce: row.get("title_nonce")?,
                ciphertext: row.get("title_ciphertext")?,
            },
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn insert(conn: &Connection, title: &SecureBody, now: i64) -> AppResult<i64> {
    conn.execute(
        "INSERT INTO conversations (title_nonce, title_ciphertext, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?3)",
        params![title.nonce, title.ciphertext, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn rename(conn: &Connection, id: i64, title: &SecureBody) -> AppResult<()> {
    let changed = conn.execute(
        "UPDATE conversations SET title_nonce = ?2, title_ciphertext = ?3 WHERE id = ?1",
        params![id, title.nonce, title.ciphertext],
    )?;
    expect_changed(changed)
}

pub fn delete(conn: &Connection, id: i64) -> AppResult<()> {
    expect_changed(conn.execute("DELETE FROM conversations WHERE id = ?1", [id])?)
}

/// In the order they were added.
pub fn messages(conn: &Connection, conversation_id: i64) -> AppResult<Vec<MessageRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, nonce, ciphertext, created_at FROM conversation_messages
         WHERE conversation_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([conversation_id], |row| {
        Ok(MessageRow {
            id: row.get("id")?,
            turn: SecureBody {
                nonce: row.get("nonce")?,
                ciphertext: row.get("ciphertext")?,
            },
            created_at: row.get("created_at")?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Replaces a stored message, e.g. a draft once it is applied or discarded.
pub fn update_message(conn: &Connection, id: i64, turn: &SecureBody) -> AppResult<()> {
    expect_changed(conn.execute(
        "UPDATE conversation_messages SET nonce = ?2, ciphertext = ?3 WHERE id = ?1",
        params![id, turn.nonce, turn.ciphertext],
    )?)
}

/// Appends a message and marks the conversation active.
pub fn insert_message(
    conn: &Connection,
    conversation_id: i64,
    turn: &SecureBody,
    now: i64,
) -> AppResult<i64> {
    let tx = Change::begin(conn)?;
    let changed = tx.execute(
        "UPDATE conversations SET updated_at = ?2 WHERE id = ?1",
        params![conversation_id, now],
    )?;
    expect_changed(changed)?;
    tx.execute(
        "INSERT INTO conversation_messages (conversation_id, nonce, ciphertext, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![conversation_id, turn.nonce, turn.ciphertext, now],
    )?;
    let id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(id)
}
