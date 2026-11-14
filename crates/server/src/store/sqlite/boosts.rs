//! Server boosts: one boost per member per server, with Discord's level tiers.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};

pub const LEVEL_ONE: u32 = 2;
pub const LEVEL_TWO: u32 = 7;
pub const LEVEL_THREE: u32 = 14;

/// Upload cap in bytes for a boost level: 48/64/96/128 MB.
pub fn upload_limit_bytes(level: u32) -> u64 {
    match level {
        0 => 48 * 1024 * 1024,
        1 => 64 * 1024 * 1024,
        2 => 96 * 1024 * 1024,
        _ => 128 * 1024 * 1024,
    }
}

/// Opus bitrate for a boost level: 64/128/256/384 kbps.
pub fn voice_bitrate_kbps(level: u32) -> u32 {
    match level {
        0 => 64,
        1 => 128,
        2 => 256,
        _ => 384,
    }
}

pub fn boost_level(boosts: u32) -> u32 {
    if boosts >= LEVEL_THREE {
        3
    } else if boosts >= LEVEL_TWO {
        2
    } else if boosts >= LEVEL_ONE {
        1
    } else {
        0
    }
}

/// Discord unlocks server banners at Level 2 (7 Boosts).
pub fn banner_unlocked(level: u32) -> bool {
    level >= 2
}

/// Role icons unlock at Boost Level 2 (7 Boosts), like server banners.
pub fn role_icon_unlocked(level: u32) -> bool {
    level >= 2
}

pub fn boost_guild(conn: &Connection, guild: Id, user: Id) -> bool {
    conn.execute(
        "INSERT OR IGNORE INTO guild_boosts(guild_id, user_id, created_at) VALUES(?1,?2,?3)",
        params![guild, user, now_ms()],
    )
    .map(|changed| changed > 0)
    .unwrap_or(false)
}

pub fn unboost_guild(conn: &Connection, guild: Id, user: Id) -> bool {
    conn.execute(
        "DELETE FROM guild_boosts WHERE guild_id=?1 AND user_id=?2",
        params![guild, user],
    )
    .map(|changed| changed > 0)
    .unwrap_or(false)
}

pub fn boost_count(conn: &Connection, guild: Id) -> u32 {
    conn.query_row(
        "SELECT COUNT(*) FROM guild_boosts WHERE guild_id=?1",
        params![guild],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0) as u32
}

pub fn boosted_by(conn: &Connection, guild: Id, user: Id) -> bool {
    conn.query_row(
        "SELECT 1 FROM guild_boosts WHERE guild_id=?1 AND user_id=?2",
        params![guild, user],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}
