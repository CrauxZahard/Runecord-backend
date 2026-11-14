//! media.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use std::sync::Arc;

pub(crate) async fn upload(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel): Path<Id>,
    Query(q): Query<UploadQuery>,
    body: axum::body::Bytes,
) -> Result<Json<UploadResponse>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let limit = {
        let conn = state.store.conn();
        if !store::can_send(&conn, channel, me) {
            return Err(forbidden());
        }
        let level = store::channel_by_id(&conn, channel)
            .and_then(|target| target.guild_id)
            .map(|guild| store::boost_level(store::boost_count(&conn, guild)))
            .unwrap_or(0);
        store::upload_limit_bytes(level)
    };
    if body.len() as u64 > limit {
        let mb = limit / (1024 * 1024);
        return Err(bad(format!(
            "File is larger than {mb} MB. Boost the server to raise the limit."
        )));
    }
    let clean: String = q
        .name
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect();
    let filename = if clean.trim().is_empty() {
        "file".to_string()
    } else {
        clean
    };
    let ext = filename.rsplit('.').next().unwrap_or("bin").to_lowercase();
    let asset = state.media.save_with_ext(&body, &ext).map_err(internal)?;
    let url = asset.url.clone();
    let hash = asset.hash.clone();
    let conn = state.store.conn();
    store::record_cdn(&conn, &hash, &filename, &q.content_type, body.len() as u64);
    let attachment = store::insert_attachment(
        &conn,
        me,
        &filename,
        body.len() as u64,
        &q.content_type,
        &url,
        q.width,
        q.height,
    )
    .ok_or_else(|| internal("could not store attachment"))?;
    Ok(Json(UploadResponse { attachment }))
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub(crate) struct CdnQuery {
    #[serde(default)]
    w: Option<u32>,
}

pub(crate) async fn cdn_file(
    State(state): State<Arc<AppState>>,
    Path(file): Path<String>,
    Query(query): Query<CdnQuery>,
) -> axum::response::Response {
    use axum::body::Bytes;
    use axum::response::IntoResponse;
    let hash = file.split('.').next().unwrap_or("").to_string();
    let content_type = {
        let conn = state.store.conn();
        store::cdn_content_type(&conn, &hash)
    };
    let Some(bytes) = state.media.read(&file) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    if let Some(width) = query.w.filter(|w| (64..=2048).contains(w))
        && content_type.starts_with("image/")
    {
        let key = (hash.clone(), width);
        if let Some(hit) = state
            .thumb_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
        {
            return image_response(hit);
        }
        if let Some(thumb) = state.media.read_thumb(&hash, width) {
            let thumb = Bytes::from(thumb);
            state
                .thumb_cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .put(key, thumb.clone());
            return image_response(thumb);
        }
        if let Ok(image) = image::load_from_memory(&bytes) {
            let (w, h) = (image.width(), image.height());
            if w > width {
                let target_h = ((h as f32) * (width as f32 / w as f32)).round().max(1.0) as u32;
                let thumb = image::imageops::thumbnail(&image, width, target_h);
                let mut out = Vec::new();
                if thumb
                    .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
                    .is_ok()
                {
                    let _ = state.media.write_thumb(&hash, width, &out);
                    let thumb = Bytes::from(out);
                    state
                        .thumb_cache
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .put(key, thumb.clone());
                    return image_response(thumb);
                }
            }
        }
    }

    (
        [
            ("content-type", content_type),
            (
                "cache-control",
                "public, max-age=31536000, immutable".to_string(),
            ),
        ],
        bytes,
    )
        .into_response()
}

fn image_response(bytes: axum::body::Bytes) -> axum::response::Response {
    use axum::response::IntoResponse;
    (
        [
            ("content-type", "image/png".to_string()),
            (
                "cache-control",
                "public, max-age=31536000, immutable".to_string(),
            ),
        ],
        bytes,
    )
        .into_response()
}
