//! Phase B/C platform endpoints: applications, interactions, webhooks, audit, emojis, voice.

use crate::{api::ApiError, store, AppState, PendingInteraction};
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use runecord_protocol::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/applications", get(list_apps).post(create_app))
        .route("/api/applications/{id}", axum::routing::delete(delete_app))
        .route("/api/applications/{id}/install-link", post(app_install_link))
        .route("/api/applications/install/{code}", post(install_app))
        .route("/api/applications/{id}/token", post(app_token))
        .route("/api/applications/{id}/commands", get(list_commands).post(create_command))
        .route("/api/applications/{id}/commands/{command}", axum::routing::delete(delete_command))
        .route("/api/interactions", post(invoke_interaction))
        .route("/api/interactions/{id}/{token}/callback", post(interaction_callback))
        .route("/api/channels/{id}/webhooks", get(list_webhooks).post(create_webhook))
        .route("/api/webhooks/{id}", axum::routing::delete(delete_webhook))
        .route("/api/webhooks/{id}/{token}", post(execute_webhook))
        .route("/api/guilds/{id}/audit-logs", get(audit_logs))
        .route("/api/guilds/{id}/emojis", get(list_emojis).post(create_emoji))
        .route("/api/guilds/{id}/emojis/{emoji}", axum::routing::delete(delete_emoji))
        .route("/api/voice", post(voice_state))
        .with_state(state)
}

fn bad(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

fn forbidden() -> ApiError {
    ApiError(StatusCode::FORBIDDEN, "missing permission".into())
}

fn not_found() -> ApiError {
    ApiError(StatusCode::NOT_FOUND, "not found".into())
}

fn internal(msg: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, msg.to_string())
}

fn save_cdn(state: &AppState, bytes: &[u8], filename: &str, content_type: &str) -> Result<String, ApiError> {
    let hash = store::hash_bytes(bytes);
    let ext = filename.rsplit('.').next().unwrap_or("bin").to_lowercase();
    let stored = format!("{hash}.{ext}");
    let dir = state.data_dir.join("cdn");
    std::fs::create_dir_all(&dir).map_err(internal)?;
    let path = dir.join(&stored);
    if !path.exists() {
        let sealed = crate::crypto::seal(&state.store.key(), bytes);
        std::fs::write(&path, sealed).map_err(internal)?;
    }
    store::record_cdn(&state.store.conn(), &hash, filename, content_type, bytes.len() as u64);
    Ok(format!("/cdn/{stored}"))
}

// ---------------------------------------------------------------------------
// Applications
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CreateApp {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    color: u32,
}

async fn list_apps(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<Vec<Application>>, ApiError> {
    let me = crate::api::auth_human(&state, &headers)?;
    let conn = state.store.conn();
    Ok(Json(store::applications_owned(&conn, me)))
}

async fn create_app(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<CreateApp>) -> Result<Json<Value>, ApiError> {
    let me = crate::api::auth_human(&state, &headers)?;
    let name = body.name.trim();
    if name.chars().count() < 2 || name.chars().count() > 48 {
        return Err(bad("Application name must be 2-48 characters."));
    }
    let conn = state.store.conn();
    let mut app = store::create_application(&conn, me, name, body.color).map_err(bad)?;
    if !body.description.is_empty() {
        conn.execute("UPDATE applications SET description=?1 WHERE id=?2", rusqlite::params![body.description, app.id]).ok();
        app.description = body.description;
    }
    let token = store::regenerate_bot_token(&conn, app.bot.id);
    Ok(Json(json!({ "application": app, "token": token })))
}

async fn delete_app(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<Id>) -> Result<Json<Value>, ApiError> {
    let me = crate::api::auth_human(&state, &headers)?;
    if !store::delete_application(&state.store.conn(), me, id) {
        return Err(not_found());
    }
    Ok(Json(json!({ "ok": true })))
}

async fn app_install_link(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<Id>) -> Result<Json<Value>, ApiError> {
    let me = crate::api::auth_human(&state, &headers)?;
    let conn = state.store.conn();
    let app = store::application_by_id(&conn, id).ok_or_else(not_found)?;
    if app.owner_id != me {
        return Err(forbidden());
    }
    let code = store::create_install(&conn, id);
    Ok(Json(json!({ "code": code, "url": format!("runecord://install/{code}") })))
}

#[derive(Deserialize)]
struct InstallBody {
    guild_id: Id,
    #[serde(default)]
    permissions: u64,
    #[serde(default)]
    scopes: Vec<String>,
}

async fn install_app(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(code): Path<String>,
    Json(body): Json<InstallBody>,
) -> Result<Json<Application>, ApiError> {
    let me = crate::api::auth_human(&state, &headers)?;
    let conn = state.store.conn();
    if !has(store::base_permissions(&conn, body.guild_id, me), perm::MANAGE_GUILD) {
        return Err(forbidden());
    }
    let actor = store::base_permissions(&conn, body.guild_id, me);
    if body.permissions & !actor != 0 && actor & perm::ADMINISTRATOR == 0 {
        return Err(forbidden());
    }
    let app = store::install_application(&conn, &code, body.guild_id).ok_or_else(not_found)?;
    if body.permissions != 0 {
        let role_id = store::next_id(&conn);
        conn.execute("INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(?1,?2,?3,?4,?5,1,0,0)", rusqlite::params![role_id, body.guild_id, format!("{} bot", app.name), app.icon_color as i64, body.permissions as i64]).ok();
        conn.execute("INSERT OR IGNORE INTO member_roles(guild_id, user_id, role_id) VALUES(?1,?2,?3)", rusqlite::params![body.guild_id, app.bot.id, role_id]).ok();
    }
    let scopes = if body.scopes.is_empty() { "bot applications.commands".to_string() } else { body.scopes.join(" ") };
    conn.execute("UPDATE applications SET scopes=?1, permissions=?2 WHERE id=?3", rusqlite::params![scopes, body.permissions as i64, app.id]).ok();
    drop(conn);
    let _ = state.tx.send(GatewayEvent::GuildMembers { guild_id: body.guild_id, members: Vec::new() });
    Ok(Json(app))
}

async fn app_token(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<Id>) -> Result<Json<Value>, ApiError> {
    let me = crate::api::auth_human(&state, &headers)?;
    let conn = state.store.conn();
    let app = store::application_by_id(&conn, id).ok_or_else(not_found)?;
    if app.owner_id != me {
        return Err(forbidden());
    }
    let token = store::regenerate_bot_token(&conn, app.bot.id);
    Ok(Json(json!({ "token": token })))
}

// ---------------------------------------------------------------------------
// Commands and interactions
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct NewCommand {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    options: Value,
}

async fn list_commands(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(app): Path<Id>) -> Result<Json<Vec<ApplicationCommand>>, ApiError> {
    let me = crate::api::auth_human(&state, &headers)?;
    let conn = state.store.conn();
    let found = store::application_by_id(&conn, app).ok_or_else(not_found)?;
    if found.owner_id != me {
        return Err(forbidden());
    }
    Ok(Json(store::commands_for(&conn, app)))
}

async fn create_command(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(app): Path<Id>,
    Json(body): Json<NewCommand>,
) -> Result<Json<ApplicationCommand>, ApiError> {
    let me = crate::api::auth_human(&state, &headers)?;
    let conn = state.store.conn();
    let found = store::application_by_id(&conn, app).ok_or_else(not_found)?;
    if found.owner_id != me {
        return Err(forbidden());
    }
    let name = body.name.trim().to_lowercase();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
        return Err(bad("Command names are lowercase letters, numbers, - and _."));
    }
    let options = if body.options.is_null() { json!([]) } else { body.options };
    Ok(Json(store::create_command(&conn, app, &name, body.description.trim(), &options)))
}

async fn delete_command(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((app, command)): Path<(Id, Id)>,
) -> Result<Json<Value>, ApiError> {
    let me = crate::api::auth_human(&state, &headers)?;
    let conn = state.store.conn();
    let found = store::application_by_id(&conn, app).ok_or_else(not_found)?;
    if found.owner_id != me || !store::delete_command(&conn, app, command) {
        return Err(not_found());
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct Invoke {
    command: String,
    channel_id: Id,
    #[serde(default)]
    options: Value,
}

async fn invoke_interaction(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Invoke>,
) -> Result<Json<Value>, ApiError> {
    let me = crate::api::auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let channel = store::channel_by_id(&conn, body.channel_id).ok_or_else(not_found)?;
    let guild = channel.guild_id.ok_or_else(|| bad("Commands run in guild channels."))?;
    if !store::can_view(&conn, body.channel_id, me) {
        return Err(forbidden());
    }
    let (app, command) = store::command_in_guild(&conn, body.command.trim_start_matches('/'), guild).ok_or_else(not_found)?;
    let interaction_id = store::next_id(&conn);
    let mut bytes = [0u8; 24];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut bytes);
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let user = store::user_by_id(&conn, me).unwrap_or_default();
    let payload = json!({
        "id": interaction_id.to_string(),
        "application_id": app.id.to_string(),
        "type": 2,
        "token": token,
        "guild_id": guild.to_string(),
        "channel_id": body.channel_id.to_string(),
        "member": { "user": crate::compat::user_json(&user), "roles": [] },
        "data": {
            "id": command.id.to_string(),
            "name": command.name,
            "type": 1,
            "options": body.options
        }
    });
    state.interactions.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(
        interaction_id.to_string(),
        PendingInteraction {
            app_id: app.id,
            bot_id: app.bot.id,
            channel_id: body.channel_id,
            user_id: me,
            token: token.clone(),
            created_at: now_ms(),
        },
    );
    let _ = state.tx.send(GatewayEvent::InteractionCreate { app_id: app.id, payload });
    Ok(Json(json!({ "id": interaction_id.to_string(), "token": token })))
}

#[derive(Deserialize)]
struct Callback {
    #[serde(rename = "type")]
    kind: u8,
    #[serde(default)]
    data: Value,
}

async fn interaction_callback(
    State(state): State<Arc<AppState>>,
    Path((id, token)): Path<(String, String)>,
    Json(body): Json<Callback>,
) -> Result<Json<Value>, ApiError> {
    let pending = {
        let mut map = state.interactions.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(item) = map.get(&id).cloned() else { return Err(not_found()) };
        if item.token != token {
            return Err(forbidden());
        }
        map.remove(&id);
        item
    };
    if body.kind == 5 {
        return Ok(Json(json!({ "ok": true, "deferred": true })));
    }
    if body.kind != 4 {
        return Err(bad("Only CHANNEL_MESSAGE_WITH_SOURCE (4) and DEFERRED (5) are supported."));
    }
    let content = body.data.get("content").and_then(|c| c.as_str()).unwrap_or("").trim_end().to_string();
    if content.is_empty() {
        return Err(bad("Interaction response needs content."));
    }
    let conn = state.store.conn();
    let mid = store::next_id(&conn);
    conn.execute(
        "INSERT INTO messages(id, channel_id, author_id, content, timestamp) VALUES(?1,?2,?3,?4,?5)",
        rusqlite::params![mid, pending.channel_id, pending.bot_id, content, now_ms()],
    ).map_err(internal)?;
    conn.execute("UPDATE channels SET last_message_id=?1 WHERE id=?2", rusqlite::params![mid, pending.channel_id]).ok();
    let msg = store::message_by_id(&conn, mid, pending.bot_id).ok_or_else(not_found)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::MessageCreate(Box::new(msg)));
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Webhooks
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct NewWebhook {
    name: String,
    #[serde(default)]
    color: u32,
}

async fn create_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
    Json(body): Json<NewWebhook>,
) -> Result<Json<WebhookInfo>, ApiError> {
    let me = crate::api::auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let ch = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
    let guild = ch.guild_id.ok_or_else(|| bad("Webhooks need a guild channel."))?;
    if !has(store::base_permissions(&conn, guild, me), perm::MANAGE_WEBHOOKS) {
        return Err(forbidden());
    }
    let name = body.name.trim();
    if name.is_empty() {
        return Err(bad("Webhook name is required."));
    }
    let (info, _token) = store::create_webhook(&conn, guild, channel, me, name, body.color);
    store::record_audit(&conn, guild, me, 50, Some(info.id), json!([{ "key": "name", "new_value": name }]));
    Ok(Json(info))
}

async fn list_webhooks(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(channel): Path<Id>) -> Result<Json<Vec<WebhookInfo>>, ApiError> {
    let me = crate::api::auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let ch = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
    if let Some(guild) = ch.guild_id {
        if !has(store::base_permissions(&conn, guild, me), perm::MANAGE_WEBHOOKS) {
            return Err(forbidden());
        }
    }
    Ok(Json(store::webhooks_for_channel(&conn, channel)))
}

async fn delete_webhook(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<Id>) -> Result<Json<Value>, ApiError> {
    let me = crate::api::auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let guild: Option<Id> = conn.query_row("SELECT guild_id FROM webhooks WHERE id=?1", rusqlite::params![id], |r| r.get(0)).ok();
    let Some(guild) = guild else { return Err(not_found()) };
    if !has(store::base_permissions(&conn, guild, me), perm::MANAGE_WEBHOOKS) {
        return Err(forbidden());
    }
    store::delete_webhook(&conn, guild, id);
    store::record_audit(&conn, guild, me, 51, Some(id), json!([]));
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct WebhookBody {
    #[serde(default)]
    content: String,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    avatar_color: Option<u32>,
}

#[derive(Deserialize)]
struct WaitQuery {
    #[serde(default)]
    wait: bool,
}

async fn execute_webhook(
    State(state): State<Arc<AppState>>,
    Path((id, token)): Path<(Id, String)>,
    Query(q): Query<WaitQuery>,
    Json(body): Json<WebhookBody>,
) -> Result<axum::response::Response, ApiError> {
    let conn = state.store.conn();
    let hook = store::webhook_by_token(&conn, id, &token).ok_or_else(not_found)?;
    let content = body.content.trim_end().to_string();
    if content.is_empty() {
        return Err(bad("Webhook message needs content."));
    }
    let name = body.username.filter(|n| !n.trim().is_empty()).unwrap_or(hook.name.clone());
    let color = body.avatar_color.unwrap_or(hook.avatar_color);
    let mid = store::next_id(&conn);
    conn.execute(
        "INSERT INTO messages(id, channel_id, author_id, content, timestamp, webhook_id, webhook_name, webhook_color)
         VALUES(?1,?2,0,?3,?4,?5,?6,?7)",
        rusqlite::params![mid, hook.channel_id, content, now_ms(), hook.id, name, color as i64],
    ).map_err(internal)?;
    conn.execute("UPDATE channels SET last_message_id=?1 WHERE id=?2", rusqlite::params![mid, hook.channel_id]).ok();
    let msg = store::message_by_id(&conn, mid, 0).ok_or_else(not_found)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::MessageCreate(Box::new(msg.clone())));
    if q.wait {
        Ok(Json(msg).into_response())
    } else {
        Ok(StatusCode::NO_CONTENT.into_response())
    }
}

use axum::response::IntoResponse as _;

// ---------------------------------------------------------------------------
// Audit, emoji, voice
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AuditQuery {
    limit: Option<u32>,
    before: Option<Id>,
}

async fn audit_logs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEntry>>, ApiError> {
    let me = crate::api::auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(store::base_permissions(&conn, guild, me), perm::VIEW_AUDIT_LOG) {
        return Err(forbidden());
    }
    Ok(Json(store::audit_entries(&conn, guild, q.limit.unwrap_or(50), q.before)))
}

#[derive(Deserialize)]
struct EmojiQuery {
    name: String,
    #[serde(default)]
    sticker: bool,
}

async fn list_emojis(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(guild): Path<Id>) -> Result<Json<Vec<Emoji>>, ApiError> {
    let me = crate::api::auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(store::base_permissions(&conn, guild, me), perm::VIEW_CHANNEL) {
        return Err(forbidden());
    }
    Ok(Json(store::guild_emojis(&conn, guild)))
}

async fn create_emoji(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
    Query(q): Query<EmojiQuery>,
    bytes: Bytes,
) -> Result<Json<Emoji>, ApiError> {
    let me = crate::api::auth_user(&state, &headers)?;
    if !has(store::base_permissions(&state.store.conn(), guild, me), perm::MANAGE_GUILD) {
        return Err(forbidden());
    }
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(bad("Emoji files must be under 2 MB."));
    }
    let name = q.name.trim().to_lowercase();
    if name.chars().count() < 2 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(bad("Emoji names are 2+ letters, numbers or underscores."));
    }
    let ext = if q.sticker { "png" } else { "png" };
    let url = save_cdn(&state, &bytes, &format!("{name}.{ext}"), "image/png")?;
    let conn = state.store.conn();
    let emoji = store::create_emoji(&conn, guild, me, &name, q.sticker, &url);
    store::record_audit(&conn, guild, me, 60, Some(emoji.id), json!([{ "key": "name", "new_value": name }]));
    drop(conn);
    let _ = state.tx.send(GatewayEvent::GuildMembers { guild_id: guild, members: Vec::new() });
    Ok(Json(emoji))
}

async fn delete_emoji(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, emoji)): Path<(Id, Id)>,
) -> Result<Json<Value>, ApiError> {
    let me = crate::api::auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(store::base_permissions(&conn, guild, me), perm::MANAGE_GUILD) {
        return Err(forbidden());
    }
    if !store::delete_emoji(&conn, guild, emoji) {
        return Err(not_found());
    }
    store::record_audit(&conn, guild, me, 61, Some(emoji), json!([]));
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct VoiceBody {
    guild_id: Id,
    #[serde(default)]
    channel_id: Option<Id>,
    #[serde(default)]
    self_mute: bool,
    #[serde(default)]
    self_deaf: bool,
}

pub fn set_voice(
    state: &AppState,
    user: Id,
    guild: Id,
    channel: Option<Id>,
    mute: bool,
    deaf: bool,
) -> Result<VoiceState, ApiError> {
    let conn = state.store.conn();
    if !has(store::base_permissions(&conn, guild, user), perm::VIEW_CHANNEL) {
        return Err(forbidden());
    }
    if let Some(ch) = channel {
        let found = store::channel_by_id(&conn, ch).ok_or_else(not_found)?;
        if found.guild_id != Some(guild) {
            return Err(bad("Channel is not in that guild."));
        }
    }
    drop(conn);
    let vs = VoiceState { user_id: user, guild_id: guild, channel_id: channel, self_mute: mute, self_deaf: deaf };
    let mut map = state.voice.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if channel.is_some() {
        map.insert(user, vs.clone());
    } else {
        map.remove(&user);
    }
    drop(map);
    let _ = state.tx.send(GatewayEvent::VoiceStateUpdate(Box::new(vs.clone())));
    Ok(vs)
}

async fn voice_state(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<VoiceBody>,
) -> Result<Json<VoiceState>, ApiError> {
    let me = crate::api::auth_user(&state, &headers)?;
    set_voice(&state, me, body.guild_id, body.channel_id, body.self_mute, body.self_deaf).map(Json)
}
