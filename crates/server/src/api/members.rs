//! members.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use rusqlite::params;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub(crate) struct MembersQuery {
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    role_id: Option<Id>,
    #[serde(default)]
    status: Option<String>,
}

pub(crate) async fn list_guild_members(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
    Query(query): Query<MembersQuery>,
) -> Result<Json<MemberPage>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::guild_member(&conn, guild, me) {
        return Err(not_found());
    }
    if !has(
        store::base_permissions(&conn, guild, me),
        perm::VIEW_CHANNEL,
    ) {
        return Err(forbidden());
    }
    let page = store::members_page(
        &conn,
        guild,
        me,
        query.offset.unwrap_or(0),
        query.limit.unwrap_or(100),
        query.q.as_deref().unwrap_or(""),
        query.role_id.unwrap_or(0),
        query.status.as_deref().unwrap_or("all"),
    )
    .ok_or_else(not_found)?;
    Ok(Json(page))
}

pub(crate) async fn get_guild_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, user)): Path<(Id, Id)>,
) -> Result<Json<Member>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::guild_member(&conn, guild, me) {
        return Err(not_found());
    }
    if !has(
        store::base_permissions(&conn, guild, me),
        perm::VIEW_CHANNEL,
    ) {
        return Err(forbidden());
    }
    let member = store::guild_member_detail(&conn, guild, user).ok_or_else(not_found)?;
    Ok(Json(member))
}

pub(crate) async fn update_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, user)): Path<(Id, Id)>,
    Json(body): Json<MemberUpdate>,
) -> Result<Json<Member>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if me != user {
        let actor = store::base_permissions(&conn, guild, me);
        if !has(actor, perm::MANAGE_ROLES) {
            return Err(forbidden());
        }
        if !store::can_manage_member(&conn, guild, me, user) {
            return Err(forbidden());
        }
    }
    let roles_changed = body.role_ids.is_some();
    if let Some(nick) = body.nickname {
        conn.execute(
            "UPDATE guild_members SET nickname=?1 WHERE guild_id=?2 AND user_id=?3",
            params![nick, guild, user],
        )
        .ok();
    }
    if let Some(roles) = body.role_ids {
        let actor = store::base_permissions(&conn, guild, me);
        let admin = actor & perm::ADMINISTRATOR != 0;
        let top = store::member_top_role(&conn, guild, me);
        conn.execute(
            "DELETE FROM member_roles WHERE guild_id=?1 AND user_id=?2",
            params![guild, user],
        )
        .ok();
        for rid in roles {
            if !admin {
                if let Some(pos) = store::role_position(&conn, guild, rid) {
                    if pos >= top {
                        continue;
                    }
                }
            }
            conn.execute(
                "INSERT OR IGNORE INTO member_roles(guild_id, user_id, role_id) VALUES(?1,?2,?3)",
                params![guild, user, rid],
            )
            .ok();
        }
    }
    let u = store::user_by_id(&conn, user).ok_or_else(not_found)?;
    let member = Member {
        guild_id: guild,
        user: u,
        nickname: conn
            .query_row(
                "SELECT nickname FROM guild_members WHERE guild_id=?1 AND user_id=?2",
                params![guild, user],
                |r| r.get::<_, String>(0),
            )
            .unwrap_or_default(),
        roles: store::member_role_ids(&conn, guild, user),
        joined_at: now_ms(),
    };
    if roles_changed {
        store::record_audit(
            &conn,
            guild,
            me,
            20,
            Some(user),
            json!([{ "key": "roles", "new_value": member.roles }]),
        );
    }
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::MemberUpdate(Box::new(member.clone())));
    Ok(Json(member))
}

// ---------------------------------------------------------------------------
// Invites
// ---------------------------------------------------------------------------
