use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, atomic::AtomicBool},
};

use rusqlite::Connection;

use crate::session::Session;

/// Lock order: `db`, then `session`, then `replies`, whenever more than one is held.
pub struct AppState {
    db: Mutex<Connection>,
    pub session: Arc<Mutex<Session>>,
    /// Assistant replies in progress by conversation, with their cancel flags.
    replies: Mutex<HashMap<i64, Arc<AtomicBool>>>,
}

impl AppState {
    pub fn new(conn: Connection) -> Self {
        Self {
            db: Mutex::new(conn),
            session: Arc::new(Mutex::new(Session::default())),
            replies: Mutex::new(HashMap::new()),
        }
    }

    pub fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().expect("db lock poisoned")
    }

    pub fn session(&self) -> MutexGuard<'_, Session> {
        self.session.lock().expect("session lock poisoned")
    }

    pub fn replies(&self) -> MutexGuard<'_, HashMap<i64, Arc<AtomicBool>>> {
        self.replies.lock().expect("replies lock poisoned")
    }
}
