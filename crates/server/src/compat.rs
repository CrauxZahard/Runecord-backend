//! Discord-compatible `/api/v1` surface: paths, payload shapes, query semantics and errors.
//! This runs beside the native API and uses the same store; the gateway it advertises still
//! speaks Runecord's native event envelope until Phase 2 adds Discord opcodes/RESUME.

use crate::AppState;
use crate::store;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use runecord_protocol::*;
use serde::Deserialize;
use serde_json::{Value, json};
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
        .route(
            "/channels/{channel_id}/messages",
            get(list_messages).post(send_message),
        )
        .route(
            "/channels/{channel_id}/messages/{message_id}",
            get(single_message)
                .patch(edit_message)
                .delete(delete_message),
        )
        .route(
            "/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/@me",
            put(add_reaction).delete(remove_reaction),
        )
        .route("/channels/{channel_id}/pins", get(list_pins))
        .route(
            "/channels/{channel_id}/pins/{message_id}",
            put(add_pin).delete(remove_pin),
        )
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

pub(crate) fn iso(ts: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ts)
        .map(|d| d.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
        .unwrap_or_default()
}

pub(crate) fn user_json(u: &User) -> Value {
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
        "avatar_border": u.avatar_border,
        "banner_color": u.banner_color,
        "status": match u.status {
            Presence::Online => "online",
            Presence::Idle => "idle",
            Presence::Dnd => "dnd",
            Presence::Offline => "offline",
        }
    })
}

pub(crate) fn guild_json(conn: &rusqlite::Connection, g: &Guild, me: Id) -> Value {
    json!({
        "id": uid(g.id),
        "name": g.name,
        "icon": Value::Null,
        "icon_color": g.icon_color,
        "description": g.description,
        "owner_id": uid(g.owner_id),
        "permissions": store::base_permissions(conn, g.id, me).to_string(),
        "features": [],
        "member_count": g.member_count,
        "tag": g.tag
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

pub(crate) fn channel_json(c: &Channel) -> Value {
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

pub(crate) fn message_json(conn: &rusqlite::Connection, m: &Message) -> Value {
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
    store::guild_basic(conn, guild_id)
}

fn is_member(conn: &rusqlite::Connection, guild: Id, user: Id) -> bool {
    store::guild_member(conn, guild, user)
}

fn can_manage(conn: &rusqlite::Connection, guild: Id, user: Id, bit: u64) -> bool {
    has(store::base_permissions(conn, guild, user), bit)
}

// ---------------------------------------------------------------------------
// Users and guilds
// ---------------------------------------------------------------------------

async fn me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, DiscordError> {
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

async fn user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> Result<Json<Value>, DiscordError> {
    let _ = auth(&state, &headers)?;
    let conn = state.store.conn();
    let user = store::user_by_id(&conn, id)
        .ok_or_else(|| DiscordError(StatusCode::NOT_FOUND, 10013, "Unknown User".into()))?;
    Ok(Json(user_json(&user)))
}

async fn my_guilds(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    let out = store::guild_ids_for_user(&conn, me)
        .into_iter()
        .filter_map(|guild_id| store::guild_basic(&conn, guild_id))
        .map(|guild| guild_json(&conn, &guild, me))
        .collect();
    Ok(Json(Value::Array(out)))
}

async fn guild(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild_id): Path<Id>,
) -> Result<Json<Value>, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    if !is_member(&conn, guild_id, me) {
        return Err(missing_access());
    }
    let g = guild_of(&conn, guild_id).ok_or(missing_access())?;
    Ok(Json(guild_json(&conn, &g, me)))
}

async fn guild_channels(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild_id): Path<Id>,
) -> Result<Json<Value>, DiscordError> {
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

async fn guild_members(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild_id): Path<Id>,
) -> Result<Json<Value>, DiscordError> {
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

async fn channel(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id): Path<Id>,
) -> Result<Json<Value>, DiscordError> {
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
    let messages = store::list_messages_window(
        &conn,
        channel_id,
        me,
        q.before,
        q.after,
        q.around,
        q.limit.unwrap_or(50),
    );
    Ok(Json(Value::Array(
        messages.iter().map(|m| message_json(&conn, m)).collect(),
    )))
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
    let target = store::channel_by_id(&conn, channel_id).ok_or_else(unknown_channel)?;
    let (msg, _) = store::create_message(&conn, me, &target, content, reply_to, &[], None)
        .map_err(|error| bad(error.to_string()))?;
    let body_json = message_json(&conn, &msg);
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::MessageCreate(Box::new(msg.clone())));
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
    let Some(msg) = store::message_by_id(&conn, message_id, me) else {
        return Err(unknown_message());
    };
    if msg.channel_id != channel_id {
        return Err(unknown_message());
    }
    if msg.author_id != me
        && !has(
            store::channel_permissions(&conn, channel_id, me),
            perm::MANAGE_MESSAGES,
        )
    {
        return Err(missing_access());
    }
    store::set_message_content(&conn, message_id, body.content.trim_end(), now_ms())
        .map_err(|error| bad(error.to_string()))?;
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
    let Some(msg) = store::message_by_id(&conn, message_id, me) else {
        return Err(unknown_message());
    };
    if msg.channel_id != channel_id {
        return Err(unknown_message());
    }
    if msg.author_id != me
        && !has(
            store::channel_permissions(&conn, channel_id, me),
            perm::MANAGE_MESSAGES,
        )
    {
        return Err(missing_access());
    }
    store::delete_message(&conn, message_id, channel_id).map_err(|error| bad(error.to_string()))?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::MessageDelete {
        channel_id,
        message_id,
    });
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Reactions, pins, typing
// ---------------------------------------------------------------------------

fn toggle_reaction(
    conn: &rusqlite::Connection,
    channel_id: Id,
    message_id: Id,
    me: Id,
    emoji: &str,
    add: bool,
) -> Result<(), DiscordError> {
    let Some(msg) = store::message_by_id(conn, message_id, me) else {
        return Err(unknown_message());
    };
    if msg.channel_id != channel_id {
        return Err(unknown_message());
    }
    let emoji: String = emoji.chars().take(8).collect();
    store::set_reaction(conn, message_id, me, &emoji, add);
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

async fn list_pins(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id): Path<Id>,
) -> Result<Json<Value>, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel_id, me) {
        return Err(missing_access());
    }
    let out = store::pinned_ids(&conn, channel_id)
        .into_iter()
        .filter_map(|id| store::message_by_id(&conn, id, me))
        .map(|message| message_json(&conn, &message))
        .collect();
    Ok(Json(Value::Array(out)))
}

async fn add_pin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel_id, message_id)): Path<(Id, Id)>,
) -> Result<StatusCode, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    let Some(msg) = store::message_by_id(&conn, message_id, me) else {
        return Err(unknown_message());
    };
    if msg.channel_id != channel_id {
        return Err(unknown_message());
    }
    store::set_pin(&conn, message_id, me, true);
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_pin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel_id, message_id)): Path<(Id, Id)>,
) -> Result<StatusCode, DiscordError> {
    let me = auth(&state, &headers)?;
    let conn = state.store.conn();
    let Some(msg) = store::message_by_id(&conn, message_id, me) else {
        return Err(unknown_message());
    };
    if msg.channel_id != channel_id {
        return Err(unknown_message());
    }
    store::set_pin(&conn, message_id, me, false);
    Ok(StatusCode::NO_CONTENT)
}

async fn typing(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id): Path<Id>,
) -> Result<StatusCode, DiscordError> {
    let me = auth(&state, &headers)?;
    {
        let conn = state.store.conn();
        if !store::can_view(&conn, channel_id, me) {
            return Err(missing_access());
        }
    }
    let _ = state.tx.send(GatewayEvent::TypingStart {
        channel_id,
        user_id: me,
        until_ms: now_ms() + 8000,
    });
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
    if store::role_in_guild(&conn, guild_id, role_id).is_none() {
        return Err(DiscordError(
            StatusCode::NOT_FOUND,
            10011,
            "Unknown Role".into(),
        ));
    }
    store::add_member_role(&conn, guild_id, user_id, role_id);
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
    store::remove_member_role(&conn, guild_id, user_id, role_id);
    Ok(StatusCode::NO_CONTENT)
}

async fn gateway(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({ "url": format!("ws://127.0.0.1:{}/gateway/v1", state.port) }))
}
