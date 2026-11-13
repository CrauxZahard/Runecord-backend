//! roles.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use rusqlite::params;
use serde_json::json;
use std::sync::Arc;

pub(crate) async fn create_role(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
    Json(body): Json<CreateRole>,
) -> Result<Json<Role>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let actor = store::base_permissions(&conn, guild, me);
    if !has(actor, perm::MANAGE_ROLES) {
        return Err(forbidden());
    }
    if actor & perm::ADMINISTRATOR == 0 && body.permissions & !actor != 0 {
        return Err(forbidden());
    }
    let id = store::next_id(&conn);
    let pos: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position),0)+1 FROM roles WHERE guild_id=?1",
            params![guild],
            |r| r.get(0),
        )
        .unwrap_or(1);
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(?1,?2,?3,?4,?5,?6,0,1)",
        params![id, guild, body.name.trim(), body.color as i64, body.permissions as i64, pos],
    )
    .map_err(internal)?;
    let role = conn
        .query_row(
            "SELECT * FROM roles WHERE id=?1",
            params![id],
            store::role_from_row,
        )
        .map_err(internal)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::RoleCreate(Box::new(role.clone())));
    Ok(Json(role))
}

pub(crate) async fn update_role(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, role)): Path<(Id, Id)>,
    Json(body): Json<RoleUpdate>,
) -> Result<Json<Role>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let actor = store::base_permissions(&conn, guild, me);
    if !has(actor, perm::MANAGE_ROLES) {
        return Err(forbidden());
    }
    let admin = actor & perm::ADMINISTRATOR != 0;
    if !admin {
        if let Some(pos) = store::role_position(&conn, guild, role) {
            if pos > 0 && pos >= store::member_top_role(&conn, guild, me) {
                return Err(forbidden());
            }
        }
        if let Some(perms) = body.permissions {
            if perms & !actor != 0 {
                return Err(forbidden());
            }
        }
    }
    if let Some(name) = body.name {
        conn.execute(
            "UPDATE roles SET name=?1 WHERE id=?2 AND guild_id=?3 AND name<>'@everyone'",
            params![name, role, guild],
        )
        .ok();
    }
    if let Some(color) = body.color {
        conn.execute(
            "UPDATE roles SET color=?1 WHERE id=?2 AND guild_id=?3",
            params![color as i64, role, guild],
        )
        .ok();
    }
    if let Some(perms) = body.permissions {
        conn.execute(
            "UPDATE roles SET permissions=?1 WHERE id=?2 AND guild_id=?3",
            params![perms as i64, role, guild],
        )
        .ok();
    }
    if let Some(hoist) = body.hoist {
        conn.execute(
            "UPDATE roles SET hoist=?1 WHERE id=?2 AND guild_id=?3",
            params![hoist as i64, role, guild],
        )
        .ok();
    }
    if let Some(mention) = body.mentionable {
        conn.execute(
            "UPDATE roles SET mentionable=?1 WHERE id=?2 AND guild_id=?3",
            params![mention as i64, role, guild],
        )
        .ok();
    }
    if let Some(position) = body.position {
        conn.execute(
            "UPDATE roles SET position=?1 WHERE id=?2 AND guild_id=?3 AND name<>'@everyone'",
            params![position, role, guild],
        )
        .ok();
    }
    let out = conn
        .query_row(
            "SELECT * FROM roles WHERE id=?1",
            params![role],
            store::role_from_row,
        )
        .map_err(internal)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::RoleUpdate(Box::new(out.clone())));
    Ok(Json(out))
}

pub(crate) async fn delete_role(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, role)): Path<(Id, Id)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let actor = store::base_permissions(&conn, guild, me);
    if !has(actor, perm::MANAGE_ROLES) {
        return Err(forbidden());
    }
    let admin = actor & perm::ADMINISTRATOR != 0;
    if !admin {
        if let Some(pos) = store::role_position(&conn, guild, role) {
            if pos > 0 && pos >= store::member_top_role(&conn, guild, me) {
                return Err(forbidden());
            }
        }
    }
    conn.execute(
        "DELETE FROM roles WHERE id=?1 AND guild_id=?2 AND name<>'@everyone'",
        params![role, guild],
    )
    .ok();
    conn.execute(
        "DELETE FROM member_roles WHERE guild_id=?1 AND role_id=?2",
        params![guild, role],
    )
    .ok();
    conn.execute(
        "DELETE FROM overwrites WHERE target_id=?1 AND target_kind='role' AND channel_id IN (SELECT id FROM channels WHERE guild_id=?2)",
        params![role, guild],
    )
    .ok();
    drop(conn);
    let _ = state.tx.send(GatewayEvent::RoleDelete {
        guild_id: guild,
        role_id: role,
    });
    Ok(Json(json!({ "ok": true })))
}
