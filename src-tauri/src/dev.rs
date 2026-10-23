//! Debug-only switches for unattended development: parallel dev runs and scripted testing.
//! Compiled out of release builds.

use std::{collections::BTreeMap, sync::Mutex};

use zeroize::Zeroizing;

use crate::crypto::VaultKey;

/// `COFFER_DEV_INSECURE=1` skips Touch ID and the Keychain and unlocks with a fixed key, so a dev
/// build can be driven without any prompt.
pub fn insecure() -> bool {
    std::env::var("COFFER_DEV_INSECURE").is_ok_and(|v| v == "1")
}

/// Not secret: it only ever encrypts fake dev data.
pub fn insecure_key() -> VaultKey {
    Zeroizing::new(vec![7; 32])
}

/// `COFFER_DEV_DB` picks the vault file, so dev runs in parallel worktrees never share one.
/// Insecure runs default to their own file: data sealed with the fixed key can't be read with the
/// Keychain key, and the other way round.
pub fn db_file() -> String {
    std::env::var("COFFER_DEV_DB").unwrap_or_else(|_| {
        if insecure() {
            "coffer-dev-insecure.db".to_string()
        } else {
            "coffer-dev.db".to_string()
        }
    })
}

/// Stands in for the Keychain in insecure runs (the backup key, the assistant's API key), so a rebuilt dev binary never
/// waits on a Keychain access prompt. Lost when the app quits.
static SECRETS: Mutex<BTreeMap<String, Zeroizing<Vec<u8>>>> = Mutex::new(BTreeMap::new());

pub fn load_secret(account: &str) -> Option<Zeroizing<Vec<u8>>> {
    SECRETS
        .lock()
        .expect("dev secrets lock poisoned")
        .get(account)
        .cloned()
}

pub fn store_secret(account: &str, bytes: &[u8]) {
    SECRETS
        .lock()
        .expect("dev secrets lock poisoned")
        .insert(account.to_string(), Zeroizing::new(bytes.to_vec()));
}

pub fn delete_secret(account: &str) {
    SECRETS
        .lock()
        .expect("dev secrets lock poisoned")
        .remove(account);
}
