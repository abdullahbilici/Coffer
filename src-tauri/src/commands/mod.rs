//! Tauri command handlers. Keep them thin: validate input, take locks, delegate to `db`.

pub mod assistant;
pub mod backup;
pub mod capture;
pub mod folders;
pub mod items;
pub mod payments;
pub mod pins;
pub mod projects;
pub mod sealing;
pub mod session;
pub mod settings;
pub mod todos;
pub mod vault;

use rusqlite::Connection;

use crate::{
    db,
    error::{AppError, AppResult},
};

const MAX_NAME_LEN: usize = 200;

/// Trims a user-provided name and rejects empty or oversized values.
fn required_name(value: &str, label: &str) -> AppResult<String> {
    let name = value.trim();
    if name.is_empty() {
        return Err(AppError::Invalid(format!("{label} is required")));
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(AppError::Invalid(format!(
            "{label} must be at most {MAX_NAME_LEN} characters"
        )));
    }
    Ok(name.to_string())
}

/// Rejects a folder that doesn't belong to `project_id`. `None` means the project root.
fn ensure_folder_in_project(
    conn: &Connection,
    folder_id: Option<i64>,
    project_id: i64,
) -> AppResult<()> {
    if let Some(folder_id) = folder_id
        && db::folders::project_of(conn, folder_id)? != project_id
    {
        return Err(AppError::Invalid(
            "Folder belongs to another project".to_string(),
        ));
    }
    Ok(())
}

/// Rejects anything but a real calendar day written as `YYYY-MM-DD`.
pub(crate) fn valid_date(value: &str) -> AppResult<&str> {
    let invalid = || AppError::Invalid(format!("Invalid date: {value}"));
    let parts: Vec<&str> = value.split('-').collect();
    let [year, month, day] = parts[..] else {
        return Err(invalid());
    };
    if year.len() != 4 || month.len() != 2 || day.len() != 2 {
        return Err(invalid());
    }
    let number = |s: &str| s.parse::<u32>().map_err(|_| invalid());
    let (year, month, day) = (number(year)?, number(month)?, number(day)?);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err(invalid()),
    };
    if day == 0 || day > days {
        return Err(invalid());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_calendar_dates() {
        for ok in ["2026-01-31", "2024-02-29", "2000-02-29"] {
            assert!(valid_date(ok).is_ok(), "{ok}");
        }
        for bad in [
            "2026-02-29",
            "1900-02-29",
            "2026-13-01",
            "2026-1-01",
            "2026-04-31",
            "x",
        ] {
            assert!(valid_date(bad).is_err(), "{bad}");
        }
    }
}
