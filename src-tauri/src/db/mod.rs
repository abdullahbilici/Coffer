pub mod folders;
pub mod items;
pub mod pins;
pub mod projects;
pub mod settings;
pub mod vault;

use std::path::Path;

use rusqlite::Connection;

use crate::{
    error::{AppError, AppResult},
    secrets,
};

enum Migration {
    Sql(&'static str),
    /// Data changes SQL can't express. Runs inside the migration's transaction.
    Code(fn(&Connection) -> AppResult<()>),
}

/// Schema migrations, applied in order. `PRAGMA user_version` stores how many have run.
/// Never edit a shipped migration; append a new one instead.
const MIGRATIONS: &[Migration] = &[
    Migration::Sql(
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
    ),
    // Inline secrets in project overviews, same scheme as items.
    Migration::Sql(
        r#"
    ALTER TABLE projects ADD COLUMN readme_nonce BLOB;
    ALTER TABLE projects ADD COLUMN readme_ciphertext BLOB;
"#,
    ),
    // Secret markers moved from `{{n}}` to `` `!n!` ``.
    Migration::Code(upgrade_secret_markers),
    // Pins: one shared order across projects, folders and items (NULL = not pinned).
    Migration::Sql(
        r#"
    ALTER TABLE projects ADD COLUMN pin_order INTEGER;
    ALTER TABLE folders ADD COLUMN pin_order INTEGER;
    ALTER TABLE items ADD COLUMN pin_order INTEGER;
"#,
    ),
    // App settings as key/value rows; a missing key means the default (see `settings`).
    Migration::Sql(
        r#"
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
"#,
    ),
];

fn upgrade_secret_markers(conn: &Connection) -> AppResult<()> {
    let tables = [
        ("items", "body", "secure = 0 AND ciphertext IS NOT NULL"),
        ("projects", "readme", "readme_ciphertext IS NOT NULL"),
    ];
    for (table, column, filter) in tables {
        let rows: Vec<(i64, String)> = conn
            .prepare(&format!("SELECT id, {column} FROM {table} WHERE {filter}"))?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        for (id, template) in rows {
            conn.execute(
                &format!("UPDATE {table} SET {column} = ?2 WHERE id = ?1"),
                rusqlite::params![id, secrets::upgrade_legacy_markers(&template)],
            )?;
        }
    }
    Ok(())
}

pub fn open(path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> AppResult<()> {
    migrate_to(conn, MIGRATIONS.len())
}

/// Applies pending migrations up to (not including) index `target`.
fn migrate_to(conn: &Connection, target: usize) -> AppResult<()> {
    let applied: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let pending = MIGRATIONS
        .iter()
        .enumerate()
        .take(target)
        .skip(applied as usize);
    for (index, migration) in pending {
        let tx = conn.unchecked_transaction()?;
        match migration {
            Migration::Sql(sql) => tx.execute_batch(sql)?,
            Migration::Code(run) => run(&tx)?,
        }
        tx.pragma_update(None, "user_version", (index + 1) as i64)?;
        tx.commit()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_3_upgrades_legacy_secret_markers() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_to(&conn, 2).unwrap();
        conn.execute_batch(
            "INSERT INTO projects (id, name, readme, readme_ciphertext, created_at, updated_at)
             VALUES (1, 'Acme', 'vpn: {{0}}', x'00', 0, 0);
             INSERT INTO items (project_id, title, secure, body, ciphertext, created_at, updated_at)
             VALUES (1, 'Login', 0, 'pass: {{0}} {{1}}', x'00', 0, 0),
                    (1, 'Plain', 0, 'literal {{0}}', NULL, 0, 0);",
        )
        .unwrap();

        migrate(&conn).unwrap();

        let readme: String = conn
            .query_row("SELECT readme FROM projects", [], |r| r.get(0))
            .unwrap();
        let bodies: Vec<String> = conn
            .prepare("SELECT body FROM items ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(readme, "vpn: `!0!`");
        assert_eq!(bodies, vec!["pass: `!0!` `!1!`", "literal {{0}}"]);
    }

    #[test]
    fn migration_5_adds_settings_with_defaults() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_to(&conn, 4).unwrap();
        conn.execute_batch(
            "INSERT INTO projects (id, name, created_at, updated_at, pin_order)
             VALUES (1, 'Acme', 0, 0, 0);",
        )
        .unwrap();

        migrate(&conn).unwrap();

        let name: String = conn
            .query_row("SELECT name FROM projects WHERE pin_order = 0", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(name, "Acme");
        let loaded = settings::load(&conn).unwrap();
        assert_eq!(loaded.unlock_window_secs, Some(600));
        assert!(!loaded.backup.configured);
    }
}
