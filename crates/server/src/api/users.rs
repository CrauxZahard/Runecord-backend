//! users.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use rusqlite::params;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

pub(crate) async fn bootstrap(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Bootstrap>, ApiError> {
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

pub(crate) async fn get_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(user): Path<Id>,
) -> Result<Json<User>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::users_related(&conn, me, user) {
        return Err(forbidden());
    }
    store::user_by_id(&conn, user)
        .map(Json)
        .ok_or_else(not_found)
}

#[derive(Deserialize)]
pub(crate) struct LookupQuery {
    q: String,
}

pub(crate) async fn lookup_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<LookupQuery>,
) -> Result<Json<Vec<User>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let needle = query.q.trim().to_lowercase();
    if needle.len() < 2 {
        return Ok(Json(Vec::new()));
    }
    let conn = state.store.conn();
    let like = format!("%{needle}%");
    let mut ids = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id FROM users WHERE id<>?1 AND (lower(username) LIKE ?2 OR lower(display_name) LIKE ?2) ORDER BY username LIMIT 10",
    ) {
        if let Ok(rows) = stmt.query_map(params![me, like], |r| r.get::<_, Id>(0)) {
            ids.extend(rows.flatten());
        }
    }
    let users = ids
        .into_iter()
        .filter_map(|id| store::user_by_id(&conn, id))
        .collect();
    Ok(Json(users))
}

#[derive(Deserialize)]
pub(crate) struct UserIds {
    #[serde(default)]
    ids: Vec<Id>,
}

pub(crate) async fn get_users_batch(
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

#[derive(Deserialize)]
pub(crate) struct MeBody {
    display_name: Option<String>,
    avatar_color: Option<u32>,
    avatar_border: Option<u32>,
    banner_color: Option<u32>,
    bio: Option<String>,
    custom_status: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct PasswordBody {
    current: String,
    new: String,
}

pub(crate) async fn change_password(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<PasswordBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let hash: String = conn
        .query_row(
            "SELECT pass_hash FROM users WHERE id=?1",
            params![me],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?
        .ok_or_else(not_found)?;
    if !verify_password(&hash, &body.current) {
        return Err(bad("Current password is incorrect."));
    }
    if body.new.chars().count() < 6 {
        return Err(bad("New password must be at least 6 characters."));
    }
    conn.execute(
        "UPDATE users SET pass_hash=?1 WHERE id=?2",
        params![hash_password(&body.new), me],
    )
    .map_err(internal)?;
    if let Some(token) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        conn.execute(
            "DELETE FROM sessions WHERE user_id=?1 AND token<>?2",
            params![me, store::hash_bot_token(token)],
        )
        .ok();
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn update_me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<MeBody>,
) -> Result<Json<User>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if let Some(name) = body.display_name.filter(|n| !n.trim().is_empty()) {
        conn.execute(
            "UPDATE users SET display_name=?1 WHERE id=?2",
            params![name.trim(), me],
        )
        .map_err(internal)?;
    }
    if let Some(color) = body.avatar_color {
        conn.execute(
            "UPDATE users SET avatar_color=?1 WHERE id=?2",
            params![color as i64, me],
        )
        .map_err(internal)?;
    }
    if let Some(border) = body.avatar_border {
        conn.execute(
            "UPDATE users SET avatar_border=?1 WHERE id=?2",
            params![border as i64, me],
        )
        .map_err(internal)?;
    }
    if let Some(banner) = body.banner_color {
        conn.execute(
            "UPDATE users SET banner_color=?1 WHERE id=?2",
            params![banner as i64, me],
        )
        .map_err(internal)?;
    }
    if let Some(bio) = body.bio {
        conn.execute("UPDATE users SET bio=?1 WHERE id=?2", params![bio, me])
            .map_err(internal)?;
    }
    if let Some(status) = body.custom_status {
        conn.execute(
            "UPDATE users SET custom_status=?1 WHERE id=?2",
            params![status, me],
        )
        .map_err(internal)?;
    }
    let user = store::user_by_id(&conn, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::UserUpdate(Box::new(user.clone())));
    Ok(Json(user))
}

pub(crate) async fn update_read(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ReadUpdate>,
) -> Result<Json<serde_json::Value>, ApiError> {
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
