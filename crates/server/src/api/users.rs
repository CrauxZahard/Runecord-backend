//! users.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

pub(crate) async fn bootstrap(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Bootstrap>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let mut out = state.store.bootstrap(me);
    out.voice_states = state
        .voice
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .values()
        .cloned()
        .collect();
    Ok(Json(out))
}

pub(crate) async fn get_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(user): Path<Id>,
) -> Result<Json<User>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !store::users_related(&conn, me, user) {
        return Err(forbidden());
    }
    store::user_by_id(&conn, user)
        .map(Json)
        .ok_or_else(not_found)
}

#[derive(Deserialize)]
pub(crate) struct LookupQuery {
    q: String,
}

pub(crate) async fn lookup_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<LookupQuery>,
) -> Result<Json<Vec<User>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let needle = query.q.trim().to_lowercase();
    if needle.len() < 2 {
        return Ok(Json(Vec::new()));
    }
    let conn = state.store.conn();
    let users = store::lookup_ids(&conn, me, &needle)
        .into_iter()
        .filter_map(|id| store::user_by_id(&conn, id))
        .collect();
    Ok(Json(users))
}

#[derive(Deserialize)]
pub(crate) struct UserIds {
    #[serde(default)]
    ids: Vec<Id>,
}

pub(crate) async fn get_users_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<UserIds>,
) -> Result<Json<Vec<User>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let mut out = Vec::new();
    for user in body.ids.into_iter().take(100) {
        if store::users_related(&conn, me, user)
            && let Some(user) = store::user_by_id(&conn, user)
        {
            out.push(user);
        }
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
pub(crate) struct MeBody {
    display_name: Option<String>,
    #[serde(default)]
    badges: Option<Vec<String>>,
    #[serde(default)]
    avatar_decoration: Option<String>,
    avatar_color: Option<u32>,
    avatar_border: Option<u32>,
    banner_color: Option<u32>,
    bio: Option<String>,
    custom_status: Option<String>,
    pronouns: Option<String>,
    #[serde(default)]
    activity: Option<ProfileActivity>,
    #[serde(default)]
    clear_activity: bool,
    #[serde(default)]
    connections: Option<Vec<ProfileConnection>>,
    #[serde(default)]
    profile_effect: Option<String>,
    #[serde(default)]
    nameplate: Option<String>,
    #[serde(default)]
    profile_widget: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct PasswordBody {
    current: String,
    new: String,
}

pub(crate) async fn update_privacy(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<UpdatePrivacy>,
) -> Result<Json<User>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    store::set_privacy(&conn, me, body.dm_privacy, body.friend_privacy);
    let user = store::user_by_id(&conn, me).ok_or_else(not_found)?;
    Ok(Json(user))
}

pub(crate) async fn change_password(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<PasswordBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let hash = store::password_hash(&conn, me).ok_or_else(not_found)?;
    if !verify_password(&hash, &body.current) {
        return Err(bad("Current password is incorrect."));
    }
    if body.new.chars().count() < 6 {
        return Err(bad("New password must be at least 6 characters."));
    }
    store::set_password_hash(&conn, me, &hash_password(&body.new)).map_err(internal)?;
    if let Some(token) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        store::delete_other_sessions(&conn, me, &store::hash_bot_token(token));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn mutual_guilds(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(user): Path<Id>,
) -> Result<Json<Vec<GuildSummary>>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let guilds = store::shared_guilds(&conn, me, user)
        .into_iter()
        .filter_map(|guild| store::guild_summary(&conn, guild, me))
        .collect();
    Ok(Json(guilds))
}

pub(crate) async fn update_me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<MeBody>,
) -> Result<Json<User>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if let Some(name) = body.display_name.filter(|n| !n.trim().is_empty()) {
        store::set_display_name(&conn, me, name.trim()).map_err(internal)?;
    }
    if let Some(color) = body.avatar_color {
        store::set_avatar_color(&conn, me, color).map_err(internal)?;
    }
    if let Some(border) = body.avatar_border {
        store::set_avatar_border(&conn, me, border).map_err(internal)?;
    }
    if let Some(badges) = body.badges {
        let clean = badges
            .into_iter()
            .map(|badge| badge.trim().to_string())
            .filter(|badge| !badge.is_empty() && badge.len() <= 32)
            .take(8)
            .collect::<Vec<_>>()
            .join(",");
        store::set_badges(&conn, me, &clean).map_err(internal)?;
    }
    if let Some(decoration) = body.avatar_decoration {
        store::set_avatar_decoration(
            &conn,
            me,
            &decoration.trim().chars().take(32).collect::<String>(),
        )
        .map_err(internal)?;
    }
    if let Some(pronouns) = body.pronouns {
        store::set_pronouns(
            &conn,
            me,
            &pronouns.trim().chars().take(40).collect::<String>(),
        )
        .map_err(internal)?;
    }
    if let Some(banner) = body.banner_color {
        store::set_banner_color(&conn, me, banner).map_err(internal)?;
    }
    if let Some(bio) = body.bio {
        store::set_bio(&conn, me, &bio).map_err(internal)?;
    }
    if let Some(status) = body.custom_status {
        store::set_custom_status(&conn, me, &status).map_err(internal)?;
    }
    if body.clear_activity {
        store::clear_activity(&conn, me).map_err(internal)?;
    } else if let Some(activity) = body.activity {
        let kind: String = activity.kind.trim().chars().take(16).collect();
        if !kind.is_empty() {
            store::set_activity(
                &conn,
                me,
                &kind,
                &activity.title.trim().chars().take(128).collect::<String>(),
                &activity
                    .subtitle
                    .trim()
                    .chars()
                    .take(128)
                    .collect::<String>(),
                &activity.state.trim().chars().take(128).collect::<String>(),
                &activity.url.trim().chars().take(500).collect::<String>(),
                activity.started_at.max(0),
            )
            .map_err(internal)?;
        }
    }
    if let Some(effect) = body.profile_effect {
        store::set_profile_effect(
            &conn,
            me,
            &effect.trim().chars().take(32).collect::<String>(),
        )
        .map_err(internal)?;
    }
    if let Some(nameplate) = body.nameplate {
        store::set_nameplate(
            &conn,
            me,
            &nameplate.trim().chars().take(32).collect::<String>(),
        )
        .map_err(internal)?;
    }
    if let Some(widget) = body.profile_widget {
        let widget: String = widget.trim().to_lowercase().chars().take(32).collect();
        let widget = if matches!(widget.as_str(), "stats" | "activity") {
            widget
        } else {
            "none".into()
        };
        store::set_profile_widget(&conn, me, &widget).map_err(internal)?;
    }
    if let Some(connections) = body.connections {
        let clean: Vec<ProfileConnection> = connections
            .into_iter()
            .filter_map(|connection| {
                let kind: String = connection
                    .kind
                    .trim()
                    .to_lowercase()
                    .chars()
                    .take(16)
                    .collect();
                let name: String = connection.name.trim().chars().take(64).collect();
                (!kind.is_empty() && !name.is_empty()).then_some(ProfileConnection { kind, name })
            })
            .take(6)
            .collect();
        let encoded = serde_json::to_string(&clean).unwrap_or_else(|_| "[]".into());
        store::set_connections(&conn, me, &encoded).map_err(internal)?;
    }
    let user = store::user_by_id(&conn, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::UserUpdate(Box::new(user.clone())));
    Ok(Json(user))
}

pub(crate) async fn update_read(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ReadUpdate>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    store::update_read(&conn, me, body.channel_id, Some(body.last_read_id)).map_err(internal)?;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Guilds and channels
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Profile images
// ---------------------------------------------------------------------------

const AVATAR_LIMIT: usize = 4 * 1024 * 1024;
const USER_BANNER_LIMIT: usize = 10 * 1024 * 1024;

const ACTIVITY_ARTWORK_LIMIT: usize = 4 * 1024 * 1024;

pub(crate) async fn upload_activity_artwork(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<User>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let kind = store::activity_kind(&conn, me);
    if kind.is_empty() {
        return Err(bad("Set an activity before uploading artwork."));
    }
    if body.is_empty() {
        return Err(bad("Pick an image first."));
    }
    if body.len() > ACTIVITY_ARTWORK_LIMIT {
        return Err(bad("Activity artwork must be 4 MB or smaller."));
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
        return Err(bad("Activity artwork must be square."));
    }
    if width > 4096 {
        return Err(bad("Activity artwork must be 4096 pixels or smaller."));
    }
    let url = state.media.save_with_ext(&body, ext).map_err(internal)?.url;
    store::set_activity_artwork(&conn, me, &url).map_err(internal)?;
    let user = store::user_by_id(&conn, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::UserUpdate(Box::new(user.clone())));
    Ok(Json(user))
}

pub(crate) async fn remove_activity_artwork(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<User>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    store::clear_activity_artwork(&conn, me).map_err(internal)?;
    let user = store::user_by_id(&conn, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::UserUpdate(Box::new(user.clone())));
    Ok(Json(user))
}

pub(crate) async fn upload_my_avatar(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<User>, ApiError> {
    set_my_image(state, headers, body, false).await
}

pub(crate) async fn upload_my_banner(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<User>, ApiError> {
    set_my_image(state, headers, body, true).await
}

pub(crate) async fn remove_my_avatar(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<User>, ApiError> {
    clear_my_image(state, headers, false).await
}

pub(crate) async fn remove_my_banner(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<User>, ApiError> {
    clear_my_image(state, headers, true).await
}

async fn set_my_image(
    state: Arc<AppState>,
    headers: HeaderMap,
    body: Bytes,
    banner: bool,
) -> Result<Json<User>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let limit = if banner {
        USER_BANNER_LIMIT
    } else {
        AVATAR_LIMIT
    };
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
    if !banner && width != height {
        return Err(bad("Your avatar must be square."));
    }
    if banner && width < height {
        return Err(bad("Your banner should be landscape, ideally 16:9."));
    }
    let url = state.media.save_with_ext(&body, ext).map_err(internal)?.url;
    let conn = state.store.conn();
    store::set_user_image(&conn, me, banner, &url).map_err(internal)?;
    let user = store::user_by_id(&conn, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::UserUpdate(Box::new(user.clone())));
    Ok(Json(user))
}

async fn clear_my_image(
    state: Arc<AppState>,
    headers: HeaderMap,
    banner: bool,
) -> Result<Json<User>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    store::clear_user_image(&conn, me, banner).map_err(internal)?;
    let user = store::user_by_id(&conn, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::UserUpdate(Box::new(user.clone())));
    Ok(Json(user))
}
