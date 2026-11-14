//! channels.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize)]
pub(crate) struct ThreadPatch {
    archived: bool,
}

pub(crate) async fn update_thread(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
    Json(body): Json<ThreadPatch>,
) -> Result<Json<Channel>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let target = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
    if target.kind != ChannelKind::Thread {
        return Err(bad("Not a thread."));
    }
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    let perms = store::channel_permissions(&conn, channel, me);
    if target.owner_id != Some(me)
        && !has(perms, perm::MANAGE_THREADS)
        && !has(perms, perm::MANAGE_CHANNELS)
    {
        return Err(forbidden());
    }
    store::set_thread_archived(&conn, channel, body.archived).map_err(internal)?;
    let out = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
    let out = channel_for(&conn, out, me);
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::ChannelUpdate(Box::new(out.clone())));
    Ok(Json(out))
}

pub(crate) async fn create_channel(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
    Json(body): Json<CreateChannel>,
) -> Result<Json<Channel>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(
        store::base_permissions(&conn, guild, me),
        perm::MANAGE_CHANNELS,
    ) {
        return Err(forbidden());
    }
    let name = body.name.trim().to_lowercase().replace(' ', "-");
    if name.is_empty() {
        return Err(bad("Name the channel."));
    }
    let position = store::next_channel_position(&conn, guild);
    let channel = store::insert_channel(
        &conn,
        guild,
        &name,
        kind_str(body.kind),
        position,
        body.parent_id,
        &body.topic,
        body.slowmode,
        body.nsfw,
    )
    .ok_or_else(|| internal("could not create channel"))?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::ChannelCreate(Box::new(channel.clone())));
    Ok(Json(channel))
}

pub(crate) fn kind_str(kind: ChannelKind) -> &'static str {
    match kind {
        ChannelKind::Text => "text",
        ChannelKind::Voice => "voice",
        ChannelKind::Category => "category",
        ChannelKind::Announcement => "announcement",
        ChannelKind::Forum => "forum",
        ChannelKind::Thread => "thread",
        ChannelKind::Dm => "dm",
        ChannelKind::GroupDm => "group_dm",
    }
}

pub(crate) async fn update_channel(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
    Json(body): Json<CreateChannel>,
) -> Result<Json<Channel>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let guild = store::guild_id_for_channel(&conn, id).ok_or_else(not_found)?;
    if let Some(guild) = guild {
        if !has(
            store::base_permissions(&conn, guild, me),
            perm::MANAGE_CHANNELS,
        ) {
            return Err(forbidden());
        }
    }
    if body.name.trim().is_empty() {
        return Err(bad("Name the channel."));
    }
    store::update_channel_fields(
        &conn,
        id,
        &body.name.trim().to_lowercase().replace(' ', "-"),
        &body.topic,
        body.slowmode,
        body.nsfw,
        &body.status.trim().chars().take(128).collect::<String>(),
    )
    .map_err(internal)?;
    let channel = store::channel_by_id(&conn, id).ok_or_else(not_found)?;
    let channel = channel_for(&conn, channel, me);
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::ChannelUpdate(Box::new(channel.clone())));
    Ok(Json(channel))
}

pub(crate) async fn delete_channel(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let guild = store::guild_id_for_channel(&conn, id).ok_or_else(not_found)?;
    if let Some(guild) = guild {
        if !has(
            store::base_permissions(&conn, guild, me),
            perm::MANAGE_CHANNELS,
        ) {
            return Err(forbidden());
        }
    }
    let doomed = store::delete_channel_cascade(&conn, id);
    drop(conn);
    for channel in doomed.into_iter().skip(1) {
        let _ = state.tx.send(GatewayEvent::ChannelDelete {
            channel_id: channel,
            guild_id: guild,
        });
    }
    let _ = state.tx.send(GatewayEvent::ChannelDelete {
        channel_id: id,
        guild_id: guild,
    });
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

pub(crate) async fn channel_overwrites(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
) -> Result<Json<Vec<PermissionOverwrite>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::can_view(&conn, channel, me) {
        return Err(forbidden());
    }
    Ok(Json(store::overwrites(&conn, channel)))
}

#[derive(Deserialize)]
pub(crate) struct OverwriteBody {
    kind: String,
    #[serde(default)]
    allow: u64,
    #[serde(default)]
    deny: u64,
}

pub(crate) async fn set_channel_overwrite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel, target)): Path<(Id, Id)>,
    Json(body): Json<OverwriteBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let ch = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
    let Some(guild) = ch.guild_id else {
        return Err(forbidden());
    };
    let actor = store::base_permissions(&conn, guild, me);
    if !has(actor, perm::MANAGE_ROLES) {
        return Err(forbidden());
    }
    let admin = actor & perm::ADMINISTRATOR != 0;
    if !admin && body.allow & !actor != 0 {
        return Err(forbidden());
    }
    let kind = if body.kind == "member" {
        "member"
    } else {
        "role"
    };
    if kind == "member" {
        if !store::can_manage_member(&conn, guild, me, target) {
            return Err(forbidden());
        }
    } else if !admin {
        if let Some(pos) = store::role_position(&conn, guild, target) {
            if pos >= store::member_top_role(&conn, guild, me) {
                return Err(forbidden());
            }
        }
    }
    store::set_overwrite(&conn, channel, target, kind, body.allow, body.deny);
    let updated = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::ChannelUpdate(Box::new(updated)));
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn clear_channel_overwrite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel, target)): Path<(Id, Id)>,
    Json(body): Json<OverwriteBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let ch = store::channel_by_id(&conn, channel).ok_or_else(not_found)?;
    let Some(guild) = ch.guild_id else {
        return Err(forbidden());
    };
    if !has(
        store::base_permissions(&conn, guild, me),
        perm::MANAGE_ROLES,
    ) {
        return Err(forbidden());
    }
    let kind = if body.kind == "member" {
        "member"
    } else {
        "role"
    };
    store::delete_overwrite(&conn, channel, target, kind);
    Ok(Json(json!({ "ok": true })))
}
