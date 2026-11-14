//! social.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use rusqlite::{OptionalExtension, params};
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
    conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'outgoing',?3)", params![me, other, now_ms()]).ok();
    conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'incoming',?3)", params![other, me, now_ms()]).ok();
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
    conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'friend',?3)", params![me, other, now_ms()]).ok();
    conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'friend',?3)", params![other, me, now_ms()]).ok();
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
    conn.execute("DELETE FROM relationships WHERE (user_id=?1 AND other_id=?2) OR (user_id=?2 AND other_id=?1)", params![me, other]).ok();
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
    conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'blocked',?3)", params![me, other, now_ms()]).ok();
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
    let id = store::next_id(&conn);
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, topic, owner_id) VALUES(?1,NULL,?2,'group_dm','',?3)",
        params![id, name, me],
    )
    .map_err(internal)?;
    conn.execute(
        "INSERT INTO dm_members(channel_id, user_id) VALUES(?1,?2)",
        params![id, me],
    )
    .ok();
    for user in &users {
        conn.execute(
            "INSERT INTO dm_members(channel_id, user_id) VALUES(?1,?2)",
            params![id, user],
        )
        .ok();
    }
    let channel = store::channel_by_id(&conn, id).ok_or_else(not_found)?;
    let recipients = users
        .iter()
        .filter_map(|user| store::user_by_id(&conn, *user))
        .collect::<Vec<_>>();
    let dm = DmChannel {
        channel,
        recipients,
    };
    drop(conn);
    let _ = state.tx.send(GatewayEvent::DmCreate(Box::new(dm.clone())));
    Ok(Json(dm))
}

fn dm_payload_all(conn: &rusqlite::Connection, channel_id: Id) -> Option<DmChannel> {
    let channel = store::channel_by_id(conn, channel_id)?;
    let mut recipients = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT u.* FROM users u JOIN dm_members d ON d.user_id=u.id
         WHERE d.channel_id=?1 ORDER BY d.rowid",
    ) {
        if let Ok(rows) = stmt.query_map(params![channel_id], store::user_from_row) {
            recipients.extend(rows.flatten());
        }
    }
    Some(DmChannel {
        channel,
        recipients,
    })
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
    let url = state
        .media
        .save_with_ext(&body, ext)
        .map_err(internal)?
        .url;
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
    let mut broadcast = false;
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
                    conn.execute("DELETE FROM messages WHERE channel_id=?1", params![dm])
                        .ok();
                    conn.execute("DELETE FROM dm_members WHERE channel_id=?1", params![dm])
                        .ok();
                    conn.execute("DELETE FROM channels WHERE id=?1", params![dm])
                        .ok();
                    let _ = state.tx.send(GatewayEvent::DmRemove {
                        channel_id: dm,
                        user_id: me,
                    });
                    return Ok(Json(json!({ "ok": true })));
                }
            }
        }
        store::dm_remove_member(&conn, dm, me);
        broadcast = true;
    }
    let _ = state.tx.send(GatewayEvent::DmRemove {
        channel_id: dm,
        user_id: me,
    });
    if broadcast {
        broadcast_dm_update(&state, dm);
    }
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
        if !store::can_dm(&conn, me, other) {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "This user's privacy settings do not allow new direct messages.".into(),
            ));
        }
        let id = store::next_id(&conn);
        conn.execute(
            "INSERT INTO channels(id, guild_id, name, kind, topic) VALUES(?1,NULL,'','dm','')",
            params![id],
        )
        .map_err(internal)?;
        conn.execute(
            "INSERT INTO dm_members(channel_id, user_id) VALUES(?1,?2)",
            params![id, me],
        )
        .ok();
        conn.execute(
            "INSERT INTO dm_members(channel_id, user_id) VALUES(?1,?2)",
            params![id, other],
        )
        .ok();
        id
    };
    let channel = store::channel_by_id(&conn, id).ok_or_else(not_found)?;
    Ok(Json(DmChannel {
        channel,
        recipients: vec![other_user],
    }))
}

// ---------------------------------------------------------------------------
// Roles and members
// ---------------------------------------------------------------------------
