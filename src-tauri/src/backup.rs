//! Encrypted backups: the whole decrypted vault as JSON, sealed with a key derived from a
//! backup passphrase (Argon2id + AES-256-GCM) into one `.coffer` file.
//!
//! The passphrase is independent of the Keychain vault key, so a backup survives losing the
//! Mac. Its derived key (with the salt and parameters) is kept in the Keychain so automatic
//! backups never prompt. Plaintext only ever exists in memory.

use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, Generate, KeyInit, Payload},
};
use argon2::{Algorithm, Argon2, Params, Version};
use objc2_foundation::{NSDate, NSDateFormatter, NSString};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use zeroize::Zeroizing;

use crate::{
    commands::sealing::{open_with, seal_inline_with, seal_with},
    crypto::{self, VaultKey},
    db::{
        items::Body,
        settings,
        vault::{self, FolderRow, ItemRow, PaymentRow, ProjectRow, TodoRow, VaultRows},
    },
    error::{AppError, AppResult},
    state::AppState,
    time::now_millis,
};

/// Emitted (no payload) after every backup attempt, so an open Settings refreshes its status.
pub const BACKUP_CHANGED: &str = "backup-changed";

const MAGIC: &[u8; 8] = b"COFFERBK";
const FILE_VERSION: u8 = 1;
const SALT_LEN: usize = 16;
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;
/// Magic, version, three u32 KDF parameters, salt, nonce. Authenticated as AAD.
const HEADER_LEN: usize = MAGIC.len() + 1 + 12 + SALT_LEN + NONCE_LEN;
/// Bumped when the snapshot JSON changes shape; older readers refuse newer snapshots.
/// 2 added todos and payments (missing in format 1 files, read as none).
const SNAPSHOT_FORMAT: u32 = 2;

/// 64 MiB, 3 passes: about a third of a second on Apple silicon.
const KDF: KdfParams = KdfParams {
    memory_kib: 64 * 1024,
    passes: 3,
    lanes: 1,
};

#[cfg(debug_assertions)]
const KEYCHAIN_ACCOUNT: &str = "backup-key-dev";
#[cfg(not(debug_assertions))]
const KEYCHAIN_ACCOUNT: &str = "backup-key";
#[cfg(debug_assertions)]
const FILE_PREFIX: &str = "Coffer Dev";
#[cfg(not(debug_assertions))]
const FILE_PREFIX: &str = "Coffer";
pub const FILE_EXTENSION: &str = "coffer";

/// Automatic backups wait until changes have settled this long (a drag-and-drop burst, or a
/// command between its unlock check and its write).
const AUTO_DELAY: Duration = Duration::from_secs(2);

const PASSPHRASE_WORDS: usize = 6;
const MIN_PASSPHRASE_LEN: usize = 12;
/// EFF large wordlist (CC BY 3.0, eff.org) minus its four hyphenated words, which would clash
/// with the separator: 7772 words, about 12.9 bits each.
const WORDLIST: &str = include_str!("wordlist.txt");

#[derive(Clone, Copy, Debug, PartialEq)]
struct KdfParams {
    memory_kib: u32,
    passes: u32,
    lanes: u32,
}

/// A key derived from the backup passphrase, with what's needed to derive it again.
struct BackupKey {
    params: KdfParams,
    salt: [u8; SALT_LEN],
    key: Zeroizing<[u8; KEY_LEN]>,
}

impl KdfParams {
    fn to_bytes(self) -> [u8; 12] {
        let mut out = [0; 12];
        out[..4].copy_from_slice(&self.memory_kib.to_le_bytes());
        out[4..8].copy_from_slice(&self.passes.to_le_bytes());
        out[8..].copy_from_slice(&self.lanes.to_le_bytes());
        out
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let word = |i: usize| Some(u32::from_le_bytes(bytes.get(i..i + 4)?.try_into().ok()?));
        Some(Self {
            memory_kib: word(0)?,
            passes: word(4)?,
            lanes: word(8)?,
        })
    }
}

fn crypto_error(message: &str) -> AppError {
    AppError::Crypto(message.to_string())
}

fn random<const N: usize>() -> AppResult<[u8; N]> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| crypto_error("No randomness available"))?;
    Ok(bytes)
}

impl BackupKey {
    fn derive(passphrase: &str, params: KdfParams, salt: [u8; SALT_LEN]) -> AppResult<Self> {
        let argon_params = Params::new(
            params.memory_kib,
            params.passes,
            params.lanes,
            Some(KEY_LEN),
        )
        .map_err(|_| crypto_error("Unsupported backup parameters"))?;
        let mut key = Zeroizing::new([0; KEY_LEN]);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params)
            .hash_password_into(passphrase.as_bytes(), &salt, &mut *key)
            .map_err(|_| crypto_error("Could not derive the backup key"))?;
        Ok(Self { params, salt, key })
    }

    /// Keychain layout: KDF parameters, salt, key.
    fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut out = Zeroizing::new(Vec::with_capacity(12 + SALT_LEN + KEY_LEN));
        out.extend_from_slice(&self.params.to_bytes());
        out.extend_from_slice(&self.salt);
        out.extend_from_slice(&*self.key);
        out
    }

    fn from_bytes(bytes: &[u8]) -> AppResult<Self> {
        let corrupted = || AppError::Keychain("Corrupted backup key".to_string());
        if bytes.len() != 12 + SALT_LEN + KEY_LEN {
            return Err(corrupted());
        }
        let mut key = Zeroizing::new([0; KEY_LEN]);
        key.copy_from_slice(&bytes[12 + SALT_LEN..]);
        Ok(Self {
            params: KdfParams::from_bytes(&bytes[..12]).ok_or_else(corrupted)?,
            salt: bytes[12..12 + SALT_LEN]
                .try_into()
                .map_err(|_| corrupted())?,
            key,
        })
    }

    fn cipher(&self) -> AppResult<Aes256Gcm> {
        Aes256Gcm::new_from_slice(&*self.key).map_err(|_| crypto_error("Invalid backup key"))
    }
}

/// Derives a key from a new passphrase and stores it in the Keychain, replacing any previous
/// one. Slow on purpose (Argon2id): call it off the main thread.
pub fn set_passphrase(passphrase: &str) -> AppResult<()> {
    if passphrase.trim().chars().count() < MIN_PASSPHRASE_LEN {
        return Err(AppError::Invalid(format!(
            "Passphrase must be at least {MIN_PASSPHRASE_LEN} characters"
        )));
    }
    let key = BackupKey::derive(passphrase, KDF, random()?)?;
    crypto::store_secret(KEYCHAIN_ACCOUNT, &key.to_bytes(), "backup key")
}

fn load_key() -> AppResult<BackupKey> {
    let bytes = crypto::load_secret(KEYCHAIN_ACCOUNT, "backup key")?
        .ok_or_else(|| AppError::Keychain("Backup passphrase is not set".to_string()))?;
    BackupKey::from_bytes(&bytes)
}

/// A random word passphrase, e.g. `crane-velvet-outlet-sauna-brisk-ladle`.
pub fn suggest_passphrase() -> AppResult<String> {
    let words: Vec<&str> = WORDLIST.lines().collect();
    let count = words.len() as u32;
    // Rejection sampling: only accept values from whole multiples of `count`, so every word
    // is equally likely.
    let limit = u32::MAX - u32::MAX % count;
    let mut picked = Vec::with_capacity(PASSPHRASE_WORDS);
    while picked.len() < PASSPHRASE_WORDS {
        let value = u32::from_le_bytes(random()?);
        if value < limit {
            picked.push(words[(value % count) as usize]);
        }
    }
    Ok(picked.join("-"))
}

fn seal_file(plaintext: &[u8], key: &BackupKey) -> AppResult<Vec<u8>> {
    let nonce = Nonce::generate();
    let mut out = Vec::with_capacity(HEADER_LEN + plaintext.len() + 16);
    out.extend_from_slice(MAGIC);
    out.push(FILE_VERSION);
    out.extend_from_slice(&key.params.to_bytes());
    out.extend_from_slice(&key.salt);
    out.extend_from_slice(&nonce);
    let ciphertext = key
        .cipher()?
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: &out,
            },
        )
        .map_err(|_| crypto_error("Encryption failed"))?;
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypts a backup file with its passphrase. Slow on purpose: call it off the main thread.
pub fn open_file(file: &[u8], passphrase: &str) -> AppResult<Zeroizing<Vec<u8>>> {
    let not_a_backup = || AppError::Invalid("Not a Coffer backup".to_string());
    if file.len() < HEADER_LEN || &file[..MAGIC.len()] != MAGIC {
        return Err(not_a_backup());
    }
    if file[MAGIC.len()] != FILE_VERSION {
        return Err(AppError::Invalid(
            "This backup needs a newer Coffer".to_string(),
        ));
    }
    let (header, ciphertext) = file.split_at(HEADER_LEN);
    let params_at = MAGIC.len() + 1;
    let salt_at = params_at + 12;
    let nonce_at = salt_at + SALT_LEN;
    let params = KdfParams::from_bytes(&header[params_at..salt_at]).ok_or_else(not_a_backup)?;
    let salt = header[salt_at..nonce_at]
        .try_into()
        .map_err(|_| not_a_backup())?;
    let nonce = Nonce::try_from(&header[nonce_at..]).map_err(|_| not_a_backup())?;

    let key = BackupKey::derive(passphrase, params, salt)?;
    key.cipher()?
        .decrypt(
            &nonce,
            Payload {
                msg: ciphertext,
                aad: header,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| crypto_error("Wrong passphrase or damaged backup"))
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    format: u32,
    projects: Vec<ProjectData>,
    folders: Vec<FolderData>,
    items: Vec<ItemData>,
    #[serde(default)]
    todos: Vec<TodoData>,
    #[serde(default)]
    payments: Vec<PaymentData>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectData {
    id: i64,
    name: String,
    /// The full overview, secrets included.
    readme: Zeroizing<String>,
    pin_order: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FolderData {
    id: i64,
    project_id: i64,
    parent_id: Option<i64>,
    name: String,
    pin_order: Option<i64>,
    created_at: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemData {
    id: i64,
    project_id: i64,
    folder_id: Option<i64>,
    title: String,
    secure: bool,
    /// The full body, secrets included.
    body: Zeroizing<String>,
    tags: Vec<String>,
    pin_order: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TodoData {
    id: i64,
    project_id: Option<i64>,
    item_id: Option<i64>,
    title: String,
    /// The full description, secrets included.
    description: Zeroizing<String>,
    due_date: Option<String>,
    done: bool,
    created_at: i64,
    updated_at: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaymentData {
    id: i64,
    project_id: Option<i64>,
    name: String,
    currency: String,
    cycle: String,
    start_date: String,
    cancel_date: Option<String>,
    prices: Vec<PriceData>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PriceData {
    from: String,
    amount: i64,
}

/// The whole vault, decrypted, as JSON.
fn export(conn: &Connection, key: &VaultKey) -> AppResult<Zeroizing<Vec<u8>>> {
    let rows = vault::dump(conn)?;
    let projects = rows
        .projects
        .into_iter()
        .map(|p| {
            let readme = match &p.readme_secrets {
                Some(sealed) => open_with(key, Some(&p.readme), sealed)?,
                None => Zeroizing::new(p.readme),
            };
            Ok(ProjectData {
                id: p.id,
                name: p.name,
                readme,
                pin_order: p.pin_order,
                created_at: p.created_at,
                updated_at: p.updated_at,
            })
        })
        .collect::<AppResult<_>>()?;
    let folders = rows
        .folders
        .into_iter()
        .map(|f| FolderData {
            id: f.id,
            project_id: f.project_id,
            parent_id: f.parent_id,
            name: f.name,
            pin_order: f.pin_order,
            created_at: f.created_at,
        })
        .collect();
    let items = rows
        .items
        .into_iter()
        .map(|i| {
            let (secure, body) = match i.body {
                Body::Plain(text) => (false, Zeroizing::new(text)),
                Body::Secure(sealed) => (true, open_with(key, None, &sealed)?),
                Body::Inline { template, secrets } => {
                    (false, open_with(key, Some(&template), &secrets)?)
                }
            };
            Ok(ItemData {
                id: i.id,
                project_id: i.project_id,
                folder_id: i.folder_id,
                title: i.title,
                secure,
                body,
                tags: i.tags,
                pin_order: i.pin_order,
                created_at: i.created_at,
                updated_at: i.updated_at,
            })
        })
        .collect::<AppResult<_>>()?;
    let todos = rows
        .todos
        .into_iter()
        .map(|t| {
            let description = match &t.secrets {
                Some(sealed) => open_with(key, Some(&t.description), sealed)?,
                None => Zeroizing::new(t.description),
            };
            Ok(TodoData {
                id: t.id,
                project_id: t.project_id,
                item_id: t.item_id,
                title: t.title,
                description,
                due_date: t.due_date,
                done: t.done,
                created_at: t.created_at,
                updated_at: t.updated_at,
            })
        })
        .collect::<AppResult<_>>()?;
    let payments = rows
        .payments
        .into_iter()
        .map(|p| PaymentData {
            id: p.id,
            project_id: p.project_id,
            name: p.name,
            currency: p.currency,
            cycle: p.cycle,
            start_date: p.start_date,
            cancel_date: p.cancel_date,
            prices: p
                .prices
                .into_iter()
                .map(|(from, amount)| PriceData { from, amount })
                .collect(),
            created_at: p.created_at,
            updated_at: p.updated_at,
        })
        .collect();

    let snapshot = Snapshot {
        format: SNAPSHOT_FORMAT,
        projects,
        folders,
        items,
        todos,
        payments,
    };
    Ok(Zeroizing::new(
        serde_json::to_vec(&snapshot).expect("a snapshot always serializes"),
    ))
}

/// Re-seals a decrypted snapshot with the vault key.
fn import(json: &[u8], key: &VaultKey) -> AppResult<VaultRows> {
    let snapshot: Snapshot =
        serde_json::from_slice(json).map_err(|_| crypto_error("Damaged backup"))?;
    if snapshot.format > SNAPSHOT_FORMAT {
        return Err(AppError::Invalid(
            "This backup needs a newer Coffer".to_string(),
        ));
    }
    let projects = snapshot
        .projects
        .into_iter()
        .map(|p| {
            let (readme, readme_secrets) = match seal_inline_with(key, &p.readme)? {
                Some((template, sealed)) => (template, Some(sealed)),
                None => (p.readme.to_string(), None),
            };
            Ok(ProjectRow {
                id: p.id,
                name: p.name,
                readme,
                readme_secrets,
                pin_order: p.pin_order,
                created_at: p.created_at,
                updated_at: p.updated_at,
            })
        })
        .collect::<AppResult<_>>()?;
    let folders = snapshot
        .folders
        .into_iter()
        .map(|f| FolderRow {
            id: f.id,
            project_id: f.project_id,
            parent_id: f.parent_id,
            name: f.name,
            pin_order: f.pin_order,
            created_at: f.created_at,
        })
        .collect();
    let items = snapshot
        .items
        .into_iter()
        .map(|i| {
            let body = if i.secure {
                Body::Secure(seal_with(key, &i.body)?)
            } else {
                match seal_inline_with(key, &i.body)? {
                    Some((template, secrets)) => Body::Inline { template, secrets },
                    None => Body::Plain(i.body.to_string()),
                }
            };
            Ok(ItemRow {
                id: i.id,
                project_id: i.project_id,
                folder_id: i.folder_id,
                title: i.title,
                body,
                tags: i.tags,
                pin_order: i.pin_order,
                created_at: i.created_at,
                updated_at: i.updated_at,
            })
        })
        .collect::<AppResult<_>>()?;
    let todos = snapshot
        .todos
        .into_iter()
        .map(|t| {
            let (description, secrets) = match seal_inline_with(key, &t.description)? {
                Some((template, sealed)) => (template, Some(sealed)),
                None => (t.description.to_string(), None),
            };
            Ok(TodoRow {
                id: t.id,
                project_id: t.project_id,
                item_id: t.item_id,
                title: t.title,
                description,
                secrets,
                due_date: t.due_date,
                done: t.done,
                created_at: t.created_at,
                updated_at: t.updated_at,
            })
        })
        .collect::<AppResult<_>>()?;
    let payments = snapshot
        .payments
        .into_iter()
        .map(|p| PaymentRow {
            id: p.id,
            project_id: p.project_id,
            name: p.name,
            currency: p.currency,
            cycle: p.cycle,
            start_date: p.start_date,
            cancel_date: p.cancel_date,
            prices: p
                .prices
                .into_iter()
                .map(|pr| (pr.from, pr.amount))
                .collect(),
            created_at: p.created_at,
            updated_at: p.updated_at,
        })
        .collect();
    Ok(VaultRows {
        projects,
        folders,
        items,
        todos,
        payments,
    })
}

/// Replaces the vault with a decrypted backup.
pub fn restore(conn: &Connection, json: &[u8], key: &VaultKey) -> AppResult<()> {
    vault::replace(conn, &import(json, key)?)
}

/// One file per day, in local time; later backups that day replace it.
fn file_name() -> String {
    let formatter = NSDateFormatter::new();
    formatter.setDateFormat(Some(&NSString::from_str("yyyy-MM-dd")));
    let today = formatter.stringFromDate(&NSDate::now());
    format!("{FILE_PREFIX} {today}.{FILE_EXTENSION}")
}

/// Writes next to the target and renames, so a crash never leaves a half-written backup.
fn write_file(dir: &Path, bytes: &[u8]) -> AppResult<PathBuf> {
    fs::create_dir_all(dir)?;
    let name = file_name();
    let target = dir.join(&name);
    let partial = dir.join(format!(".{name}.partial"));
    fs::write(&partial, bytes)?;
    fs::rename(&partial, &target)?;
    Ok(target)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Trigger {
    /// The background worker: only with automatic backups on, once changes have settled.
    Auto,
    /// The unlock window is about to close: write pending changes while the key is here.
    Flush,
    /// "Back up now" or a new passphrase: always, and errors are returned.
    Manual,
}

/// Backs up the vault if `trigger` calls for it. Needs the unlock window open to decrypt.
pub fn run(app: &AppHandle, trigger: Trigger) -> AppResult<()> {
    let state = app.state::<AppState>();
    let Some((json, dir)) = prepare(&state, trigger)? else {
        return Ok(());
    };
    let result = load_key()
        .and_then(|key| seal_file(&json, &key))
        .and_then(|file| write_file(Path::new(&dir), &file));
    drop(json);
    let outcome = match &result {
        Ok(_) => Ok(now_millis()),
        Err(e) => Err(e.to_string()),
    };
    settings::record_backup(
        &state.db(),
        outcome.as_ref().map(|at| *at).map_err(String::as_str),
    )?;
    let _ = app.emit(BACKUP_CHANGED, ());
    result.map(|_| ())
}

/// Under the locks: decides whether to back up and exports the vault if so.
fn prepare(state: &AppState, trigger: Trigger) -> AppResult<Option<(Zeroizing<Vec<u8>>, String)>> {
    let conn = state.db();
    let backup = settings::load(&conn)?.backup;
    let dir = match trigger {
        Trigger::Manual => backup
            .configured
            .then_some(backup.dir)
            .flatten()
            .ok_or_else(|| AppError::Invalid("Set a backup passphrase first".to_string()))?,
        Trigger::Auto | Trigger::Flush => match backup.auto_dir() {
            Some(dir) => dir.to_string(),
            None => return Ok(None),
        },
    };

    let mut session = state.session();
    // Changes made just before a timed window ends are written at once: the worker's next
    // round would find the key gone.
    let settled = match trigger {
        Trigger::Auto if !session.ends_within(2 * AUTO_DELAY) => AUTO_DELAY,
        _ => Duration::ZERO,
    };
    if !session.take_changed(settled) && trigger != Trigger::Manual {
        return Ok(None);
    }
    let key = match session.active_key() {
        Ok(key) => key,
        Err(e) => {
            // Locked before the changes were written out: try again in the next window.
            session.mark_changed();
            return if trigger == Trigger::Manual {
                Err(e)
            } else {
                Ok(None)
            };
        }
    };
    drop(session);
    Ok(Some((export(&conn, &key)?, dir)))
}

/// Polls for settled changes and writes automatic backups.
pub fn spawn_worker(app: AppHandle) {
    thread::spawn(move || {
        loop {
            thread::sleep(AUTO_DELAY);
            // Failures are recorded in the settings and shown in the Backup section.
            let _ = run(&app, Trigger::Auto);
        }
    });
}

#[cfg(test)]
mod tests {
    use aes_gcm::aead::Key;

    use super::*;
    use crate::db::{
        items::{self, ItemRecord},
        open_in_memory,
        payments::{self, PaymentRecord},
        projects,
        todos::{self, TodoRecord},
    };

    /// Cheap parameters so tests stay fast; real backups use `KDF`.
    const TEST_KDF: KdfParams = KdfParams {
        memory_kib: 64,
        passes: 1,
        lanes: 1,
    };

    fn vault_key() -> VaultKey {
        Zeroizing::new(Key::<Aes256Gcm>::generate().to_vec())
    }

    fn sample_vault(key: &VaultKey) -> Connection {
        let conn = open_in_memory();
        let project = projects::insert(&conn, "Acme", "", 1).unwrap();
        let (template, sealed) = seal_inline_with(key, "vpn: `!hunter2!`").unwrap().unwrap();
        projects::update_readme(&conn, project, &template, Some(&sealed), 2).unwrap();
        conn.execute(
            "INSERT INTO folders (id, project_id, parent_id, name, created_at)
             VALUES (2, ?1, 3, 'child', 0), (3, ?1, NULL, 'parent', 0)",
            [project],
        )
        .unwrap();
        let secure = Body::Secure(seal_with(key, "whole secret").unwrap());
        let (template, secrets) = seal_inline_with(key, "token `!abc!`").unwrap().unwrap();
        let inline = Body::Inline { template, secrets };
        let plain = Body::Plain("just text".to_string());
        for (title, body, folder) in [
            ("Secure", &secure, Some(2)),
            ("Inline", &inline, None),
            ("Plain", &plain, Some(3)),
        ] {
            items::insert(
                &conn,
                &ItemRecord {
                    project_id: project,
                    folder_id: folder,
                    title,
                    body,
                    tags: &["env/prod".to_string()],
                },
                3,
            )
            .unwrap();
        }
        let (template, sealed) = seal_inline_with(key, "pin `!4321!`").unwrap().unwrap();
        let secret_todo = TodoRecord {
            project_id: Some(project),
            item_id: Some(2),
            title: "Rotate key",
            description: &template,
            secrets: Some(&sealed),
            due_date: Some("2026-11-01"),
        };
        let general_todo = TodoRecord {
            project_id: None,
            item_id: None,
            title: "Renew passport",
            description: "",
            secrets: None,
            due_date: None,
        };
        todos::insert(&conn, &secret_todo, 4).unwrap();
        todos::insert(&conn, &general_todo, 4).unwrap();
        let payment = PaymentRecord {
            project_id: None,
            name: "Hosting",
            currency: "USD",
            cycle: "monthly",
            start_date: "2026-01-15",
            cancel_date: None,
        };
        let id = payments::insert(&conn, &payment, 1000, 5).unwrap();
        payments::update(&conn, id, &payment, Some(("2026-06-15", 1500)), 6).unwrap();
        conn
    }

    fn backup_file(conn: &Connection, key: &VaultKey, passphrase: &str) -> Vec<u8> {
        let backup_key = BackupKey::derive(passphrase, TEST_KDF, random().unwrap()).unwrap();
        seal_file(&export(conn, key).unwrap(), &backup_key).unwrap()
    }

    #[test]
    fn round_trip_restores_everything_under_a_new_vault_key() {
        let old_key = vault_key();
        let source = sample_vault(&old_key);
        let file = backup_file(&source, &old_key, "correct horse battery");

        let new_key = vault_key();
        let target = open_in_memory();
        projects::insert(&target, "Replaced", "", 0).unwrap();
        let json = open_file(&file, "correct horse battery").unwrap();
        restore(&target, &json, &new_key).unwrap();

        assert_eq!(
            export(&target, &new_key).unwrap(),
            export(&source, &old_key).unwrap()
        );
        let names: Vec<String> = projects::list(&target)
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, vec!["Acme"]);
        // Secrets are sealed again, never stored as plain text.
        let plain = items::get(&target, 2).unwrap().body.unwrap();
        assert_eq!(plain, "token `!0!`");
        let todos = todos::list(&target).unwrap();
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].description, "pin `!0!`");
        assert_eq!(payments::list(&target).unwrap()[0].prices.len(), 2);
    }

    #[test]
    fn wrong_passphrase_fails() {
        let key = vault_key();
        let file = backup_file(&sample_vault(&key), &key, "correct horse battery");
        let error = open_file(&file, "wrong horse battery").unwrap_err();
        assert!(matches!(error, AppError::Crypto(_)));
    }

    #[test]
    fn tampered_header_fails() {
        let key = vault_key();
        let mut file = backup_file(&sample_vault(&key), &key, "correct horse battery");
        file[MAGIC.len() + 1 + 12] ^= 1; // first salt byte
        assert!(open_file(&file, "correct horse battery").is_err());
        assert!(open_file(b"not a backup", "correct horse battery").is_err());
    }

    #[test]
    fn keychain_layout_round_trips() {
        let key = BackupKey::derive("correct horse battery", TEST_KDF, [7; SALT_LEN]).unwrap();
        let back = BackupKey::from_bytes(&key.to_bytes()).unwrap();
        assert_eq!(back.params, TEST_KDF);
        assert_eq!(back.salt, key.salt);
        assert_eq!(*back.key, *key.key);
    }

    #[test]
    fn suggested_passphrases_are_six_words() {
        let passphrase = suggest_passphrase().unwrap();
        assert_eq!(passphrase.split('-').count(), PASSPHRASE_WORDS);
        assert_ne!(passphrase, suggest_passphrase().unwrap());
    }
}
