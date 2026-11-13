//! messages.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize)]
pub(crate) struct MessageQuery {
    before: Option<Id>,
    limit: Option<u32>,
}

pub(crate) async fn list_messages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
    Query(q): Query<MessageQuery>,
) -> Result<Json<Vec<Message>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let mut ids: Vec<Id> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id FROM messages WHERE channel_id=?1 AND (?2 IS NULL OR id<?2) ORDER BY id DESC LIMIT ?3",
    ) {
        if let Ok(rows) = stmt.query_map(params![channel, q.before, limit], |r| r.get::<_, Id>(0)) {
            ids.extend(rows.flatten());
        }
    }
    ids.reverse();
    let messages: Vec<Message> = ids
        .into_iter()
        .filter_map(|id| store::message_by_id(&conn, id, me))
        .collect();
    Ok(Json(messages))
}

pub(crate) async fn send_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
    Json(body): Json<SendMessage>,
) -> Result<Json<Message>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_send(&conn, channel, me) {
        return Err(forbidden());
    }
    let content = body.content.trim_end().to_string();
    if content.is_empty() && body.attachment_ids.is_empty() {
        return Err(bad("Message can't be empty."));
    }
    let target = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
    let mut dest = channel;
    let mut new_thread: Option<Channel> = None;
    if target.kind == ChannelKind::Forum {
        let title = thread_title(&content);
        let tid = store::next_id(&conn);
        conn.execute(
            "INSERT INTO channels(id, guild_id, name, kind, position, thread_parent_id, owner_id) VALUES(?1,?2,?3,'thread',0,?4,?5)",
            params![tid, target.guild_id, title, channel, me],
        )
        .map_err(internal)?;
        dest = tid;
        new_thread = store::channel_by_id(&conn, tid);
    }
    let id = store::next_id(&conn);
    let kind = if body.reply_to.is_some() {
        "reply"
    } else {
        "default"
    };
    conn.execute(
        "INSERT INTO messages(id, channel_id, author_id, content, timestamp, kind, reply_to) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![id, dest, me, content, now_ms(), kind, body.reply_to],
    )
    .map_err(internal)?;
    for aid in &body.attachment_ids {
        conn.execute(
            "UPDATE attachments SET message_id=?1 WHERE id=?2 AND uploader_id=?3 AND message_id IS NULL",
            params![id, aid, me],
        )
        .ok();
    }
    conn.execute(
        "UPDATE channels SET last_message_id=?1, thread_message_count=thread_message_count + CASE WHEN kind='thread' THEN 1 ELSE 0 END WHERE id=?2",
        params![id, dest],
    )
    .ok();
    conn.execute(
        "INSERT INTO reads(user_id, channel_id, last_read_id) VALUES(?1,?2,?3)
         ON CONFLICT(user_id, channel_id) DO UPDATE SET last_read_id=excluded.last_read_id",
        params![me, dest, id],
    )
    .ok();
    let msg = store::message_by_id(&conn, id, me).ok_or_else(not_found)?;
    drop(conn);
    if let Some(thread) = new_thread {
        let _ = state.tx.send(GatewayEvent::ChannelCreate(Box::new(thread)));
    }
    let _ = state
        .tx
        .send(GatewayEvent::MessageCreate(Box::new(msg.clone())));
    Ok(Json(msg))
}

pub(crate) async fn edit_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
    Json(body): Json<EditMessage>,
) -> Result<Json<Message>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let author: Option<Id> = conn
        .query_row(
            "SELECT author_id FROM messages WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?;
    let Some(author) = author else {
        return Err(not_found());
    };
    let channel: Id = conn
        .query_row(
            "SELECT channel_id FROM messages WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .map_err(internal)?;
    if author != me
        && !has(
            store::channel_permissions(&conn, channel, me),
            perm::MANAGE_MESSAGES,
        )
    {
        return Err(forbidden());
    }
    conn.execute(
        "UPDATE messages SET content=?1, edited_at=?2 WHERE id=?3",
        params![body.content.trim_end(), now_ms(), id],
    )
    .map_err(internal)?;
    let msg = store::message_by_id(&conn, id, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::MessageUpdate(Box::new(msg.clone())));
    Ok(Json(msg))
}

pub(crate) async fn delete_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let row: Option<(Id, Id)> = conn
        .query_row(
            "SELECT author_id, channel_id FROM messages WHERE id=?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(internal)?;
    let Some((author, channel)) = row else {
        return Err(not_found());
    };
    if author != me
        && !has(
            store::channel_permissions(&conn, channel, me),
            perm::MANAGE_MESSAGES,
        )
    {
        return Err(forbidden());
    }
    conn.execute("DELETE FROM messages WHERE id=?1", params![id])
        .ok();
    conn.execute("DELETE FROM reactions WHERE message_id=?1", params![id])
        .ok();
    conn.execute(
        "UPDATE channels SET thread_message_count=MAX(thread_message_count-1,0) WHERE id=?1 AND kind='thread'",
        params![channel],
    )
    .ok();
    drop(conn);
    let _ = state.tx.send(GatewayEvent::MessageDelete {
        channel_id: channel,
        message_id: id,
    });
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn toggle_reaction(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
    Json(body): Json<ReactionToggle>,
) -> Result<Json<Message>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let channel: Option<Id> = conn
        .query_row(
            "SELECT channel_id FROM messages WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?;
    let Some(channel) = channel else {
        return Err(not_found());
    };
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    let emoji = body.emoji.chars().take(8).collect::<String>();
    let existing: bool = conn
        .query_row(
            "SELECT 1 FROM reactions WHERE message_id=?1 AND emoji=?2 AND user_id=?3",
            params![id, emoji, me],
            |_| Ok(()),
        )
        .optional()
        .map_err(internal)?
        .is_some();
    if existing {
        conn.execute(
            "DELETE FROM reactions WHERE message_id=?1 AND emoji=?2 AND user_id=?3",
            params![id, emoji, me],
        )
        .ok();
    } else {
        conn.execute(
            "INSERT OR IGNORE INTO reactions(message_id, emoji, user_id) VALUES(?1,?2,?3)",
            params![id, emoji, me],
        )
        .ok();
    }
    let msg = store::message_by_id(&conn, id, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::ReactionUpdate(Box::new(msg.clone())));
    Ok(Json(msg))
}

pub(crate) async fn toggle_pin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> Result<Json<Message>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let pinned: Option<bool> = conn
        .query_row(
            "SELECT pinned FROM messages WHERE id=?1",
            params![id],
            |r| Ok(r.get::<_, i64>(0)? != 0),
        )
        .optional()
        .map_err(internal)?;
    let Some(pinned) = pinned else {
        return Err(not_found());
    };
    conn.execute(
        "UPDATE messages SET pinned=?1 WHERE id=?2",
        params![(!pinned) as i64, id],
    )
    .map_err(internal)?;
    let msg = store::message_by_id(&conn, id, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::MessageUpdate(Box::new(msg.clone())));
    Ok(Json(msg))
}

pub(crate) async fn list_pins(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
) -> Result<Json<Vec<Message>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    let mut ids = Vec::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT id FROM messages WHERE channel_id=?1 AND pinned=1 ORDER BY id DESC")
    {
        if let Ok(rows) = stmt.query_map(params![channel], |r| r.get::<_, Id>(0)) {
            ids.extend(rows.flatten());
        }
    }
    let out = ids
        .into_iter()
        .filter_map(|id| store::message_by_id(&conn, id, me))
        .collect();
    Ok(Json(out))
}

pub(crate) async fn typing(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    drop(conn);
    let _ = state.tx.send(GatewayEvent::TypingStart {
        channel_id: channel,
        user_id: me,
        until_ms: now_ms() + 8000,
    });
    Ok(Json(json!({ "ok": true })))
}

fn thread_title(content: &str) -> String {
    let line = content.lines().next().unwrap_or("New thread");
    let cleaned: String = line
        .chars()
        .filter(|c| !matches!(c, '*' | '_' | '`' | '~' | '|' | '>' | '#'))
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let name = collapsed.chars().take(60).collect::<String>();
    if name.trim().is_empty() {
        "New thread".to_string()
    } else {
        name
    }
}

pub(crate) async fn list_threads(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
) -> Result<Json<Vec<Channel>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    Ok(Json(
        store::list_threads(&conn, channel, me)
            .into_iter()
            .map(|thread| channel_for(&conn, thread, me))
            .collect::<Vec<_>>(),
    ))
}

pub(crate) async fn create_thread(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
    Json(body): Json<CreateMessageThread>,
) -> Result<Json<Channel>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    let parent = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
    let id = store::next_id(&conn);
    let name = if body.name.trim().is_empty() {
        body.message_id
            .and_then(|mid| {
                conn.query_row(
                    "SELECT content FROM messages WHERE id=?1",
                    params![mid],
                    |r| r.get::<_, String>(0),
                )
                .optional()
                .ok()
                .flatten()
            })
            .map(|content| thread_title(&content))
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "New thread".to_string())
    } else {
        body.name.trim().chars().take(80).collect()
    };
    let source: Option<(Id, String, i64)> = body.message_id.and_then(|mid| {
        conn.query_row(
            "SELECT author_id, content, timestamp FROM messages WHERE id=?1",
            params![mid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .ok()
        .flatten()
    });
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, position, thread_parent_id, owner_id, thread_message_count) VALUES(?1,?2,?3,'thread',0,?4,?5,?6)",
        params![id, parent.guild_id, name, channel, me, i64::from(source.is_some())],
    )
    .map_err(internal)?;
    if let Some((author, content, timestamp)) = source {
        let message_id = store::next_id(&conn);
        conn.execute(
            "INSERT INTO messages(id, channel_id, author_id, content, timestamp, kind) VALUES(?1,?2,?3,?4,?5,'default')",
            params![message_id, id, author, content, timestamp],
        )
        .ok();
        conn.execute(
            "UPDATE channels SET last_message_id=?1 WHERE id=?2",
            params![message_id, id],
        )
        .ok();
    }
    let thread = store::channel_by_id(&conn, id).ok_or_else(not_found)?;
    let thread = channel_for(&conn, thread, me);
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::ChannelCreate(Box::new(thread.clone())));
    Ok(Json(thread))
}

// ---------------------------------------------------------------------------
// Uploads
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct SearchQuery {
    q: String,
    limit: Option<u32>,
}

pub(crate) async fn search(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<SearchQuery>,
) -> Result<Json<SearchResults>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let term = format!("%{}%", q.q.trim());
    if q.q.trim().is_empty() {
        return Ok(Json(SearchResults {
            total: 0,
            hits: Vec::new(),
        }));
    }
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let mut rows: Vec<(Id, Id)> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, channel_id FROM messages WHERE content LIKE ?1 ORDER BY id DESC LIMIT 400",
    ) {
        if let Ok(iter) = stmt.query_map(params![term], |r| Ok((r.get(0)?, r.get(1)?))) {
            for (id, ch) in iter.flatten() {
                if store::can_view(&conn, ch, me) {
                    rows.push((id, ch));
                    if rows.len() as u32 >= limit {
                        break;
                    }
                }
            }
        }
    }
    let mut hits = Vec::new();
    for (id, ch) in rows {
        let Some(message) = store::message_by_id(&conn, id, me) else {
            continue;
        };
        let Some(channel) = store::channel_by_id(&conn, ch) else {
            continue;
        };
        let guild = channel.guild_id.and_then(|g| {
            conn.query_row("SELECT name FROM guilds WHERE id=?1", params![g], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .ok()
            .flatten()
        });
        let author = store::user_by_id(&conn, message.author_id).unwrap_or_default();
        hits.push(SearchHit {
            message,
            channel_id: ch,
            channel_name: channel.name,
            guild_id: channel.guild_id,
            guild_name: guild,
            author,
        });
    }
    Ok(Json(SearchResults {
        total: hits.len() as u32,
        hits,
    }))
}

// ---------------------------------------------------------------------------
// Friends and DMs
// ---------------------------------------------------------------------------
