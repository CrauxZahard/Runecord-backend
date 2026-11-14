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

pub fn guild_notification_settings(
    conn: &Connection,
    user: Id,
    guild: Id,
) -> GuildNotificationSettings {
    conn.query_row(
        "SELECT level, muted, suppress_everyone, suppress_roles, suppress_highlights,
                mute_new_events, mobile_push
         FROM guild_notifications WHERE user_id=?1 AND guild_id=?2",
        params![user, guild],
        |r| {
            Ok(GuildNotificationSettings {
                level: level_from(&r.get::<_, String>(0)?),
                muted: r.get::<_, i64>(1).unwrap_or(0) != 0,
                suppress_everyone: r.get::<_, i64>(2).unwrap_or(0) != 0,
                suppress_roles: r.get::<_, i64>(3).unwrap_or(0) != 0,
                suppress_highlights: r.get::<_, i64>(4).unwrap_or(0) != 0,
                mute_new_events: r.get::<_, i64>(5).unwrap_or(0) != 0,
                mobile_push: r.get::<_, i64>(6).unwrap_or(0) != 0,
            })
        },
    )
    .unwrap_or_default()
}

pub fn guild_notifications_for(
    conn: &Connection,
    user: Id,
) -> BTreeMap<Id, GuildNotificationSettings> {
    let mut out = BTreeMap::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT guild_id, level, muted, suppress_everyone, suppress_roles, suppress_highlights,
                mute_new_events, mobile_push
         FROM guild_notifications WHERE user_id=?1",
    ) {
        if let Ok(rows) = stmt.query_map(params![user], |r| {
            Ok((
                r.get::<_, Id>(0)?,
                GuildNotificationSettings {
                    level: level_from(&r.get::<_, String>(1)?),
                    muted: r.get::<_, i64>(2).unwrap_or(0) != 0,
                    suppress_everyone: r.get::<_, i64>(3).unwrap_or(0) != 0,
                    suppress_roles: r.get::<_, i64>(4).unwrap_or(0) != 0,
                    suppress_highlights: r.get::<_, i64>(5).unwrap_or(0) != 0,
                    mute_new_events: r.get::<_, i64>(6).unwrap_or(0) != 0,
                    mobile_push: r.get::<_, i64>(7).unwrap_or(0) != 0,
                },
            ))
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn set_guild_notification_settings(
    conn: &Connection,
    user: Id,
    guild: Id,
    settings: &GuildNotificationSettings,
) {
    if settings == &GuildNotificationSettings::default() {
        conn.execute(
            "DELETE FROM guild_notifications WHERE user_id=?1 AND guild_id=?2",
            params![user, guild],
        )
        .ok();
        return;
    }
    conn.execute(
        "INSERT INTO guild_notifications(user_id, guild_id, level, muted, suppress_everyone, suppress_roles, suppress_highlights, mute_new_events, mobile_push)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(user_id, guild_id) DO UPDATE SET
           level=excluded.level,
           muted=excluded.muted,
           suppress_everyone=excluded.suppress_everyone,
           suppress_roles=excluded.suppress_roles,
           suppress_highlights=excluded.suppress_highlights,
           mute_new_events=excluded.mute_new_events,
           mobile_push=excluded.mobile_push",
        params![
            user,
            guild,
            level_str(settings.level),
            settings.muted as i64,
            settings.suppress_everyone as i64,
            settings.suppress_roles as i64,
            settings.suppress_highlights as i64,
            settings.mute_new_events as i64,
            settings.mobile_push as i64,
        ],
    )
    .ok();
}

pub fn channel_notification_override(
    conn: &Connection,
    user: Id,
    channel: Id,
) -> Option<NotificationLevel> {
    conn.query_row(
        "SELECT level FROM channel_notifications WHERE user_id=?1 AND channel_id=?2",
        params![user, channel],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .map(|value| level_from(&value))
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
    conn.execute(
        "INSERT INTO channel_notifications(user_id, channel_id, level, suppress_everyone) VALUES(?1,?2,?3,?4)
         ON CONFLICT(user_id, channel_id) DO UPDATE SET level=excluded.level, suppress_everyone=excluded.suppress_everyone",
        params![user, channel, level_str(level), suppress_everyone as i64],
    )
    .ok();
}

pub fn clear_channel_notification(conn: &Connection, user: Id, channel: Id) {
    conn.execute(
        "DELETE FROM channel_notifications WHERE user_id=?1 AND channel_id=?2",
        params![user, channel],
    )
    .ok();
}
