//! Debug-only switches for unattended development: parallel dev runs and scripted testing.
//! Compiled out of release builds.

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
