use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime},
};

use serde::Serialize;

use crate::{
    crypto::VaultKey,
    error::{AppError, AppResult},
    time::to_millis,
};

/// How long secure items stay readable after a successful Touch ID.
const UNLOCK_WINDOW: Duration = Duration::from_secs(10 * 60);

#[derive(Default)]
pub struct Session {
    /// Set after the first successful authentication; never reset while the app runs.
    entered: bool,
    /// The vault key, held only during the unlock window.
    key: Option<VaultKey>,
    expires_at: Option<SystemTime>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    entered: bool,
    /// Unix millis when the unlock window ends, if one is active.
    expires_at: Option<i64>,
}

impl Session {
    pub fn info(&mut self) -> SessionInfo {
        self.clear_if_expired();
        SessionInfo {
            entered: self.entered,
            expires_at: self.expires_at.map(to_millis),
        }
    }

    pub fn require_entered(&self) -> AppResult<()> {
        if self.entered {
            Ok(())
        } else {
            Err(AppError::Locked)
        }
    }

    /// Any change to the vault needs an open unlock window, not just being in the app.
    pub fn require_unlocked(&mut self) -> AppResult<()> {
        self.active_key().map(|_| ())
    }

    /// Returns a copy of the key while the unlock window is open.
    pub fn active_key(&mut self) -> AppResult<VaultKey> {
        self.require_entered()?;
        self.clear_if_expired();
        self.key.clone().ok_or(AppError::Locked)
    }

    pub fn open_window(&mut self, key: VaultKey) -> SessionInfo {
        self.entered = true;
        self.key = Some(key);
        self.expires_at = Some(SystemTime::now() + UNLOCK_WINDOW);
        self.info()
    }

    pub fn close_window(&mut self) -> SessionInfo {
        self.key = None;
        self.expires_at = None;
        self.info()
    }

    fn clear_if_expired(&mut self) {
        if self.expires_at.is_some_and(|t| SystemTime::now() >= t) {
            self.close_window();
        }
    }
}

/// Drops the key from memory when the window ends, even if nothing touches the session.
pub fn schedule_expiry(session: Arc<Mutex<Session>>) {
    thread::spawn(move || {
        thread::sleep(UNLOCK_WINDOW);
        session
            .lock()
            .expect("session lock poisoned")
            .clear_if_expired();
    });
}

#[cfg(test)]
mod tests {
    use zeroize::Zeroizing;

    use super::*;

    #[test]
    fn edits_need_an_open_window_not_just_entry() {
        let mut session = Session::default();
        assert!(matches!(session.require_unlocked(), Err(AppError::Locked)));

        session.open_window(Zeroizing::new(vec![0; 32]));
        assert!(session.require_unlocked().is_ok());

        // Window closed (expiry, screen lock, Lock button): still in the app, but read-only.
        session.close_window();
        assert!(session.require_entered().is_ok());
        assert!(matches!(session.require_unlocked(), Err(AppError::Locked)));
    }
}
