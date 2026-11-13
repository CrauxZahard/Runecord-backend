//! model.

use super::*;
use rusqlite::{Connection, OptionalExtension, Row, params};

pub fn presence_from(s: &str) -> Presence {
    match s {
        "online" => Presence::Online,
        "idle" => Presence::Idle,
        "dnd" => Presence::Dnd,
        _ => Presence::Offline,
    }
}

pub fn presence_str(p: Presence) -> &'static str {
    match p {
        Presence::Online => "online",
        Presence::Idle => "idle",
        Presence::Dnd => "dnd",
        Presence::Offline => "offline",
    }
}

pub fn user_from_row(r: &Row<'_>) -> rusqlite::Result<User> {
    Ok(User {
        id: r.get("id")?,
        username: r.get("username")?,
        display_name: r.get("display_name")?,
        avatar_color: r.get::<_, i64>("avatar_color")? as u32,
        avatar_border: r.get::<_, i64>("avatar_border").unwrap_or(0) as u32,
        banner_color: r.get::<_, i64>("banner_color").unwrap_or(0) as u32,
        bio: r.get("bio")?,
        custom_status: r.get("custom_status")?,
        status: presence_from(&r.get::<_, String>("status")?),
        is_bot: r.get::<_, i64>("is_bot").unwrap_or(0) != 0,
        created_at: r.get("created_at")?,
    })
}

pub fn user_by_id(conn: &Connection, id: Id) -> Option<User> {
    conn.query_row(
        "SELECT * FROM users WHERE id=?1",
        params![id],
        user_from_row,
    )
    .optional()
    .ok()
    .flatten()
}

pub fn user_by_name(conn: &Connection, name: &str) -> Option<User> {
    conn.query_row(
        "SELECT * FROM users WHERE username=?1",
        params![name],
        user_from_row,
    )
    .optional()
    .ok()
    .flatten()
}

pub fn channel_from_row(r: &Row<'_>) -> rusqlite::Result<Channel> {
    let kind: String = r.get("kind")?;
    let kind = match kind.as_str() {
        "text" => ChannelKind::Text,
        "voice" => ChannelKind::Voice,
        "category" => ChannelKind::Category,
        "announcement" => ChannelKind::Announcement,
        "forum" => ChannelKind::Forum,
        "thread" => ChannelKind::Thread,
        "dm" => ChannelKind::Dm,
        "group_dm" => ChannelKind::GroupDm,
        _ => ChannelKind::Text,
    };
    let thread_parent: Option<Id> = r.get("thread_parent_id")?;
    let thread = if kind == ChannelKind::Thread {
        let parent = thread_parent.unwrap_or(0);
        Some(ThreadInfo {
            parent_id: parent,
            name: r.get("name")?,
            message_count: r.get::<_, i64>("thread_message_count")? as u32,
            archived: false,
        })
    } else {
        None
    };
    Ok(Channel {
        id: r.get("id")?,
        guild_id: r.get("guild_id")?,
        name: r.get("name")?,
        kind,
        position: r.get::<_, i64>("position")? as i32,
        parent_id: r.get("parent_id")?,
        topic: r.get("topic")?,
        nsfw: r.get::<_, i64>("nsfw")? != 0,
        slowmode: r.get::<_, i64>("slowmode")? as u32,
        last_message_id: r.get("last_message_id")?,
        owner_id: r.get("owner_id")?,
        thread,
        my_permissions: 0,
    })
}

pub fn channel_by_id(conn: &Connection, id: Id) -> Option<Channel> {
    conn.query_row(
        "SELECT * FROM channels WHERE id=?1",
        params![id],
        channel_from_row,
    )
    .optional()
    .ok()
    .flatten()
}

pub fn role_from_row(r: &Row<'_>) -> rusqlite::Result<Role> {
    Ok(Role {
        id: r.get("id")?,
        guild_id: r.get("guild_id")?,
        name: r.get("name")?,
        color: r.get::<_, i64>("color")? as u32,
        permissions: r.get::<_, i64>("permissions")? as u64,
        position: r.get::<_, i64>("position")? as i32,
        hoist: r.get::<_, i64>("hoist")? != 0,
        mentionable: r.get::<_, i64>("mentionable")? != 0,
    })
}

pub fn attachments(conn: &Connection, message_id: Id) -> Vec<Attachment> {
    let mut stmt = match conn.prepare("SELECT * FROM attachments WHERE message_id=?1 ORDER BY id") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map(params![message_id], |r| {
        Ok(Attachment {
            id: r.get("id")?,
            filename: r.get("filename")?,
            size: r.get::<_, i64>("size")? as u64,
            content_type: r.get("content_type")?,
            url: r.get("url")?,
            width: r.get::<_, i64>("width")? as u32,
            height: r.get::<_, i64>("height")? as u32,
        })
    })
    .map(|rows| rows.filter_map(Result::ok).collect())
    .unwrap_or_default()
}

pub fn reactions(conn: &Connection, message_id: Id, me: Id) -> Vec<Reaction> {
    let mut stmt = match conn
        .prepare("SELECT emoji, user_id FROM reactions WHERE message_id=?1 ORDER BY emoji, user_id")
    {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut grouped: Vec<Reaction> = Vec::new();
    if let Ok(rows) = stmt.query_map(params![message_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Id>(1)?))
    }) {
        for (emoji, user_id) in rows.flatten() {
            if let Some(last) = grouped.last_mut().filter(|x| x.emoji == emoji) {
                last.count += 1;
                last.user_ids.push(user_id);
                last.me |= user_id == me;
            } else {
                grouped.push(Reaction {
                    emoji,
                    count: 1,
                    me: user_id == me,
                    user_ids: vec![user_id],
                });
            }
        }
    }
    grouped
}

pub fn mentions_in(content: &str) -> Vec<Id> {
    let mut out = Vec::new();
    let mut rest = content;
    while let Some(i) = rest.find("<@") {
        let after = &rest[i + 2..];
        let Some(end) = after.find('>') else { break };
        if let Ok(id) = after[..end].parse::<Id>() {
            out.push(id);
        }
        rest = &after[end..];
    }
    out
}

pub fn message_by_id(conn: &Connection, id: Id, me: Id) -> Option<Message> {
    let base = conn
        .query_row(
            "SELECT m.*, u.display_name AS reply_author, rm.content AS reply_content, rm.author_id AS reply_author_id
             FROM messages m
             LEFT JOIN messages rm ON rm.id = m.reply_to
             LEFT JOIN users u ON u.id = rm.author_id
             WHERE m.id=?1",
            params![id],
            |r| {
                let content: String = r.get("content")?;
                let reply_to: Option<Id> = r.get("reply_to")?;
                Ok(Message {
                    id: r.get("id")?,
                    channel_id: r.get("channel_id")?,
                    author_id: r.get("author_id")?,
                    content: content.clone(),
                    timestamp: r.get("timestamp")?,
                    edited_at: r.get("edited_at")?,
                    kind: MessageKind::Default,
                    reply_to: reply_to.map(|rid| MessageReply {
                        message_id: rid,
                        author_id: r.get::<_, Option<Id>>("reply_author_id").ok().flatten().unwrap_or(0),
                        author_name: r.get::<_, Option<String>>("reply_author").ok().flatten().unwrap_or_default(),
                        content: r.get::<_, Option<String>>("reply_content").ok().flatten().unwrap_or_default(),
                    }),
                    attachments: Vec::new(),
                    reactions: Vec::new(),
                    pinned: r.get::<_, i64>("pinned")? != 0,
                    webhook: r.get::<_, Option<Id>>("webhook_id").ok().flatten().map(|wid| WebhookAuthor {
                        id: wid,
                        name: r.get::<_, Option<String>>("webhook_name").ok().flatten().unwrap_or_default(),
                        color: r.get::<_, Option<i64>>("webhook_color").ok().flatten().unwrap_or(0) as u32,
                    }),
                    thread_id: None,
                    mentions: mentions_in(&content),
                    mention_everyone: content.contains("@everyone") || content.contains("@here"),
                    pending: false,
                })
            },
        )
        .optional()
        .ok()
        .flatten()?;
    let mut msg = base;
    msg.attachments = attachments(conn, msg.id);
    msg.reactions = reactions(conn, msg.id, me);
    Some(msg)
}

// ---------------------------------------------------------------------------
// Permissions
// ---------------------------------------------------------------------------
