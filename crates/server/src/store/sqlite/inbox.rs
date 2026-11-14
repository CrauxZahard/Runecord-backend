//! Inbox queries.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};

pub type MentionRow = (Id, Id, Id, String, i64, String);

pub fn mention_rows(conn: &Connection, me: Id) -> Vec<MentionRow> {
    let mut rows = Vec::new();
    if let Ok(mut statement) = conn.prepare(
        "SELECT m.id, m.channel_id, m.author_id, m.content, m.timestamp, u.display_name
         FROM messages m
         JOIN users u ON u.id=m.author_id
         LEFT JOIN reads r ON r.channel_id=m.channel_id AND r.user_id=?2
         WHERE m.author_id<>?2 AND m.id>COALESCE(r.last_read_id,0)
         ORDER BY m.timestamp DESC LIMIT 200",
    ) && let Ok(iter) = statement.query_map(params![me, me], |row| {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
        ))
    }) {
        rows.extend(iter.flatten());
    }
    rows
}

pub fn guild_ids_for_user(conn: &Connection, user: Id) -> Vec<Id> {
    let mut ids = Vec::new();
    if let Ok(mut statement) =
        conn.prepare("SELECT guild_id FROM guild_members WHERE user_id=?1 ORDER BY guild_id")
        && let Ok(rows) = statement.query_map(params![user], |row| row.get::<_, Id>(0))
    {
        ids.extend(rows.flatten());
    }
    ids
}

pub fn dm_channel_ids(conn: &Connection, user: Id) -> Vec<Id> {
    let mut ids = Vec::new();
    if let Ok(mut statement) = conn.prepare("SELECT channel_id FROM dm_members WHERE user_id=?1")
        && let Ok(rows) = statement.query_map(params![user], |row| row.get::<_, Id>(0))
    {
        ids.extend(rows.flatten());
    }
    ids
}

pub fn latest_message(
    conn: &Connection,
    channel: Id,
    me: Id,
) -> Option<(Id, Id, String, String, i64)> {
    conn.query_row(
        "SELECT m.id, m.author_id, u.display_name, m.content, m.timestamp
         FROM messages m JOIN users u ON u.id=m.author_id
         WHERE m.channel_id=?1 AND m.author_id<>?2
         ORDER BY m.id DESC LIMIT 1",
        params![channel, me],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        },
    )
    .optional()
    .ok()
    .flatten()
}

pub fn dm_recipient_names(conn: &Connection, channel: Id, me: Id) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(mut statement) = conn.prepare(
        "SELECT u.display_name FROM users u JOIN dm_members d ON d.user_id=u.id
         WHERE d.channel_id=?1 AND u.id<>?2 ORDER BY d.rowid",
    ) && let Ok(rows) = statement.query_map(params![channel, me], |row| row.get::<_, String>(0))
    {
        names.extend(rows.flatten());
    }
    names
}
