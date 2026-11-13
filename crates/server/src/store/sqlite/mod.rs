//! SQLite adapter: queries, permissions and the demo seed.

use runecord_protocol::*;
use rusqlite::{Connection, OptionalExtension, params};

const SCHEMA: &str = include_str!("../../../../../migrations/0001_init.sql");

const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", SCHEMA),
    (
        "0002_bots",
        include_str!("../../../../../migrations/0002_bots.sql"),
    ),
    (
        "0003_platform",
        include_str!("../../../../../migrations/0003_platform.sql"),
    ),
    (
        "0004_profile",
        include_str!("../../../../../migrations/0004_profile.sql"),
    ),
    (
        "0005_permissions",
        include_str!("../../../../../migrations/0005_permissions.sql"),
    ),
    (
        "0006_threads",
        include_str!("../../../../../migrations/0006_threads.sql"),
    ),
    (
        "0007_channel_notifications",
        include_str!("../../../../../migrations/0007_channel_notifications.sql"),
    ),
    (
        "0008_discovery",
        include_str!("../../../../../migrations/0008_discovery.sql"),
    ),
];

/// Applies numbered migrations once each and returns the connection.
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version TEXT PRIMARY KEY,
            applied_at INTEGER NOT NULL
        );",
    )?;
    for (version, sql) in MIGRATIONS {
        let applied: bool = conn
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE version=?1",
                params![version],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if applied {
            continue;
        }
        conn.execute_batch(sql)?;
        conn.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES(?1,?2)",
            params![version, now_ms()],
        )?;
    }
    Ok(())
}

pub fn next_id(conn: &Connection) -> i64 {
    conn.query_row(
        "INSERT INTO meta(k, v) VALUES('id', 1000)
         ON CONFLICT(k) DO UPDATE SET v = v + 1
         RETURNING v",
        [],
        |r| r.get(0),
    )
    .unwrap_or(1000)
}

// ---------------------------------------------------------------------------
// Mappers
// ---------------------------------------------------------------------------

mod auth;
mod bootstrap;
mod discovery;
mod guilds;
mod members;
mod messages;
mod model;
mod notifications;
mod permissions;
mod platform;
mod seed;

pub use auth::*;
pub use bootstrap::*;
pub use discovery::*;
pub use guilds::*;
pub use members::*;
pub use messages::*;
pub use model::*;
pub use notifications::*;
pub use permissions::*;
pub use platform::*;
pub use seed::*;

#[cfg(test)]
mod tests;
