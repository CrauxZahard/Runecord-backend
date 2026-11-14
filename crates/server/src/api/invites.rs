//! invites.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use std::sync::Arc;

pub(crate) fn invite_code() -> String {
    const ALPHA: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut bytes = [0u8; 8];
    rand::rng().fill_bytes(&mut bytes);
    bytes
        .iter()
        .map(|b| ALPHA[*b as usize % ALPHA.len()] as char)
        .collect()
}

pub(crate) async fn create_invite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
    Json(body): Json<CreateInvite>,
) -> Result<Json<Invite>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(
        store::base_permissions(&conn, guild, me),
        perm::CREATE_INVITE,
    ) {
        return Err(forbidden());
    }
    let code = invite_code();
    let invite = store::create_invite(&conn, &code, guild, me, body.max_uses)
        .ok_or_else(|| internal("could not create invite"))?;
    Ok(Json(invite))
}

pub(crate) async fn invite_info(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<Json<Invite>, ApiError> {
    let conn = state.store.conn();
    store::invite_by_code(&conn, &code)
        .map(Json)
        .ok_or_else(not_found)
}

pub(crate) async fn join_invite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> Result<Json<Guild>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let Some(guild) = store::guild_for_invite(&conn, &code) else {
        return Err(not_found());
    };
    if store::is_banned(&conn, guild, me) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "You are banned from this server.".into(),
        ));
    }
    store::join_invite(&conn, guild, &code, me);
    let out = Guild {
        id: guild,
        ..Guild::default()
    };
    let count = store::guild_member_count(&conn, guild);
    let member = store::guild_member_detail(&conn, guild, me);
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::GuildCreate(Box::new(out.clone())));
    let _ = state.tx.send(GatewayEvent::MemberCountUpdate {
        guild_id: guild,
        count,
    });
    if let Some(member) = member {
        let _ = state.tx.send(GatewayEvent::MemberUpdate(Box::new(member)));
    }
    Ok(Json(out))
}

// ---------------------------------------------------------------------------
// Gateway
// ---------------------------------------------------------------------------
