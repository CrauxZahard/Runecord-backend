//! Media metadata and attachment storage.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};

#[allow(clippy::too_many_arguments)] // plain SQL parameter mapping
pub fn insert_attachment(
    conn: &Connection,
    uploader: Id,
    filename: &str,
    size: u64,
    content_type: &str,
    url: &str,
    width: u32,
    height: u32,
) -> Option<Attachment> {
    let id = next_id(conn);
    conn.execute(
        "INSERT INTO attachments(id, uploader_id, filename, size, content_type, url, width, height)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            id,
            uploader,
            filename,
            size as i64,
            content_type,
            url,
            width as i64,
            height as i64
        ],
    )
    .ok()?;
    Some(Attachment {
        id,
        filename: filename.to_string(),
        size,
        content_type: content_type.to_string(),
        url: url.to_string(),
        width,
        height,
    })
}

pub fn cdn_content_type(conn: &Connection, hash: &str) -> String {
    conn.query_row(
        "SELECT content_type FROM cdn_assets WHERE hash=?1",
        params![hash],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
    .unwrap_or_else(|| "application/octet-stream".to_string())
}
