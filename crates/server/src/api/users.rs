//! users.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use rusqlite::params;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

pub(crate) async fn bootstrap(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Bootstrap>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let mut out = crate::store::Store::bootstrap(&state.store, me);
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
    let like = format!("%{needle}%");
    let mut ids = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id FROM users WHERE id<>?1 AND (lower(username) LIKE ?2 OR lower(display_name) LIKE ?2) ORDER BY username LIMIT 10",
    ) {
        if let Ok(rows) = stmt.query_map(params![me, like], |r| r.get::<_, Id>(0)) {
            ids.extend(rows.flatten());
        }
    }
    let users = ids
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
        if store::users_related(&conn, me, user) {
            if let Some(user) = store::user_by_id(&conn, user) {
                out.push(user);
            }
        }
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
pub(crate) struct MeBody {
    display_name: Option<String>,
    avatar_color: Option<u32>,
    avatar_border: Option<u32>,
    banner_color: Option<u32>,
    bio: Option<String>,
    custom_status: Option<String>,
    pronouns: Option<String>,
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
    let hash: String = conn
        .query_row(
            "SELECT pass_hash FROM users WHERE id=?1",
            params![me],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?
        .ok_or_else(not_found)?;
    if !verify_password(&hash, &body.current) {
        return Err(bad("Current password is incorrect."));
    }
    if body.new.chars().count() < 6 {
        return Err(bad("New password must be at least 6 characters."));
    }
    conn.execute(
        "UPDATE users SET pass_hash=?1 WHERE id=?2",
        params![hash_password(&body.new), me],
    )
    .map_err(internal)?;
    if let Some(token) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        conn.execute(
            "DELETE FROM sessions WHERE user_id=?1 AND token<>?2",
            params![me, store::hash_bot_token(token)],
        )
        .ok();
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
        conn.execute(
            "UPDATE users SET display_name=?1 WHERE id=?2",
            params![name.trim(), me],
        )
        .map_err(internal)?;
    }
    if let Some(color) = body.avatar_color {
        conn.execute(
            "UPDATE users SET avatar_color=?1 WHERE id=?2",
            params![color as i64, me],
        )
        .map_err(internal)?;
    }
    if let Some(border) = body.avatar_border {
        conn.execute(
            "UPDATE users SET avatar_border=?1 WHERE id=?2",
            params![border as i64, me],
        )
        .map_err(internal)?;
    }
    if let Some(pronouns) = body.pronouns {
        conn.execute(
            "UPDATE users SET pronouns=?1 WHERE id=?2",
            params![pronouns.trim().chars().take(40).collect::<String>(), me],
        )
        .map_err(internal)?;
    }
    if let Some(banner) = body.banner_color {
        conn.execute(
            "UPDATE users SET banner_color=?1 WHERE id=?2",
            params![banner as i64, me],
        )
        .map_err(internal)?;
    }
    if let Some(bio) = body.bio {
        conn.execute("UPDATE users SET bio=?1 WHERE id=?2", params![bio, me])
            .map_err(internal)?;
    }
    if let Some(status) = body.custom_status {
        conn.execute(
            "UPDATE users SET custom_status=?1 WHERE id=?2",
            params![status, me],
        )
        .map_err(internal)?;
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
    conn.execute(
        "INSERT INTO reads(user_id, channel_id, last_read_id) VALUES(?1,?2,?3)
         ON CONFLICT(user_id, channel_id) DO UPDATE SET last_read_id=excluded.last_read_id",
        params![me, body.channel_id, body.last_read_id],
    )
    .map_err(internal)?;
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
        return Err(bad(&format!(
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
    let hash = store::hash_bytes(&body);
    let stored = format!("{hash}.{ext}");
    let dir = state.data_dir.join("cdn");
    std::fs::create_dir_all(&dir).map_err(internal)?;
    if !dir.join(&stored).exists() {
        let sealed = crate::crypto::seal(&state.store.key(), &body);
        std::fs::write(dir.join(&stored), sealed).map_err(internal)?;
    }
    let url = format!("/cdn/{stored}");
    let column = if banner { "banner_url" } else { "avatar_url" };
    let conn = state.store.conn();
    conn.execute(
        &format!("UPDATE users SET {column}=?1 WHERE id=?2"),
        params![url, me],
    )
    .map_err(internal)?;
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
    let column = if banner { "banner_url" } else { "avatar_url" };
    conn.execute(
        &format!("UPDATE users SET {column}='' WHERE id=?1"),
        params![me],
    )
    .map_err(internal)?;
    let user = store::user_by_id(&conn, me).ok_or_else(not_found)?;
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::UserUpdate(Box::new(user.clone())));
    Ok(Json(user))
}
