//! permissions.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};

pub fn base_permissions(conn: &Connection, guild_id: Id, user_id: Id) -> u64 {
    let owner: Option<Id> = conn
        .query_row(
            "SELECT owner_id FROM guilds WHERE id=?1",
            params![guild_id],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten();
    if owner == Some(user_id) {
        return u64::MAX / 2;
    }
    let mut perms = 0u64;
    if let Ok(v) = conn.query_row(
        "SELECT permissions FROM roles WHERE guild_id=?1 AND name='@everyone' LIMIT 1",
        params![guild_id],
        |r| r.get::<_, i64>(0),
    ) {
        perms |= v as u64;
    }
    if let Ok(mut stmt) = conn.prepare(
        "SELECT r.permissions FROM roles r JOIN member_roles mr ON mr.role_id=r.id AND mr.user_id=?2 AND mr.guild_id=?1",
    ) {
        if let Ok(rows) = stmt.query_map(params![guild_id, user_id], |r| r.get::<_, i64>(0)) {
            for v in rows.flatten() {
                perms |= v as u64;
            }
        }
    }
    perms
}

pub fn channel_permissions(conn: &Connection, channel_id: Id, user_id: Id) -> u64 {
    let Some(channel) = channel_by_id(conn, channel_id) else {
        return 0;
    };
    let Some(guild_id) = channel.guild_id else {
        return u64::MAX / 2;
    };
    let mut perms = match channel.thread.as_ref() {
        Some(info) => channel_permissions(conn, info.parent_id, user_id),
        None => base_permissions(conn, guild_id, user_id),
    };
    if perms & perm::ADMINISTRATOR != 0 {
        return perms;
    }
    let mut applies: Vec<(i64, u64, u64)> = Vec::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT target_id, allow, deny FROM overwrites WHERE channel_id=?1")
    {
        if let Ok(rows) = stmt.query_map(params![channel_id], |r| {
            Ok((
                r.get::<_, Id>(0)?,
                r.get::<_, i64>(1)? as u64,
                r.get::<_, i64>(2)? as u64,
            ))
        }) {
            for row in rows.flatten() {
                applies.push(row);
            }
        }
    }
    let role_ids: Vec<Id> = member_role_ids(conn, guild_id, user_id);
    for (target, allow, deny) in &applies {
        let is_everyone = *target == 0;
        let is_member = *target == user_id;
        let is_role = role_ids.contains(target);
        if is_everyone || is_member || is_role {
            perms &= !deny;
            perms |= allow;
        }
    }
    perms
}

pub fn member_role_ids(conn: &Connection, guild_id: Id, user_id: Id) -> Vec<Id> {
    let mut out = Vec::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT role_id FROM member_roles WHERE guild_id=?1 AND user_id=?2")
    {
        if let Ok(rows) = stmt.query_map(params![guild_id, user_id], |r| r.get::<_, Id>(0)) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn can_view(conn: &Connection, channel_id: Id, user_id: Id) -> bool {
    let Some(channel) = channel_by_id(conn, channel_id) else {
        return false;
    };
    if channel.guild_id.is_none() {
        return is_dm_member(conn, channel_id, user_id);
    }
    has(
        channel_permissions(conn, channel_id, user_id),
        perm::VIEW_CHANNEL,
    )
}

pub fn can_send(conn: &Connection, channel_id: Id, user_id: Id) -> bool {
    let Some(channel) = channel_by_id(conn, channel_id) else {
        return false;
    };
    if channel.guild_id.is_none() {
        return is_dm_member(conn, channel_id, user_id);
    }
    has(
        channel_permissions(conn, channel_id, user_id),
        perm::SEND_MESSAGES,
    )
}

pub fn is_dm_member(conn: &Connection, channel_id: Id, user_id: Id) -> bool {
    conn.query_row(
        "SELECT 1 FROM dm_members WHERE channel_id=?1 AND user_id=?2",
        params![channel_id, user_id],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

pub fn overwrites(conn: &Connection, channel_id: Id) -> Vec<PermissionOverwrite> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT * FROM overwrites WHERE channel_id=?1") {
        if let Ok(rows) = stmt.query_map(params![channel_id], |r| {
            let kind: String = r.get("target_kind")?;
            Ok(PermissionOverwrite {
                target_id: r.get("target_id")?,
                kind: if kind == "member" {
                    OverwriteKind::Member
                } else {
                    OverwriteKind::Role
                },
                allow: r.get::<_, i64>("allow")? as u64,
                deny: r.get::<_, i64>("deny")? as u64,
            })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn unread_for(conn: &Connection, channel_id: Id, me: Id) -> u32 {
    let last: Option<Id> = conn
        .query_row(
            "SELECT last_read_id FROM reads WHERE user_id=?1 AND channel_id=?2",
            params![me, channel_id],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten();
    let after = last.unwrap_or(0);
    conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE channel_id=?1 AND id>?2 AND author_id<>?3",
        params![channel_id, after, me],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0) as u32
}

pub fn unread_mentions_for(conn: &Connection, channel_id: Id, me: Id) -> u32 {
    let last: Option<Id> = conn
        .query_row(
            "SELECT last_read_id FROM reads WHERE user_id=?1 AND channel_id=?2",
            params![me, channel_id],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten();
    let after = last.unwrap_or(0);
    let needle = format!("%<@{me}>%");
    conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE channel_id=?1 AND id>?2 AND author_id<>?3
         AND (content LIKE ?4 OR content LIKE '%@everyone%' OR content LIKE '%@here%')",
        params![channel_id, after, me, needle],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0) as u32
}

pub fn set_overwrite(
    conn: &Connection,
    channel: Id,
    target: Id,
    kind: &str,
    allow: u64,
    deny: u64,
) {
    conn.execute(
        "INSERT INTO overwrites(channel_id, target_id, target_kind, allow, deny) VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(channel_id, target_id, target_kind) DO UPDATE SET allow=excluded.allow, deny=excluded.deny",
        params![channel, target, kind, allow as i64, deny as i64],
    ).ok();
}

pub fn delete_overwrite(conn: &Connection, channel: Id, target: Id, kind: &str) -> bool {
    conn.execute(
        "DELETE FROM overwrites WHERE channel_id=?1 AND target_id=?2 AND target_kind=?3",
        params![channel, target, kind],
    )
    .ok();
    conn.changes() > 0
}
pub fn bot_scopes(conn: &Connection, bot: Id) -> Vec<String> {
    conn.query_row(
        "SELECT scopes FROM applications WHERE bot_id=?1",
        params![bot],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .map(|s| s.split_whitespace().map(str::to_string).collect())
    .unwrap_or_else(|| vec!["bot".to_string()])
}
