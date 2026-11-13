//! media.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use rusqlite::params;
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
        return Err(bad(&format!(
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
    let hash = store::hash_bytes(&body);
    let stored = format!("{hash}.{ext}");
    let dir = state.data_dir.join("cdn");
    std::fs::create_dir_all(&dir).map_err(internal)?;
    if !dir.join(&stored).exists() {
        let sealed = crate::crypto::seal(&state.store.key(), &body);
        std::fs::write(dir.join(&stored), sealed).map_err(internal)?;
    }
    let url = format!("/cdn/{stored}");
    let conn = state.store.conn();
    store::record_cdn(&conn, &hash, &filename, &q.content_type, body.len() as u64);
    let id = store::next_id(&conn);
    conn.execute(
        "INSERT INTO attachments(id, uploader_id, filename, size, content_type, url, width, height) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![id, me, filename, body.len() as i64, q.content_type, url, q.width as i64, q.height as i64],
    )
    .map_err(internal)?;
    let attachment = Attachment {
        id,
        filename,
        size: body.len() as u64,
        content_type: q.content_type,
        url,
        width: q.width,
        height: q.height,
    };
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
    let content_type: String = {
        let conn = state.store.conn();
        conn.query_row(
            "SELECT content_type FROM cdn_assets WHERE hash=?1",
            params![hash],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "application/octet-stream".to_string())
    };
    let path = state.data_dir.join("cdn").join(&file);
    let Ok(sealed) = std::fs::read(path) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    let Some(bytes) = crate::crypto::open(&state.store.key(), &sealed) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "could not decrypt").into_response();
    };

    if let Some(width) = query.w.filter(|w| (64..=2048).contains(w)) {
        if content_type.starts_with("image/") {
            let key = (hash.clone(), width);
            if let Some(hit) = state
                .thumb_cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&key)
            {
                return image_response(hit);
            }
            let thumb_path = state
                .data_dir
                .join("thumbs")
                .join(format!("{hash}-{width}.enc"));
            if let Ok(sealed) = std::fs::read(&thumb_path) {
                if let Some(thumb) = crate::crypto::open(&state.store.key(), &sealed) {
                    let thumb = Bytes::from(thumb);
                    state
                        .thumb_cache
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .put(key, thumb.clone());
                    return image_response(thumb);
                }
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
                        if let Some(dir) = thumb_path.parent() {
                            let _ = std::fs::create_dir_all(dir);
                        }
                        let _ = std::fs::write(
                            &thumb_path,
                            crate::crypto::seal(&state.store.key(), &out),
                        );
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
