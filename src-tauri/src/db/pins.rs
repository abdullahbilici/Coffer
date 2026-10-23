//! Pins across projects, folders and items. `pin_order` (NULL = not pinned) is one shared
//! ordering, so the Pinned pane can mix all three kinds in the order the user arranged.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use super::{Change, expect_changed};
use crate::error::AppResult;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PinKind {
    Project,
    Folder,
    Item,
}

impl PinKind {
    fn table(self) -> &'static str {
        match self {
            Self::Project => "projects",
            Self::Folder => "folders",
            Self::Item => "items",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinRef {
    pub kind: PinKind,
    pub id: i64,
}

const ALL_PINS: &str =
    "SELECT 'project' AS kind, id, pin_order FROM projects WHERE pin_order IS NOT NULL
     UNION ALL SELECT 'folder', id, pin_order FROM folders WHERE pin_order IS NOT NULL
     UNION ALL SELECT 'item', id, pin_order FROM items WHERE pin_order IS NOT NULL";

/// Every pin, in the user's order.
pub fn list(conn: &Connection) -> AppResult<Vec<PinRef>> {
    let mut stmt = conn.prepare(&format!("{ALL_PINS} ORDER BY pin_order"))?;
    let rows = stmt.query_map([], |row| {
        let kind = match row.get::<_, String>("kind")?.as_str() {
            "project" => PinKind::Project,
            "folder" => PinKind::Folder,
            "item" => PinKind::Item,
            other => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    format!("unknown pin kind {other:?}").into(),
                ));
            }
        };
        Ok(PinRef {
            kind,
            id: row.get("id")?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Pins at the end of the list; pinning something already pinned keeps its place.
pub fn pin(conn: &Connection, pin: PinRef) -> AppResult<()> {
    let next: i64 = conn.query_row(
        &format!("SELECT COALESCE(MAX(pin_order), 0) + 1 FROM ({ALL_PINS})"),
        [],
        |row| row.get(0),
    )?;
    let table = pin.kind.table();
    let changed = conn.execute(
        &format!("UPDATE {table} SET pin_order = COALESCE(pin_order, ?2) WHERE id = ?1"),
        params![pin.id, next],
    )?;
    expect_changed(changed)
}

pub fn unpin(conn: &Connection, pin: PinRef) -> AppResult<()> {
    let table = pin.kind.table();
    let changed = conn.execute(
        &format!("UPDATE {table} SET pin_order = NULL WHERE id = ?1"),
        [pin.id],
    )?;
    expect_changed(changed)
}

/// Renumbers pins in the given order. Entries that aren't pinned are ignored.
pub fn reorder(conn: &Connection, order: &[PinRef]) -> AppResult<()> {
    let tx = Change::begin(conn)?;
    for (index, pin) in order.iter().enumerate() {
        let table = pin.kind.table();
        tx.execute(
            &format!("UPDATE {table} SET pin_order = ?2 WHERE id = ?1 AND pin_order IS NOT NULL"),
            params![pin.id, index as i64 + 1],
        )?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{folders, open_in_memory, projects};

    #[test]
    fn pins_keep_a_shared_user_order() {
        let conn = open_in_memory();
        let project = projects::insert(&conn, "Acme", "", 0).unwrap();
        let folder = folders::insert(&conn, project, None, "Infra", 0).unwrap();
        let p = PinRef {
            kind: PinKind::Project,
            id: project,
        };
        let f = PinRef {
            kind: PinKind::Folder,
            id: folder,
        };

        pin(&conn, p).unwrap();
        pin(&conn, f).unwrap();
        pin(&conn, p).unwrap(); // already pinned: stays first
        assert_eq!(list(&conn).unwrap(), vec![p, f]);

        reorder(&conn, &[f, p]).unwrap();
        assert_eq!(list(&conn).unwrap(), vec![f, p]);

        unpin(&conn, f).unwrap();
        assert_eq!(list(&conn).unwrap(), vec![p]);
    }
}
