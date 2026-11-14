//! guilds.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
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

pub(crate) async fn guild_commands(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
) -> Result<Json<Vec<ApplicationCommand>>, ApiError> {
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
    Ok(Json(store::commands_in_guild(&conn, guild)))
}

pub(crate) async fn boost_guild(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
) -> Result<Json<GuildSummary>, ApiError> {
    change_boost(state, headers, guild, true).await
}

pub(crate) async fn unboost_guild(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
) -> Result<Json<GuildSummary>, ApiError> {
    change_boost(state, headers, guild, false).await
}

async fn change_boost(
    state: Arc<AppState>,
    headers: HeaderMap,
    guild: Id,
    on: bool,
) -> Result<Json<GuildSummary>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::guild_member(&conn, guild, me) {
        return Err(forbidden());
    }
    if on {
        store::boost_guild(&conn, guild, me);
    } else {
        store::unboost_guild(&conn, guild, me);
    }
    let summary = store::guild_summary_with_unread(&conn, guild, me)
        .map(|(summary, _)| summary)
        .ok_or_else(not_found)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::GuildUpdate(Box::new(Guild {
        id: guild,
        name: summary.name.clone(),
        icon_color: summary.icon_color,
        owner_id: summary.owner_id,
        description: summary.description.clone(),
        member_count: summary.member_count,
        tag: summary.tag.clone(),
        permissions: summary.permissions,
        created_at: summary.created_at,
        ..Guild::default()
    })));
    Ok(Json(summary))
}

pub(crate) async fn leave_guild(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let Some(owner) = store::guild_owner(&conn, guild) else {
        return Err(not_found());
    };
    if owner == me {
        return Err(bad(
            "Delete the server or transfer ownership before leaving.",
        ));
    }
    if !store::guild_member(&conn, guild, me) {
        return Err(not_found());
    }
    store::leave_guild(&conn, guild, me);
    drop(conn);
    let _ = state.tx.send(GatewayEvent::MemberRemove {
        guild_id: guild,
        user_id: me,
    });
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub(crate) struct DiscoverableBody {
    discoverable: bool,
}

pub(crate) async fn set_guild_discoverable(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
    Json(body): Json<DiscoverableBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(
        store::base_permissions(&conn, guild, me),
        perm::MANAGE_GUILD,
    ) {
        return Err(forbidden());
    }
    if !store::set_guild_discoverable(&conn, guild, body.discoverable) {
        return Err(not_found());
    }
    Ok(Json(json!({ "discoverable": body.discoverable })))
}

pub(crate) async fn public_guilds(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<PublicGuild>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    Ok(Json(store::public_guilds(&conn, me)))
}

pub(crate) async fn join_public_guild(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::discoverable(&conn, guild) {
        return Err(not_found());
    }
    if !store::guild_member(&conn, guild, me) && store::is_banned(&conn, guild, me) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "You are banned from this server.".into(),
        ));
    }
    let already = store::guild_member(&conn, guild, me);
    if !already {
        store::join_public_guild(&conn, guild, me);
    }
    let count = store::guild_member_count(&conn, guild);
    drop(conn);
    let _ = state.tx.send(GatewayEvent::MemberCountUpdate {
        guild_id: guild,
        count,
    });
    Ok(Json(
        json!({ "ok": true, "guild_id": guild, "joined": !already }),
    ))
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
    let conn = state.store.conn();
    let guild = store::create_guild(&conn, me, name, body.icon_color)
        .ok_or_else(|| internal("could not create guild"))?;
    drop(conn);
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
) -> Result<Json<GuildSummary>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(store::base_permissions(&conn, id, me), perm::MANAGE_GUILD) {
        return Err(forbidden());
    }
    let name = body.name.as_deref().map(str::trim);
    if name.is_some_and(str::is_empty) {
        return Err(bad("Name the server."));
    }
    store::update_guild_fields(
        &conn,
        id,
        name,
        body.icon_color,
        body.tag.as_deref(),
        body.description.as_deref(),
        body.discoverable,
    )
    .map_err(internal)?;
    let summary = store::guild_summary_with_unread(&conn, id, me)
        .map(|(summary, _)| summary)
        .ok_or_else(not_found)?;
    drop(conn);
    let _ = state.tx.send(GatewayEvent::GuildUpdate(Box::new(Guild {
        id,
        name: summary.name.clone(),
        icon_color: summary.icon_color,
        owner_id: summary.owner_id,
        description: summary.description.clone(),
        member_count: summary.member_count,
        tag: summary.tag.clone(),
        permissions: summary.permissions,
        created_at: summary.created_at,
        ..Guild::default()
    })));
    Ok(Json(summary))
}

pub(crate) async fn delete_guild(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if store::guild_owner(&conn, id) != Some(me) {
        return Err(forbidden());
    }
    store::delete_guild_cascade(&conn, id);
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

// ---------------------------------------------------------------------------
// Server icon and banner
// ---------------------------------------------------------------------------

const ICON_LIMIT: usize = 4 * 1024 * 1024;
const BANNER_LIMIT: usize = 10 * 1024 * 1024;

#[derive(Deserialize)]
pub(crate) struct GuildNotifyBody {
    #[serde(default)]
    level: Option<NotificationLevel>,
    #[serde(default)]
    muted: Option<bool>,
    #[serde(default)]
    suppress_everyone: Option<bool>,
    #[serde(default)]
    suppress_roles: Option<bool>,
    #[serde(default)]
    suppress_highlights: Option<bool>,
    #[serde(default)]
    mute_new_events: Option<bool>,
    #[serde(default)]
    mobile_push: Option<bool>,
}

pub(crate) async fn guild_notifications(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
) -> Result<Json<GuildNotificationSettings>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::guild_member(&conn, guild, me) {
        return Err(not_found());
    }
    Ok(Json(store::guild_notification_settings(&conn, me, guild)))
}

pub(crate) async fn update_guild_notifications(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
    Json(body): Json<GuildNotifyBody>,
) -> Result<Json<GuildNotificationSettings>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::guild_member(&conn, guild, me) {
        return Err(not_found());
    }
    let mut settings = store::guild_notification_settings(&conn, me, guild);
    if let Some(level) = body.level {
        settings.level = level;
    }
    if let Some(value) = body.muted {
        settings.muted = value;
    }
    if let Some(value) = body.suppress_everyone {
        settings.suppress_everyone = value;
    }
    if let Some(value) = body.suppress_roles {
        settings.suppress_roles = value;
    }
    if let Some(value) = body.suppress_highlights {
        settings.suppress_highlights = value;
    }
    if let Some(value) = body.mute_new_events {
        settings.mute_new_events = value;
    }
    if let Some(value) = body.mobile_push {
        settings.mobile_push = value;
    }
    store::set_guild_notification_settings(&conn, me, guild, &settings);
    Ok(Json(settings))
}

pub(crate) async fn upload_guild_icon(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
    body: Bytes,
) -> Result<Json<GuildSummary>, ApiError> {
    set_guild_image(state, headers, guild, body, true).await
}

pub(crate) async fn upload_guild_banner(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
    body: Bytes,
) -> Result<Json<GuildSummary>, ApiError> {
    set_guild_image(state, headers, guild, body, false).await
}

pub(crate) async fn remove_guild_icon(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
) -> Result<Json<GuildSummary>, ApiError> {
    clear_guild_image(state, headers, guild, true).await
}

pub(crate) async fn remove_guild_banner(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
) -> Result<Json<GuildSummary>, ApiError> {
    clear_guild_image(state, headers, guild, false).await
}

async fn set_guild_image(
    state: Arc<AppState>,
    headers: HeaderMap,
    guild: Id,
    body: Bytes,
    icon: bool,
) -> Result<Json<GuildSummary>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(
        store::base_permissions(&conn, guild, me),
        perm::MANAGE_GUILD,
    ) {
        return Err(forbidden());
    }
    if !icon {
        let level = store::boost_level(store::boost_count(&conn, guild));
        if !store::banner_unlocked(level) {
            return Err(ApiError(
                StatusCode::FORBIDDEN,
                "Server banners unlock at Boost Level 2 (7 Boosts).".into(),
            ));
        }
    }
    let limit = if icon { ICON_LIMIT } else { BANNER_LIMIT };
    if body.is_empty() {
        return Err(bad("Pick an image first."));
    }
    if body.len() > limit {
        return Err(bad(format!(
            "Image is larger than {} MB.",
            limit / (1024 * 1024)
        )));
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
    if width > 4096 || height > 4096 {
        return Err(bad("Images must be 4096 pixels or smaller on each side."));
    }
    if icon && width != height {
        return Err(bad("The server icon must be square."));
    }
    if !icon && width < height {
        return Err(bad("The banner should be landscape, ideally 16:9."));
    }
    let url = state.media.save_with_ext(&body, ext).map_err(internal)?.url;
    store::set_guild_image(&conn, guild, !icon, &url).map_err(internal)?;
    let summary = store::guild_summary_with_unread(&conn, guild, me)
        .map(|(summary, _)| summary)
        .ok_or_else(not_found)?;
    drop(conn);
    broadcast_guild(&state, &summary);
    Ok(Json(summary))
}

async fn clear_guild_image(
    state: Arc<AppState>,
    headers: HeaderMap,
    guild: Id,
    icon: bool,
) -> Result<Json<GuildSummary>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(
        store::base_permissions(&conn, guild, me),
        perm::MANAGE_GUILD,
    ) {
        return Err(forbidden());
    }
    store::clear_guild_image(&conn, guild, !icon).map_err(internal)?;
    let summary = store::guild_summary_with_unread(&conn, guild, me)
        .map(|(summary, _)| summary)
        .ok_or_else(not_found)?;
    drop(conn);
    broadcast_guild(&state, &summary);
    Ok(Json(summary))
}

fn broadcast_guild(state: &AppState, summary: &GuildSummary) {
    let _ = state.tx.send(GatewayEvent::GuildUpdate(Box::new(Guild {
        id: summary.id,
        name: summary.name.clone(),
        icon_color: summary.icon_color,
        owner_id: summary.owner_id,
        description: summary.description.clone(),
        member_count: summary.member_count,
        tag: summary.tag.clone(),
        permissions: summary.permissions,
        icon_url: summary.icon_url.clone(),
        banner_url: summary.banner_url.clone(),
        created_at: summary.created_at,
        ..Guild::default()
    })));
}
