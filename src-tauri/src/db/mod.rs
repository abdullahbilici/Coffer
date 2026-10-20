pub mod folders;
pub mod items;
pub mod projects;

use std::path::Path;

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

/// Schema migrations, applied in order. `PRAGMA user_version` stores how many have run.
/// Never edit a shipped migration; append a new one instead.
const MIGRATIONS: &[&str] = &[
    r#"
    CREATE TABLE projects (
        id         INTEGER PRIMARY KEY,
        name       TEXT    NOT NULL,
        readme     TEXT    NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );

    CREATE TABLE folders (
        id         INTEGER PRIMARY KEY,
        project_id INTEGER NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
        parent_id  INTEGER REFERENCES folders (id) ON DELETE CASCADE,
        name       TEXT    NOT NULL,
        created_at INTEGER NOT NULL
    );

    CREATE TABLE items (
        id         INTEGER PRIMARY KEY,
        project_id INTEGER NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
        folder_id  INTEGER REFERENCES folders (id) ON DELETE CASCADE,
        title      TEXT    NOT NULL,
        secure     INTEGER NOT NULL,
        body       TEXT,
        nonce      BLOB,
        ciphertext BLOB,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );

    CREATE TABLE item_tags (
        item_id INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
        tag     TEXT    NOT NULL,
        PRIMARY KEY (item_id, tag)
    );

    CREATE INDEX folders_project ON folders (project_id);
    CREATE INDEX items_project ON items (project_id);
    CREATE INDEX items_folder ON items (folder_id);
"#,
    // Inline secrets in project overviews, same scheme as items.
    r#"
    ALTER TABLE projects ADD COLUMN readme_nonce BLOB;
    ALTER TABLE projects ADD COLUMN readme_ciphertext BLOB;
"#,
];

pub fn open(path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> AppResult<()> {
    let applied: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(applied as usize) {
        conn.execute_batch(&format!(
            "BEGIN; {sql}; PRAGMA user_version = {}; COMMIT;",
            index + 1
        ))?;
    }
    Ok(())
}

/// An AES-GCM ciphertext and its nonce.
pub struct SecureBody {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

/// Turns an UPDATE that matched no rows into `NotFound`.
fn expect_changed(changed: usize) -> AppResult<()> {
    if changed == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(())
    }
}

#[cfg(test)]
pub fn open_in_memory() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    migrate(&conn).unwrap();
    conn
}
