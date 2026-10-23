//! App settings as key/value rows. A missing row means the default, so new settings need no
//! migration. Nothing here is secret: the backup key and the assistant's API key live in the
//! Keychain.

use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::error::{AppError, AppResult};

const UNLOCK_WINDOW: &str = "unlock_window_secs";
const CLIPBOARD_CLEAR: &str = "clipboard_clear_secs";
const BACKUP_CONFIGURED: &str = "backup_configured";
const BACKUP_AUTO: &str = "backup_auto";
const BACKUP_DIR: &str = "backup_dir";
const BACKUP_LAST_AT: &str = "backup_last_at";
const BACKUP_LAST_ERROR: &str = "backup_last_error";
const ASSISTANT_URL: &str = "assistant_url";
const ASSISTANT_MODEL: &str = "assistant_model";
const ASSISTANT_CONTEXT: &str = "assistant_context";
const ASSISTANT_KEY_SET: &str = "assistant_key_set";
const MCP_ACCESS: &str = "mcp_access";
const CAPTURE_SHORTCUT: &str = "capture_shortcut";

/// Stored for an optional duration that is switched off ("until screen locks", "never").
const OFF: &str = "off";

pub const UNLOCK_WINDOW_CHOICES: &[u64] = &[60, 5 * 60, 10 * 60, 30 * 60, 60 * 60];
const DEFAULT_UNLOCK_WINDOW: u64 = 10 * 60;
pub const CLIPBOARD_CLEAR_CHOICES: &[u64] = &[30, 60];
const DEFAULT_CLIPBOARD_CLEAR: u64 = 30;
pub const DEFAULT_ASSISTANT_URL: &str = "http://localhost:11434";
pub const DEFAULT_ASSISTANT_MODEL: &str = "qwen3.8:27b";
/// Option+Shift+Space: free in macOS and the common apps, unlike Option+Space (a no-break space).
pub const DEFAULT_CAPTURE_SHORTCUT: &str = "Alt+Shift+Space";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// `None`: no timer, the window stays open until the screen locks.
    pub unlock_window_secs: Option<u64>,
    /// `None`: copied secrets stay on the clipboard.
    pub clipboard_clear_secs: Option<u64>,
    pub backup: BackupSettings,
    pub assistant: AssistantSettings,
    /// `coffer --mcp` may answer an MCP client. Off by default.
    pub mcp_access: bool,
    /// The global shortcut that opens quick capture, as the global-shortcut plugin parses it.
    /// `None`: switched off.
    pub capture_shortcut: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSettings {
    /// A backup passphrase has been set (its derived key is in the Keychain).
    pub configured: bool,
    pub auto: bool,
    pub dir: Option<String>,
    pub last_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSettings {
    /// Base URL of an Ollama server.
    pub url: String,
    pub model: String,
    /// What the assistant should know: read into every conversation, never edited by it.
    pub context: String,
    /// An API key is stored in the Keychain; the key itself is never read for settings.
    pub has_api_key: bool,
}

impl Settings {
    pub fn unlock_window(&self) -> Option<Duration> {
        self.unlock_window_secs.map(Duration::from_secs)
    }

    pub fn clipboard_clear(&self) -> Option<Duration> {
        self.clipboard_clear_secs.map(Duration::from_secs)
    }
}

impl BackupSettings {
    /// The folder automatic backups go to, if they're on and set up.
    pub fn auto_dir(&self) -> Option<&str> {
        (self.configured && self.auto)
            .then_some(self.dir.as_deref())
            .flatten()
    }
}

fn get(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
            row.get("value")
        })
        .optional()?)
}

fn set(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

fn remove(conn: &Connection, key: &str) -> AppResult<()> {
    conn.execute("DELETE FROM settings WHERE key = ?1", [key])?;
    Ok(())
}

fn get_secs(conn: &Connection, key: &str, default: u64) -> AppResult<Option<u64>> {
    match get(conn, key)?.as_deref() {
        None => Ok(Some(default)),
        Some(OFF) => Ok(None),
        Some(value) => value
            .parse()
            .map(Some)
            .map_err(|_| AppError::Invalid(format!("Corrupted setting {key}"))),
    }
}

fn set_secs(conn: &Connection, key: &str, secs: Option<u64>, choices: &[u64]) -> AppResult<()> {
    match secs {
        None => set(conn, key, OFF),
        Some(secs) if choices.contains(&secs) => set(conn, key, &secs.to_string()),
        Some(_) => Err(AppError::Invalid("Unsupported duration".to_string())),
    }
}

fn get_flag(conn: &Connection, key: &str, default: bool) -> AppResult<bool> {
    Ok(get(conn, key)?.map_or(default, |value| value == "1"))
}

fn set_flag(conn: &Connection, key: &str, on: bool) -> AppResult<()> {
    set(conn, key, if on { "1" } else { "0" })
}

pub fn load(conn: &Connection) -> AppResult<Settings> {
    Ok(Settings {
        unlock_window_secs: get_secs(conn, UNLOCK_WINDOW, DEFAULT_UNLOCK_WINDOW)?,
        clipboard_clear_secs: get_secs(conn, CLIPBOARD_CLEAR, DEFAULT_CLIPBOARD_CLEAR)?,
        backup: BackupSettings {
            configured: get_flag(conn, BACKUP_CONFIGURED, false)?,
            auto: get_flag(conn, BACKUP_AUTO, true)?,
            dir: get(conn, BACKUP_DIR)?,
            last_at: get(conn, BACKUP_LAST_AT)?.and_then(|v| v.parse().ok()),
            last_error: get(conn, BACKUP_LAST_ERROR)?,
        },
        assistant: AssistantSettings {
            url: get(conn, ASSISTANT_URL)?.unwrap_or_else(|| DEFAULT_ASSISTANT_URL.to_string()),
            model: get(conn, ASSISTANT_MODEL)?
                .unwrap_or_else(|| DEFAULT_ASSISTANT_MODEL.to_string()),
            context: get(conn, ASSISTANT_CONTEXT)?.unwrap_or_default(),
            has_api_key: get_flag(conn, ASSISTANT_KEY_SET, false)?,
        },
        mcp_access: mcp_access(conn)?,
        capture_shortcut: capture_shortcut(conn)?,
    })
}

pub fn set_unlock_window(conn: &Connection, secs: Option<u64>) -> AppResult<()> {
    set_secs(conn, UNLOCK_WINDOW, secs, UNLOCK_WINDOW_CHOICES)
}

pub fn set_clipboard_clear(conn: &Connection, secs: Option<u64>) -> AppResult<()> {
    set_secs(conn, CLIPBOARD_CLEAR, secs, CLIPBOARD_CLEAR_CHOICES)
}

pub fn set_backup_configured(conn: &Connection) -> AppResult<()> {
    set_flag(conn, BACKUP_CONFIGURED, true)
}

pub fn set_backup_auto(conn: &Connection, on: bool) -> AppResult<()> {
    set_flag(conn, BACKUP_AUTO, on)
}

pub fn set_backup_dir(conn: &Connection, dir: &str) -> AppResult<()> {
    set(conn, BACKUP_DIR, dir)
}

/// Records the outcome of a backup: success clears the last error, failure keeps the last
/// successful time.
pub fn record_backup(conn: &Connection, result: Result<i64, &str>) -> AppResult<()> {
    match result {
        Ok(at) => {
            set(conn, BACKUP_LAST_AT, &at.to_string())?;
            remove(conn, BACKUP_LAST_ERROR)
        }
        Err(message) => set(conn, BACKUP_LAST_ERROR, message),
    }
}

pub fn set_assistant(conn: &Connection, url: &str, model: &str, context: &str) -> AppResult<()> {
    set(conn, ASSISTANT_URL, url)?;
    set(conn, ASSISTANT_MODEL, model)?;
    set(conn, ASSISTANT_CONTEXT, context)
}

pub fn set_assistant_key_set(conn: &Connection, on: bool) -> AppResult<()> {
    set_flag(conn, ASSISTANT_KEY_SET, on)
}

pub fn mcp_access(conn: &Connection) -> AppResult<bool> {
    get_flag(conn, MCP_ACCESS, false)
}

pub fn set_mcp_access(conn: &Connection, on: bool) -> AppResult<()> {
    set_flag(conn, MCP_ACCESS, on)
}

pub fn capture_shortcut(conn: &Connection) -> AppResult<Option<String>> {
    Ok(match get(conn, CAPTURE_SHORTCUT)? {
        None => Some(DEFAULT_CAPTURE_SHORTCUT.to_string()),
        Some(value) if value == OFF => None,
        Some(value) => Some(value),
    })
}

pub fn set_capture_shortcut(conn: &Connection, shortcut: Option<&str>) -> AppResult<()> {
    set(conn, CAPTURE_SHORTCUT, shortcut.unwrap_or(OFF))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn defaults_until_set() {
        let conn = open_in_memory();
        let settings = load(&conn).unwrap();
        assert_eq!(settings.unlock_window_secs, Some(DEFAULT_UNLOCK_WINDOW));
        assert_eq!(settings.clipboard_clear_secs, Some(DEFAULT_CLIPBOARD_CLEAR));
        assert!(!settings.backup.configured);
        assert!(settings.backup.auto);
        assert_eq!(settings.backup.auto_dir(), None);
        assert_eq!(settings.assistant.url, DEFAULT_ASSISTANT_URL);
        assert_eq!(settings.assistant.model, DEFAULT_ASSISTANT_MODEL);
        assert!(settings.assistant.context.is_empty() && !settings.assistant.has_api_key);
        assert!(!settings.mcp_access);
        assert_eq!(
            settings.capture_shortcut.as_deref(),
            Some(DEFAULT_CAPTURE_SHORTCUT)
        );
    }

    #[test]
    fn capture_shortcut_can_be_changed_or_switched_off() {
        let conn = open_in_memory();
        set_capture_shortcut(&conn, Some("Control+Alt+KeyN")).unwrap();
        assert_eq!(
            capture_shortcut(&conn).unwrap().as_deref(),
            Some("Control+Alt+KeyN")
        );
        set_capture_shortcut(&conn, None).unwrap();
        assert_eq!(capture_shortcut(&conn).unwrap(), None);
    }

    #[test]
    fn stores_choices_and_rejects_others() {
        let conn = open_in_memory();
        set_unlock_window(&conn, None).unwrap();
        set_clipboard_clear(&conn, Some(60)).unwrap();
        assert!(set_unlock_window(&conn, Some(42)).is_err());

        let settings = load(&conn).unwrap();
        assert_eq!(settings.unlock_window(), None);
        assert_eq!(settings.clipboard_clear_secs, Some(60));
    }

    #[test]
    fn backup_status() {
        let conn = open_in_memory();
        set_backup_configured(&conn).unwrap();
        set_backup_dir(&conn, "/tmp/backups").unwrap();
        record_backup(&conn, Err("Disk full")).unwrap();
        record_backup(&conn, Ok(5)).unwrap();
        record_backup(&conn, Err("Disk full")).unwrap();

        let backup = load(&conn).unwrap().backup;
        assert_eq!(backup.auto_dir(), Some("/tmp/backups"));
        assert_eq!(backup.last_at, Some(5));
        assert_eq!(backup.last_error.as_deref(), Some("Disk full"));
    }
}
