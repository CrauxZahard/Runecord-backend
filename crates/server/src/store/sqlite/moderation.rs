//! Moderation: kicks, bans and communication timeouts.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};

fn clear_member_state(conn: &Connection, guild: Id, user: Id) {
    conn.execute(
        "DELETE FROM member_roles WHERE guild_id=?1 AND user_id=?2",
        params![guild, user],
    )
    .ok();
    conn.execute(
        "DELETE FROM reads WHERE channel_id IN (SELECT id FROM channels WHERE guild_id=?1)",
        params![guild],
    )
    .ok();
    conn.execute(
        "DELETE FROM channel_notifications WHERE user_id=?1 AND channel_id IN (SELECT id FROM channels WHERE guild_id=?2)",
        params![user, guild],
    )
    .ok();
    conn.execute(
        "DELETE FROM guild_members WHERE guild_id=?1 AND user_id=?2",
        params![guild, user],
    )
    .ok();
}

pub fn kick_member(conn: &Connection, guild: Id, user: Id) -> bool {
    if !guild_member(conn, guild, user) {
        return false;
    }
    clear_member_state(conn, guild, user);
    true
}

pub fn ban_member(conn: &Connection, guild: Id, user: Id, actor: Id, reason: &str) -> bool {
    conn.execute(
        "INSERT INTO bans(guild_id, user_id, actor_id, reason, created_at) VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET actor_id=excluded.actor_id,
             reason=excluded.reason, created_at=excluded.created_at",
        params![guild, user, actor, reason, now_ms()],
    )
    .ok();
    clear_member_state(conn, guild, user);
    true
}

pub fn unban_member(conn: &Connection, guild: Id, user: Id) -> bool {
    conn.execute(
        "DELETE FROM bans WHERE guild_id=?1 AND user_id=?2",
        params![guild, user],
    )
    .map(|changed| changed > 0)
    .unwrap_or(false)
}

pub fn is_banned(conn: &Connection, guild: Id, user: Id) -> bool {
    conn.query_row(
        "SELECT 1 FROM bans WHERE guild_id=?1 AND user_id=?2",
        params![guild, user],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

pub fn timeout_member(conn: &Connection, guild: Id, user: Id, until: i64) -> bool {
    conn.execute(
        "UPDATE guild_members SET timeout_until=?1 WHERE guild_id=?2 AND user_id=?3",
        params![until.max(0), guild, user],
    )
    .map(|changed| changed > 0)
    .unwrap_or(false)
}

pub fn member_timeout(conn: &Connection, guild: Id, user: Id) -> i64 {
    conn.query_row(
        "SELECT timeout_until FROM guild_members WHERE guild_id=?1 AND user_id=?2",
        params![guild, user],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .ok()
    .flatten()
    .unwrap_or(0)
}

pub fn list_bans(conn: &Connection, guild: Id) -> Vec<Ban> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT b.user_id, b.actor_id, b.reason, b.created_at FROM bans b
         WHERE b.guild_id=?1 ORDER BY b.created_at DESC",
    ) {
        if let Ok(rows) = stmt.query_map(params![guild], |r| {
            Ok((
                r.get::<_, Id>(0)?,
                r.get::<_, Id>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        }) {
            for (user, actor, reason, created) in rows.flatten() {
                let Some(found) = user_by_id(conn, user) else {
                    continue;
                };
                out.push(Ban {
                    user: found,
                    reason,
                    actor_id: actor,
                    created_at: created,
                });
            }
        }
    }
    out
}
