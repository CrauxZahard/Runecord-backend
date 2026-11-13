//! members.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashMap;

pub fn guild_member(conn: &Connection, guild_id: Id, user_id: Id) -> bool {
    conn.query_row(
        "SELECT 1 FROM guild_members WHERE guild_id=?1 AND user_id=?2",
        params![guild_id, user_id],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

pub fn guild_member_detail(conn: &Connection, guild_id: Id, user_id: Id) -> Option<Member> {
    let mut member = conn
        .query_row(
            "SELECT u.*, gm.nickname, gm.joined_at FROM guild_members gm JOIN users u ON u.id=gm.user_id WHERE gm.guild_id=?1 AND gm.user_id=?2",
            params![guild_id, user_id],
            |r| {
                Ok(Member {
                    guild_id,
                    user: user_from_row(r)?,
                    nickname: r.get("nickname")?,
                    roles: Vec::new(),
                    joined_at: r.get("joined_at")?,
                })
            },
        )
        .optional()
        .ok()
        .flatten()?;
    member.roles = member_role_ids(conn, guild_id, user_id);
    Some(member)
}

pub fn users_related(conn: &Connection, a: Id, b: Id) -> bool {
    a == b
        || conn
            .query_row(
                "SELECT 1 FROM guild_members x JOIN guild_members y ON x.guild_id=y.guild_id WHERE x.user_id=?1 AND y.user_id=?2 LIMIT 1",
                params![a, b],
                |_| Ok(()),
            )
            .optional()
            .ok()
            .flatten()
            .is_some()
        || conn
            .query_row(
                "SELECT 1 FROM relationships WHERE (user_id=?1 AND other_id=?2) OR (user_id=?2 AND other_id=?1) LIMIT 1",
                params![a, b],
                |_| Ok(()),
            )
            .optional()
            .ok()
            .flatten()
            .is_some()
}

pub(crate) fn member_role_map(conn: &Connection, guild_id: Id) -> HashMap<Id, Vec<Id>> {
    let mut out: HashMap<Id, Vec<Id>> = HashMap::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT user_id, role_id FROM member_roles WHERE guild_id=?1")
    {
        if let Ok(rows) = stmt.query_map(params![guild_id], |r| {
            Ok((r.get::<_, Id>(0)?, r.get::<_, Id>(1)?))
        }) {
            for (user_id, role_id) in rows.flatten() {
                out.entry(user_id).or_default().push(role_id);
            }
        }
    }
    out
}

pub(crate) fn everyone_can_view(conn: &Connection, guild_id: Id) -> bool {
    conn.query_row(
        "SELECT permissions FROM roles WHERE guild_id=?1 AND name='@everyone'",
        params![guild_id],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .ok()
    .flatten()
    .map(|bits| has(bits as u64, perm::VIEW_CHANNEL))
    .unwrap_or(false)
}

pub(crate) fn member_count(
    conn: &Connection,
    guild_id: Id,
    like: &str,
    role: Id,
    status: &str,
) -> u32 {
    let sql = "SELECT COUNT(*) FROM guild_members gm JOIN users u ON u.id=gm.user_id
        WHERE gm.guild_id=?1
          AND (?2='' OR u.username LIKE ?2 OR u.display_name LIKE ?2 OR gm.nickname LIKE ?2)
          AND (?3=0 OR EXISTS(SELECT 1 FROM member_roles mr WHERE mr.guild_id=gm.guild_id AND mr.user_id=gm.user_id AND mr.role_id=?3))
          AND (?4='all' OR (?4='online' AND u.status<>'offline') OR (?4<>'all' AND ?4<>'online' AND u.status=?4))";
    conn.query_row(sql, params![guild_id, like, role, status], |r| {
        r.get::<_, i64>(0)
    })
    .unwrap_or(0) as u32
}

pub fn members_page(
    conn: &Connection,
    guild_id: Id,
    me: Id,
    channel: Option<Id>,
    offset: u32,
    limit: u32,
    query: &str,
    role: Id,
    status: &str,
) -> Option<MemberPage> {
    let summary = guild_summary(conn, guild_id, me)?;
    let limit = limit.clamp(1, 100);
    let like = if query.trim().is_empty() {
        String::new()
    } else {
        format!("%{}%", query.trim())
    };
    let status = match status {
        "online" | "offline" | "idle" | "dnd" => status,
        _ => "all",
    };
    let all_visible =
        channel.is_none() && (everyone_can_view(conn, guild_id) || summary.owner_id == me);
    let can_see = |user: Id| match channel {
        Some(channel) => can_view(conn, channel, user),
        None => base_permissions(conn, guild_id, user) & perm::VIEW_CHANNEL != 0,
    };

    let mut members = Vec::new();
    let mut raw_count = 0u32;
    let sql = "SELECT u.*, gm.nickname, gm.joined_at FROM guild_members gm JOIN users u ON u.id=gm.user_id
        WHERE gm.guild_id=?1
          AND (?2='' OR u.username LIKE ?2 OR u.display_name LIKE ?2 OR gm.nickname LIKE ?2)
          AND (?3=0 OR EXISTS(SELECT 1 FROM member_roles mr WHERE mr.guild_id=gm.guild_id AND mr.user_id=gm.user_id AND mr.role_id=?3))
          AND (?4='all' OR (?4='online' AND u.status<>'offline') OR (?4<>'all' AND ?4<>'online' AND u.status=?4))
        ORDER BY (u.status<>'offline') DESC, gm.joined_at ASC, u.id ASC
        LIMIT ?5 OFFSET ?6";
    if let Ok(mut stmt) = conn.prepare(sql) {
        if let Ok(rows) =
            stmt.query_map(params![guild_id, like, role, status, limit, offset], |r| {
                Ok(Member {
                    guild_id,
                    user: user_from_row(r)?,
                    nickname: r.get("nickname")?,
                    roles: Vec::new(),
                    joined_at: r.get("joined_at")?,
                })
            })
        {
            for member in rows.flatten() {
                raw_count += 1;
                members.push(member);
            }
        }
    }
    let roles = member_role_map(conn, guild_id);
    for member in &mut members {
        member.roles = roles.get(&member.user.id).cloned().unwrap_or_default();
    }
    if !all_visible {
        members.retain(|member| can_see(member.user.id));
    }

    let total = if all_visible {
        member_count(conn, guild_id, &like, role, status)
    } else {
        // Count the filtered set instead of returning a misleading total.
        let mut matched = 0u32;
        if let Ok(mut stmt) = conn.prepare(sql) {
            if let Ok(rows) = stmt
                .query_map(params![guild_id, like, role, status, u32::MAX, 0], |r| {
                    Ok(user_from_row(r)?.id)
                })
            {
                for user in rows.flatten() {
                    if can_see(user) {
                        matched += 1;
                    }
                }
            }
        }
        matched
    };

    let presences = members
        .iter()
        .map(|member| (member.user.id, member.user.status))
        .collect();
    let next_offset = if raw_count >= limit {
        Some(offset.saturating_add(raw_count))
    } else {
        None
    };
    Some(MemberPage {
        members,
        total,
        next_offset,
        presences,
    })
}

pub fn guild_roles(conn: &Connection, guild: Id) -> Vec<Role> {
    let mut out = Vec::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT * FROM roles WHERE guild_id=?1 ORDER BY position DESC")
    {
        if let Ok(rows) = stmt.query_map(params![guild], role_from_row) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn member_top_role(conn: &Connection, guild: Id, user: Id) -> i32 {
    let owner: Option<Id> = conn
        .query_row(
            "SELECT owner_id FROM guilds WHERE id=?1",
            params![guild],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten();
    if owner == Some(user) {
        return i32::MAX;
    }
    let mut top = 0;
    for role in guild_roles(conn, guild) {
        if role.name == "@everyone" {
            continue;
        }
        if member_role_ids(conn, guild, user).contains(&role.id) {
            top = top.max(role.position);
        }
    }
    top
}

pub fn can_manage_member(conn: &Connection, guild: Id, actor: Id, target: Id) -> bool {
    if actor == target {
        return true;
    }
    let actor_top = member_top_role(conn, guild, actor);
    let target_top = member_top_role(conn, guild, target);
    actor_top > target_top
}

pub fn role_position(conn: &Connection, guild: Id, role: Id) -> Option<i32> {
    conn.query_row(
        "SELECT position FROM roles WHERE id=?1 AND guild_id=?2",
        params![role, guild],
        |r| r.get(0),
    )
    .optional()
    .ok()
    .flatten()
}
