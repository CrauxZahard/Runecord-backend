//! Public server discovery.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};

pub fn public_guilds(conn: &Connection, me: Id) -> Vec<PublicGuild> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT g.id, g.name, g.icon_color, g.description,
                (SELECT COUNT(*) FROM guild_members m WHERE m.guild_id=g.id) AS members
         FROM guilds g
         WHERE g.discoverable<>0
           AND g.id NOT IN (SELECT guild_id FROM guild_members WHERE user_id=?1)
         ORDER BY members DESC, g.name COLLATE NOCASE",
    ) {
        if let Ok(rows) = stmt.query_map(params![me], |r| {
            Ok(PublicGuild {
                id: r.get(0)?,
                name: r.get(1)?,
                icon_color: r.get::<_, i64>(2)? as u32,
                description: r.get(3)?,
                member_count: r.get::<_, i64>(4)? as u32,
            })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn discoverable(conn: &Connection, guild: Id) -> bool {
    conn.query_row(
        "SELECT discoverable FROM guilds WHERE id=?1",
        params![guild],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .ok()
    .flatten()
    .is_some_and(|value| value != 0)
}

pub fn join_public_guild(conn: &Connection, guild: Id, user: Id) -> bool {
    if !discoverable(conn, guild) {
        return false;
    }
    conn.execute(
        "INSERT OR IGNORE INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(?1,?2,'',?3)",
        params![guild, user, now_ms()],
    )
    .map(|changed| changed > 0)
    .unwrap_or(false)
}
