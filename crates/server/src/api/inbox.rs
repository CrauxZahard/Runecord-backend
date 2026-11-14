//! The Inbox: recent mentions and unread channels for the signed-in user.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use std::collections::HashMap;
use std::sync::Arc;

const MENTION_LIMIT: usize = 20;
const UNREAD_LIMIT: usize = 30;

pub(crate) async fn inbox(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Inbox>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let mut mentions = Vec::new();
    let mut unreads = Vec::new();
    let mut channels: HashMap<Id, Option<Channel>> = HashMap::new();

    for (message, channel_id, author, content, timestamp, author_name) in
        store::mention_rows(&conn, me)
    {
        if mentions.len() >= MENTION_LIMIT {
            break;
        }
        let channel = channels
            .entry(channel_id)
            .or_insert_with(|| store::channel_by_id(&conn, channel_id))
            .clone();
        let Some(channel) = channel else {
            continue;
        };
        if !store::can_view(&conn, channel_id, me) {
            continue;
        }
        let guild = channel.guild_id;
        let suppressed = store::channel_suppressed(&conn, me, channel_id);
        if !mentions_me(&conn, &content, me, guild, suppressed) {
            continue;
        }
        let (guild_name, channel_name) = labels(&conn, &channel, me);
        mentions.push(InboxEntry {
            channel_id,
            guild_id: guild,
            message_id: message,
            guild_name,
            channel_name,
            author_id: author,
            author_name,
            snippet: snippet(&content),
            timestamp,
            unread: 1,
            mention: true,
        });
    }

    for guild in store::guild_ids_for_user(&conn, me) {
        for (channel_id, count) in store::guild_unread_channels(&conn, guild, me) {
            if unreads.len() >= UNREAD_LIMIT {
                break;
            }
            if let Some(entry) = latest_entry(&conn, channel_id, me, count, false) {
                unreads.push(entry);
            }
        }
    }

    for channel in store::dm_channel_ids(&conn, me) {
        if unreads.len() >= UNREAD_LIMIT {
            break;
        }
        let count = store::unread_for(&conn, channel, me);
        if count == 0 {
            continue;
        }
        if let Some(entry) = latest_entry(&conn, channel, me, count, false) {
            unreads.push(entry);
        }
    }

    mentions.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp));
    unreads.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp));
    Ok(Json(Inbox { mentions, unreads }))
}

fn latest_entry(
    conn: &rusqlite::Connection,
    channel_id: Id,
    me: Id,
    count: u32,
    mention: bool,
) -> Option<InboxEntry> {
    let channel = store::channel_by_id(conn, channel_id)?;
    if !store::can_view(conn, channel_id, me) {
        return None;
    }
    let (message, author, author_name, content, timestamp) =
        store::latest_message(conn, channel_id, me)?;
    let (guild_name, channel_name) = labels(conn, &channel, me);
    Some(InboxEntry {
        channel_id,
        guild_id: channel.guild_id,
        message_id: message,
        guild_name,
        channel_name,
        author_id: author,
        author_name,
        snippet: snippet(&content),
        timestamp,
        unread: count,
        mention,
    })
}

fn labels(conn: &rusqlite::Connection, channel: &Channel, me: Id) -> (String, String) {
    if let Some(guild) = channel.guild_id {
        return (
            store::guild_name(conn, guild).unwrap_or_default(),
            channel.name.clone(),
        );
    }
    let names = store::dm_recipient_names(conn, channel.id, me);
    let title = if channel.name.is_empty() {
        names.join(", ")
    } else {
        channel.name.clone()
    };
    ("Direct Messages".into(), title)
}

fn mentions_me(
    conn: &rusqlite::Connection,
    content: &str,
    me: Id,
    guild: Option<Id>,
    suppress_everyone: bool,
) -> bool {
    if content.contains(&format!("<@{me}>")) {
        return true;
    }
    if !suppress_everyone && (content.contains("@everyone") || content.contains("@here")) {
        return true;
    }
    if let Some(guild) = guild {
        for role in store::member_role_ids(conn, guild, me) {
            if content.contains(&format!("<@&{role}>")) {
                return true;
            }
        }
    }
    false
}

fn snippet(content: &str) -> String {
    let flat: String = content
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let trimmed = flat.trim();
    if trimmed.chars().count() <= 140 {
        trimmed.to_string()
    } else {
        let mut out: String = trimmed.chars().take(137).collect();
        out.push_str("...");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{mentions_me, snippet};
    use crate::store;
    use rusqlite::Connection;

    #[test]
    fn snippets_flatten_newlines_and_truncate() {
        assert_eq!(snippet("hello\nworld"), "hello world");
        let long = "x".repeat(200);
        let out = snippet(&long);
        assert_eq!(out.chars().count(), 140);
        assert!(out.ends_with("..."));
    }

    #[test]
    fn mentions_cover_direct_everyone_and_roles() {
        let conn = Connection::open_in_memory().expect("memory db");
        store::migrate(&conn).expect("migrate");
        conn.execute(
            "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(1,'nova','Nova','x',1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,1,'',1,10)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(10,1,'@everyone',0,0,0,0,0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(11,1,'Designer',0,0,1,1,1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO member_roles(guild_id, user_id, role_id) VALUES(1,1,11)",
            [],
        )
        .unwrap();

        assert!(mentions_me(&conn, "<@1> hi", 1, Some(1), false));
        assert!(mentions_me(&conn, "@everyone hi", 1, Some(1), false));
        assert!(!mentions_me(&conn, "@everyone hi", 1, Some(1), true));
        assert!(mentions_me(&conn, "<@&11> hi", 1, Some(1), false));
        assert!(!mentions_me(&conn, "<@&10> hi", 1, Some(1), false));
        assert!(!mentions_me(&conn, "plain", 1, Some(1), false));
    }
}
