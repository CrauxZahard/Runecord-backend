//! roles.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use rusqlite::params;
use serde_json::json;
use std::sync::Arc;

const ROLE_ICON_LIMIT: usize = 4 * 1024 * 1024;
const ROLE_LINK_LIMIT: usize = 5;

fn role_in_guild(conn: &rusqlite::Connection, guild: Id, role: Id) -> Option<Role> {
    conn.query_row(
        "SELECT * FROM roles WHERE id=?1 AND guild_id=?2",
        params![role, guild],
        store::role_from_row,
    )
    .ok()
}

fn can_manage_role(conn: &rusqlite::Connection, guild: Id, role: Id, me: Id) -> bool {
    let actor = store::base_permissions(conn, guild, me);
    if !has(actor, perm::MANAGE_ROLES) {
        return false;
    }
    if actor & perm::ADMINISTRATOR != 0 {
        return true;
    }
    match store::role_position(conn, guild, role) {
        Some(position) => position == 0 || position < store::member_top_role(conn, guild, me),
        None => false,
    }
}

fn role_links(conn: &rusqlite::Connection, guild: Id, role: Id) -> Vec<RoleLink> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, guild_id, role_id, label, url FROM role_links
         WHERE guild_id=?1 AND role_id=?2 ORDER BY id",
    ) {
        if let Ok(rows) = stmt.query_map(params![guild, role], |r| {
            Ok(RoleLink {
                id: r.get(0)?,
                guild_id: r.get(1)?,
                role_id: r.get(2)?,
                label: r.get(3)?,
                url: r.get(4)?,
            })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub(crate) async fn role_links_list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, role)): Path<(Id, Id)>,
) -> Result<Json<Vec<RoleLink>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::guild_member(&conn, guild, me) || role_in_guild(&conn, guild, role).is_none() {
        return Err(not_found());
    }
    Ok(Json(role_links(&conn, guild, role)))
}

pub(crate) async fn create_role_link(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, role)): Path<(Id, Id)>,
    Json(body): Json<CreateRoleLink>,
) -> Result<Json<Vec<RoleLink>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if role_in_guild(&conn, guild, role).is_none() {
        return Err(not_found());
    }
    if !can_manage_role(&conn, guild, role, me) {
        return Err(forbidden());
    }
    let label: String = body.label.trim().chars().take(80).collect();
    let url: String = body.url.trim().chars().take(500).collect();
    if label.is_empty() {
        return Err(bad("Give the link a label."));
    }
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(bad("Links must start with http:// or https://."));
    }
    let existing = role_links(&conn, guild, role).len();
    if existing >= ROLE_LINK_LIMIT {
        return Err(bad("A role can have up to 5 links."));
    }
    let id = store::next_id(&conn);
    conn.execute(
        "INSERT INTO role_links(id, guild_id, role_id, label, url, created_at) VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, guild, role, label, url, now_ms()],
    )
    .map_err(internal)?;
    store::record_audit(
        &conn,
        guild,
        me,
        11,
        Some(role),
        json!([{ "key": "link", "new_value": label }]),
    );
    Ok(Json(role_links(&conn, guild, role)))
}

pub(crate) async fn delete_role_link(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, role, link)): Path<(Id, Id, Id)>,
) -> Result<Json<Vec<RoleLink>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if role_in_guild(&conn, guild, role).is_none() {
        return Err(not_found());
    }
    if !can_manage_role(&conn, guild, role, me) {
        return Err(forbidden());
    }
    let changed = conn
        .execute(
            "DELETE FROM role_links WHERE id=?1 AND guild_id=?2 AND role_id=?3",
            params![link, guild, role],
        )
        .unwrap_or(0);
    if changed == 0 {
        return Err(not_found());
    }
    store::record_audit(
        &conn,
        guild,
        me,
        11,
        Some(role),
        json!([{ "key": "link", "old_value": link }]),
    );
    Ok(Json(role_links(&conn, guild, role)))
}

pub(crate) async fn upload_role_icon(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, role)): Path<(Id, Id)>,
    body: Bytes,
) -> Result<Json<Role>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if role_in_guild(&conn, guild, role).is_none() {
        return Err(not_found());
    }
    if !can_manage_role(&conn, guild, role, me) {
        return Err(forbidden());
    }
    let level = store::boost_level(store::boost_count(&conn, guild));
    if !store::role_icon_unlocked(level) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "Role icons unlock at Boost Level 2 (7 Boosts).".into(),
        ));
    }
    if body.is_empty() {
        return Err(bad("Pick an image first."));
    }
    if body.len() > ROLE_ICON_LIMIT {
        return Err(bad("Role icons must be 4 MB or smaller."));
    }
    let format = image::guess_format(&body)
        .map_err(|_| bad("That file is not a PNG, JPEG, GIF or WebP image."))?;
    let ext = match format {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Gif => "gif",
        image::ImageFormat::WebP => "webp",
        _ => return Err(bad("That file is not a PNG, JPEG, GIF or WebP image.")),
    };
    let decoded =
        image::load_from_memory(&body).map_err(|_| bad("That image could not be decoded."))?;
    let (width, height) = (decoded.width(), decoded.height());
    if width != height {
        return Err(bad("Role icons must be square."));
    }
    if width > 4096 {
        return Err(bad("Role icons must be 4096 pixels or smaller."));
    }
    let url = state
        .media
        .save_with_ext(&body, ext)
        .map_err(internal)?
        .url;
    conn.execute(
        "UPDATE roles SET icon_url=?1 WHERE id=?2 AND guild_id=?3",
        params![url, role, guild],
    )
    .map_err(internal)?;
    let out = role_in_guild(&conn, guild, role).ok_or_else(not_found)?;
    store::record_audit(
        &conn,
        guild,
        me,
        11,
        Some(role),
        json!([{ "key": "icon_url", "new_value": url }]),
    );
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::RoleUpdate(Box::new(out.clone())));
    Ok(Json(out))
}

pub(crate) async fn remove_role_icon(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, role)): Path<(Id, Id)>,
) -> Result<Json<Role>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if role_in_guild(&conn, guild, role).is_none() {
        return Err(not_found());
    }
    if !can_manage_role(&conn, guild, role, me) {
        return Err(forbidden());
    }
    conn.execute(
        "UPDATE roles SET icon_url='' WHERE id=?1 AND guild_id=?2",
        params![role, guild],
    )
    .map_err(internal)?;
    let out = role_in_guild(&conn, guild, role).ok_or_else(not_found)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::RoleUpdate(Box::new(out.clone())));
    Ok(Json(out))
}

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
    store::record_audit(
        &conn,
        guild,
        me,
        10,
        Some(role.id),
        json!([{ "key": "name", "new_value": role.name }, { "key": "permissions", "new_value": role.permissions }]),
    );
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
    let audit_changes = json!({
        "name": body.name.clone(),
        "color": body.color,
        "permissions": body.permissions,
        "hoist": body.hoist,
        "mentionable": body.mentionable,
        "position": body.position,
    });
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
    store::record_audit(&conn, guild, me, 11, Some(role), audit_changes);
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
    let name: String = conn
        .query_row(
            "SELECT name FROM roles WHERE id=?1 AND guild_id=?2",
            params![role, guild],
            |r| r.get(0),
        )
        .unwrap_or_default();
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
    store::record_audit(
        &conn,
        guild,
        me,
        12,
        Some(role),
        json!([{ "key": "name", "old_value": name }]),
    );
    drop(conn);
    let _ = state.tx.send(GatewayEvent::RoleDelete {
        guild_id: guild,
        role_id: role,
    });
    Ok(Json(json!({ "ok": true })))
}
