//! Invite storage.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};

pub fn create_invite(
    conn: &Connection,
    code: &str,
    guild: Id,
    inviter: Id,
    max_uses: Option<u32>,
) -> Option<Invite> {
    let created_at = now_ms();
    conn.execute(
        "INSERT INTO invites(code, guild_id, inviter_id, uses, max_uses, created_at)
         VALUES(?1,?2,?3,0,?4,?5)",
        params![code, guild, inviter, max_uses, created_at],
    )
    .ok()?;

    let (name, color, count): (String, i64, i64) = conn
        .query_row(
            "SELECT name, icon_color,
                    (SELECT COUNT(*) FROM guild_members WHERE guild_id=?1)
             FROM guilds WHERE id=?1",
            params![guild],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok()?;

    Some(Invite {
        code: code.to_string(),
        guild_id: guild,
        guild_name: name,
        guild_icon_color: color as u32,
        inviter_id: inviter,
        uses: 0,
        created_at,
        member_count: count as u32,
    })
}

pub fn invite_by_code(conn: &Connection, code: &str) -> Option<Invite> {
    conn.query_row(
        "SELECT i.*, g.name, g.icon_color,
                (SELECT COUNT(*) FROM guild_members WHERE guild_id=g.id) AS member_count
         FROM invites i JOIN guilds g ON g.id=i.guild_id
         WHERE i.code=?1",
        params![code],
        |row| {
            Ok(Invite {
                code: row.get("code")?,
                guild_id: row.get("guild_id")?,
                guild_name: row.get("name")?,
                guild_icon_color: row.get::<_, i64>("icon_color")? as u32,
                inviter_id: row.get("inviter_id")?,
                uses: row.get::<_, i64>("uses")? as u32,
                created_at: row.get("created_at")?,
                member_count: row.get::<_, i64>("member_count")? as u32,
            })
        },
    )
    .optional()
    .ok()
    .flatten()
}

pub fn guild_for_invite(conn: &Connection, code: &str) -> Option<Id> {
    conn.query_row(
        "SELECT guild_id FROM invites WHERE code=?1",
        params![code],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn join_invite(conn: &Connection, guild: Id, code: &str, user: Id) {
    conn.execute(
        "INSERT OR IGNORE INTO guild_members(guild_id, user_id, nickname, joined_at)
         VALUES(?1,?2,'',?3)",
        params![guild, user, now_ms()],
    )
    .ok();
    conn.execute(
        "UPDATE invites SET uses=uses+1 WHERE code=?1",
        params![code],
    )
    .ok();
}
