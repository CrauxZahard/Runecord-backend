//! bots.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize)]
pub(crate) struct CreateBotBody {
    name: String,
    guild_id: Option<Id>,
}

#[derive(serde::Serialize)]
pub(crate) struct BotCreated {
    bot: User,
    token: String,
}

pub(crate) async fn list_bots(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<User>>, ApiError> {
    let me = auth_human(&state, &headers)?;
    let conn = state.store.conn();
    Ok(Json(store::bots_owned(&conn, me)))
}

pub(crate) async fn create_bot_account(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateBotBody>,
) -> Result<Json<BotCreated>, ApiError> {
    let me = auth_human(&state, &headers)?;
    let conn = state.store.conn();
    if let Some(guild) = body.guild_id
        && !has(
            store::base_permissions(&conn, guild, me),
            perm::MANAGE_GUILD,
        )
    {
        return Err(forbidden());
    }
    let (bot, token) = store::create_bot(&conn, me, &body.name).map_err(bad)?;
    if let Some(guild) = body.guild_id {
        let _ = store::add_bot_to_guild(&conn, bot.id, guild);
        let member = Member {
            guild_id: guild,
            user: bot.clone(),
            ..Member::default()
        };
        drop(conn);
        let _ = state.tx.send(GatewayEvent::MemberUpdate(Box::new(member)));
        return Ok(Json(BotCreated { bot, token }));
    }
    drop(conn);
    Ok(Json(BotCreated { bot, token }))
}

pub(crate) async fn delete_bot_account(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_human(&state, &headers)?;
    let conn = state.store.conn();
    if !store::delete_bot(&conn, me, id) {
        return Err(not_found());
    }
    drop(conn);
    let _ = state.tx.send(GatewayEvent::GuildMembers {
        guild_id: 0,
        members: Vec::new(),
    });
    Ok(Json(json!({ "ok": true })))
}
