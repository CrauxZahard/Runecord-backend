//! guilds.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::BTreeMap;

pub fn guild_summary(conn: &Connection, guild_id: Id, me: Id) -> Option<GuildSummary> {
    guild_summary_with_unread(conn, guild_id, me).map(|(summary, _)| summary)
}

pub fn guild_summary_with_unread(
    conn: &Connection,
    guild_id: Id,
    me: Id,
) -> Option<(GuildSummary, BTreeMap<Id, u32>)> {
    let mut summary = conn
        .query_row(
            "SELECT id, name, icon_color, owner_id, description, created_at, tag FROM guilds WHERE id=?1",
            params![guild_id],
            |r| {
                Ok(GuildSummary {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    icon_color: r.get::<_, i64>(2)? as u32,
                    owner_id: r.get(3)?,
                    description: r.get(4)?,
                    created_at: r.get(5)?,
                    tag: r.get(6)?,
                    ..GuildSummary::default()
                })
            },
        )
        .optional()
        .ok()
        .flatten()?;
    summary.permissions = base_permissions(conn, guild_id, me);
    summary.member_count = conn
        .query_row(
            "SELECT COUNT(*) FROM guild_members WHERE guild_id=?1",
            params![guild_id],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as u32;
    let unread = guild_unread_channels(conn, guild_id, me);
    summary.unread_count = unread.values().copied().sum();
    summary.mention_count = 0;
    Some((summary, unread))
}

pub(crate) fn guild_unread_channels(conn: &Connection, guild_id: Id, me: Id) -> BTreeMap<Id, u32> {
    let mut out = BTreeMap::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id FROM channels WHERE guild_id=?1 AND kind IN ('text','announcement','forum','thread')",
    ) {
        if let Ok(rows) = stmt.query_map(params![guild_id], |r| r.get::<_, Id>(0)) {
            for channel in rows.flatten() {
                if can_view(conn, channel, me) {
                    let count = unread_for(conn, channel, me);
                    if count > 0 {
                        out.insert(channel, count);
                    }
                }
            }
        }
    }
    out
}

pub fn guild_detail(conn: &Connection, guild_id: Id, me: Id) -> Option<GuildDetail> {
    let guild = guild_summary(conn, guild_id, me)?;
    let mut channels = Vec::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT * FROM channels WHERE guild_id=?1 ORDER BY position, id")
    {
        if let Ok(rows) = stmt.query_map(params![guild_id], channel_from_row) {
            channels.extend(rows.flatten());
        }
    }
    let mut overwrite_list = Vec::new();
    channels
        .retain(|channel| channel.kind == ChannelKind::Category || can_view(conn, channel.id, me));
    for channel in &channels {
        overwrite_list.push(ChannelOverwrites {
            channel_id: channel.id,
            overwrites: overwrites(conn, channel.id),
        });
    }
    for channel in &mut channels {
        channel.my_permissions = channel_permissions(conn, channel.id, me);
    }
    let roles = guild_roles(conn, guild_id);
    let permissions = guild.permissions;
    Some(GuildDetail {
        guild,
        channels,
        roles,
        overwrites: overwrite_list,
        permissions,
    })
}

pub fn list_children(conn: &Connection, guild: Id) -> Vec<Channel> {
    let mut out = Vec::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT * FROM channels WHERE guild_id=?1 ORDER BY position, id")
    {
        if let Ok(rows) = stmt.query_map(params![guild], channel_from_row) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn guild_members(conn: &Connection, guild: Id) -> Vec<Member> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT u.*, gm.nickname, gm.joined_at FROM guild_members gm JOIN users u ON u.id=gm.user_id WHERE gm.guild_id=?1",
    ) {
        if let Ok(rows) = stmt.query_map(params![guild], |r| {
            let user = user_from_row(r)?;
            Ok(Member {
                guild_id: guild,
                user,
                nickname: r.get("nickname")?,
                roles: Vec::new(),
                joined_at: r.get("joined_at")?,
            })
        }) {
            out.extend(rows.flatten());
        }
    }
    for m in out.iter_mut() {
        m.roles = member_role_ids(conn, guild, m.user.id);
    }
    out
}
// ---------------------------------------------------------------------------
// Applications, webhooks, audit, emojis, CDN registry
// ---------------------------------------------------------------------------

pub fn guild_emojis(conn: &Connection, guild: Id) -> Vec<Emoji> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT * FROM emojis WHERE guild_id=?1 ORDER BY id") {
        if let Ok(rows) = stmt.query_map(params![guild], |r| {
            Ok(Emoji {
                id: r.get("id")?,
                guild_id: r.get("guild_id")?,
                name: r.get("name")?,
                sticker: r.get::<_, i64>("sticker")? != 0,
                url: r.get("url")?,
            })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn create_emoji(
    conn: &Connection,
    guild: Id,
    uploader: Id,
    name: &str,
    sticker: bool,
    url: &str,
) -> Emoji {
    let id = next_id(conn);
    conn.execute(
        "INSERT INTO emojis(id, guild_id, name, sticker, url, uploader_id, created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![id, guild, name, sticker as i64, url, uploader, now_ms()],
    ).ok();
    Emoji {
        id,
        guild_id: guild,
        name: name.into(),
        sticker,
        url: url.into(),
    }
}

pub fn delete_emoji(conn: &Connection, guild: Id, id: Id) -> bool {
    conn.execute(
        "DELETE FROM emojis WHERE id=?1 AND guild_id=?2",
        params![id, guild],
    )
    .ok();
    conn.changes() > 0
}
