//! Storage boundary.
//!
//! Phase 1 ships the SQLite adapter. Handlers that still run SQL directly do so through
//! [`SqliteStore::conn`]; Phase 2 replaces those calls with [`Store`] methods and adds a
//! Postgres implementation behind the same trait.

pub mod sqlite;

pub use sqlite::*;

use runecord_protocol::{Bootstrap, Id, User};
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::{Mutex, MutexGuard};

pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    pub fn open(path: &std::path::Path) -> rusqlite::Result<Self> {
        Ok(Self { conn: Mutex::new(sqlite::open(path)?) })
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn bootstrap(&self, me: Id) -> Bootstrap {
        sqlite::bootstrap(&self.conn(), me)
    }

    pub fn session_user(&self, token: &str) -> Option<Id> {
        self.conn()
            .query_row("SELECT user_id FROM sessions WHERE token=?1", params![token], |r| r.get(0))
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
        self.conn()
            .execute(
                "INSERT INTO sessions(token, user_id, created_at) VALUES(?1,?2,?3)",
                params![token, user_id, runecord_protocol::now_ms()],
            )
            .ok();
    }
}

/// The storage contract a Postgres adapter will implement in Phase 2. Phase 1 routes the
/// session and bootstrap reads through it.
pub trait Store: Send + Sync {
    fn session_user(&self, token: &str) -> Option<Id>;
    fn bootstrap(&self, me: Id) -> Bootstrap;
    fn user_by_id(&self, id: Id) -> Option<User>;
    fn user_by_name(&self, username: &str) -> Option<User>;
}

impl Store for SqliteStore {
    fn session_user(&self, token: &str) -> Option<Id> {
        SqliteStore::session_user(self, token)
    }

    fn bootstrap(&self, me: Id) -> Bootstrap {
        SqliteStore::bootstrap(self, me)
    }

    fn user_by_id(&self, id: Id) -> Option<User> {
        SqliteStore::user_by_id(self, id)
    }

    fn user_by_name(&self, username: &str) -> Option<User> {
        SqliteStore::user_by_name(self, username)
    }
}
