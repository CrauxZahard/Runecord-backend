//! Storage boundary.
//!
//! The database lives in memory while the server runs and is persisted as one AES-256-GCM
//! encrypted snapshot (`data/runecord.db.enc`). Uploaded files are encrypted the same way.
//! Phase 2 moves the same trait to a networked Postgres adapter.

pub mod sqlite;

pub use sqlite::*;

use crate::crypto;
use runecord_protocol::{Bootstrap, Id, User};
use rusqlite::{Connection, MAIN_DB, OptionalExtension, params};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard};

pub struct Database {
    conn: Mutex<Connection>,
    key: [u8; 32],
    path: PathBuf,
    flushed_changes: AtomicI64,
}

impl Database {
    pub fn open(path: &std::path::Path, key: [u8; 32]) -> rusqlite::Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        if let Ok(encrypted) = std::fs::read(path) {
            if let Some(plain) = crypto::open(&key, &encrypted) {
                let mut slice: &[u8] = &plain;
                if conn
                    .deserialize_read_exact(MAIN_DB, &mut slice, plain.len(), false)
                    .is_err()
                {
                    eprintln!("runecord: encrypted database could not be opened; starting empty");
                }
            } else {
                eprintln!("runecord: wrong key for encrypted database; starting empty");
            }
        }
        sqlite::migrate(&conn)?;
        let store = Self {
            conn: Mutex::new(conn),
            key,
            path: path.to_path_buf(),
            flushed_changes: AtomicI64::new(0),
        };
        store.flush_now();
        Ok(store)
    }

    pub fn key(&self) -> [u8; 32] {
        self.key
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Persists the in-memory database as an encrypted snapshot when writes happened.
    pub fn flush_if_changed(&self) {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let total = conn.total_changes() as i64;
        if total == self.flushed_changes.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(data) = conn.serialize(MAIN_DB) {
            let encrypted = crypto::seal(&self.key, &data);
            if let Some(dir) = self.path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if std::fs::write(&self.path, encrypted).is_ok() {
                self.flushed_changes.store(total, Ordering::Relaxed);
            }
        }
    }

    pub fn flush_now(&self) {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Ok(data) = conn.serialize(MAIN_DB) {
            let encrypted = crypto::seal(&self.key, &data);
            if let Some(dir) = self.path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if std::fs::write(&self.path, encrypted).is_ok() {
                self.flushed_changes
                    .store(conn.total_changes() as i64, Ordering::Relaxed);
            }
        }
    }

    pub fn bootstrap(&self, me: Id) -> Bootstrap {
        sqlite::bootstrap(&self.conn(), me)
    }

    pub fn session_user(&self, token: &str) -> Option<Id> {
        let hash = sqlite::hash_bot_token(token);
        self.conn()
            .query_row(
                "SELECT user_id FROM sessions WHERE token=?1",
                params![hash],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten()
    }

    pub fn user_by_name(&self, username: &str) -> Option<User> {
        sqlite::user_by_name(&self.conn(), username)
    }

    pub fn user_by_id(&self, id: Id) -> Option<User> {
        sqlite::user_by_id(&self.conn(), id)
    }

    pub fn create_session(&self, user_id: Id, token: &str) {
        let hash = sqlite::hash_bot_token(token);
        self.conn()
            .execute(
                "INSERT INTO sessions(token, user_id, created_at) VALUES(?1,?2,?3)",
                params![hash, user_id, runecord_protocol::now_ms()],
            )
            .ok();
    }
}


