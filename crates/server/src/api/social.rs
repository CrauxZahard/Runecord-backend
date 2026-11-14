//! social.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

pub(crate) async fn friend_request(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(other): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if me == other || store::user_by_id(&conn, other).is_none() {
        return Err(bad("No such user."));
    }
    if !store::can_friend_request(&conn, me, other) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "This user is not accepting friend requests.".into(),
        ));
    }
    store::set_relationship(&conn, me, other, RelationshipKind::Outgoing);
    store::set_relationship(&conn, other, me, RelationshipKind::Incoming);
    drop(conn);
    let rel = Relationship {
        kind: RelationshipKind::Outgoing,
        user: User::default(),
        since: now_ms(),
    };
    let _ = state
        .tx
        .send(GatewayEvent::RelationshipUpdate(Box::new(rel)));
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn friend_accept(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(other): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    store::set_relationship(&conn, me, other, RelationshipKind::Friend);
    store::set_relationship(&conn, other, me, RelationshipKind::Friend);
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::RelationshipUpdate(Box::new(Relationship {
            kind: RelationshipKind::Friend,
            user: User::default(),
            since: now_ms(),
        })));
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn friend_remove(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(other): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    store::remove_relationship(&conn, me, other);
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::RelationshipRemove { user_id: other });
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn friend_block(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(other): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    store::set_relationship(&conn, me, other, RelationshipKind::Blocked);
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::RelationshipUpdate(Box::new(Relationship {
            kind: RelationshipKind::Blocked,
            user: User::default(),
            since: now_ms(),
        })));
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub(crate) struct GroupDmBody {
    #[serde(default)]
    name: String,
    user_ids: Vec<Id>,
}

pub(crate) async fn create_group_dm(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<GroupDmBody>,
) -> Result<Json<DmChannel>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let mut users: Vec<Id> = Vec::new();
    for user in body.user_ids {
        if user != me && !users.contains(&user) {
            users.push(user);
        }
    }
    if users.len() < 2 {
        return Err(bad("Pick at least two friends."));
    }
    if users.len() > 9 {
        return Err(bad("Group DMs support up to 10 people."));
    }
    let conn = state.store.conn();
    for user in &users {
        let Some(target) = store::user_by_id(&conn, *user) else {
            return Err(not_found());
        };
        if !store::can_dm(&conn, me, target.id) {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                format!(
                    "{}'s privacy settings do not allow new direct messages.",
                    target.display_name
                ),
            ));
        }
    }
    let name = body.name.trim().chars().take(100).collect::<String>();
    let dm = store::create_group_dm(&conn, me, &name, &users).ok_or_else(not_found)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::DmCreate(Box::new(dm.clone())));
    Ok(Json(dm))
}

fn dm_payload_all(conn: &rusqlite::Connection, channel_id: Id) -> Option<DmChannel> {
    store::dm_payload(conn, channel_id)
}

fn broadcast_dm_update(state: &AppState, channel_id: Id) {
    let payload = {
        let conn = state.store.conn();
        dm_payload_all(&conn, channel_id)
    };
    if let Some(dm) = payload {
        let _ = state.tx.send(GatewayEvent::DmUpdate(Box::new(dm)));
    }
}

#[derive(Deserialize)]
pub(crate) struct DmRenameBody {
    #[serde(default)]
    name: String,
}

pub(crate) async fn update_dm(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(dm): Path<Id>,
    Json(body): Json<DmRenameBody>,
) -> Result<Json<DmChannel>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let payload = {
        let conn = state.store.conn();
        let channel = store::channel_by_id(&conn, dm).ok_or_else(not_found)?;
        if channel.kind != ChannelKind::GroupDm {
            return Err(bad("Only group DMs can be renamed."));
        }
        if !store::is_dm_member(&conn, dm, me) {
            return Err(forbidden());
        }
        if store::dm_owner(&conn, dm) != Some(me) {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "Only the group owner can change the name.".into(),
            ));
        }
        let name: String = body.name.trim().chars().take(100).collect();
        if name.is_empty() {
            return Err(bad("Group name can't be empty."));
        }
        store::dm_rename(&conn, dm, &name);
        dm_payload_all(&conn, dm).ok_or_else(not_found)?
    };
    let _ = state
        .tx
        .send(GatewayEvent::DmUpdate(Box::new(payload.clone())));
    Ok(Json(payload))
}

const DM_ICON_LIMIT: usize = 4 * 1024 * 1024;

pub(crate) async fn upload_dm_icon(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(dm): Path<Id>,
    body: Bytes,
) -> Result<Json<DmChannel>, ApiError> {
    set_dm_icon(state, headers, dm, body).await
}

pub(crate) async fn remove_dm_icon(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(dm): Path<Id>,
) -> Result<Json<DmChannel>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let payload = {
        let conn = state.store.conn();
        let channel = store::channel_by_id(&conn, dm).ok_or_else(not_found)?;
        if channel.kind != ChannelKind::GroupDm {
            return Err(bad("Only group DMs have icons."));
        }
        if !store::is_dm_member(&conn, dm, me) {
            return Err(forbidden());
        }
        if store::dm_owner(&conn, dm) != Some(me) {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "Only the group owner can change the icon.".into(),
            ));
        }
        store::dm_set_icon(&conn, dm, "");
        dm_payload_all(&conn, dm).ok_or_else(not_found)?
    };
    let _ = state
        .tx
        .send(GatewayEvent::DmUpdate(Box::new(payload.clone())));
    Ok(Json(payload))
}

async fn set_dm_icon(
    state: Arc<AppState>,
    headers: HeaderMap,
    dm: Id,
    body: Bytes,
) -> Result<Json<DmChannel>, ApiError> {
    let me = auth_user(&state, &headers)?;
    {
        let conn = state.store.conn();
        let channel = store::channel_by_id(&conn, dm).ok_or_else(not_found)?;
        if channel.kind != ChannelKind::GroupDm {
            return Err(bad("Only group DMs have icons."));
        }
        if !store::is_dm_member(&conn, dm, me) {
            return Err(forbidden());
        }
        if store::dm_owner(&conn, dm) != Some(me) {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "Only the group owner can change the icon.".into(),
            ));
        }
    }
    if body.is_empty() {
        return Err(bad("Pick an image first."));
    }
    if body.len() > DM_ICON_LIMIT {
        return Err(bad("Group icons must be 4 MB or smaller."));
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
        return Err(bad("Group icons must be square."));
    }
    if width > 4096 {
        return Err(bad("Group icons must be 4096 pixels or smaller."));
    }
    let url = state.media.save_with_ext(&body, ext).map_err(internal)?.url;
    let payload = {
        let conn = state.store.conn();
        store::dm_set_icon(&conn, dm, &url);
        dm_payload_all(&conn, dm).ok_or_else(not_found)?
    };
    let _ = state
        .tx
        .send(GatewayEvent::DmUpdate(Box::new(payload.clone())));
    Ok(Json(payload))
}

#[derive(Deserialize)]
pub(crate) struct DmMemberBody {
    user_id: Id,
}

pub(crate) async fn add_dm_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(dm): Path<Id>,
    Json(body): Json<DmMemberBody>,
) -> Result<Json<DmChannel>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let payload = {
        let conn = state.store.conn();
        let channel = store::channel_by_id(&conn, dm).ok_or_else(not_found)?;
        if channel.kind != ChannelKind::GroupDm {
            return Err(bad("Only group DMs can take new members."));
        }
        if !store::is_dm_member(&conn, dm, me) {
            return Err(forbidden());
        }
        if store::dm_members(&conn, dm).len() >= 10 {
            return Err(bad("Group DMs support up to 10 people."));
        }
        let target = store::user_by_id(&conn, body.user_id).ok_or_else(not_found)?;
        if store::is_dm_member(&conn, dm, target.id) {
            return Err(bad("That person is already in this group."));
        }
        if !store::can_dm(&conn, me, target.id) {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                format!(
                    "{}'s privacy settings do not allow new direct messages.",
                    target.display_name
                ),
            ));
        }
        store::dm_add_member(&conn, dm, target.id);
        dm_payload_all(&conn, dm).ok_or_else(not_found)?
    };
    let _ = state
        .tx
        .send(GatewayEvent::DmUpdate(Box::new(payload.clone())));
    Ok(Json(payload))
}

pub(crate) async fn remove_dm_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((dm, user)): Path<(Id, Id)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    {
        let conn = state.store.conn();
        let channel = store::channel_by_id(&conn, dm).ok_or_else(not_found)?;
        if channel.kind != ChannelKind::GroupDm {
            return Err(bad("Only group DMs have members to remove."));
        }
        if !store::is_dm_member(&conn, dm, me) {
            return Err(forbidden());
        }
        if store::dm_owner(&conn, dm) != Some(me) {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "Only the group owner can remove members.".into(),
            ));
        }
        if user == me {
            return Err(bad("Leave the group instead of removing yourself."));
        }
        if !store::is_dm_member(&conn, dm, user) {
            return Err(not_found());
        }
        if store::dm_owner(&conn, dm) == Some(user) {
            return Err(forbidden());
        }
        store::dm_remove_member(&conn, dm, user);
    }
    let _ = state.tx.send(GatewayEvent::DmRemove {
        channel_id: dm,
        user_id: user,
    });
    broadcast_dm_update(&state, dm);
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn leave_dm(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(dm): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    {
        let conn = state.store.conn();
        let channel = store::channel_by_id(&conn, dm).ok_or_else(not_found)?;
        if channel.kind != ChannelKind::GroupDm {
            return Err(bad("Only group DMs can be left."));
        }
        if !store::is_dm_member(&conn, dm, me) {
            return Err(not_found());
        }
        let members = store::dm_members(&conn, dm);
        if store::dm_owner(&conn, dm) == Some(me) {
            match members.iter().copied().find(|member| *member != me) {
                Some(next) => {
                    store::dm_transfer_owner(&conn, dm, next);
                }
                None => {
                    store::delete_dm(&conn, dm);
                    let _ = state.tx.send(GatewayEvent::DmRemove {
                        channel_id: dm,
                        user_id: me,
                    });
                    return Ok(Json(json!({ "ok": true })));
                }
            }
        }
        store::dm_remove_member(&conn, dm, me);
    }
    let _ = state.tx.send(GatewayEvent::DmRemove {
        channel_id: dm,
        user_id: me,
    });
    broadcast_dm_update(&state, dm);
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn open_dm(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(other): Path<Id>,
) -> Result<Json<DmChannel>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let other_user = store::user_by_id(&conn, other).ok_or_else(not_found)?;
    let dm = if let Some(id) = store::find_direct_dm(&conn, me, other) {
        let channel = store::channel_by_id(&conn, id).ok_or_else(not_found)?;
        DmChannel {
            channel,
            recipients: vec![other_user],
        }
    } else {
        if !store::can_dm(&conn, me, other) {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "This user's privacy settings do not allow new direct messages.".into(),
            ));
        }
        store::create_direct_dm(&conn, me, other).ok_or_else(not_found)?
    };
    Ok(Json(dm))
}

// ---------------------------------------------------------------------------
// Roles and members
// ---------------------------------------------------------------------------
