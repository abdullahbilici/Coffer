use std::{fs, path::Path};

use tauri::{AppHandle, Manager, State};
use zeroize::Zeroizing;

use super::session::{authenticate, open_window_after_auth};
use crate::{
    backup::{self, Trigger},
    db::{
        settings::{self, Settings},
        vault,
    },
    error::{AppError, AppResult},
    state::AppState,
};

/// Runs slow work (Argon2id, Keychain, file IO) off the main thread.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))?
}

fn required_dir(dir: &str) -> AppResult<&str> {
    if Path::new(dir).is_absolute() {
        Ok(dir)
    } else {
        Err(AppError::Invalid("Choose a backup folder".to_string()))
    }
}

/// Whether there's nothing to lose by restoring. Asked before entering, so the launch screen
/// can offer a restore; reveals nothing but emptiness.
#[tauri::command]
pub fn vault_is_empty(state: State<AppState>) -> AppResult<bool> {
    vault::is_empty(&state.db())
}

/// iCloud Drive's Coffer folder when iCloud Drive is on, so backups leave the Mac; otherwise
/// Documents.
#[tauri::command]
pub fn default_backup_dir(app: AppHandle) -> AppResult<String> {
    let home = app
        .path()
        .home_dir()
        .map_err(|e| AppError::File(e.to_string()))?;
    let icloud = home.join("Library/Mobile Documents/com~apple~CloudDocs");
    let base = if icloud.is_dir() {
        icloud
    } else {
        home.join("Documents")
    };
    Ok(base.join("Coffer").to_string_lossy().into_owned())
}

#[tauri::command]
pub fn suggest_passphrase() -> AppResult<String> {
    backup::suggest_passphrase()
}

/// Sets up backups or changes the passphrase. Always asks for Touch ID, never for the old
/// passphrase (forgetting it is the main reason to change it), then writes a fresh backup.
#[tauri::command]
pub async fn set_backup_passphrase(
    app: AppHandle,
    state: State<'_, AppState>,
    passphrase: String,
    dir: String,
) -> AppResult<Settings> {
    let passphrase = Zeroizing::new(passphrase);
    required_dir(&dir)?;
    authenticate("set your backup passphrase").await?;
    if !state.session().is_unlocked() {
        open_window_after_auth(&app, &state)?;
    }
    blocking(move || backup::set_passphrase(&passphrase)).await?;
    {
        let conn = state.db();
        settings::set_backup_dir(&conn, &dir)?;
        settings::set_backup_configured(&conn)?;
    }
    back_up_now(app, state).await
}

#[tauri::command]
pub async fn back_up_now(app: AppHandle, state: State<'_, AppState>) -> AppResult<Settings> {
    state.session().require_unlocked()?;
    let handle = app.clone();
    blocking(move || backup::run(&handle, Trigger::Manual)).await?;
    settings::load(&app.state::<AppState>().db())
}

#[tauri::command]
pub fn set_backup_dir(state: State<AppState>, dir: String) -> AppResult<Settings> {
    let conn = state.db();
    // Marks the vault changed, so the next automatic backup lands in the new folder.
    state.session().require_unlocked()?;
    settings::set_backup_dir(&conn, required_dir(&dir)?)?;
    settings::load(&conn)
}

#[tauri::command]
pub fn set_backup_auto(state: State<AppState>, on: bool) -> AppResult<Settings> {
    let conn = state.db();
    state.session().require_unlocked()?;
    settings::set_backup_auto(&conn, on)?;
    settings::load(&conn)
}

/// Replaces the vault with a backup. The passphrase is checked first, so a wrong one never
/// asks for Touch ID; on the launch screen the restore itself opens the unlock window.
#[tauri::command]
pub async fn restore_backup(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    passphrase: String,
) -> AppResult<()> {
    let passphrase = Zeroizing::new(passphrase);
    let json = blocking(move || backup::open_file(&fs::read(path)?, &passphrase)).await?;
    if !state.session().is_unlocked() {
        authenticate("restore your Coffer").await?;
        open_window_after_auth(&app, &state)?;
    }
    let conn = state.db();
    let key = {
        let mut session = state.session();
        session.require_unlocked()?;
        session.active_key()?
    };
    backup::restore(&conn, &json, &key)
}
