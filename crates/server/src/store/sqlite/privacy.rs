//! Privacy settings and server discovery.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};

pub fn set_privacy(conn: &Connection, user: Id, dm: PrivacyLevel, friend: PrivacyLevel) {
    conn.execute(
        "UPDATE users SET dm_privacy=?1, friend_privacy=?2 WHERE id=?3",
        params![privacy_str(dm), privacy_str(friend), user],
    )
    .ok();
}

pub fn friends(conn: &Connection, a: Id, b: Id) -> bool {
    conn.query_row(
        "SELECT 1 FROM relationships WHERE user_id=?1 AND other_id=?2 AND kind='friend'",
        params![a, b],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

pub fn shares_guild(conn: &Connection, a: Id, b: Id) -> bool {
    conn.query_row(
        "SELECT 1 FROM guild_members x JOIN guild_members y ON y.guild_id=x.guild_id
         WHERE x.user_id=?1 AND y.user_id=?2 LIMIT 1",
        params![a, b],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

pub fn can_dm(conn: &Connection, from: Id, to: Id) -> bool {
    if friends(conn, from, to) {
        return true;
    }
    let privacy = user_by_id(conn, to)
        .map(|user| user.dm_privacy)
        .unwrap_or_default();
    match privacy {
        PrivacyLevel::Everyone => true,
        PrivacyLevel::ServerMembers => shares_guild(conn, from, to),
        PrivacyLevel::Nobody => false,
    }
}

pub fn can_friend_request(conn: &Connection, from: Id, to: Id) -> bool {
    if friends(conn, from, to) {
        return true;
    }
    let privacy = user_by_id(conn, to)
        .map(|user| user.friend_privacy)
        .unwrap_or_default();
    match privacy {
        PrivacyLevel::Everyone => true,
        PrivacyLevel::ServerMembers => shares_guild(conn, from, to),
        PrivacyLevel::Nobody => false,
    }
}

pub fn set_guild_discoverable(conn: &Connection, guild: Id, discoverable: bool) -> bool {
    conn.execute(
        "UPDATE guilds SET discoverable=?1 WHERE id=?2",
        params![discoverable as i64, guild],
    )
    .map(|changed| changed > 0)
    .unwrap_or(false)
}
