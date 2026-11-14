//! guilds.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::BTreeMap;

pub fn guild_exists(conn: &Connection, guild: Id) -> bool {
    conn.query_row("SELECT 1 FROM guilds WHERE id=?1", params![guild], |_| {
        Ok(())
    })
    .optional()
    .ok()
    .flatten()
    .is_some()
}

pub fn guild_owner(conn: &Connection, guild: Id) -> Option<Id> {
    conn.query_row(
        "SELECT owner_id FROM guilds WHERE id=?1",
        params![guild],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn guild_member_count(conn: &Connection, guild: Id) -> u32 {
    conn.query_row(
        "SELECT COUNT(*) FROM guild_members WHERE guild_id=?1",
        params![guild],
        |row| row.get::<_, i64>(0),
    )
    .unwrap_or(0) as u32
}

pub fn create_guild(conn: &Connection, me: Id, name: &str, icon_color: u32) -> Option<Guild> {
    let tx = conn.unchecked_transaction().ok()?;
    let guild_id = next_id(&tx);
    let everyone = next_id(&tx);
    let created_at = now_ms();

    tx.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id)
         VALUES(?1,?2,?3,?4,'',?5,?6)",
        params![guild_id, name, icon_color as i64, me, created_at, everyone],
    )
    .ok()?;
    tx.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable)
         VALUES(?1,?2,'@everyone',0,?3,0,0,0)",
        params![
            everyone,
            guild_id,
            (perm::EVERYONE | perm::CREATE_INVITE) as i64
        ],
    )
    .ok()?;
    tx.execute(
        "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(?1,?2,'',?3)",
        params![guild_id, me, created_at],
    )
    .ok()?;

    let category = next_id(&tx);
    tx.execute(
        "INSERT INTO channels(id, guild_id, name, kind, position, topic)
         VALUES(?1,?2,'Text Channels','category',0,'')",
        params![category, guild_id],
    )
    .ok()?;
    let general = next_id(&tx);
    tx.execute(
        "INSERT INTO channels(id, guild_id, name, kind, position, parent_id, topic)
         VALUES(?1,?2,'general','text',1,?3,'This is where it all begins')",
        params![general, guild_id, category],
    )
    .ok()?;
    tx.commit().ok()?;

    Some(Guild {
        id: guild_id,
        name: name.to_string(),
        icon_color,
        owner_id: me,
        member_count: 1,
        created_at,
        channels: vec![
            Channel {
                id: category,
                guild_id: Some(guild_id),
                name: "Text Channels".into(),
                kind: ChannelKind::Category,
                position: 0,
                ..Channel::default()
            },
            Channel {
                id: general,
                guild_id: Some(guild_id),
                name: "general".into(),
                kind: ChannelKind::Text,
                position: 1,
                parent_id: Some(category),
                topic: "This is where it all begins".into(),
                ..Channel::default()
            },
        ],
        ..Guild::default()
    })
}

pub fn update_guild_fields(
    conn: &Connection,
    guild: Id,
    name: Option<&str>,
    icon_color: Option<u32>,
    tag: Option<&str>,
    description: Option<&str>,
    discoverable: Option<bool>,
) -> Result<(), rusqlite::Error> {
    if let Some(name) = name {
        conn.execute(
            "UPDATE guilds SET name=?1 WHERE id=?2",
            params![name, guild],
        )?;
    }
    if let Some(color) = icon_color {
        conn.execute(
            "UPDATE guilds SET icon_color=?1 WHERE id=?2",
            params![color as i64, guild],
        )?;
    }
    if let Some(tag) = tag {
        conn.execute(
            "UPDATE guilds SET tag=?1 WHERE id=?2",
            params![tag.chars().take(8).collect::<String>(), guild],
        )?;
    }
    if let Some(description) = description {
        conn.execute(
            "UPDATE guilds SET description=?1 WHERE id=?2",
            params![description.chars().take(500).collect::<String>(), guild],
        )?;
    }
    if let Some(discoverable) = discoverable {
        conn.execute(
            "UPDATE guilds SET discoverable=?1 WHERE id=?2",
            params![discoverable as i64, guild],
        )?;
    }
    Ok(())
}

pub fn delete_guild_cascade(conn: &Connection, guild: Id) {
    conn.execute("DELETE FROM channels WHERE guild_id=?1", params![guild])
        .ok();
    conn.execute(
        "DELETE FROM guild_members WHERE guild_id=?1",
        params![guild],
    )
    .ok();
    conn.execute("DELETE FROM roles WHERE guild_id=?1", params![guild])
        .ok();
    conn.execute("DELETE FROM guilds WHERE id=?1", params![guild])
        .ok();
}

pub fn set_guild_image(
    conn: &Connection,
    guild: Id,
    banner: bool,
    url: &str,
) -> Result<(), rusqlite::Error> {
    let column = if banner { "banner_url" } else { "icon_url" };
    conn.execute(
        &format!("UPDATE guilds SET {column}=?1 WHERE id=?2"),
        params![url, guild],
    )?;
    Ok(())
}

pub fn clear_guild_image(
    conn: &Connection,
    guild: Id,
    banner: bool,
) -> Result<(), rusqlite::Error> {
    let column = if banner { "banner_url" } else { "icon_url" };
    conn.execute(
        &format!("UPDATE guilds SET {column}='' WHERE id=?1"),
        params![guild],
    )?;
    Ok(())
}

pub fn guild_basic(conn: &Connection, guild_id: Id) -> Option<Guild> {
    conn.query_row(
        "SELECT id, name, icon_color, owner_id, description, created_at
         FROM guilds WHERE id=?1",
        params![guild_id],
        |row| {
            Ok(Guild {
                id: row.get(0)?,
                name: row.get(1)?,
                icon_color: row.get::<_, i64>(2)? as u32,
                owner_id: row.get(3)?,
                description: row.get(4)?,
                created_at: row.get(5)?,
                ..Guild::default()
            })
        },
    )
    .optional()
    .ok()
    .flatten()
}

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
            "SELECT id, name, icon_color, owner_id, description, created_at, tag, discoverable, icon_url, banner_url FROM guilds WHERE id=?1",
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
                    discoverable: r.get::<_, i64>(7).unwrap_or(1) != 0,
                    icon_url: r.get(8).unwrap_or_default(),
                    banner_url: r.get(9).unwrap_or_default(),
                    ..GuildSummary::default()
                })
            },
        )
        .optional()
        .ok()
        .flatten()?;
    summary.permissions = base_permissions(conn, guild_id, me);
    summary.boosts = boost_count(conn, guild_id);
    summary.boost_level = boost_level(summary.boosts);
    summary.boosted_by_me = boosted_by(conn, guild_id, me);
    summary.upload_limit_bytes = upload_limit_bytes(summary.boost_level);
    summary.voice_kbps = voice_bitrate_kbps(summary.boost_level);
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
    let settings = guild_notification_settings(conn, me, guild_id);
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id FROM channels WHERE guild_id=?1 AND kind IN ('text','announcement','forum','thread')",
    ) {
        if let Ok(rows) = stmt.query_map(params![guild_id], |r| r.get::<_, Id>(0)) {
            for channel in rows.flatten() {
                if can_view(conn, channel, me) {
                    let level = channel_notification_override(conn, me, channel).unwrap_or(
                        if settings.muted {
                            NotificationLevel::Mentions
                        } else {
                            settings.level
                        },
                    );
                    if level == NotificationLevel::None {
                        continue;
                    }
                    let count = if level == NotificationLevel::Mentions {
                        let suppress = channel_suppressed(conn, me, channel)
                            || settings.suppress_everyone;
                        unread_mentions_for(
                            conn,
                            channel,
                            me,
                            suppress,
                            settings.suppress_roles,
                        )
                    } else {
                        unread_for(conn, channel, me)
                    };
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
        fill_thread_preview(conn, channel);
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
        "SELECT u.*, gm.nickname, gm.joined_at, gm.timeout_until FROM guild_members gm JOIN users u ON u.id=gm.user_id WHERE gm.guild_id=?1",
    ) {
        if let Ok(rows) = stmt.query_map(params![guild], |r| {
            let user = user_from_row(r)?;
            Ok(Member {
                guild_id: guild,
                user,
                nickname: r.get("nickname")?,
                roles: Vec::new(),
                joined_at: r.get("joined_at")?,
                timeout_until: r.get("timeout_until").unwrap_or(0),
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
