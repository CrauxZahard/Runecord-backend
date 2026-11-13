//! auth.

use super::*;
use crate::{store, AppState};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use rusqlite::{params, OptionalExtension};
use serde_json::json;
use std::sync::Arc;

pub(crate) async fn register(State(state): State<Arc<AppState>>, Json(body): Json<AuthRequest>) -> Result<Json<AuthResponse>, ApiError> {
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

pub(crate) async fn login(State(state): State<Arc<AppState>>, Json(body): Json<AuthRequest>) -> Result<Json<AuthResponse>, ApiError> {
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

pub(crate) async fn logout(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<serde_json::Value>, ApiError> {
    auth_user(&state, &headers)?;
    if let Some(token) = headers.get("authorization").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")) {
        let conn = state.store.conn();
        conn.execute("DELETE FROM sessions WHERE token=?1", params![store::hash_bot_token(token.trim())]).ok();
    }
    Ok(Json(json!({ "ok": true })))
}


