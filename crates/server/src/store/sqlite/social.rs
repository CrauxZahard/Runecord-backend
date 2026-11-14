//! Relationships and direct-message storage.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};

pub fn set_relationship(conn: &Connection, user: Id, other: Id, kind: RelationshipKind) -> bool {
    let kind = match kind {
        RelationshipKind::Friend => "friend",
        RelationshipKind::Incoming => "incoming",
        RelationshipKind::Outgoing => "outgoing",
        RelationshipKind::Blocked => "blocked",
    };
    conn.execute(
        "INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since)
         VALUES(?1,?2,?3,?4)",
        params![user, other, kind, now_ms()],
    )
    .is_ok()
}

pub fn remove_relationship(conn: &Connection, a: Id, b: Id) {
    conn.execute(
        "DELETE FROM relationships
         WHERE (user_id=?1 AND other_id=?2) OR (user_id=?2 AND other_id=?1)",
        params![a, b],
    )
    .ok();
}

pub fn dm_payload(conn: &Connection, channel_id: Id) -> Option<DmChannel> {
    let channel = channel_by_id(conn, channel_id)?;
    let mut recipients = Vec::new();
    if let Ok(mut statement) = conn.prepare(
        "SELECT u.* FROM users u JOIN dm_members d ON d.user_id=u.id
         WHERE d.channel_id=?1 ORDER BY d.rowid",
    ) {
        if let Ok(rows) = statement.query_map(params![channel_id], user_from_row) {
            recipients.extend(rows.flatten());
        }
    }
    Some(DmChannel {
        channel,
        recipients,
    })
}

pub fn create_group_dm(conn: &Connection, me: Id, name: &str, users: &[Id]) -> Option<DmChannel> {
    let tx = conn.unchecked_transaction().ok()?;
    let channel_id = next_id(&tx);
    tx.execute(
        "INSERT INTO channels(id, guild_id, name, kind, topic, owner_id)
         VALUES(?1,NULL,?2,'group_dm','',?3)",
        params![channel_id, name, me],
    )
    .ok()?;
    tx.execute(
        "INSERT INTO dm_members(channel_id, user_id) VALUES(?1,?2)",
        params![channel_id, me],
    )
    .ok()?;
    for user in users {
        tx.execute(
            "INSERT INTO dm_members(channel_id, user_id) VALUES(?1,?2)",
            params![channel_id, user],
        )
        .ok()?;
    }
    tx.commit().ok()?;
    dm_payload(conn, channel_id)
}

pub fn find_direct_dm(conn: &Connection, me: Id, other: Id) -> Option<Id> {
    conn.query_row(
        "SELECT c.id FROM channels c
         JOIN dm_members a ON a.channel_id=c.id AND a.user_id=?1
         JOIN dm_members b ON b.channel_id=c.id AND b.user_id=?2
         WHERE c.kind='dm' LIMIT 1",
        params![me, other],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn create_direct_dm(conn: &Connection, me: Id, other: Id) -> Option<DmChannel> {
    let channel_id = next_id(conn);
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, topic) VALUES(?1,NULL,'','dm','')",
        params![channel_id],
    )
    .ok()?;
    conn.execute(
        "INSERT INTO dm_members(channel_id, user_id) VALUES(?1,?2)",
        params![channel_id, me],
    )
    .ok()?;
    conn.execute(
        "INSERT INTO dm_members(channel_id, user_id) VALUES(?1,?2)",
        params![channel_id, other],
    )
    .ok()?;
    dm_payload(conn, channel_id)
}

pub fn delete_dm(conn: &Connection, channel: Id) {
    conn.execute("DELETE FROM messages WHERE channel_id=?1", params![channel])
        .ok();
    conn.execute(
        "DELETE FROM dm_members WHERE channel_id=?1",
        params![channel],
    )
    .ok();
    conn.execute("DELETE FROM channels WHERE id=?1", params![channel])
        .ok();
}
