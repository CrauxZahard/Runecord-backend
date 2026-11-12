//! Discord-compatible `/api/v10` surface: paths, payload shapes, query semantics and errors.
//! This runs beside the native API and uses the same store; the gateway it advertises still
//! speaks Runecord's native event envelope until Phase 2 adds Discord opcodes/RESUME.

use crate::store;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use runecord_protocol::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/users/@me", get(me))
        .route("/users/@me/guilds", get(my_guilds))
        .route("/users/{id}", get(user))
        .route("/guilds/{guild_id}", get(guild))
        .route("/guilds/{guild_id}/channels", get(guild_channels))
        .route("/guilds/{guild_id}/members", get(guild_members))
        .route(
            "/guilds/{guild_id}/members/{user_id}/roles/{role_id}",
            put(add_member_role).delete(remove_member_role),
        )
        .route("/channels/{channel_id}", get(channel))
        .route("/channels/{channel_id}/messages", get(list_messages).post(send_message))
        .route(
            "/channels/{channel_id}/messages/{message_id}",
            get(single_message).patch(edit_message).delete(delete_message),
        )
        .route(
            "/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/@me",
            put(add_reaction).delete(remove_reaction),
        )
        .route("/channels/{channel_id}/pins", get(list_pins))
        .route("/channels/{channel_id}/pins/{message_id}", put(add_pin).delete(remove_pin))
        .route("/channels/{channel_id}/typing", post(typing))
        .route("/gateway", get(gateway))
        .route("/gateway/bot", get(gateway))
        .with_state(state)
}

pub struct DiscordError(pub StatusCode, pub u32, pub String);

impl IntoResponse for DiscordError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "code": self.1, "message": self.2 }))).into_response()
    }
}

fn unauthorized() -> DiscordError {
    DiscordError(StatusCode::UNAUTHORIZED, 0, "401: Unauthorized".into())
}

fn missing_access() -> DiscordError {
    DiscordError(StatusCode::FORBIDDEN, 50001, "Missing Access".into())
}

fn unknown_channel() -> DiscordError {
    DiscordError(StatusCode::NOT_FOUND, 10003, "Unknown Channel".into())
}

fn unknown_message() -> DiscordError {
    DiscordError(StatusCode::NOT_FOUND, 10008, "Unknown Message".into())
}

fn bad(message: impl Into<String>) -> DiscordError {
    DiscordError(StatusCode::BAD_REQUEST, 50035, message.into())
}

fn auth(state: &AppState, headers: &HeaderMap) -> Result<Id, DiscordError> {
    crate::api::auth_user(state, headers).map_err(|_| unauthorized())
}

// ---------------------------------------------------------------------------
// Payload shaping
// ---------------------------------------------------------------------------

fn uid(id: Id) -> String {
    id.to_string()
}

fn iso(ts: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ts)
        .map(|d| d.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
        .unwrap_or_default()
}

fn user_json(u: &User) -> Value {
    json!({
        "id": uid(u.id),
        "username": u.username,
        "global_name": u.display_name,
        "display_name": u.display_name,
        "avatar": Value::Null,
        "bot": u.is_bot,
        "system": false,
        "public_flags": 0,
        "bio": u.bio,
        "status": match u.status {
            Presence::Online => "online",
            Presence::Idle => "idle",
            Presence::Dnd => "dnd",
            Presence::Offline => "offline",
        }
    })
}

fn guild_json(conn: &rusqlite::Connection, g: &Guild, me: Id) -> Value {
    json!({
        "id": uid(g.id),
        "name": g.name,
        "icon": Value::Null,
        "icon_color": g.icon_color,
        "description": g.description,
        "owner_id": uid(g.owner_id),
        "permissions": store::base_permissions(conn, g.id, me).to_string(),
        "features": [],
        "member_count": g.member_count
    })
}

fn channel_type(kind: ChannelKind) -> u32 {
    match kind {
        ChannelKind::Text => 0,
        ChannelKind::Dm => 1,
        ChannelKind::Voice => 2,
        ChannelKind::GroupDm => 3,
        ChannelKind::Category => 4,
        ChannelKind::Announcement => 5,
        ChannelKind::Thread => 11,
        ChannelKind::Forum => 15,
    }
}

fn channel_json(c: &Channel) -> Value {
    json!({
        "id": uid(c.id),
        "type": channel_type(c.kind),
        "guild_id": c.guild_id.map(uid),
        "name": c.name,
        "topic": c.topic,
        "position": c.position,
        "parent_id": c.parent_id.map(uid),
        "nsfw": c.nsfw,
        "rate_limit_per_user": c.slowmode,
        "last_message_id": c.last_message_id.map(uid)
    })
}

fn attachment_json(a: &Attachment) -> Value {
    json!({
        "id": uid(a.id),
        "filename": a.filename,
        "size": a.size,
        "url": a.url,
        "proxy_url": a.url,
        "content_type": a.content_type,
        "width": a.width,
        "height": a.height
    })
}

fn message_json(conn: &rusqlite::Connection, m: &Message) -> Value {
    let author = store::user_by_id(conn, m.author_id).unwrap_or_default();
    let guild_id = store::channel_by_id(conn, m.channel_id).and_then(|c| c.guild_id);
    json!({
        "id": uid(m.id),
        "channel_id": uid(m.channel_id),
        "guild_id": guild_id.map(uid),
        "author": user_json(&author),
        "content": m.content,
        "timestamp": iso(m.timestamp),
        "edited_timestamp": m.edited_at.map(iso),
        "tts": false,
        "mention_everyone": m.mention_everyone,
        "mentions": m.mentions.iter().map(|id| uid(*id)).collect::<Vec<_>>(),
        "mention_roles": [],
        "attachments": m.attachments.iter().map(attachment_json).collect::<Vec<_>>(),
        "embeds": [],
        "reactions": m.reactions.iter().map(|r| json!({
            "count": r.count,
            "me": r.me,
            "emoji": { "id": Value::Null, "name": r.emoji }
        })).collect::<Vec<_>>(),
        "pinned": m.pinned,
        "type": 0
    })
}

fn guild_of(conn: &rusqlite::Connection, guild_id: Id) -> Option<Guild> {
    conn.query_row(
        "SELECT id, name, icon_color, owner_id, description, created_at FROM guilds WHERE id=?1",
        rusqlite::params![guild_id],
        |r| {
            Ok(Guild {
                id: r.get(0)?,
                name: r.get(1)?,
                icon_color: r.get::<_, i64>(2)? as u32,
                owner_id: r.get(3)?,
                description: r.get(4)?,
                member_count: 0,
                created_at: r.get(5)?,
                ..Guild::default()
            })
        },
    )
    .ok()
}

fn is_member(conn: &rusqlite::Connection, guild: Id, user: Id) -> bool {
    conn.query_row(
        "SELECT 1 FROM guild_members WHERE guild_id=?1 AND user_id=?2",
        rusqlite::params![guild, user],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

use rusqlite::OptionalExtension as _;

fn can_manage(conn: &rusqlite::Connection, guild: Id, user: Id, bit: u64) -> bool {
    has(store::base_permissions(conn, guild, user), bit)
}

// ---------------------------------------------------------------------------
// Users and guilds
// ---------------------------------------------------------------------------

async fn me(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<Value>, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    let user = store::user_by_id(&conn, me).ok_or_else(unauthorized)?;
    let mut body = user_json(&user);
    if let Some(obj) = body.as_object_mut() {
        obj.insert("verified".into(), json!(true));
        obj.insert("email".into(), Value::Null);
        obj.insert("mfa_enabled".into(), json!(false));
        obj.insert("flags".into(), json!(0));
    }
    Ok(Json(body))
}

async fn user(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<Id>) -> Result<Json<Value>, DiscordError> {
    let _ = auth(&state, &headers)?;
    let conn = state.store.conn();
    let user = store::user_by_id(&conn, id).ok_or_else(|| DiscordError(StatusCode::NOT_FOUND, 10013, "Unknown User".into()))?;
    Ok(Json(user_json(&user)))
}

async fn my_guilds(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<Value>, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT g.id, g.name, g.icon_color, g.owner_id, g.description, g.created_at
         FROM guilds g JOIN guild_members m ON m.guild_id=g.id WHERE m.user_id=?1",
    ) {
        if let Ok(rows) = stmt.query_map(rusqlite::params![me], |r| {
            Ok(Guild {
                id: r.get(0)?,
                name: r.get(1)?,
                icon_color: r.get::<_, i64>(2)? as u32,
                owner_id: r.get(3)?,
                description: r.get(4)?,
                created_at: r.get(5)?,
                ..Guild::default()
            })
        }) {
            for g in rows.flatten() {
                out.push(guild_json(&conn, &g, me));
            }
        }
    }
    Ok(Json(Value::Array(out)))
}

async fn guild(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(guild_id): Path<Id>) -> Result<Json<Value>, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    if !is_member(&conn, guild_id, me) {
        return Err(missing_access());
    }
    let g = guild_of(&conn, guild_id).ok_or(missing_access())?;
    Ok(Json(guild_json(&conn, &g, me)))
}

async fn guild_channels(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(guild_id): Path<Id>) -> Result<Json<Value>, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    if !is_member(&conn, guild_id, me) {
        return Err(missing_access());
    }
    let out: Vec<Value> = store::list_children(&conn, guild_id)
        .iter()
        .filter(|c| store::can_view(&conn, c.id, me))
        .map(channel_json)
        .collect();
    Ok(Json(Value::Array(out)))
}

async fn guild_members(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(guild_id): Path<Id>) -> Result<Json<Value>, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    if !is_member(&conn, guild_id, me) {
        return Err(missing_access());
    }
    let out: Vec<Value> = store::guild_members(&conn, guild_id)
        .iter()
        .map(|m| json!({ "user": user_json(&m.user), "nick": m.nickname, "roles": m.roles.iter().map(|r| uid(*r)).collect::<Vec<_>>(), "joined_at": iso(m.joined_at) }))
        .collect();
    Ok(Json(Value::Array(out)))
}

async fn channel(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(channel_id): Path<Id>) -> Result<Json<Value>, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    let c = store::channel_by_id(&conn, channel_id).ok_or_else(unknown_channel)?;
    if !store::can_view(&conn, channel_id, me) {
        return Err(missing_access());
    }
    Ok(Json(channel_json(&c)))
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct MessagesQuery {
    before: Option<Id>,
    after: Option<Id>,
    around: Option<Id>,
    limit: Option<u32>,
}

async fn list_messages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id): Path<Id>,
    Query(q): Query<MessagesQuery>,
) -> Result<Json<Value>, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel_id, me) {
        return Err(missing_access());
    }
    let messages = store::list_messages_window(&conn, channel_id, me, q.before, q.after, q.around, q.limit.unwrap_or(50));
    Ok(Json(Value::Array(messages.iter().map(|m| message_json(&conn, m)).collect())))
}

#[derive(Deserialize)]
struct MessageRef {
    message_id: String,
    #[serde(default)]
    #[allow(dead_code)]
    fail_if_not_exists: bool,
}

#[derive(Deserialize)]
struct NewMessage {
    content: String,
    #[serde(default)]
    #[allow(dead_code)]
    nonce: Option<Value>,
    message_reference: Option<MessageRef>,
    #[serde(default)]
    #[allow(dead_code)]
    tts: bool,
}

async fn send_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id): Path<Id>,
    Json(body): Json<NewMessage>,
) -> Result<Json<Value>, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel_id, me) {
        return Err(unknown_channel());
    }
    if !store::can_send(&conn, channel_id, me) {
        return Err(missing_access());
    }
    let content = body.content.trim_end().to_string();
    if content.is_empty() {
        return Err(bad("Cannot send an empty message"));
    }
    let reply_to = body
        .message_reference
        .as_ref()
        .and_then(|r| r.message_id.parse::<Id>().ok())
        .filter(|id| store::message_by_id(&conn, *id, me).is_some());
    let id = store::next_id(&conn);
    conn.execute(
        "INSERT INTO messages(id, channel_id, author_id, content, timestamp, kind, reply_to) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        rusqlite::params![id, channel_id, me, content, now_ms(), if reply_to.is_some() { "reply" } else { "default" }, reply_to],
    )
    .map_err(|e| bad(e.to_string()))?;
    conn.execute("UPDATE channels SET last_message_id=?1 WHERE id=?2", rusqlite::params![id, channel_id]).ok();
    conn.execute(
        "INSERT INTO reads(user_id, channel_id, last_read_id) VALUES(?1,?2,?3)
         ON CONFLICT(user_id, channel_id) DO UPDATE SET last_read_id=excluded.last_read_id",
        rusqlite::params![me, channel_id, id],
    )
    .ok();
    let msg = store::message_by_id(&conn, id, me).ok_or_else(unknown_message)?;
    let body_json = message_json(&conn, &msg);
    drop(conn);
    let _ = state.tx.send(GatewayEvent::MessageCreate(Box::new(msg.clone())));
    Ok(Json(body_json))
}

#[derive(Deserialize)]
struct EditBody {
    content: String,
}

async fn single_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel_id, message_id)): Path<(Id, Id)>,
) -> Result<Json<Value>, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel_id, me) {
        return Err(missing_access());
    }
    let msg = store::message_by_id(&conn, message_id, me).ok_or_else(unknown_message)?;
    if msg.channel_id != channel_id {
        return Err(unknown_message());
    }
    Ok(Json(message_json(&conn, &msg)))
}

async fn edit_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel_id, message_id)): Path<(Id, Id)>,
    Json(body): Json<EditBody>,
) -> Result<Json<Value>, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    let Some(msg) = store::message_by_id(&conn, message_id, me) else { return Err(unknown_message()) };
    if msg.channel_id != channel_id {
        return Err(unknown_message());
    }
    if msg.author_id != me && !has(store::channel_permissions(&conn, channel_id, me), perm::MANAGE_MESSAGES) {
        return Err(missing_access());
    }
    conn.execute(
        "UPDATE messages SET content=?1, edited_at=?2 WHERE id=?3",
        rusqlite::params![body.content.trim_end(), now_ms(), message_id],
    )
    .map_err(|e| bad(e.to_string()))?;
    let msg = store::message_by_id(&conn, message_id, me).ok_or_else(unknown_message)?;
    let out = message_json(&conn, &msg);
    drop(conn);
    let _ = state.tx.send(GatewayEvent::MessageUpdate(Box::new(msg)));
    Ok(Json(out))
}

async fn delete_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel_id, message_id)): Path<(Id, Id)>,
) -> Result<StatusCode, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    let Some(msg) = store::message_by_id(&conn, message_id, me) else { return Err(unknown_message()) };
    if msg.channel_id != channel_id {
        return Err(unknown_message());
    }
    if msg.author_id != me && !has(store::channel_permissions(&conn, channel_id, me), perm::MANAGE_MESSAGES) {
        return Err(missing_access());
    }
    conn.execute("DELETE FROM messages WHERE id=?1", rusqlite::params![message_id]).ok();
    conn.execute("DELETE FROM reactions WHERE message_id=?1", rusqlite::params![message_id]).ok();
    drop(conn);
    let _ = state.tx.send(GatewayEvent::MessageDelete { channel_id, message_id });
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Reactions, pins, typing
// ---------------------------------------------------------------------------

fn toggle_reaction(conn: &rusqlite::Connection, channel_id: Id, message_id: Id, me: Id, emoji: &str, add: bool) -> Result<(), DiscordError> {
    let Some(msg) = store::message_by_id(conn, message_id, me) else { return Err(unknown_message()) };
    if msg.channel_id != channel_id {
        return Err(unknown_message());
    }
    let emoji: String = emoji.chars().take(8).collect();
    if add {
        conn.execute(
            "INSERT OR IGNORE INTO reactions(message_id, emoji, user_id) VALUES(?1,?2,?3)",
            rusqlite::params![message_id, emoji, me],
        )
        .ok();
    } else {
        conn.execute(
            "DELETE FROM reactions WHERE message_id=?1 AND emoji=?2 AND user_id=?3",
            rusqlite::params![message_id, emoji, me],
        )
        .ok();
    }
    Ok(())
}

async fn add_reaction(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel_id, message_id, emoji)): Path<(Id, Id, String)>,
) -> Result<StatusCode, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    toggle_reaction(&conn, channel_id, message_id, me, &emoji, true)?;
    let msg = store::message_by_id(&conn, message_id, me).ok_or_else(unknown_message)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::ReactionUpdate(Box::new(msg)));
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_reaction(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel_id, message_id, emoji)): Path<(Id, Id, String)>,
) -> Result<StatusCode, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    toggle_reaction(&conn, channel_id, message_id, me, &emoji, false)?;
    let msg = store::message_by_id(&conn, message_id, me).ok_or_else(unknown_message)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::ReactionUpdate(Box::new(msg)));
    Ok(StatusCode::NO_CONTENT)
}

async fn list_pins(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(channel_id): Path<Id>) -> Result<Json<Value>, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel_id, me) {
        return Err(missing_access());
    }
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id FROM messages WHERE channel_id=?1 AND pinned=1 ORDER BY id DESC") {
        if let Ok(rows) = stmt.query_map(rusqlite::params![channel_id], |r| r.get::<_, Id>(0)) {
            for id in rows.flatten() {
                if let Some(m) = store::message_by_id(&conn, id, me) {
                    out.push(message_json(&conn, &m));
                }
            }
        }
    }
    Ok(Json(Value::Array(out)))
}

async fn add_pin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel_id, message_id)): Path<(Id, Id)>,
) -> Result<StatusCode, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    let Some(msg) = store::message_by_id(&conn, message_id, me) else { return Err(unknown_message()) };
    if msg.channel_id != channel_id {
        return Err(unknown_message());
    }
    conn.execute("UPDATE messages SET pinned=1 WHERE id=?1", rusqlite::params![message_id]).ok();
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_pin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel_id, message_id)): Path<(Id, Id)>,
) -> Result<StatusCode, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    let Some(msg) = store::message_by_id(&conn, message_id, me) else { return Err(unknown_message()) };
    if msg.channel_id != channel_id {
        return Err(unknown_message());
    }
    conn.execute("UPDATE messages SET pinned=0 WHERE id=?1", rusqlite::params![message_id]).ok();
    Ok(StatusCode::NO_CONTENT)
}

async fn typing(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(channel_id): Path<Id>) -> Result<StatusCode, DiscordError> {
    let me = auth(&state, &headers)?;
    {
        let conn = state.store.conn();
        if !store::can_view(&conn, channel_id, me) {
            return Err(missing_access());
        }
    }
    let _ = state.tx.send(GatewayEvent::TypingStart { channel_id, user_id: me, until_ms: now_ms() + 8000 });
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Roles and gateway
// ---------------------------------------------------------------------------

async fn add_member_role(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild_id, user_id, role_id)): Path<(Id, Id, Id)>,
) -> Result<StatusCode, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    if !can_manage(&conn, guild_id, me, perm::MANAGE_ROLES) {
        return Err(missing_access());
    }
    let role_exists: bool = conn
        .query_row(
            "SELECT 1 FROM roles WHERE id=?1 AND guild_id=?2",
            rusqlite::params![role_id, guild_id],
            |_| Ok(()),
        )
        .optional()
        .ok()
        .flatten()
        .is_some();
    if !role_exists {
        return Err(DiscordError(StatusCode::NOT_FOUND, 10011, "Unknown Role".into()));
    }
    conn.execute(
        "INSERT OR IGNORE INTO member_roles(guild_id, user_id, role_id) VALUES(?1,?2,?3)",
        rusqlite::params![guild_id, user_id, role_id],
    )
    .ok();
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_member_role(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild_id, user_id, role_id)): Path<(Id, Id, Id)>,
) -> Result<StatusCode, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    if !can_manage(&conn, guild_id, me, perm::MANAGE_ROLES) {
        return Err(missing_access());
    }
    conn.execute(
        "DELETE FROM member_roles WHERE guild_id=?1 AND user_id=?2 AND role_id=?3",
        rusqlite::params![guild_id, user_id, role_id],
    )
    .ok();
    Ok(StatusCode::NO_CONTENT)
}

async fn gateway() -> Json<Value> {
    let port = std::env::var("RUNECORD_PORT").unwrap_or_else(|_| "47800".into());
    Json(json!({ "url": format!("ws://127.0.0.1:{port}/gateway") }))
}
