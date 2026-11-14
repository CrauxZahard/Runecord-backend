//! Scheduled guild events.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};

fn event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GuildEvent> {
    Ok(GuildEvent {
        id: row.get(0)?,
        guild_id: row.get(1)?,
        creator_id: row.get(2)?,
        name: row.get(3)?,
        description: row.get(4)?,
        start_at: row.get(5)?,
        channel_id: row.get(6)?,
        created_at: row.get(7)?,
    })
}

pub fn list_events(conn: &Connection, guild: Id) -> Vec<GuildEvent> {
    let mut out = Vec::new();
    if let Ok(mut statement) = conn.prepare(
        "SELECT id, guild_id, creator_id, name, description, start_at, channel_id, created_at
         FROM guild_events WHERE guild_id=?1 ORDER BY start_at",
    ) {
        if let Ok(rows) = statement.query_map(params![guild], event_from_row) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn create_event(
    conn: &Connection,
    guild: Id,
    creator: Id,
    name: &str,
    description: &str,
    start_at: i64,
    channel: Option<Id>,
) -> Option<GuildEvent> {
    let id = next_id(conn);
    let created_at = now_ms();
    conn.execute(
        "INSERT INTO guild_events(id, guild_id, creator_id, name, description, start_at, channel_id, created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![id, guild, creator, name, description, start_at, channel, created_at],
    )
    .ok()?;
    event_by_id(conn, id)
}

pub fn event_by_id(conn: &Connection, event: Id) -> Option<GuildEvent> {
    conn.query_row(
        "SELECT id, guild_id, creator_id, name, description, start_at, channel_id, created_at
         FROM guild_events WHERE id=?1",
        params![event],
        event_from_row,
    )
    .optional()
    .ok()
    .flatten()
}

pub fn event_creator(conn: &Connection, guild: Id, event: Id) -> Option<Id> {
    conn.query_row(
        "SELECT creator_id FROM guild_events WHERE id=?1 AND guild_id=?2",
        params![event, guild],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn delete_event(conn: &Connection, guild: Id, event: Id) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM guild_events WHERE id=?1 AND guild_id=?2",
        params![event, guild],
    )?;
    Ok(())
}
