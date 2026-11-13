//! Kick, ban and timeout, with permission and hierarchy checks.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize, Default)]
pub(crate) struct ReasonBody {
    #[serde(default)]
    reason: String,
}

#[derive(Deserialize)]
pub(crate) struct TimeoutBody {
    seconds: u64,
}

#[derive(Deserialize, Default)]
pub(crate) struct PruneBody {
    #[serde(default)]
    days: u32,
    #[serde(default)]
    role_ids: Vec<Id>,
}

fn guild_of(conn: &rusqlite::Connection, guild: Id) -> Option<Id> {
    conn.query_row("SELECT id FROM guilds WHERE id=?1", params![guild], |r| {
        r.get(0)
    })
    .optional()
    .ok()
    .flatten()
}

fn owner_of(conn: &rusqlite::Connection, guild: Id) -> Option<Id> {
    conn.query_row(
        "SELECT owner_id FROM guilds WHERE id=?1",
        params![guild],
        |r| r.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

/// Shared validation for all three actions. Returns the actor's reason or an error.
fn check(
    conn: &rusqlite::Connection,
    guild: Id,
    me: Id,
    user: Id,
    permission: u64,
) -> Result<(), ApiError> {
    if guild_of(conn, guild).is_none() {
        return Err(not_found());
    }
    if me == user {
        return Err(bad("You cannot moderate yourself."));
    }
    if owner_of(conn, guild) == Some(user) {
        return Err(forbidden());
    }
    if !store::guild_member(conn, guild, me) || !store::guild_member(conn, guild, user) {
        return Err(not_found());
    }
    let perms = store::base_permissions(conn, guild, me);
    if !has(perms, permission) {
        return Err(forbidden());
    }
    if !store::can_manage_member(conn, guild, me, user) {
        return Err(forbidden());
    }
    Ok(())
}

fn disconnect_voice(state: &AppState, user: Id) {
    let removed = state
        .voice
        .lock()
        .ok()
        .and_then(|mut voice| voice.remove(&user));
    if let Some(previous) = removed {
        let _ = state
            .tx
            .send(GatewayEvent::VoiceStateUpdate(Box::new(VoiceState {
                user_id: user,
                guild_id: previous.guild_id,
                channel_id: None,
                self_mute: false,
                self_deaf: false,
            })));
    }
}

pub(crate) async fn list_bans(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
) -> Result<Json<Vec<Ban>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if guild_of(&conn, guild).is_none() {
        return Err(not_found());
    }
    if !has(store::base_permissions(&conn, guild, me), perm::BAN_MEMBERS) {
        return Err(forbidden());
    }
    Ok(Json(store::list_bans(&conn, guild)))
}

pub(crate) async fn unban_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, user)): Path<(Id, Id)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if guild_of(&conn, guild).is_none() {
        return Err(not_found());
    }
    if !has(store::base_permissions(&conn, guild, me), perm::BAN_MEMBERS) {
        return Err(forbidden());
    }
    if !store::unban_member(&conn, guild, user) {
        return Err(not_found());
    }
    let name = store::user_by_id(&conn, user)
        .map(|user| user.display_name)
        .unwrap_or_default();
    store::record_audit(
        &conn,
        guild,
        me,
        24,
        Some(user),
        json!([{ "key": "name", "new_value": name }]),
    );
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn prune_preview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
    Json(body): Json<PruneBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if guild_of(&conn, guild).is_none() {
        return Err(not_found());
    }
    if !has(
        store::base_permissions(&conn, guild, me),
        perm::KICK_MEMBERS,
    ) {
        return Err(forbidden());
    }
    let days = body.days.clamp(1, 30);
    let candidates = store::prune_candidates(&conn, guild, days, &body.role_ids, me);
    Ok(Json(
        json!({ "days": days, "count": candidates.len() as u32 }),
    ))
}

pub(crate) async fn prune_members(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
    Json(body): Json<PruneBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if guild_of(&conn, guild).is_none() {
        return Err(not_found());
    }
    if !has(
        store::base_permissions(&conn, guild, me),
        perm::KICK_MEMBERS,
    ) {
        return Err(forbidden());
    }
    let days = body.days.clamp(1, 30);
    let candidates = store::prune_candidates(&conn, guild, days, &body.role_ids, me);
    let pruned = store::prune_members(&conn, guild, &candidates);
    store::record_audit(
        &conn,
        guild,
        me,
        25,
        None,
        json!([
            { "key": "days", "new_value": days },
            { "key": "pruned", "new_value": pruned.len() as u32 }
        ]),
    );
    let count: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM guild_members WHERE guild_id=?1",
            params![guild],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as u32;
    drop(conn);
    for user in &pruned {
        let _ = state.tx.send(GatewayEvent::MemberRemove {
            guild_id: guild,
            user_id: *user,
        });
    }
    let _ = state.tx.send(GatewayEvent::MemberCountUpdate {
        guild_id: guild,
        count,
    });
    Ok(Json(
        json!({ "days": days, "pruned": pruned.len() as u32, "members": pruned }),
    ))
}

pub(crate) async fn kick_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, user)): Path<(Id, Id)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    check(&conn, guild, me, user, perm::KICK_MEMBERS)?;
    let name = store::user_by_id(&conn, user)
        .map(|user| user.display_name)
        .unwrap_or_default();
    store::kick_member(&conn, guild, user);
    store::record_audit(
        &conn,
        guild,
        me,
        21,
        Some(user),
        json!([{ "key": "name", "new_value": name }]),
    );
    drop(conn);
    disconnect_voice(&state, user);
    let _ = state.tx.send(GatewayEvent::MemberRemove {
        guild_id: guild,
        user_id: user,
    });
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn ban_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, user)): Path<(Id, Id)>,
    body: Json<ReasonBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let reason = body.0.reason.trim().chars().take(300).collect::<String>();
    let conn = state.store.conn();
    check(&conn, guild, me, user, perm::BAN_MEMBERS)?;
    let name = store::user_by_id(&conn, user)
        .map(|user| user.display_name)
        .unwrap_or_default();
    store::ban_member(&conn, guild, user, me, &reason);
    store::record_audit(
        &conn,
        guild,
        me,
        22,
        Some(user),
        json!([
            { "key": "name", "new_value": name },
            { "key": "reason", "new_value": reason }
        ]),
    );
    drop(conn);
    disconnect_voice(&state, user);
    let _ = state.tx.send(GatewayEvent::MemberRemove {
        guild_id: guild,
        user_id: user,
    });
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn timeout_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, user)): Path<(Id, Id)>,
    Json(body): Json<TimeoutBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let seconds = body.seconds.min(28 * 24 * 60 * 60);
    let conn = state.store.conn();
    check(&conn, guild, me, user, perm::MODERATE_MEMBERS)?;
    let until = if seconds == 0 {
        0
    } else {
        now_ms() + seconds as i64 * 1000
    };
    let name = store::user_by_id(&conn, user)
        .map(|user| user.display_name)
        .unwrap_or_default();
    store::timeout_member(&conn, guild, user, until);
    let until_label = if until == 0 {
        json!("cleared")
    } else {
        json!(until)
    };
    store::record_audit(
        &conn,
        guild,
        me,
        23,
        Some(user),
        json!([
            { "key": "name", "new_value": name },
            { "key": "until", "new_value": until_label }
        ]),
    );
    let member = store::guild_member_detail(&conn, guild, user);
    drop(conn);
    if let Some(member) = member {
        let _ = state.tx.send(GatewayEvent::MemberUpdate(Box::new(member)));
    }
    Ok(Json(json!({ "ok": true, "until": until })))
}

#[derive(Deserialize, Default)]
pub(crate) struct AppealBody {
    #[serde(default)]
    message: String,
}

pub(crate) async fn submit_appeal(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(code): Path<String>,
    Json(body): Json<AppealBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let guild: Option<Id> = conn
        .query_row(
            "SELECT guild_id FROM invites WHERE code=?1",
            params![code],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?;
    let Some(guild) = guild else {
        return Err(not_found());
    };
    if !store::is_banned(&conn, guild, me) {
        return Err(bad("You are not banned from this server."));
    }
    let message = body.message.trim().chars().take(1000).collect::<String>();
    if message.is_empty() {
        return Err(bad("Explain why the ban should be lifted."));
    }
    let id = store::create_appeal(&conn, guild, me, &message);
    store::record_audit(
        &conn,
        guild,
        me,
        28,
        None,
        json!([{ "key": "message", "new_value": message }]),
    );
    Ok(Json(json!({ "ok": true, "appeal": id })))
}

pub(crate) async fn list_appeals(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
) -> Result<Json<Vec<BanAppeal>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if guild_of(&conn, guild).is_none() {
        return Err(not_found());
    }
    if !has(store::base_permissions(&conn, guild, me), perm::BAN_MEMBERS) {
        return Err(forbidden());
    }
    Ok(Json(store::list_appeals(&conn, guild)))
}

pub(crate) async fn accept_appeal(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, appeal)): Path<(Id, Id)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    resolve_appeal_call(state, headers, guild, appeal, true).await
}

pub(crate) async fn deny_appeal(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((guild, appeal)): Path<(Id, Id)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    resolve_appeal_call(state, headers, guild, appeal, false).await
}

async fn resolve_appeal_call(
    state: Arc<AppState>,
    headers: HeaderMap,
    guild: Id,
    appeal: Id,
    accepted: bool,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if guild_of(&conn, guild).is_none() {
        return Err(not_found());
    }
    if !has(store::base_permissions(&conn, guild, me), perm::BAN_MEMBERS) {
        return Err(forbidden());
    }
    let user = store::resolve_appeal(&conn, guild, appeal, me, accepted).ok_or_else(not_found)?;
    store::record_audit(
        &conn,
        guild,
        me,
        if accepted { 26 } else { 27 },
        Some(user),
        json!([{ "key": "appeal", "new_value": appeal }]),
    );
    Ok(Json(
        json!({ "ok": true, "user": user, "accepted": accepted }),
    ))
}
