//! Group DM membership and settings.

use super::*;
use rusqlite::params;

pub fn dm_owner(conn: &Connection, channel: Id) -> Option<Id> {
    conn.query_row(
        "SELECT COALESCE(owner_id,0) FROM channels WHERE id=?1 AND kind='group_dm'",
        params![channel],
        |r| r.get::<_, Id>(0),
    )
    .optional()
    .ok()
    .flatten()
    .filter(|owner| *owner != 0)
}

pub fn dm_members(conn: &Connection, channel: Id) -> Vec<Id> {
    let mut out = Vec::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT user_id FROM dm_members WHERE channel_id=?1 ORDER BY rowid")
        && let Ok(rows) = stmt.query_map(params![channel], |r| r.get::<_, Id>(0))
    {
        out.extend(rows.flatten());
    }
    out
}

pub fn dm_rename(conn: &Connection, channel: Id, name: &str) -> bool {
    conn.execute(
        "UPDATE channels SET name=?1 WHERE id=?2 AND kind='group_dm'",
        params![name, channel],
    )
    .map(|changed| changed > 0)
    .unwrap_or(false)
}

pub fn dm_set_icon(conn: &Connection, channel: Id, url: &str) -> bool {
    conn.execute(
        "UPDATE channels SET icon_url=?1 WHERE id=?2 AND kind='group_dm'",
        params![url, channel],
    )
    .map(|changed| changed > 0)
    .unwrap_or(false)
}

pub fn dm_add_member(conn: &Connection, channel: Id, user: Id) -> bool {
    conn.execute(
        "INSERT OR IGNORE INTO dm_members(channel_id, user_id) VALUES(?1,?2)",
        params![channel, user],
    )
    .map(|changed| changed > 0)
    .unwrap_or(false)
}

pub fn dm_remove_member(conn: &Connection, channel: Id, user: Id) -> bool {
    conn.execute(
        "DELETE FROM dm_members WHERE channel_id=?1 AND user_id=?2",
        params![channel, user],
    )
    .map(|changed| changed > 0)
    .unwrap_or(false)
}

pub fn dm_transfer_owner(conn: &Connection, channel: Id, to: Id) -> bool {
    conn.execute(
        "UPDATE channels SET owner_id=?1 WHERE id=?2 AND kind='group_dm'",
        params![to, channel],
    )
    .map(|changed| changed > 0)
    .unwrap_or(false)
}
