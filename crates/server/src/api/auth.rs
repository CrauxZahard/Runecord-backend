//! auth.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde_json::json;
use std::sync::Arc;

pub(crate) async fn register(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    let username = body.username.trim().to_lowercase();
    if !valid_username(&username) {
        return Err(bad(
            "Username must be 2-32 characters: letters, numbers, dots or underscores.",
        ));
    }
    if body.password.len() < 4 {
        return Err(bad("Password must be at least 4 characters."));
    }
    let conn = state.store.conn();
    if store::user_by_name(&conn, &username).is_some() {
        return Err(bad("That username is already taken."));
    }
    let id = store::next_id(&conn);
    let color = (id.unsigned_abs() % 10) as u32;
    let user = store::create_user(
        &conn,
        id,
        &username,
        &username,
        &hash_password(&body.password),
        color,
    )
    .ok_or_else(|| internal("could not create user"))?;
    let token = make_session(&conn, id);
    Ok(Json(AuthResponse { token, user }))
}

pub(crate) async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    let conn = state.store.conn();
    let Some((id, hash)) = store::credentials_by_name(&conn, body.username.trim()) else {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "Wrong username or password.".into(),
        ));
    };
    if !verify_password(&hash, &body.password) {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "Wrong username or password.".into(),
        ));
    }
    let token = make_session(&conn, id);
    let user = store::user_by_id(&conn, id).ok_or_else(not_found)?;
    Ok(Json(AuthResponse { token, user }))
}

pub(crate) async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    auth_user(&state, &headers)?;
    if let Some(token) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        let conn = state.store.conn();
        store::delete_session(&conn, token.trim());
    }
    Ok(Json(json!({ "ok": true })))
}
