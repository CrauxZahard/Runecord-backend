//! users.

use super::*;
use crate::{store, AppState};
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use rusqlite::params;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

pub(crate) async fn bootstrap(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<Bootstrap>, ApiError> {
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

pub(crate) async fn get_user(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(user): Path<Id>) -> Result<Json<User>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::users_related(&conn, me, user) {
        return Err(forbidden());
    }
    store::user_by_id(&conn, user).map(Json).ok_or_else(not_found)
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
pub(crate) struct MeBody {    display_name: Option<String>,
    avatar_color: Option<u32>,
    avatar_border: Option<u32>,
    banner_color: Option<u32>,
    bio: Option<String>,
    custom_status: Option<String>,
}

pub(crate) async fn update_me(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<MeBody>) -> Result<Json<User>, ApiError> {
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

pub(crate) async fn update_read(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<ReadUpdate>) -> Result<Json<serde_json::Value>, ApiError> {
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


