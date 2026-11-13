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

pub fn channel_suppressed(conn: &Connection, user: Id, channel: Id) -> bool {
    conn.query_row(
        "SELECT suppress_everyone FROM channel_notifications WHERE user_id=?1 AND channel_id=?2",
        params![user, channel],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .ok()
    .flatten()
    .is_some_and(|value| value != 0)
}

pub fn channel_suppress_for(conn: &Connection, user: Id) -> BTreeMap<Id, bool> {
    let mut out = BTreeMap::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT channel_id, suppress_everyone FROM channel_notifications
         WHERE user_id=?1 AND suppress_everyone<>0",
    ) {
        if let Ok(rows) = stmt.query_map(params![user], |r| {
            Ok((r.get::<_, Id>(0)?, r.get::<_, i64>(1)? != 0))
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn set_channel_notification(
    conn: &Connection,
    user: Id,
    channel: Id,
    level: NotificationLevel,
    suppress_everyone: bool,
) {
    if level == NotificationLevel::All && !suppress_everyone {
        conn.execute(
            "DELETE FROM channel_notifications WHERE user_id=?1 AND channel_id=?2",
            params![user, channel],
        )
        .ok();
        return;
    }
    conn.execute(
        "INSERT INTO channel_notifications(user_id, channel_id, level, suppress_everyone) VALUES(?1,?2,?3,?4)
         ON CONFLICT(user_id, channel_id) DO UPDATE SET level=excluded.level, suppress_everyone=excluded.suppress_everyone",
        params![user, channel, level_str(level), suppress_everyone as i64],
    )
    .ok();
}
