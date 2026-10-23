use std::collections::HashMap;

use rusqlite::{Connection, params};
use serde::Serialize;

use super::{Change, expect_changed};
use crate::error::{AppError, AppResult};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Price {
    /// `YYYY-MM-DD`: the first due date this amount applies to.
    pub from: String,
    /// Integer hundredths of `currency`.
    pub amount: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Payment {
    pub id: i64,
    /// `None` for a personal payment.
    pub project_id: Option<i64>,
    pub name: String,
    /// ISO 4217 code, e.g. `USD`.
    pub currency: String,
    /// `monthly`, `yearly` or `once`.
    pub cycle: String,
    /// `YYYY-MM-DD`: the first due date.
    pub start_date: String,
    /// `YYYY-MM-DD`: nothing is due on or after this day.
    pub cancel_date: Option<String>,
    /// Oldest first; never empty.
    pub prices: Vec<Price>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct PaymentRecord<'a> {
    pub project_id: Option<i64>,
    pub name: &'a str,
    pub currency: &'a str,
    pub cycle: &'a str,
    pub start_date: &'a str,
    pub cancel_date: Option<&'a str>,
}

pub fn list(conn: &Connection) -> AppResult<Vec<Payment>> {
    let mut prices = all_prices(conn)?;
    let mut stmt = conn.prepare(
        "SELECT id, project_id, name, currency, cycle, start_date, cancel_date, created_at,
         updated_at FROM payments ORDER BY name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Payment {
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
    })?;
    rows.map(|row| {
        let mut payment = row?;
        payment.prices = prices.remove(&payment.id).unwrap_or_default();
        Ok(payment)
    })
    .collect()
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Payment> {
    list(conn)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or(AppError::NotFound)
}

/// Creates a payment whose first price applies from its start date.
pub fn insert(conn: &Connection, record: &PaymentRecord, amount: i64, now: i64) -> AppResult<i64> {
    let tx = Change::begin(conn)?;
    tx.execute(
        "INSERT INTO payments (project_id, name, currency, cycle, start_date, cancel_date,
         created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![
            record.project_id,
            record.name,
            record.currency,
            record.cycle,
            record.start_date,
            record.cancel_date,
            now
        ],
    )?;
    let id = tx.last_insert_rowid();
    set_price(&tx, id, record.start_date, amount)?;
    tx.commit()?;
    Ok(id)
}

/// Updates a payment; `price` (from, amount) records a price change at the same time.
pub fn update(
    conn: &Connection,
    id: i64,
    record: &PaymentRecord,
    price: Option<(&str, i64)>,
    now: i64,
) -> AppResult<()> {
    let tx = Change::begin(conn)?;
    let changed = tx.execute(
        "UPDATE payments SET project_id = ?2, name = ?3, currency = ?4, cycle = ?5,
         start_date = ?6, cancel_date = ?7, updated_at = ?8 WHERE id = ?1",
        params![
            id,
            record.project_id,
            record.name,
            record.currency,
            record.cycle,
            record.start_date,
            record.cancel_date,
            now
        ],
    )?;
    expect_changed(changed)?;
    if let Some((from, amount)) = price {
        set_price(&tx, id, from, amount)?;
    }
    tx.commit()?;
    Ok(())
}

/// Sets the amount due from `from` on. Later prices are dropped, so the new amount holds until
/// the next change; earlier periods keep their old amounts.
fn set_price(conn: &Connection, id: i64, from: &str, amount: i64) -> AppResult<()> {
    conn.execute(
        "DELETE FROM payment_prices WHERE payment_id = ?1 AND effective_date >= ?2",
        params![id, from],
    )?;
    conn.execute(
        "INSERT INTO payment_prices (payment_id, effective_date, amount) VALUES (?1, ?2, ?3)",
        params![id, from, amount],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> AppResult<()> {
    expect_changed(conn.execute("DELETE FROM payments WHERE id = ?1", [id])?)
}

fn all_prices(conn: &Connection) -> AppResult<HashMap<i64, Vec<Price>>> {
    let mut stmt = conn.prepare(
        "SELECT payment_id, effective_date, amount FROM payment_prices ORDER BY effective_date",
    )?;
    let mut rows = stmt.query([])?;
    let mut prices: HashMap<i64, Vec<Price>> = HashMap::new();
    while let Some(row) = rows.next()? {
        prices
            .entry(row.get("payment_id")?)
            .or_default()
            .push(Price {
                from: row.get("effective_date")?,
                amount: row.get("amount")?,
            });
    }
    Ok(prices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, projects};

    #[test]
    fn price_changes_keep_history() {
        let conn = open_in_memory();
        let project = projects::insert(&conn, "Acme", "", 0).unwrap();
        let record = PaymentRecord {
            project_id: Some(project),
            name: "Hosting",
            currency: "USD",
            cycle: "monthly",
            start_date: "2026-01-15",
            cancel_date: None,
        };
        let id = insert(&conn, &record, 1000, 0).unwrap();
        update(&conn, id, &record, Some(("2026-06-15", 1500)), 1).unwrap();
        set_price(&conn, id, "2026-09-15", 2000).unwrap();
        // Correcting an earlier change drops the ones after it.
        set_price(&conn, id, "2026-08-15", 1800).unwrap();

        let prices: Vec<(String, i64)> = get(&conn, id)
            .unwrap()
            .prices
            .into_iter()
            .map(|p| (p.from, p.amount))
            .collect();
        assert_eq!(
            prices,
            [
                ("2026-01-15".to_string(), 1000),
                ("2026-06-15".to_string(), 1500),
                ("2026-08-15".to_string(), 1800),
            ]
        );

        projects::delete(&conn, project).unwrap();
        assert!(list(&conn).unwrap().is_empty());
        let orphans: i64 = conn
            .query_row("SELECT COUNT(*) FROM payment_prices", [], |r| r.get(0))
            .unwrap();
        assert_eq!(orphans, 0);
    }
}
