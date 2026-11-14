//! Server scheduled events.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use rusqlite::params;
use std::sync::Arc;

fn event_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<GuildEvent> {
    Ok(GuildEvent {
        id: r.get(0)?,
        guild_id: r.get(1)?,
        creator_id: r.get(2)?,
        name: r.get(3)?,
        description: r.get(4)?,
        start_at: r.get(5)?,
        channel_id: r.get(6)?,
        created_at: r.get(7)?,
    })
}

fn list_for(conn: &rusqlite::Connection, guild: Id) -> Vec<GuildEvent> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, guild_id, creator_id, name, description, start_at, channel_id, created_at
         FROM guild_events WHERE guild_id=?1 ORDER BY start_at",
    ) {
        if let Ok(rows) = stmt.query_map(params![guild], event_from_row) {
            out.extend(rows.flatten());
        }
    }
    out
}

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
    Ok(Json(list_for(&conn, guild)))
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
    if let Some(channel) = body.channel_id {
        let channel_guild: Option<Id> = conn
            .query_row(
                "SELECT guild_id FROM channels WHERE id=?1",
                params![channel],
                |r| r.get(0),
            )
            .optional()
            .map_err(internal)?;
        if channel_guild != Some(guild) {
            return Err(bad("That channel is not in this server."));
        }
    }
    let id = store::next_id(&conn);
    let created = now_ms();
    conn.execute(
        "INSERT INTO guild_events(id, guild_id, creator_id, name, description, start_at, channel_id, created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            id,
            guild,
            me,
            name,
            body.description.trim().chars().take(600).collect::<String>(),
            body.start_at,
            body.channel_id,
            created
        ],
    )
    .map_err(internal)?;
    let event = conn
        .query_row(
            "SELECT id, guild_id, creator_id, name, description, start_at, channel_id, created_at
             FROM guild_events WHERE id=?1",
            params![id],
            event_from_row,
        )
        .map_err(internal)?;
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
    let creator: Option<Id> = conn
        .query_row(
            "SELECT creator_id FROM guild_events WHERE id=?1 AND guild_id=?2",
            params![event, guild],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?;
    let Some(creator) = creator else {
        return Err(not_found());
    };
    let can_manage = has(
        store::base_permissions(&conn, guild, me),
        perm::MANAGE_GUILD,
    );
    if creator != me && !can_manage {
        return Err(forbidden());
    }
    conn.execute(
        "DELETE FROM guild_events WHERE id=?1 AND guild_id=?2",
        params![event, guild],
    )
    .map_err(internal)?;
    Ok(Json(json!({ "ok": true })))
}
