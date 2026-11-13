//! social.

use super::*;
use crate::{store, AppState};
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use rusqlite::{params, OptionalExtension};
use serde_json::json;
use std::sync::Arc;


pub(crate) async fn friend_request(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(other): Path<Id>) -> Result<Json<serde_json::Value>, ApiError> {
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

pub(crate) async fn friend_accept(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(other): Path<Id>) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'friend',?3)", params![me, other, now_ms()]).ok();
    conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'friend',?3)", params![other, me, now_ms()]).ok();
    drop(conn);
    let _ = state.tx.send(GatewayEvent::RelationshipUpdate(Box::new(Relationship { kind: RelationshipKind::Friend, user: User::default(), since: now_ms() })));
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn friend_remove(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(other): Path<Id>) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    conn.execute("DELETE FROM relationships WHERE (user_id=?1 AND other_id=?2) OR (user_id=?2 AND other_id=?1)", params![me, other]).ok();
    drop(conn);
    let _ = state.tx.send(GatewayEvent::RelationshipRemove { user_id: other });
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn friend_block(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(other): Path<Id>) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'blocked',?3)", params![me, other, now_ms()]).ok();
    drop(conn);
    let _ = state.tx.send(GatewayEvent::RelationshipUpdate(Box::new(Relationship { kind: RelationshipKind::Blocked, user: User::default(), since: now_ms() })));
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn open_dm(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(other): Path<Id>) -> Result<Json<DmChannel>, ApiError> {
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


