//! messages.

use super::*;
use rusqlite::{Connection, params};

pub fn list_messages_window(
    conn: &Connection,
    channel: Id,
    me: Id,
    before: Option<Id>,
    after: Option<Id>,
    around: Option<Id>,
    limit: u32,
) -> Vec<Message> {
    let limit = limit.clamp(1, 100);
    let mut ids: Vec<Id> = Vec::new();
    if let Some(after) = after {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id FROM messages WHERE channel_id=?1 AND id>?2 ORDER BY id ASC LIMIT ?3",
        ) {
            if let Ok(rows) = stmt.query_map(params![channel, after, limit], |r| r.get::<_, Id>(0))
            {
                ids.extend(rows.flatten());
            }
        }
    } else if let Some(around) = around {
        let half = limit / 2 + 1;
        let mut older: Vec<Id> = Vec::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id FROM messages WHERE channel_id=?1 AND id<=?2 ORDER BY id DESC LIMIT ?3",
        ) {
            if let Ok(rows) = stmt.query_map(params![channel, around, half], |r| r.get::<_, Id>(0))
            {
                older.extend(rows.flatten());
            }
        }
        older.reverse();
        let mut newer: Vec<Id> = Vec::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id FROM messages WHERE channel_id=?1 AND id>?2 ORDER BY id ASC LIMIT ?3",
        ) {
            if let Ok(rows) =
                stmt.query_map(params![channel, around, limit / 2], |r| r.get::<_, Id>(0))
            {
                newer.extend(rows.flatten());
            }
        }
        ids.extend(older);
        ids.extend(newer);
    } else {
        let before = before.unwrap_or(i64::MAX);
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id FROM messages WHERE channel_id=?1 AND id<?2 ORDER BY id DESC LIMIT ?3",
        ) {
            if let Ok(rows) = stmt.query_map(params![channel, before, limit], |r| r.get::<_, Id>(0))
            {
                ids.extend(rows.flatten());
            }
        }
        ids.reverse();
    }
    ids.into_iter()
        .filter_map(|id| message_by_id(conn, id, me))
        .collect()
}

pub fn list_threads(conn: &Connection, parent: Id, me: Id) -> Vec<Channel> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT * FROM channels WHERE thread_parent_id=?1 AND kind='thread' ORDER BY id DESC",
    ) {
        if let Ok(rows) = stmt.query_map(params![parent], channel_from_row) {
            out.extend(rows.flatten().filter(|c| can_view(conn, c.id, me)));
        }
    }
    out
}
