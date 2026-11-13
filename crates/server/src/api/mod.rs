//! HTTP API and the WebSocket gateway.

use crate::{AppState, store};
use argon2::{PasswordHash, PasswordVerifier};
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use runecord_protocol::*;
use rusqlite::{OptionalExtension, params};
use serde_json::json;
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

pub(crate) fn bad(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

pub(crate) fn unauthorized() -> ApiError {
    ApiError(
        StatusCode::UNAUTHORIZED,
        "invalid or missing session".into(),
    )
}

pub(crate) fn forbidden() -> ApiError {
    ApiError(
        StatusCode::FORBIDDEN,
        "you don't have permission to do that".into(),
    )
}

/// A channel row read outside guild detail still needs the caller's permissions.
pub(crate) fn channel_for(conn: &rusqlite::Connection, mut channel: Channel, me: Id) -> Channel {
    channel.my_permissions = store::channel_permissions(conn, channel.id, me);
    channel
}

pub(crate) fn not_found() -> ApiError {
    ApiError(StatusCode::NOT_FOUND, "not found".into())
}

pub(crate) fn internal(err: impl std::fmt::Display) -> ApiError {
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
            .query_row(
                "SELECT user_id FROM sessions WHERE token=?1",
                params![store::hash_bot_token(token.trim())],
                |r| r.get(0),
            )
            .optional()
            .map_err(internal)?;
        return me.ok_or_else(unauthorized);
    }
    if let Some(token) = value.strip_prefix("Bot ") {
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
    conn.query_row(
        "SELECT user_id FROM sessions WHERE token=?1",
        params![store::hash_bot_token(token)],
        |r| r.get(0),
    )
    .optional()
    .map_err(internal)?
    .ok_or_else(unauthorized)
}

pub(crate) fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

pub(crate) fn argon() -> argon2::Argon2<'static> {
    let params = argon2::Params::new(8 * 1024, 1, 1, None).unwrap_or_default();
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
}

pub(crate) fn hash_password(password: &str) -> String {
    use argon2::password_hash::{PasswordHasher, SaltString, rand_core::OsRng};
    let salt = SaltString::generate(&mut OsRng);
    argon()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .unwrap_or_default()
}

pub(crate) fn verify_password(hash: &str, password: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| {
            argon()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

pub(crate) fn make_session(conn: &rusqlite::Connection, user_id: Id) -> String {
    let token = new_token();
    conn.execute(
        "INSERT INTO sessions(token, user_id, created_at) VALUES(?1,?2,?3)",
        params![store::hash_bot_token(&token), user_id, now_ms()],
    )
    .ok();
    token
}

pub(crate) fn valid_username(name: &str) -> bool {
    (2..=32).contains(&name.chars().count())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route(
            "/api/version",
            get(|| async { Json(json!({ "name": "runecord", "version": "0.1.0" })) }),
        )
        .route("/api/cache", get(cache_stats))
        .route("/api/register", post(register))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/bootstrap", get(bootstrap))
        .route("/api/me", patch(update_me))
        .route("/api/me/password", post(change_password))
        .route("/api/users/{id}", get(get_user))
        .route("/api/users/lookup", get(lookup_users))
        .route("/api/users/batch", post(get_users_batch))
        .route("/api/read", post(update_read))
        .route("/api/bots", get(list_bots).post(create_bot_account))
        .route("/api/bots/{id}", delete(delete_bot_account))
        .route("/api/guilds", post(create_guild))
        .route(
            "/api/guilds/{id}",
            get(guild_info).patch(update_guild).delete(delete_guild),
        )
        .route("/api/guilds/{id}/channels", post(create_channel))
        .route("/api/guilds/{id}/commands", get(guild_commands))
        .route("/api/guilds/{id}/members", get(list_guild_members))
        .route(
            "/api/guilds/{id}/members/{user_id}",
            get(get_guild_member).patch(update_member),
        )
        .route("/api/guilds/{id}/roles", get(list_roles).post(create_role))
        .route("/api/guilds/{id}/permissions", get(my_permissions))
        .route("/api/channels/{id}/permissions", get(channel_overwrites))
        .route(
            "/api/channels/{id}/permissions/{target}",
            put(set_channel_overwrite).delete(clear_channel_overwrite),
        )
        .route(
            "/api/guilds/{id}/roles/{role_id}",
            patch(update_role).delete(delete_role),
        )
        .route("/api/guilds/{id}/invites", post(create_invite))
        .route("/api/invites/{code}", get(invite_info))
        .route("/api/invites/{code}/join", post(join_invite))
        .route(
            "/api/channels/{id}",
            patch(update_channel).delete(delete_channel),
        )
        .route("/api/channels/{id}/thread", patch(update_thread))
        .route(
            "/api/channels/{id}/messages",
            get(list_messages).post(send_message),
        )
        .route("/api/channels/{id}/typing", post(typing))
        .route("/api/channels/{id}/upload", post(upload))
        .route("/cdn/{file}", get(cdn_file))
        .route("/api/channels/{id}/pins", get(list_pins))
        .route(
            "/api/channels/{id}/threads",
            get(list_threads).post(create_thread),
        )
        .route(
            "/api/messages/{id}",
            patch(edit_message).delete(delete_message),
        )
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

mod auth;
mod bots;
mod channels;
mod gateway;
mod guilds;
mod invites;
mod media;
mod members;
mod messages;
mod misc;
mod roles;
mod social;
mod users;

pub(crate) use auth::*;
pub(crate) use bots::*;
pub(crate) use channels::*;
pub(crate) use gateway::*;
pub(crate) use guilds::*;
pub(crate) use invites::*;
pub(crate) use media::*;
pub(crate) use members::*;
pub(crate) use messages::*;
pub(crate) use misc::*;
pub(crate) use roles::*;
pub(crate) use social::*;
pub(crate) use users::*;
