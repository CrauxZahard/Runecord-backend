//! Channel and thread storage.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};

pub fn next_channel_position(conn: &Connection, guild: Id) -> i32 {
    conn.query_row(
        "SELECT COALESCE(MAX(position),0)+1 FROM channels WHERE guild_id=?1",
        params![guild],
        |row| row.get(0),
    )
    .unwrap_or(1)
}

pub fn insert_channel(
    conn: &Connection,
    guild: Id,
    name: &str,
    kind: &str,
    position: i32,
    parent_id: Option<Id>,
    topic: &str,
    slowmode: u32,
    nsfw: bool,
) -> Option<Channel> {
    let id = next_id(conn);
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, position, parent_id, topic, slowmode, nsfw)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            id,
            guild,
            name,
            kind,
            position,
            parent_id,
            topic,
            slowmode as i64,
            nsfw as i64
        ],
    )
    .ok()?;
    channel_by_id(conn, id)
}

pub fn set_thread_archived(
    conn: &Connection,
    channel: Id,
    archived: bool,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE channels SET thread_archived=?1 WHERE id=?2",
        params![archived as i64, channel],
    )?;
    Ok(())
}

pub fn guild_id_for_channel(conn: &Connection, channel: Id) -> Option<Option<Id>> {
    conn.query_row(
        "SELECT guild_id FROM channels WHERE id=?1",
        params![channel],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn update_channel_fields(
    conn: &Connection,
    channel: Id,
    name: &str,
    topic: &str,
    slowmode: u32,
    nsfw: bool,
    status: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE channels SET name=?1, topic=?2, slowmode=?3, nsfw=?4, status=?5 WHERE id=?6",
        params![name, topic, slowmode as i64, nsfw as i64, status, channel],
    )?;
    Ok(())
}

pub fn delete_channel_cascade(conn: &Connection, channel: Id) -> Vec<Id> {
    let mut doomed = vec![channel];
    if let Ok(mut statement) = conn.prepare("SELECT id FROM channels WHERE thread_parent_id=?1") {
        if let Ok(rows) = statement.query_map(params![channel], |row| row.get::<_, Id>(0)) {
            doomed.extend(rows.flatten());
        }
    }

    for doomed_channel in &doomed {
        conn.execute(
            "DELETE FROM reactions WHERE message_id IN (SELECT id FROM messages WHERE channel_id=?1)",
            params![doomed_channel],
        )
        .ok();
        conn.execute(
            "DELETE FROM attachments WHERE message_id IN (SELECT id FROM messages WHERE channel_id=?1)",
            params![doomed_channel],
        )
        .ok();
        conn.execute(
            "DELETE FROM messages WHERE channel_id=?1",
            params![doomed_channel],
        )
        .ok();
        conn.execute(
            "DELETE FROM overwrites WHERE channel_id=?1",
            params![doomed_channel],
        )
        .ok();
        conn.execute(
            "DELETE FROM reads WHERE channel_id=?1",
            params![doomed_channel],
        )
        .ok();
        conn.execute("DELETE FROM channels WHERE id=?1", params![doomed_channel])
            .ok();
    }

    doomed
}
