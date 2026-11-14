//! roles.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde_json::json;
use std::sync::Arc;

const ROLE_ICON_LIMIT: usize = 4 * 1024 * 1024;
const ROLE_LINK_LIMIT: usize = 5;

fn role_in_guild(conn: &rusqlite::Connection, guild: Id, role: Id) -> Option<Role> {
    store::role_in_guild(conn, guild, role)
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
    Ok(Json(store::role_links(&conn, guild, role)))
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
    if store::role_links(&conn, guild, role).len() >= ROLE_LINK_LIMIT {
        return Err(bad("A role can have up to 5 links."));
    }
    store::add_role_link(&conn, guild, role, &label, &url)
        .ok_or_else(|| internal("could not add role link"))?;
    store::record_audit(
        &conn,
        guild,
        me,
        11,
        Some(role),
        json!([{ "key": "link", "new_value": label }]),
    );
    Ok(Json(store::role_links(&conn, guild, role)))
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
    if !store::remove_role_link(&conn, guild, role, link) {
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
    Ok(Json(store::role_links(&conn, guild, role)))
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
    let url = state.media.save_with_ext(&body, ext).map_err(internal)?.url;
    store::set_role_icon(&conn, guild, role, &url).map_err(internal)?;
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
    store::clear_role_icon(&conn, guild, role).map_err(internal)?;
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
    let position = store::next_role_position(&conn, guild);
    let role = store::insert_role(
        &conn,
        guild,
        body.name.trim(),
        body.color,
        body.permissions,
        position,
    )
    .ok_or_else(|| internal("could not create role"))?;
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
    store::update_role_fields(
        &conn,
        guild,
        role,
        body.name.as_deref(),
        body.color,
        body.permissions,
        body.hoist,
        body.mentionable,
        body.position,
    )
    .map_err(internal)?;
    let out = store::role_from_id(&conn, role).ok_or_else(not_found)?;
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
    let name = store::role_name(&conn, guild, role);
    store::delete_role_cascade(&conn, guild, role);
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
