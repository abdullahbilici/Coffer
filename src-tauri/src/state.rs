use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::Connection;

use crate::session::Session;

/// Lock order: `db` before `session` whenever both are held.
pub struct AppState {
    db: Mutex<Connection>,
    pub session: Arc<Mutex<Session>>,
}

impl AppState {
    pub fn new(conn: Connection) -> Self {
        Self {
            db: Mutex::new(conn),
            session: Arc::new(Mutex::new(Session::default())),
        }
    }

    pub fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().expect("db lock poisoned")
    }

    pub fn session(&self) -> MutexGuard<'_, Session> {
        self.session.lock().expect("session lock poisoned")
    }
}
