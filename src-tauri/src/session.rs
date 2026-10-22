use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime},
};

use serde::Serialize;

use crate::{
    crypto::VaultKey,
    error::{AppError, AppResult},
    time::to_millis,
};

/// How long the unlock window stays open, from the settings. `None` means until the screen
/// locks or the Mac sleeps (`lock_events`), with no timer.
pub type WindowLength = Option<Duration>;

#[derive(Default)]
pub struct Session {
    /// Set after the first successful authentication; never reset while the app runs.
    entered: bool,
    /// The vault key, held only during the unlock window.
    key: Option<VaultKey>,
    expires_at: Option<SystemTime>,
    /// When the vault last changed since the last backup (see `backup`).
    changed_at: Option<Instant>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    entered: bool,
    unlocked: bool,
    /// Unix millis when the unlock window ends; `None` while locked or with no timer.
    expires_at: Option<i64>,
}

impl Session {
    pub fn info(&mut self) -> SessionInfo {
        self.clear_if_expired();
        SessionInfo {
            entered: self.entered,
            unlocked: self.key.is_some(),
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
    /// Every change passes through here, so it also marks the vault for the next backup.
    pub fn require_unlocked(&mut self) -> AppResult<()> {
        self.active_key()?;
        self.mark_changed();
        Ok(())
    }

    pub fn is_unlocked(&mut self) -> bool {
        self.clear_if_expired();
        self.key.is_some()
    }

    /// Whether the vault changed and has been quiet for at least `settled`; resets the mark
    /// when it returns true.
    pub fn take_changed(&mut self, settled: Duration) -> bool {
        let ready = self.changed_at.is_some_and(|at| at.elapsed() >= settled);
        if ready {
            self.changed_at = None;
        }
        ready
    }

    pub fn mark_changed(&mut self) {
        self.changed_at = Some(Instant::now());
    }

    /// Returns a copy of the key while the unlock window is open.
    pub fn active_key(&mut self) -> AppResult<VaultKey> {
        self.require_entered()?;
        self.clear_if_expired();
        self.key.clone().ok_or(AppError::Locked)
    }

    pub fn open_window(&mut self, key: VaultKey, length: WindowLength) -> SessionInfo {
        self.entered = true;
        self.key = Some(key);
        self.expires_at = length.map(|length| SystemTime::now() + length);
        self.info()
    }

    /// Starts the open window over with a new length, e.g. after the setting changed.
    pub fn restart_window(&mut self, length: WindowLength) -> AppResult<SessionInfo> {
        let key = self.active_key()?;
        Ok(self.open_window(key, length))
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
/// A window restarted meanwhile is unaffected: the check uses its current end.
pub fn schedule_expiry(session: Arc<Mutex<Session>>, length: WindowLength) {
    let Some(length) = length else { return };
    thread::spawn(move || {
        thread::sleep(length);
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

        session.open_window(Zeroizing::new(vec![0; 32]), Some(Duration::from_secs(60)));
        assert!(session.require_unlocked().is_ok());
        assert!(!session.take_changed(Duration::from_secs(60)));
        assert!(session.take_changed(Duration::ZERO));
        assert!(!session.take_changed(Duration::ZERO));

        // Window closed (expiry, screen lock, Lock button): still in the app, but read-only.
        session.close_window();
        assert!(session.require_entered().is_ok());
        assert!(matches!(session.require_unlocked(), Err(AppError::Locked)));
    }

    #[test]
    fn window_without_timer_stays_open_until_closed() {
        let mut session = Session::default();
        let info = session.open_window(Zeroizing::new(vec![0; 32]), None);
        assert!(info.unlocked && info.expires_at.is_none());
        assert!(session.is_unlocked());

        let info = session.restart_window(Some(Duration::ZERO)).unwrap();
        assert!(!info.unlocked);
        assert!(!session.is_unlocked());
    }
}
