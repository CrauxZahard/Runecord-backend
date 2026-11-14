//! Server scheduled events.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use std::sync::Arc;

pub(crate) async fn list_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
) -> Result<Json<Vec<GuildEvent>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::guild_member(&conn, guild, me) {
        return Err(not_found());
    }
    Ok(Json(store::list_events(&conn, guild)))
}

pub(crate) async fn create_event(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
    Json(body): Json<CreateGuildEvent>,
) -> Result<Json<GuildEvent>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::guild_member(&conn, guild, me) {
        return Err(not_found());
    }
    if !has(
        store::base_permissions(&conn, guild, me),
        perm::MANAGE_GUILD,
    ) {
        return Err(forbidden());
    }
    let name: String = body.name.trim().chars().take(100).collect();
    if name.is_empty() {
        return Err(bad("Name the event."));
    }
    if body.start_at <= 0 {
        return Err(bad("Pick a start time."));
    }
    if let Some(channel) = body.channel_id
        && store::guild_id_for_channel(&conn, channel) != Some(Some(guild))
    {
        return Err(bad("That channel is not in this server."));
    }
    let event = store::create_event(
        &conn,
        guild,
        me,
        &name,
        &body
            .description
            .trim()
            .chars()
            .take(600)
            .collect::<String>(),
        body.start_at,
        body.channel_id,
    )
    .ok_or_else(|| internal("could not create event"))?;
    let id = event.id;
    store::record_audit(
        &conn,
        guild,
        me,
        10,
        Some(id),
        json!([{ "key": "event", "new_value": event.name }]),
    );
    Ok(Json(event))
}

pub(crate) async fn delete_event(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, event)): Path<(Id, Id)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::guild_member(&conn, guild, me) {
        return Err(not_found());
    }
    let Some(creator) = store::event_creator(&conn, guild, event) else {
        return Err(not_found());
    };
    let can_manage = has(
        store::base_permissions(&conn, guild, me),
        perm::MANAGE_GUILD,
    );
    if creator != me && !can_manage {
        return Err(forbidden());
    }
    store::delete_event(&conn, guild, event).map_err(internal)?;
    Ok(Json(json!({ "ok": true })))
}
