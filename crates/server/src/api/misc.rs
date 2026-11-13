//! Miscellaneous routes.

use super::*;
use crate::AppState;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde_json::json;
use std::sync::Arc;

pub(crate) async fn cache_stats(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    auth_user(&state, &headers)?;
    let guilds = state
        .guild_cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .stats();
    let thumbs = state
        .thumb_cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .stats();
    Ok(Json(json!({ "guilds": guilds, "thumbs": thumbs })))
}
