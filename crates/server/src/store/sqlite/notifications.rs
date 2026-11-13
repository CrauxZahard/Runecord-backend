//! Per-user, per-channel notification levels.

use super::*;
use rusqlite::{Connection, params};
use std::collections::BTreeMap;

fn level_from(value: &str) -> NotificationLevel {
    match value {
        "mentions" => NotificationLevel::Mentions,
        "none" => NotificationLevel::None,
        _ => NotificationLevel::All,
    }
}

fn level_str(level: NotificationLevel) -> &'static str {
    match level {
        NotificationLevel::All => "all",
        NotificationLevel::Mentions => "mentions",
        NotificationLevel::None => "none",
    }
}

pub fn channel_notification(conn: &Connection, user: Id, channel: Id) -> NotificationLevel {
    conn.query_row(
        "SELECT level FROM channel_notifications WHERE user_id=?1 AND channel_id=?2",
        params![user, channel],
        |r| r.get::<_, String>(0),
    )
    .map(|value| level_from(&value))
    .unwrap_or_default()
}

pub fn channel_notifications_for(conn: &Connection, user: Id) -> BTreeMap<Id, NotificationLevel> {
    let mut out = BTreeMap::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT channel_id, level FROM channel_notifications WHERE user_id=?1")
    {
        if let Ok(rows) = stmt.query_map(params![user], |r| {
            Ok((r.get::<_, Id>(0)?, r.get::<_, String>(1)?))
        }) {
            for (channel, level) in rows.flatten() {
                out.insert(channel, level_from(&level));
            }
        }
    }
    out
}

pub fn set_channel_notification(
    conn: &Connection,
    user: Id,
    channel: Id,
    level: NotificationLevel,
) {
    if level == NotificationLevel::All {
        conn.execute(
            "DELETE FROM channel_notifications WHERE user_id=?1 AND channel_id=?2",
            params![user, channel],
        )
        .ok();
        return;
    }
    conn.execute(
        "INSERT INTO channel_notifications(user_id, channel_id, level) VALUES(?1,?2,?3)
         ON CONFLICT(user_id, channel_id) DO UPDATE SET level=excluded.level",
        params![user, channel, level_str(level)],
    )
    .ok();
}
