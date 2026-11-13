//! guilds.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use rusqlite::{OptionalExtension, params};
use serde_json::json;
use std::sync::Arc;

pub(crate) async fn guild_info(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
) -> Result<Json<GuildDetail>, ApiError> {
    let me = auth_user(&state, &headers)?;
    // `total_changes` is a monotonic write counter: folding it into the key means
    // any channel, role or overwrite edit invalidates the entry without a
    // separate invalidation path.
    let generation = { state.store.conn().total_changes() as i64 };
    let key = (guild, me, generation);
    if let Some(hit) = state
        .guild_cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
    {
        return Ok(Json((*hit).clone()));
    }

    let detail = {
        let conn = state.store.conn();
        if !store::guild_member(&conn, guild, me) {
            return Err(not_found());
        }
        let detail = store::guild_detail(&conn, guild, me).ok_or_else(not_found)?;
        if !has(detail.permissions, perm::VIEW_CHANNEL) {
            return Err(forbidden());
        }
        detail
    };
    state
        .guild_cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .put(key, Arc::new(detail.clone()));
    Ok(Json(detail))
}

pub(crate) async fn create_guild(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateGuild>,
) -> Result<Json<Guild>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(bad("Give the server a name."));
    }
    let mut conn = state.store.conn();
    let tx = conn.transaction().map_err(internal)?;
    let gid = store::next_id(&tx);
    let everyone = store::next_id(&tx);
    tx.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(?1,?2,?3,?4,'',?5,?6)",
        params![gid, name, body.icon_color as i64, me, now_ms(), everyone],
    )
    .map_err(internal)?;
    tx.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(?1,?2,'@everyone',0,?3,0,0,0)",
        params![everyone, gid, (perm::EVERYONE | perm::CREATE_INVITE) as i64],
    )
    .map_err(internal)?;
    tx.execute(
        "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(?1,?2,'',?3)",
        params![gid, me, now_ms()],
    )
    .map_err(internal)?;
    let cat = store::next_id(&tx);
    tx.execute(
        "INSERT INTO channels(id, guild_id, name, kind, position, topic) VALUES(?1,?2,'Text Channels','category',0,'')",
        params![cat, gid],
    )
    .map_err(internal)?;
    let general = store::next_id(&tx);
    tx.execute(
        "INSERT INTO channels(id, guild_id, name, kind, position, parent_id, topic) VALUES(?1,?2,'general','text',1,?3,'This is where it all begins')",
        params![general, gid, cat],
    )
    .map_err(internal)?;
    tx.commit().map_err(internal)?;
    drop(conn);

    let guild = Guild {
        id: gid,
        name: name.to_string(),
        icon_color: body.icon_color,
        owner_id: me,
        member_count: 1,
        created_at: now_ms(),
        channels: vec![
            Channel {
                id: cat,
                guild_id: Some(gid),
                name: "Text Channels".into(),
                kind: ChannelKind::Category,
                position: 0,
                ..Channel::default()
            },
            Channel {
                id: general,
                guild_id: Some(gid),
                name: "general".into(),
                kind: ChannelKind::Text,
                position: 1,
                parent_id: Some(cat),
                topic: "This is where it all begins".into(),
                ..Channel::default()
            },
        ],
        ..Guild::default()
    };
    let _ = state
        .tx
        .send(GatewayEvent::GuildCreate(Box::new(guild.clone())));
    Ok(Json(guild))
}

pub(crate) async fn update_guild(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
    Json(body): Json<UpdateGuild>,
) -> Result<Json<Guild>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let owner: Option<Id> = conn
        .query_row(
            "SELECT owner_id FROM guilds WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?;
    if owner != Some(me) {
        return Err(forbidden());
    }
    if let Some(name) = body.name {
        conn.execute(
            "UPDATE guilds SET name=?1 WHERE id=?2",
            params![name.trim(), id],
        )
        .map_err(internal)?;
    }
    if let Some(color) = body.icon_color {
        conn.execute(
            "UPDATE guilds SET icon_color=?1 WHERE id=?2",
            params![color as i64, id],
        )
        .map_err(internal)?;
    }
    if let Some(tag) = body.tag {
        conn.execute(
            "UPDATE guilds SET tag=?1 WHERE id=?2",
            params![tag.chars().take(8).collect::<String>(), id],
        )
        .map_err(internal)?;
    }
    if let Some(desc) = body.description {
        conn.execute(
            "UPDATE guilds SET description=?1 WHERE id=?2",
            params![desc, id],
        )
        .map_err(internal)?;
    }
    let guild = Guild {
        id,
        ..Guild::default()
    };
    let _ = state
        .tx
        .send(GatewayEvent::GuildUpdate(Box::new(guild.clone())));
    Ok(Json(guild))
}

pub(crate) async fn delete_guild(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let owner: Option<Id> = conn
        .query_row(
            "SELECT owner_id FROM guilds WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?;
    if owner != Some(me) {
        return Err(forbidden());
    }
    conn.execute("DELETE FROM channels WHERE guild_id=?1", params![id])
        .ok();
    conn.execute("DELETE FROM guild_members WHERE guild_id=?1", params![id])
        .ok();
    conn.execute("DELETE FROM roles WHERE guild_id=?1", params![id])
        .ok();
    conn.execute("DELETE FROM guilds WHERE id=?1", params![id])
        .ok();
    let _ = state.tx.send(GatewayEvent::GuildDelete { guild_id: id });
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn list_roles(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
) -> Result<Json<Vec<Role>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(
        store::base_permissions(&conn, guild, me),
        perm::VIEW_CHANNEL,
    ) {
        return Err(forbidden());
    }
    Ok(Json(store::guild_roles(&conn, guild)))
}

pub(crate) async fn my_permissions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(
        store::base_permissions(&conn, guild, me),
        perm::VIEW_CHANNEL,
    ) {
        return Err(forbidden());
    }
    let mut channels = serde_json::Map::new();
    for channel in store::list_children(&conn, guild) {
        channels.insert(
            channel.id.to_string(),
            json!(store::channel_permissions(&conn, channel.id, me)),
        );
    }
    Ok(Json(
        json!({ "permissions": store::base_permissions(&conn, guild, me), "channels": channels }),
    ))
}
