//! HTTP API and the WebSocket gateway.

use crate::{store, AppState};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use runecord_protocol::*;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

// ---------------------------------------------------------------------------
// Errors and auth
// ---------------------------------------------------------------------------

pub struct ApiError(pub StatusCode, pub String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

fn bad(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

fn unauthorized() -> ApiError {
    ApiError(StatusCode::UNAUTHORIZED, "invalid or missing session".into())
}

fn forbidden() -> ApiError {
    ApiError(StatusCode::FORBIDDEN, "you don't have permission to do that".into())
}

fn not_found() -> ApiError {
    ApiError(StatusCode::NOT_FOUND, "not found".into())
}

fn internal(err: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

pub fn auth_user(state: &AppState, headers: &HeaderMap) -> Result<Id, ApiError> {
    let value = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(unauthorized)?;
    let conn = state.store.conn();
    if let Some(token) = value.strip_prefix("Bearer ") {
        let me: Option<Id> = conn
            .query_row("SELECT user_id FROM sessions WHERE token=?1", params![store::hash_bot_token(token.trim())], |r| r.get(0))
            .optional()
            .map_err(internal)?;
        return me.ok_or_else(unauthorized);
    }    if let Some(token) = value.strip_prefix("Bot ") {
        let bot = store::bot_for_token(&conn, token.trim()).ok_or_else(unauthorized)?;
        if !store::bot_scopes(&conn, bot).iter().any(|s| s == "bot") {
            return Err(unauthorized());
        }
        return Ok(bot);
    }
    Err(unauthorized())
}

/// Session auth for human-only endpoints (login, bot management, friends).
pub fn auth_human(state: &AppState, headers: &HeaderMap) -> Result<Id, ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(unauthorized)?;
    let conn = state.store.conn();
    conn.query_row("SELECT user_id FROM sessions WHERE token=?1", params![store::hash_bot_token(token)], |r| r.get(0))
        .optional()
        .map_err(internal)?
        .ok_or_else(unauthorized)
}

fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn argon() -> argon2::Argon2<'static> {
    let params = argon2::Params::new(8 * 1024, 1, 1, None).unwrap_or_default();
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
}

fn hash_password(password: &str) -> String {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    let salt = SaltString::generate(&mut OsRng);
    argon()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .unwrap_or_default()
}

fn verify_password(hash: &str, password: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| argon().verify_password(password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

fn make_session(conn: &rusqlite::Connection, user_id: Id) -> String {
    let token = new_token();
    conn.execute(
        "INSERT INTO sessions(token, user_id, created_at) VALUES(?1,?2,?3)",
        params![store::hash_bot_token(&token), user_id, now_ms()],
    )
    .ok();
    token
}

fn valid_username(name: &str) -> bool {
    (2..=32).contains(&name.chars().count())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/api/version", get(|| async { Json(json!({ "name": "runecord", "version": "0.1.0" })) }))
        .route("/api/register", post(register))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/bootstrap", get(bootstrap))
        .route("/api/me", patch(update_me))
        .route("/api/users/{id}", get(get_user))
        .route("/api/users/batch", post(get_users_batch))
        .route("/api/read", post(update_read))
        .route("/api/bots", get(list_bots).post(create_bot_account))
        .route("/api/bots/{id}", delete(delete_bot_account))
        .route("/api/guilds", post(create_guild))
        .route("/api/guilds/{id}", get(guild_info).patch(update_guild).delete(delete_guild))
        .route("/api/guilds/{id}/channels", post(create_channel))
        .route("/api/guilds/{id}/members", get(list_guild_members))
        .route("/api/guilds/{id}/members/{user_id}", get(get_guild_member).patch(update_member))
        .route("/api/guilds/{id}/roles", get(list_roles).post(create_role))
        .route("/api/guilds/{id}/permissions", get(my_permissions))
        .route("/api/channels/{id}/permissions", get(channel_overwrites))
        .route("/api/channels/{id}/permissions/{target}", put(set_channel_overwrite).delete(clear_channel_overwrite))
        .route("/api/guilds/{id}/roles/{role_id}", patch(update_role).delete(delete_role))
        .route("/api/guilds/{id}/invites", post(create_invite))
        .route("/api/invites/{code}", get(invite_info))
        .route("/api/invites/{code}/join", post(join_invite))
        .route("/api/channels/{id}", patch(update_channel).delete(delete_channel))
        .route("/api/channels/{id}/messages", get(list_messages).post(send_message))
        .route("/api/channels/{id}/typing", post(typing))
        .route("/api/channels/{id}/upload", post(upload))
        .route("/cdn/{file}", get(cdn_file))
        .route("/api/channels/{id}/pins", get(list_pins))
        .route("/api/channels/{id}/threads", get(list_threads).post(create_thread))
        .route("/api/messages/{id}", patch(edit_message).delete(delete_message))
        .route("/api/messages/{id}/reactions", post(toggle_reaction))
        .route("/api/messages/{id}/pin", post(toggle_pin))
        .route("/api/search", get(search))
        .route("/api/friends/{id}/request", post(friend_request))
        .route("/api/friends/{id}/accept", post(friend_accept))
        .route("/api/friends/{id}/remove", post(friend_remove))
        .route("/api/friends/{id}/block", post(friend_block))
        .route("/api/dms/{id}", post(open_dm))
        .route("/gateway", get(gateway))
        .route("/gateway/v1", get(crate::gateway_v10::handler))
        .with_state(state)
}

async fn register(State(state): State<Arc<AppState>>, Json(body): Json<AuthRequest>) -> Result<Json<AuthResponse>, ApiError> {
    let username = body.username.trim().to_lowercase();
    if !valid_username(&username) {
        return Err(bad("Username must be 2-32 characters: letters, numbers, dots or underscores."));
    }
    if body.password.len() < 4 {
        return Err(bad("Password must be at least 4 characters."));
    }
    let conn = state.store.conn();
    if store::user_by_name(&conn, &username).is_some() {
        return Err(bad("That username is already taken."));
    }
    let id = store::next_id(&conn);
    let display = if username.is_empty() { username.clone() } else { username.clone() };
    let color = (id.unsigned_abs() % 10) as u32;
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, avatar_color, status, created_at)
         VALUES(?1,?2,?3,?4,?5,'online',?6)",
        params![id, username, display, hash_password(&body.password), color as i64, now_ms()],
    )
    .map_err(internal)?;
    let token = make_session(&conn, id);
    let user = store::user_by_id(&conn, id).ok_or_else(not_found)?;
    Ok(Json(AuthResponse { token, user }))
}

async fn login(State(state): State<Arc<AppState>>, Json(body): Json<AuthRequest>) -> Result<Json<AuthResponse>, ApiError> {
    let conn = state.store.conn();
    let row: Option<(Id, String)> = conn
        .query_row(
            "SELECT id, pass_hash FROM users WHERE username=?1",
            params![body.username.trim()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(internal)?;
    let Some((id, hash)) = row else {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "Wrong username or password.".into()));
    };
    if !verify_password(&hash, &body.password) {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "Wrong username or password.".into()));
    }
    let token = make_session(&conn, id);
    let user = store::user_by_id(&conn, id).ok_or_else(not_found)?;
    Ok(Json(AuthResponse { token, user }))
}

async fn logout(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<serde_json::Value>, ApiError> {
    auth_user(&state, &headers)?;
    if let Some(token) = headers.get("authorization").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")) {
        let conn = state.store.conn();
        conn.execute("DELETE FROM sessions WHERE token=?1", params![store::hash_bot_token(token.trim())]).ok();
    }
    Ok(Json(json!({ "ok": true })))
}

async fn bootstrap(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<Bootstrap>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let mut out = crate::store::Store::bootstrap(&state.store, me);
    out.voice_states = state
        .voice
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .values()
        .cloned()
        .collect();
    Ok(Json(out))
}

async fn get_user(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(user): Path<Id>) -> Result<Json<User>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::users_related(&conn, me, user) {
        return Err(forbidden());
    }
    store::user_by_id(&conn, user).map(Json).ok_or_else(not_found)
}

#[derive(Deserialize)]
struct UserIds {
    #[serde(default)]
    ids: Vec<Id>,
}

async fn get_users_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<UserIds>,
) -> Result<Json<Vec<User>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let mut out = Vec::new();
    for user in body.ids.into_iter().take(100) {
        if store::users_related(&conn, me, user) {
            if let Some(user) = store::user_by_id(&conn, user) {
                out.push(user);
            }
        }
    }
    Ok(Json(out))
}

async fn guild_info(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(guild): Path<Id>) -> Result<Json<GuildDetail>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::guild_member(&conn, guild, me) {
        return Err(not_found());
    }
    let detail = store::guild_detail(&conn, guild, me).ok_or_else(not_found)?;
    if !has(detail.permissions, perm::VIEW_CHANNEL) {
        return Err(forbidden());
    }
    Ok(Json(detail))
}

#[derive(Deserialize)]
struct MembersQuery {
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    role_id: Option<Id>,
    #[serde(default)]
    status: Option<String>,
}

async fn list_guild_members(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
    Query(query): Query<MembersQuery>,
) -> Result<Json<MemberPage>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::guild_member(&conn, guild, me) {
        return Err(not_found());
    }
    if !has(store::base_permissions(&conn, guild, me), perm::VIEW_CHANNEL) {
        return Err(forbidden());
    }
    let page = store::members_page(
        &conn,
        guild,
        me,
        query.offset.unwrap_or(0),
        query.limit.unwrap_or(100),
        query.q.as_deref().unwrap_or(""),
        query.role_id.unwrap_or(0),
        query.status.as_deref().unwrap_or("all"),
    )
    .ok_or_else(not_found)?;
    Ok(Json(page))
}

async fn get_guild_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, user)): Path<(Id, Id)>,
) -> Result<Json<Member>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::guild_member(&conn, guild, me) {
        return Err(not_found());
    }
    if !has(store::base_permissions(&conn, guild, me), perm::VIEW_CHANNEL) {
        return Err(forbidden());
    }
    let member = store::guild_member_detail(&conn, guild, user).ok_or_else(not_found)?;
    Ok(Json(member))
}

#[derive(Deserialize)]
struct CreateBotBody {
    name: String,
    guild_id: Option<Id>,
}

#[derive(serde::Serialize)]
struct BotCreated {
    bot: User,
    token: String,
}

async fn list_bots(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<Vec<User>>, ApiError> {
    let me = auth_human(&state, &headers)?;
    let conn = state.store.conn();
    Ok(Json(store::bots_owned(&conn, me)))
}

async fn create_bot_account(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateBotBody>,
) -> Result<Json<BotCreated>, ApiError> {
    let me = auth_human(&state, &headers)?;
    let conn = state.store.conn();
    if let Some(guild) = body.guild_id {
        if !has(store::base_permissions(&conn, guild, me), perm::MANAGE_GUILD) {
            return Err(forbidden());
        }
    }
    let (bot, token) = store::create_bot(&conn, me, &body.name).map_err(bad)?;
    if let Some(guild) = body.guild_id {
        let _ = store::add_bot_to_guild(&conn, bot.id, guild);
        let member = Member { guild_id: guild, user: bot.clone(), ..Member::default() };
        drop(conn);
        let _ = state.tx.send(GatewayEvent::MemberUpdate(Box::new(member)));
        return Ok(Json(BotCreated { bot, token }));
    }
    drop(conn);
    Ok(Json(BotCreated { bot, token }))
}

async fn delete_bot_account(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_human(&state, &headers)?;
    let conn = state.store.conn();
    if !store::delete_bot(&conn, me, id) {
        return Err(not_found());
    }
    drop(conn);
    let _ = state.tx.send(GatewayEvent::GuildMembers { guild_id: 0, members: Vec::new() });
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct MeBody {    display_name: Option<String>,
    avatar_color: Option<u32>,
    avatar_border: Option<u32>,
    banner_color: Option<u32>,
    bio: Option<String>,
    custom_status: Option<String>,
}

async fn update_me(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<MeBody>) -> Result<Json<User>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if let Some(name) = body.display_name.filter(|n| !n.trim().is_empty()) {
        conn.execute("UPDATE users SET display_name=?1 WHERE id=?2", params![name.trim(), me]).map_err(internal)?;
    }
    if let Some(color) = body.avatar_color {
        conn.execute("UPDATE users SET avatar_color=?1 WHERE id=?2", params![color as i64, me]).map_err(internal)?;
    }
    if let Some(border) = body.avatar_border {
        conn.execute("UPDATE users SET avatar_border=?1 WHERE id=?2", params![border as i64, me]).map_err(internal)?;
    }
    if let Some(banner) = body.banner_color {
        conn.execute("UPDATE users SET banner_color=?1 WHERE id=?2", params![banner as i64, me]).map_err(internal)?;
    }
    if let Some(bio) = body.bio {
        conn.execute("UPDATE users SET bio=?1 WHERE id=?2", params![bio, me]).map_err(internal)?;
    }
    if let Some(status) = body.custom_status {
        conn.execute("UPDATE users SET custom_status=?1 WHERE id=?2", params![status, me]).map_err(internal)?;
    }
    let user = store::user_by_id(&conn, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::UserUpdate(Box::new(user.clone())));
    Ok(Json(user))
}

async fn update_read(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<ReadUpdate>) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    conn.execute(
        "INSERT INTO reads(user_id, channel_id, last_read_id) VALUES(?1,?2,?3)
         ON CONFLICT(user_id, channel_id) DO UPDATE SET last_read_id=excluded.last_read_id",
        params![me, body.channel_id, body.last_read_id],
    )
    .map_err(internal)?;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Guilds and channels
// ---------------------------------------------------------------------------

async fn create_guild(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<CreateGuild>) -> Result<Json<Guild>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(bad("Give the server a name."));
    }
    let mut conn = state.store.conn();
    let tx = conn.transaction().map_err(internal)?;
    let gid = store::next_id(&tx);
    let everyone = store::next_id(&tx);
    tx.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(?1,?2,?3,?4,'',?5,?6)",
        params![gid, name, body.icon_color as i64, me, now_ms(), everyone],
    )
    .map_err(internal)?;
    tx.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(?1,?2,'@everyone',0,?3,0,0,0)",
        params![everyone, gid, (perm::EVERYONE | perm::CREATE_INVITE) as i64],
    )
    .map_err(internal)?;
    tx.execute(
        "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(?1,?2,'',?3)",
        params![gid, me, now_ms()],
    )
    .map_err(internal)?;
    let cat = store::next_id(&tx);
    tx.execute(
        "INSERT INTO channels(id, guild_id, name, kind, position, topic) VALUES(?1,?2,'Text Channels','category',0,'')",
        params![cat, gid],
    )
    .map_err(internal)?;
    let general = store::next_id(&tx);
    tx.execute(
        "INSERT INTO channels(id, guild_id, name, kind, position, parent_id, topic) VALUES(?1,?2,'general','text',1,?3,'This is where it all begins')",
        params![general, gid, cat],
    )
    .map_err(internal)?;
    tx.commit().map_err(internal)?;
    drop(conn);

    let guild = Guild {
        id: gid,
        name: name.to_string(),
        icon_color: body.icon_color,
        owner_id: me,
        member_count: 1,
        created_at: now_ms(),
        channels: vec![
            Channel { id: cat, guild_id: Some(gid), name: "Text Channels".into(), kind: ChannelKind::Category, position: 0, ..Channel::default() },
            Channel {
                id: general,
                guild_id: Some(gid),
                name: "general".into(),
                kind: ChannelKind::Text,
                position: 1,
                parent_id: Some(cat),
                topic: "This is where it all begins".into(),
                ..Channel::default()
            },
        ],
        ..Guild::default()
    };
    let _ = state.tx.send(GatewayEvent::GuildCreate(Box::new(guild.clone())));
    Ok(Json(guild))
}

async fn update_guild(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<Id>, Json(body): Json<UpdateGuild>) -> Result<Json<Guild>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let owner: Option<Id> = conn.query_row("SELECT owner_id FROM guilds WHERE id=?1", params![id], |r| r.get(0)).optional().map_err(internal)?;
    if owner != Some(me) {
        return Err(forbidden());
    }
    if let Some(name) = body.name {
        conn.execute("UPDATE guilds SET name=?1 WHERE id=?2", params![name.trim(), id]).map_err(internal)?;
    }
    if let Some(color) = body.icon_color {
        conn.execute("UPDATE guilds SET icon_color=?1 WHERE id=?2", params![color as i64, id]).map_err(internal)?;
    }
    if let Some(tag) = body.tag {
        conn.execute("UPDATE guilds SET tag=?1 WHERE id=?2", params![tag.chars().take(8).collect::<String>(), id]).map_err(internal)?;
    }
    if let Some(desc) = body.description {
        conn.execute("UPDATE guilds SET description=?1 WHERE id=?2", params![desc, id]).map_err(internal)?;
    }
    let guild = Guild { id, ..Guild::default() };
    let _ = state.tx.send(GatewayEvent::GuildUpdate(Box::new(guild.clone())));
    Ok(Json(guild))
}

async fn delete_guild(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<Id>) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let owner: Option<Id> = conn.query_row("SELECT owner_id FROM guilds WHERE id=?1", params![id], |r| r.get(0)).optional().map_err(internal)?;
    if owner != Some(me) {
        return Err(forbidden());
    }
    conn.execute("DELETE FROM channels WHERE guild_id=?1", params![id]).ok();
    conn.execute("DELETE FROM guild_members WHERE guild_id=?1", params![id]).ok();
    conn.execute("DELETE FROM roles WHERE guild_id=?1", params![id]).ok();
    conn.execute("DELETE FROM guilds WHERE id=?1", params![id]).ok();
    let _ = state.tx.send(GatewayEvent::GuildDelete { guild_id: id });
    Ok(Json(json!({ "ok": true })))
}

async fn create_channel(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(guild): Path<Id>, Json(body): Json<CreateChannel>) -> Result<Json<Channel>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(store::base_permissions(&conn, guild, me), perm::MANAGE_CHANNELS) {
        return Err(forbidden());
    }
    let name = body.name.trim().to_lowercase().replace(' ', "-");
    if name.is_empty() {
        return Err(bad("Name the channel."));
    }
    let id = store::next_id(&conn);
    let pos: i64 = conn.query_row("SELECT COALESCE(MAX(position),0)+1 FROM channels WHERE guild_id=?1", params![guild], |r| r.get(0)).unwrap_or(1);
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, position, parent_id, topic) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![id, guild, name, kind_str(body.kind), pos, body.parent_id, body.topic],
    )
    .map_err(internal)?;
    let channel = store::channel_by_id(&conn, id).ok_or_else(not_found)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::ChannelCreate(Box::new(channel.clone())));
    Ok(Json(channel))
}

fn kind_str(kind: ChannelKind) -> &'static str {
    match kind {
        ChannelKind::Text => "text",
        ChannelKind::Voice => "voice",
        ChannelKind::Category => "category",
        ChannelKind::Announcement => "announcement",
        ChannelKind::Forum => "forum",
        ChannelKind::Thread => "thread",
        ChannelKind::Dm => "dm",
        ChannelKind::GroupDm => "group_dm",
    }
}

async fn update_channel(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<Id>, Json(body): Json<CreateChannel>) -> Result<Json<Channel>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let guild: Option<Id> = conn.query_row("SELECT guild_id FROM channels WHERE id=?1", params![id], |r| r.get(0)).optional().map_err(internal)?;
    if let Some(guild) = guild {
        if !has(store::base_permissions(&conn, guild, me), perm::MANAGE_CHANNELS) {
            return Err(forbidden());
        }
    }
    conn.execute("UPDATE channels SET name=?1, topic=?2 WHERE id=?3", params![body.name.trim(), body.topic, id]).map_err(internal)?;
    let channel = store::channel_by_id(&conn, id).ok_or_else(not_found)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::ChannelUpdate(Box::new(channel.clone())));
    Ok(Json(channel))
}

async fn delete_channel(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<Id>) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let guild: Option<Id> = conn.query_row("SELECT guild_id FROM channels WHERE id=?1", params![id], |r| r.get(0)).optional().map_err(internal)?;
    if let Some(guild) = guild {
        if !has(store::base_permissions(&conn, guild, me), perm::MANAGE_CHANNELS) {
            return Err(forbidden());
        }
    }
    conn.execute("DELETE FROM channels WHERE id=?1", params![id]).ok();
    drop(conn);
    let _ = state.tx.send(GatewayEvent::ChannelDelete { channel_id: id, guild_id: guild });
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct MessageQuery {
    before: Option<Id>,
    limit: Option<u32>,
}

async fn list_messages(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(channel): Path<Id>, Query(q): Query<MessageQuery>) -> Result<Json<Vec<Message>>, ApiError> {
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
    let messages: Vec<Message> = ids.into_iter().filter_map(|id| store::message_by_id(&conn, id, me)).collect();
    Ok(Json(messages))
}

async fn send_message(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(channel): Path<Id>, Json(body): Json<SendMessage>) -> Result<Json<Message>, ApiError> {
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
    if target.kind == ChannelKind::Forum {
        let title: String = content.lines().next().unwrap_or("New post").chars().take(80).collect();
        let tid = store::next_id(&conn);
        conn.execute(
            "INSERT INTO channels(id, guild_id, name, kind, position, thread_parent_id, owner_id) VALUES(?1,?2,?3,'thread',0,?4,?5)",
            params![tid, target.guild_id, title, channel, me],
        )
        .map_err(internal)?;
        dest = tid;
    }
    let id = store::next_id(&conn);
    let kind = if body.reply_to.is_some() { "reply" } else { "default" };
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
    conn.execute("UPDATE channels SET last_message_id=?1 WHERE id=?2", params![id, dest]).ok();
    conn.execute(
        "INSERT INTO reads(user_id, channel_id, last_read_id) VALUES(?1,?2,?3)
         ON CONFLICT(user_id, channel_id) DO UPDATE SET last_read_id=excluded.last_read_id",
        params![me, dest, id],
    )
    .ok();
    let msg = store::message_by_id(&conn, id, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::MessageCreate(Box::new(msg.clone())));
    Ok(Json(msg))
}

async fn edit_message(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<Id>, Json(body): Json<EditMessage>) -> Result<Json<Message>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let author: Option<Id> = conn.query_row("SELECT author_id FROM messages WHERE id=?1", params![id], |r| r.get(0)).optional().map_err(internal)?;
    let Some(author) = author else { return Err(not_found()) };
    let channel: Id = conn.query_row("SELECT channel_id FROM messages WHERE id=?1", params![id], |r| r.get(0)).map_err(internal)?;
    if author != me && !has(store::channel_permissions(&conn, channel, me), perm::MANAGE_MESSAGES) {
        return Err(forbidden());
    }
    conn.execute(
        "UPDATE messages SET content=?1, edited_at=?2 WHERE id=?3",
        params![body.content.trim_end(), now_ms(), id],
    )
    .map_err(internal)?;
    let msg = store::message_by_id(&conn, id, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::MessageUpdate(Box::new(msg.clone())));
    Ok(Json(msg))
}

async fn delete_message(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<Id>) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let row: Option<(Id, Id)> = conn
        .query_row("SELECT author_id, channel_id FROM messages WHERE id=?1", params![id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .map_err(internal)?;
    let Some((author, channel)) = row else { return Err(not_found()) };
    if author != me && !has(store::channel_permissions(&conn, channel, me), perm::MANAGE_MESSAGES) {
        return Err(forbidden());
    }
    conn.execute("DELETE FROM messages WHERE id=?1", params![id]).ok();
    conn.execute("DELETE FROM reactions WHERE message_id=?1", params![id]).ok();
    drop(conn);
    let _ = state.tx.send(GatewayEvent::MessageDelete { channel_id: channel, message_id: id });
    Ok(Json(json!({ "ok": true })))
}

async fn toggle_reaction(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<Id>, Json(body): Json<ReactionToggle>) -> Result<Json<Message>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let channel: Option<Id> = conn.query_row("SELECT channel_id FROM messages WHERE id=?1", params![id], |r| r.get(0)).optional().map_err(internal)?;
    let Some(channel) = channel else { return Err(not_found()) };
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
        conn.execute("DELETE FROM reactions WHERE message_id=?1 AND emoji=?2 AND user_id=?3", params![id, emoji, me]).ok();
    } else {
        conn.execute("INSERT OR IGNORE INTO reactions(message_id, emoji, user_id) VALUES(?1,?2,?3)", params![id, emoji, me]).ok();
    }
    let msg = store::message_by_id(&conn, id, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::ReactionUpdate(Box::new(msg.clone())));
    Ok(Json(msg))
}

async fn toggle_pin(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<Id>) -> Result<Json<Message>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let pinned: Option<bool> = conn.query_row("SELECT pinned FROM messages WHERE id=?1", params![id], |r| Ok(r.get::<_, i64>(0)? != 0)).optional().map_err(internal)?;
    let Some(pinned) = pinned else { return Err(not_found()) };
    conn.execute("UPDATE messages SET pinned=?1 WHERE id=?2", params![(!pinned) as i64, id]).map_err(internal)?;
    let msg = store::message_by_id(&conn, id, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::MessageUpdate(Box::new(msg.clone())));
    Ok(Json(msg))
}

async fn list_pins(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(channel): Path<Id>) -> Result<Json<Vec<Message>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    let mut ids = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id FROM messages WHERE channel_id=?1 AND pinned=1 ORDER BY id DESC") {
        if let Ok(rows) = stmt.query_map(params![channel], |r| r.get::<_, Id>(0)) {
            ids.extend(rows.flatten());
        }
    }
    let out = ids.into_iter().filter_map(|id| store::message_by_id(&conn, id, me)).collect();
    Ok(Json(out))
}

async fn typing(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(channel): Path<Id>) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    drop(conn);
    let _ = state.tx.send(GatewayEvent::TypingStart { channel_id: channel, user_id: me, until_ms: now_ms() + 8000 });
    Ok(Json(json!({ "ok": true })))
}

async fn list_threads(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(channel): Path<Id>) -> Result<Json<Vec<Channel>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    Ok(Json(store::list_threads(&conn, channel, me)))
}

async fn create_thread(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(channel): Path<Id>, Json(body): Json<CreateMessageThread>) -> Result<Json<Channel>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    let parent = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
    let id = store::next_id(&conn);
    let name = if body.name.trim().is_empty() { "New thread".to_string() } else { body.name.trim().chars().take(80).collect() };
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, position, thread_parent_id, owner_id) VALUES(?1,?2,?3,'thread',0,?4,?5)",
        params![id, parent.guild_id, name, channel, me],
    )
    .map_err(internal)?;
    let thread = store::channel_by_id(&conn, id).ok_or_else(not_found)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::ChannelCreate(Box::new(thread.clone())));
    Ok(Json(thread))
}

// ---------------------------------------------------------------------------
// Uploads
// ---------------------------------------------------------------------------

async fn upload(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
    Query(q): Query<UploadQuery>,
    body: axum::body::Bytes,
) -> Result<Json<UploadResponse>, ApiError> {
    let me = auth_user(&state, &headers)?;
    {
        let conn = state.store.conn();
        if !store::can_send(&conn, channel, me) {
            return Err(forbidden());
        }
    }
    if body.len() > 48 * 1024 * 1024 {
        return Err(bad("File is larger than 48 MB."));
    }
    let clean: String = q.name.chars().filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')).collect();
    let filename = if clean.trim().is_empty() { "file".to_string() } else { clean };
    let ext = filename.rsplit('.').next().unwrap_or("bin").to_lowercase();
    let hash = store::hash_bytes(&body);
    let stored = format!("{hash}.{ext}");
    let dir = state.data_dir.join("cdn");
    std::fs::create_dir_all(&dir).map_err(internal)?;
    if !dir.join(&stored).exists() {
        let sealed = crate::crypto::seal(&state.store.key(), &body);
        std::fs::write(dir.join(&stored), sealed).map_err(internal)?;
    }
    let url = format!("/cdn/{stored}");
    let conn = state.store.conn();
    store::record_cdn(&conn, &hash, &filename, &q.content_type, body.len() as u64);
    let id = store::next_id(&conn);
    conn.execute(
        "INSERT INTO attachments(id, uploader_id, filename, size, content_type, url, width, height) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![id, me, filename, body.len() as i64, q.content_type, url, q.width as i64, q.height as i64],
    )
    .map_err(internal)?;
    let attachment = Attachment {
        id,
        filename,
        size: body.len() as u64,
        content_type: q.content_type,
        url,
        width: q.width,
        height: q.height,
    };
    Ok(Json(UploadResponse { attachment }))
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<u32>,
}

async fn search(State(state): State<Arc<AppState>>, headers: HeaderMap, Query(q): Query<SearchQuery>) -> Result<Json<SearchResults>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let term = format!("%{}%", q.q.trim());
    if q.q.trim().is_empty() {
        return Ok(Json(SearchResults { total: 0, hits: Vec::new() }));
    }
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let mut rows: Vec<(Id, Id)> = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, channel_id FROM messages WHERE content LIKE ?1 ORDER BY id DESC LIMIT 400") {
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
        let Some(message) = store::message_by_id(&conn, id, me) else { continue };
        let Some(channel) = store::channel_by_id(&conn, ch) else { continue };
        let guild = channel.guild_id.and_then(|g| conn.query_row("SELECT name FROM guilds WHERE id=?1", params![g], |r| r.get::<_, String>(0)).optional().ok().flatten());
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
    Ok(Json(SearchResults { total: hits.len() as u32, hits }))
}

// ---------------------------------------------------------------------------
// Friends and DMs
// ---------------------------------------------------------------------------

async fn friend_request(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(other): Path<Id>) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if me == other || store::user_by_id(&conn, other).is_none() {
        return Err(bad("No such user."));
    }
    conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'outgoing',?3)", params![me, other, now_ms()]).ok();
    conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'incoming',?3)", params![other, me, now_ms()]).ok();
    drop(conn);
    let rel = Relationship { kind: RelationshipKind::Outgoing, user: User::default(), since: now_ms() };
    let _ = state.tx.send(GatewayEvent::RelationshipUpdate(Box::new(rel)));
    Ok(Json(json!({ "ok": true })))
}

async fn friend_accept(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(other): Path<Id>) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'friend',?3)", params![me, other, now_ms()]).ok();
    conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'friend',?3)", params![other, me, now_ms()]).ok();
    drop(conn);
    let _ = state.tx.send(GatewayEvent::RelationshipUpdate(Box::new(Relationship { kind: RelationshipKind::Friend, user: User::default(), since: now_ms() })));
    Ok(Json(json!({ "ok": true })))
}

async fn friend_remove(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(other): Path<Id>) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    conn.execute("DELETE FROM relationships WHERE (user_id=?1 AND other_id=?2) OR (user_id=?2 AND other_id=?1)", params![me, other]).ok();
    drop(conn);
    let _ = state.tx.send(GatewayEvent::RelationshipRemove { user_id: other });
    Ok(Json(json!({ "ok": true })))
}

async fn friend_block(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(other): Path<Id>) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'blocked',?3)", params![me, other, now_ms()]).ok();
    drop(conn);
    let _ = state.tx.send(GatewayEvent::RelationshipUpdate(Box::new(Relationship { kind: RelationshipKind::Blocked, user: User::default(), since: now_ms() })));
    Ok(Json(json!({ "ok": true })))
}

async fn open_dm(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(other): Path<Id>) -> Result<Json<DmChannel>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let other_user = store::user_by_id(&conn, other).ok_or_else(not_found)?;
    let existing: Option<Id> = conn
        .query_row(
            "SELECT c.id FROM channels c
             JOIN dm_members a ON a.channel_id=c.id AND a.user_id=?1
             JOIN dm_members b ON b.channel_id=c.id AND b.user_id=?2
             WHERE c.kind='dm' LIMIT 1",
            params![me, other],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?;
    let id = if let Some(id) = existing {
        id
    } else {
        let id = store::next_id(&conn);
        conn.execute("INSERT INTO channels(id, guild_id, name, kind, topic) VALUES(?1,NULL,'','dm','')", params![id]).map_err(internal)?;
        conn.execute("INSERT INTO dm_members(channel_id, user_id) VALUES(?1,?2)", params![id, me]).ok();
        conn.execute("INSERT INTO dm_members(channel_id, user_id) VALUES(?1,?2)", params![id, other]).ok();
        id
    };
    let channel = store::channel_by_id(&conn, id).ok_or_else(not_found)?;
    Ok(Json(DmChannel { channel, recipients: vec![other_user] }))
}

// ---------------------------------------------------------------------------
// Roles and members
// ---------------------------------------------------------------------------

async fn create_role(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(guild): Path<Id>, Json(body): Json<CreateRole>) -> Result<Json<Role>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let actor = store::base_permissions(&conn, guild, me);
    if !has(actor, perm::MANAGE_ROLES) {
        return Err(forbidden());
    }
    if actor & perm::ADMINISTRATOR == 0 && body.permissions & !actor != 0 {
        return Err(forbidden());
    }
    let id = store::next_id(&conn);
    let pos: i64 = conn.query_row("SELECT COALESCE(MAX(position),0)+1 FROM roles WHERE guild_id=?1", params![guild], |r| r.get(0)).unwrap_or(1);
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(?1,?2,?3,?4,?5,?6,0,1)",
        params![id, guild, body.name.trim(), body.color as i64, body.permissions as i64, pos],
    )
    .map_err(internal)?;
    let role = conn.query_row("SELECT * FROM roles WHERE id=?1", params![id], store::role_from_row).map_err(internal)?;
    Ok(Json(role))
}

async fn update_role(State(state): State<Arc<AppState>>, headers: HeaderMap, Path((guild, role)): Path<(Id, Id)>, Json(body): Json<RoleUpdate>) -> Result<Json<Role>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let actor = store::base_permissions(&conn, guild, me);
    if !has(actor, perm::MANAGE_ROLES) {
        return Err(forbidden());
    }
    let admin = actor & perm::ADMINISTRATOR != 0;
    if !admin {
        if let Some(pos) = store::role_position(&conn, guild, role) {
            if pos > 0 && pos >= store::member_top_role(&conn, guild, me) {
                return Err(forbidden());
            }
        }
        if let Some(perms) = body.permissions {
            if perms & !actor != 0 {
                return Err(forbidden());
            }
        }
    }
    if let Some(name) = body.name {
        conn.execute("UPDATE roles SET name=?1 WHERE id=?2 AND guild_id=?3 AND name<>'@everyone'", params![name, role, guild]).ok();
    }
    if let Some(color) = body.color {
        conn.execute("UPDATE roles SET color=?1 WHERE id=?2 AND guild_id=?3", params![color as i64, role, guild]).ok();
    }
    if let Some(perms) = body.permissions {
        conn.execute("UPDATE roles SET permissions=?1 WHERE id=?2 AND guild_id=?3", params![perms as i64, role, guild]).ok();
    }
    if let Some(hoist) = body.hoist {
        conn.execute("UPDATE roles SET hoist=?1 WHERE id=?2 AND guild_id=?3", params![hoist as i64, role, guild]).ok();
    }
    if let Some(mention) = body.mentionable {
        conn.execute("UPDATE roles SET mentionable=?1 WHERE id=?2 AND guild_id=?3", params![mention as i64, role, guild]).ok();
    }
    let out = conn.query_row("SELECT * FROM roles WHERE id=?1", params![role], store::role_from_row).map_err(internal)?;
    Ok(Json(out))
}

async fn delete_role(State(state): State<Arc<AppState>>, headers: HeaderMap, Path((guild, role)): Path<(Id, Id)>) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let actor = store::base_permissions(&conn, guild, me);
    if !has(actor, perm::MANAGE_ROLES) {
        return Err(forbidden());
    }
    let admin = actor & perm::ADMINISTRATOR != 0;
    if !admin {
        if let Some(pos) = store::role_position(&conn, guild, role) {
            if pos > 0 && pos >= store::member_top_role(&conn, guild, me) {
                return Err(forbidden());
            }
        }
    }
    conn.execute("DELETE FROM roles WHERE id=?1 AND guild_id=?2 AND name<>'@everyone'", params![role, guild]).ok();
    Ok(Json(json!({ "ok": true })))
}

async fn update_member(State(state): State<Arc<AppState>>, headers: HeaderMap, Path((guild, user)): Path<(Id, Id)>, Json(body): Json<MemberUpdate>) -> Result<Json<Member>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if me != user {
        let actor = store::base_permissions(&conn, guild, me);
        if !has(actor, perm::MANAGE_ROLES) {
            return Err(forbidden());
        }
        if !store::can_manage_member(&conn, guild, me, user) {
            return Err(forbidden());
        }
    }
    if let Some(nick) = body.nickname {
        conn.execute("UPDATE guild_members SET nickname=?1 WHERE guild_id=?2 AND user_id=?3", params![nick, guild, user]).ok();
    }
    if let Some(roles) = body.role_ids {
        let actor = store::base_permissions(&conn, guild, me);
        let admin = actor & perm::ADMINISTRATOR != 0;
        let top = store::member_top_role(&conn, guild, me);
        conn.execute("DELETE FROM member_roles WHERE guild_id=?1 AND user_id=?2", params![guild, user]).ok();
        for rid in roles {
            if !admin {
                if let Some(pos) = store::role_position(&conn, guild, rid) {
                    if pos >= top {
                        continue;
                    }
                }
            }
            conn.execute("INSERT OR IGNORE INTO member_roles(guild_id, user_id, role_id) VALUES(?1,?2,?3)", params![guild, user, rid]).ok();
        }
    }
    let u = store::user_by_id(&conn, user).ok_or_else(not_found)?;
    let member = Member {
        guild_id: guild,
        user: u,
        nickname: conn.query_row("SELECT nickname FROM guild_members WHERE guild_id=?1 AND user_id=?2", params![guild, user], |r| r.get::<_, String>(0)).unwrap_or_default(),
        roles: store::member_role_ids(&conn, guild, user),
        joined_at: now_ms(),
    };
    drop(conn);
    let _ = state.tx.send(GatewayEvent::MemberUpdate(Box::new(member.clone())));
    Ok(Json(member))
}

// ---------------------------------------------------------------------------
// Invites
// ---------------------------------------------------------------------------

fn invite_code() -> String {
    const ALPHA: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut bytes = [0u8; 8];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| ALPHA[*b as usize % ALPHA.len()] as char).collect()
}

async fn create_invite(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(guild): Path<Id>, Json(body): Json<CreateInvite>) -> Result<Json<Invite>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(store::base_permissions(&conn, guild, me), perm::CREATE_INVITE) {
        return Err(forbidden());
    }
    let code = invite_code();
    conn.execute(
        "INSERT INTO invites(code, guild_id, inviter_id, uses, max_uses, created_at) VALUES(?1,?2,?3,0,?4,?5)",
        params![code, guild, me, body.max_uses, now_ms()],
    )
    .map_err(internal)?;
    let (name, color, count): (String, i64, i64) = conn
        .query_row("SELECT name, icon_color, (SELECT COUNT(*) FROM guild_members WHERE guild_id=?1) FROM guilds WHERE id=?1", params![guild], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(internal)?;
    Ok(Json(Invite {
        code,
        guild_id: guild,
        guild_name: name,
        guild_icon_color: color as u32,
        inviter_id: me,
        uses: 0,
        created_at: now_ms(),
        member_count: count as u32,
    }))
}

async fn invite_info(State(state): State<Arc<AppState>>, Path(code): Path<String>) -> Result<Json<Invite>, ApiError> {
    let conn = state.store.conn();
    let row = conn
        .query_row(
            "SELECT i.*, g.name, g.icon_color, (SELECT COUNT(*) FROM guild_members WHERE guild_id=g.id) AS member_count FROM invites i JOIN guilds g ON g.id=i.guild_id WHERE i.code=?1",
            params![code],
            |r| {
                Ok(Invite {
                    code: r.get("code")?,
                    guild_id: r.get("guild_id")?,
                    guild_name: r.get("name")?,
                    guild_icon_color: r.get::<_, i64>("icon_color")? as u32,
                    inviter_id: r.get("inviter_id")?,
                    uses: r.get::<_, i64>("uses")? as u32,
                    created_at: r.get("created_at")?,
                    member_count: r.get::<_, i64>("member_count")? as u32,
                })
            },
        )
        .optional()
        .map_err(internal)?;
    row.map(Json).ok_or_else(not_found)
}

async fn join_invite(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(code): Path<String>) -> Result<Json<Guild>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let guild: Option<Id> = conn.query_row("SELECT guild_id FROM invites WHERE code=?1", params![code], |r| r.get(0)).optional().map_err(internal)?;
    let Some(guild) = guild else { return Err(not_found()) };
    conn.execute("INSERT OR IGNORE INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(?1,?2,'',?3)", params![guild, me, now_ms()]).ok();
    conn.execute("UPDATE invites SET uses=uses+1 WHERE code=?1", params![code]).ok();
    let out = Guild { id: guild, ..Guild::default() };
    let count = conn
        .query_row("SELECT COUNT(*) FROM guild_members WHERE guild_id=?1", params![guild], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as u32;
    let member = store::guild_member_detail(&conn, guild, me);
    drop(conn);
    let _ = state.tx.send(GatewayEvent::GuildCreate(Box::new(out.clone())));
    let _ = state.tx.send(GatewayEvent::MemberCountUpdate { guild_id: guild, count });
    if let Some(member) = member {
        let _ = state.tx.send(GatewayEvent::MemberUpdate(Box::new(member)));
    }
    Ok(Json(out))
}

// ---------------------------------------------------------------------------
// Gateway
// ---------------------------------------------------------------------------

async fn gateway(State(state): State<Arc<AppState>>, headers: HeaderMap, ws: WebSocketUpgrade) -> Result<Response, ApiError> {
    let me = auth_user(&state, &headers)?;
    Ok(ws.on_upgrade(move |socket| gateway_loop(socket, state, me)))
}

fn related_user(conn: &rusqlite::Connection, a: Id, b: Id) -> bool {
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

fn gateway_event_visible(state: &AppState, me: Id, event: &GatewayEvent) -> bool {
    let conn = state.store.conn();
    match event {
        GatewayEvent::Ready(_) | GatewayEvent::ReadyCheck | GatewayEvent::InteractionCreate { .. } => false,
        GatewayEvent::MessageCreate(m) | GatewayEvent::MessageUpdate(m) | GatewayEvent::ReactionUpdate(m) => {
            store::can_view(&conn, m.channel_id, me)
        }
        GatewayEvent::MessageDelete { channel_id, .. } => store::can_view(&conn, *channel_id, me),
        GatewayEvent::TypingStart { channel_id, .. } => store::can_view(&conn, *channel_id, me),
        GatewayEvent::PresenceUpdate { user_id, .. } => related_user(&conn, me, *user_id),
        GatewayEvent::ChannelCreate(c) | GatewayEvent::ChannelUpdate(c) => store::can_view(&conn, c.id, me),
        GatewayEvent::ChannelDelete { guild_id: Some(guild_id), .. } => store::guild_member(&conn, *guild_id, me),
        GatewayEvent::ChannelDelete { channel_id, guild_id: None } => store::is_dm_member(&conn, *channel_id, me),
        GatewayEvent::GuildCreate(g) | GatewayEvent::GuildUpdate(g) => store::guild_member(&conn, g.id, me),
        GatewayEvent::GuildDelete { .. } => true,
        GatewayEvent::MemberUpdate(m) => store::guild_member(&conn, m.guild_id, me),
        GatewayEvent::MemberRemove { guild_id, .. }
        | GatewayEvent::MemberCountUpdate { guild_id, .. }
        | GatewayEvent::GuildMembers { guild_id, .. } => store::guild_member(&conn, *guild_id, me),
        GatewayEvent::RelationshipUpdate(_) | GatewayEvent::RelationshipRemove { .. } => true,
        GatewayEvent::UserUpdate(u) => related_user(&conn, me, u.id),
        GatewayEvent::VoiceStateUpdate(v) => store::guild_member(&conn, v.guild_id, me),
        GatewayEvent::ReadUpdate { channel_id, .. } => store::can_view(&conn, *channel_id, me),
    }
}

async fn gateway_loop(socket: WebSocket, state: Arc<AppState>, me: Id) {
    let (mut sink, mut stream) = socket.split();
    let mut rx = state.tx.subscribe();

    // Presence.
    let first = {
        let mut online = state.online.lock().await;
        let count = online.entry(me).or_insert(0);
        *count += 1;
        *count == 1
    };
    if first {
        let conn = state.store.conn();
        conn.execute("UPDATE users SET status='online' WHERE id=?1 AND status='offline'", params![me]).ok();
        let _ = state.tx.send(GatewayEvent::PresenceUpdate { user_id: me, status: Presence::Online });
    }

    let bootstrap = {
        let conn = state.store.conn();
        store::bootstrap(&conn, me)
    };
    let ready = GatewayEvent::Ready(Box::new(bootstrap));
    if let Ok(text) = serde_json::to_string(&ready) {
        let _ = sink.send(WsMessage::Text(text.into())).await;
    }

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(event) => {
                        if !gateway_event_visible(&state, me, &event) {
                            continue;
                        }
                        if let Ok(text) = serde_json::to_string(&event) {
                            if sink.send(WsMessage::Text(text.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            incoming = stream.next() => {
                match incoming {
                    Some(Ok(WsMessage::Text(text))) => {
                        if let Ok(cmd) = serde_json::from_str::<GatewayCommand>(&text) {
                            match cmd {
                                GatewayCommand::Typing { channel_id } => {
                                    let _ = state.tx.send(GatewayEvent::TypingStart {
                                        channel_id,
                                        user_id: me,
                                        until_ms: now_ms() + 8000,
                                    });
                                }
                                GatewayCommand::Heartbeat => {}
                                GatewayCommand::Subscribe { .. } => {}
                            }
                        }
                    }
                    Some(Ok(WsMessage::Ping(bytes))) => {
                        let _ = sink.send(WsMessage::Pong(bytes)).await;
                    }
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }

    {
        let mut online = state.online.lock().await;
        if let Some(count) = online.get_mut(&me) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                online.remove(&me);
                let conn = state.store.conn();
                conn.execute("UPDATE users SET status='offline' WHERE id=?1", params![me]).ok();
                let _ = state.tx.send(GatewayEvent::PresenceUpdate { user_id: me, status: Presence::Offline });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Misc
// ---------------------------------------------------------------------------

pub fn uploads_dir(data: &std::path::Path) -> std::path::PathBuf {
    data.join("uploads")
}

pub fn hub() -> broadcast::Sender<GatewayEvent> {
    broadcast::channel(512).0
}

pub fn empty_map() -> HashMap<Id, usize> {
    HashMap::new()
}
#[derive(Deserialize, Default)]
struct CdnQuery {
    #[serde(default)]
    w: Option<u32>,
}

async fn cdn_file(
    State(state): State<Arc<AppState>>,
    Path(file): Path<String>,
    Query(query): Query<CdnQuery>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let hash = file.split('.').next().unwrap_or("").to_string();
    let content_type: String = {
        let conn = state.store.conn();
        conn.query_row("SELECT content_type FROM cdn_assets WHERE hash=?1", params![hash], |r| r.get(0))
            .unwrap_or_else(|_| "application/octet-stream".to_string())
    };
    let path = state.data_dir.join("cdn").join(&file);
    let Ok(sealed) = std::fs::read(path) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    let Some(bytes) = crate::crypto::open(&state.store.key(), &sealed) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "could not decrypt").into_response();
    };

    if let Some(width) = query.w.filter(|w| (64..=2048).contains(w)) {
        if content_type.starts_with("image/") {
            let thumb_path = state.data_dir.join("thumbs").join(format!("{hash}-{width}.enc"));
            if let Ok(sealed) = std::fs::read(&thumb_path) {
                if let Some(thumb) = crate::crypto::open(&state.store.key(), &sealed) {
                    return (
                        [
                            ("content-type", "image/png".to_string()),
                            ("cache-control", "public, max-age=31536000, immutable".to_string()),
                        ],
                        thumb,
                    )
                        .into_response();
                }
            }
            if let Ok(image) = image::load_from_memory(&bytes) {
                let (w, h) = (image.width(), image.height());
                if w > width {
                    let target_h = ((h as f32) * (width as f32 / w as f32)).round().max(1.0) as u32;
                    let thumb = image::imageops::thumbnail(&image, width, target_h);
                    let mut out = Vec::new();
                    if thumb.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).is_ok() {
                        if let Some(dir) = thumb_path.parent() {
                            let _ = std::fs::create_dir_all(dir);
                        }
                        let _ = std::fs::write(&thumb_path, crate::crypto::seal(&state.store.key(), &out));
                        return (
                            [
                                ("content-type", "image/png".to_string()),
                                ("cache-control", "public, max-age=31536000, immutable".to_string()),
                            ],
                            out,
                        )
                            .into_response();
                    }
                }
            }
        }
    }

    (
        [
            ("content-type", content_type),
            ("cache-control", "public, max-age=31536000, immutable".to_string()),
        ],
        bytes,
    )
        .into_response()
}
async fn list_roles(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(guild): Path<Id>) -> Result<Json<Vec<Role>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(store::base_permissions(&conn, guild, me), perm::VIEW_CHANNEL) {
        return Err(forbidden());
    }
    Ok(Json(store::guild_roles(&conn, guild)))
}

async fn my_permissions(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(guild): Path<Id>) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(store::base_permissions(&conn, guild, me), perm::VIEW_CHANNEL) {
        return Err(forbidden());
    }
    let mut channels = serde_json::Map::new();
    for channel in store::list_children(&conn, guild) {
        channels.insert(channel.id.to_string(), json!(store::channel_permissions(&conn, channel.id, me)));
    }
    Ok(Json(json!({ "permissions": store::base_permissions(&conn, guild, me), "channels": channels })))
}

async fn channel_overwrites(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(channel): Path<Id>) -> Result<Json<Vec<PermissionOverwrite>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    Ok(Json(store::overwrites(&conn, channel)))
}

#[derive(Deserialize)]
struct OverwriteBody {
    kind: String,
    #[serde(default)]
    allow: u64,
    #[serde(default)]
    deny: u64,
}

async fn set_channel_overwrite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel, target)): Path<(Id, Id)>,
    Json(body): Json<OverwriteBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let ch = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
    let Some(guild) = ch.guild_id else { return Err(forbidden()) };
    let actor = store::base_permissions(&conn, guild, me);
    if !has(actor, perm::MANAGE_ROLES) {
        return Err(forbidden());
    }
    let admin = actor & perm::ADMINISTRATOR != 0;
    if !admin && body.allow & !actor != 0 {
        return Err(forbidden());
    }
    let kind = if body.kind == "member" { "member" } else { "role" };
    if kind == "member" {
        if !store::can_manage_member(&conn, guild, me, target) {
            return Err(forbidden());
        }
    } else if !admin {
        if let Some(pos) = store::role_position(&conn, guild, target) {
            if pos >= store::member_top_role(&conn, guild, me) {
                return Err(forbidden());
            }
        }
    }
    store::set_overwrite(&conn, channel, target, kind, body.allow, body.deny);
    let updated = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::ChannelUpdate(Box::new(updated)));
    Ok(Json(json!({ "ok": true })))
}

async fn clear_channel_overwrite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel, target)): Path<(Id, Id)>,
    Json(body): Json<OverwriteBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let ch = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
    let Some(guild) = ch.guild_id else { return Err(forbidden()) };
    if !has(store::base_permissions(&conn, guild, me), perm::MANAGE_ROLES) {
        return Err(forbidden());
    }
    let kind = if body.kind == "member" { "member" } else { "role" };
    store::delete_overwrite(&conn, channel, target, kind);
    Ok(Json(json!({ "ok": true })))
}

