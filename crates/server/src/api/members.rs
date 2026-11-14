//! members.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
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
    #[serde(default)]
    channel_id: Option<Id>,
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
    if let Some(channel) = query.channel_id {
        let target = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
        if target.guild_id != Some(guild) {
            return Err(not_found());
        }
        if !store::can_view(&conn, channel, me) {
            return Err(forbidden());
        }
    }
    let page = store::members_page(
        &conn,
        guild,
        me,
        query.channel_id,
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
        store::set_nickname(&conn, guild, user, &nick);
    }
    if let Some(roles) = body.role_ids {
        let actor = store::base_permissions(&conn, guild, me);
        let admin = actor & perm::ADMINISTRATOR != 0;
        let top = store::member_top_role(&conn, guild, me);
        store::clear_member_roles(&conn, guild, user);
        for rid in roles {
            if !admin {
                if let Some(pos) = store::role_position(&conn, guild, rid) {
                    if pos >= top {
                        continue;
                    }
                }
            }
            store::add_member_role(&conn, guild, user, rid);
        }
    }
    let u = store::user_by_id(&conn, user).ok_or_else(not_found)?;
    let member = Member {
        guild_id: guild,
        user: u,
        nickname: store::member_nickname(&conn, guild, user),
        roles: store::member_role_ids(&conn, guild, user),
        joined_at: now_ms(),
        timeout_until: store::member_timeout(&conn, guild, user),
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
