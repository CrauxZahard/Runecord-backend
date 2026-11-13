//! Moderation: kicks, bans, communication timeouts and member pruning.

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

/// The last time a member was seen: their join date or their most recent
/// message in the server, whichever is later.
pub fn member_last_active(conn: &Connection, guild: Id, user: Id) -> i64 {
    conn.query_row(
        "SELECT MAX(
             gm.joined_at,
             COALESCE((
                 SELECT MAX(m.timestamp) FROM messages m
                 JOIN channels c ON c.id = m.channel_id
                 WHERE c.guild_id = ?1 AND m.author_id = gm.user_id
             ), 0)
         )
         FROM guild_members gm WHERE gm.guild_id = ?1 AND gm.user_id = ?2",
        params![guild, user],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .ok()
    .flatten()
    .unwrap_or(0)
}

/// Members who would be removed by a prune. Roles protect members unless every
/// role they hold is explicitly included. Owners, bots and the actor are kept.
pub fn prune_candidates(
    conn: &Connection,
    guild: Id,
    days: u32,
    include_roles: &[Id],
    actor: Id,
) -> Vec<Id> {
    let days = days.clamp(1, 30);
    let cutoff = now_ms() - days as i64 * 86_400_000;
    let owner: Option<Id> = conn
        .query_row(
            "SELECT owner_id FROM guilds WHERE id=?1",
            params![guild],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten();
    let mut members: Vec<(Id, bool)> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT u.id, u.is_bot FROM guild_members gm JOIN users u ON u.id=gm.user_id
         WHERE gm.guild_id=?1",
    ) {
        if let Ok(rows) = stmt.query_map(params![guild], |r| {
            Ok((r.get::<_, Id>(0)?, r.get::<_, i64>(1).unwrap_or(0) != 0))
        }) {
            members.extend(rows.flatten());
        }
    }
    let mut out = Vec::new();
    for (user, is_bot) in members {
        if user == actor || owner == Some(user) || is_bot {
            continue;
        }
        if member_last_active(conn, guild, user) > cutoff {
            continue;
        }
        let roles = member_role_ids(conn, guild, user);
        let protected = if include_roles.is_empty() {
            !roles.is_empty()
        } else {
            roles.iter().any(|role| !include_roles.contains(role))
        };
        if protected {
            continue;
        }
        out.push(user);
    }
    out
}

pub fn prune_members(conn: &Connection, guild: Id, users: &[Id]) -> Vec<Id> {
    let mut pruned = Vec::new();
    for user in users {
        if kick_member(conn, guild, *user) {
            pruned.push(*user);
        }
    }
    pruned
}
