//! messages.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};

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
        ) && let Ok(rows) = stmt.query_map(params![channel, after, limit], |r| r.get::<_, Id>(0))
        {
            ids.extend(rows.flatten());
        }
    } else if let Some(around) = around {
        let half = limit / 2 + 1;
        let mut older: Vec<Id> = Vec::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id FROM messages WHERE channel_id=?1 AND id<=?2 ORDER BY id DESC LIMIT ?3",
        ) && let Ok(rows) = stmt.query_map(params![channel, around, half], |r| r.get::<_, Id>(0))
        {
            older.extend(rows.flatten());
        }
        older.reverse();
        let mut newer: Vec<Id> = Vec::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id FROM messages WHERE channel_id=?1 AND id>?2 ORDER BY id ASC LIMIT ?3",
        ) && let Ok(rows) =
            stmt.query_map(params![channel, around, limit / 2], |r| r.get::<_, Id>(0))
        {
            newer.extend(rows.flatten());
        }
        ids.extend(older);
        ids.extend(newer);
    } else {
        let before = before.unwrap_or(i64::MAX);
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id FROM messages WHERE channel_id=?1 AND id<?2 ORDER BY id DESC LIMIT ?3",
        ) && let Ok(rows) =
            stmt.query_map(params![channel, before, limit], |r| r.get::<_, Id>(0))
        {
            ids.extend(rows.flatten());
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
    ) && let Ok(rows) = stmt.query_map(params![parent], channel_from_row)
    {
        out.extend(rows.flatten().filter(|c| can_view(conn, c.id, me)));
    }
    for channel in &mut out {
        fill_thread_preview(conn, channel);
    }
    out
}

pub fn last_message_timestamp(conn: &Connection, channel: Id, author: Id) -> Option<i64> {
    conn.query_row(
        "SELECT MAX(timestamp) FROM messages WHERE channel_id=?1 AND author_id=?2",
        params![channel, author],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn message_content(conn: &Connection, message: Id) -> Option<String> {
    conn.query_row(
        "SELECT content FROM messages WHERE id=?1",
        params![message],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn message_location(conn: &Connection, message: Id) -> Option<(Id, Id)> {
    conn.query_row(
        "SELECT author_id, channel_id FROM messages WHERE id=?1",
        params![message],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn set_message_content(
    conn: &Connection,
    message: Id,
    content: &str,
    edited_at: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE messages SET content=?1, edited_at=?2 WHERE id=?3",
        params![content, edited_at, message],
    )?;
    Ok(())
}

pub fn delete_message(conn: &Connection, message: Id, channel: Id) -> Result<(), rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM messages WHERE id=?1", params![message])?;
    tx.execute(
        "DELETE FROM reactions WHERE message_id=?1",
        params![message],
    )?;
    tx.execute(
        "UPDATE channels SET thread_message_count=MAX(thread_message_count-1,0)
         WHERE id=?1 AND kind='thread'",
        params![channel],
    )?;
    tx.commit()
}

pub fn toggle_reaction(conn: &Connection, message: Id, me: Id, emoji: &str) -> Option<Message> {
    let existing: bool = conn
        .query_row(
            "SELECT 1 FROM reactions WHERE message_id=?1 AND emoji=?2 AND user_id=?3",
            params![message, emoji, me],
            |_| Ok(()),
        )
        .optional()
        .ok()
        .flatten()
        .is_some();

    if existing {
        conn.execute(
            "DELETE FROM reactions WHERE message_id=?1 AND emoji=?2 AND user_id=?3",
            params![message, emoji, me],
        )
        .ok()?;
    } else {
        conn.execute(
            "INSERT OR IGNORE INTO reactions(message_id, emoji, user_id) VALUES(?1,?2,?3)",
            params![message, emoji, me],
        )
        .ok()?;
    }

    message_by_id(conn, message, me)
}

pub fn create_bot_message(
    conn: &Connection,
    channel: Id,
    author: Id,
    content: &str,
) -> Option<Message> {
    let tx = conn.unchecked_transaction().ok()?;
    let message_id = next_id(&tx);
    tx.execute(
        "INSERT INTO messages(id, channel_id, author_id, content, timestamp)
         VALUES(?1,?2,?3,?4,?5)",
        params![message_id, channel, author, content, now_ms()],
    )
    .ok()?;
    tx.execute(
        "UPDATE channels SET last_message_id=?1 WHERE id=?2",
        params![message_id, channel],
    )
    .ok();
    let message = message_by_id(&tx, message_id, author);
    tx.commit().ok()?;
    message
}

pub fn set_reaction(conn: &Connection, message: Id, me: Id, emoji: &str, add: bool) -> bool {
    if add {
        conn.execute(
            "INSERT OR IGNORE INTO reactions(message_id, emoji, user_id) VALUES(?1,?2,?3)",
            params![message, emoji, me],
        )
        .is_ok()
    } else {
        conn.execute(
            "DELETE FROM reactions WHERE message_id=?1 AND emoji=?2 AND user_id=?3",
            params![message, emoji, me],
        )
        .is_ok()
    }
}

pub fn set_pin(conn: &Connection, message: Id, me: Id, pinned: bool) -> Option<Message> {
    conn.execute(
        "UPDATE messages SET pinned=?1 WHERE id=?2",
        params![pinned as i64, message],
    )
    .ok()?;
    message_by_id(conn, message, me)
}

pub fn toggle_pin(conn: &Connection, message: Id, me: Id) -> Option<Message> {
    let pinned: bool = conn
        .query_row(
            "SELECT pinned FROM messages WHERE id=?1",
            params![message],
            |row| Ok(row.get::<_, i64>(0)? != 0),
        )
        .optional()
        .ok()
        .flatten()?;

    conn.execute(
        "UPDATE messages SET pinned=?1 WHERE id=?2",
        params![(!pinned) as i64, message],
    )
    .ok()?;

    message_by_id(conn, message, me)
}

pub fn pinned_ids(conn: &Connection, channel: Id) -> Vec<Id> {
    let mut ids = Vec::new();
    if let Ok(mut statement) =
        conn.prepare("SELECT id FROM messages WHERE channel_id=?1 AND pinned=1 ORDER BY id DESC")
        && let Ok(rows) = statement.query_map(params![channel], |row| row.get::<_, Id>(0))
    {
        ids.extend(rows.flatten());
    }
    ids
}

/// Inserts a message and its side effects in one transaction.
///
/// `forum_title` is `Some` when the target is a forum channel; the store then
/// creates the thread and posts into it. Permission and slowmode checks remain
/// the caller's responsibility.
pub fn create_message(
    conn: &Connection,
    me: Id,
    target: &Channel,
    content: String,
    reply_to: Option<Id>,
    attachment_ids: &[Id],
    forum_title: Option<&str>,
) -> Result<(Message, Option<Channel>), rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;
    let mut destination = target.id;
    let mut new_thread = None;

    if let Some(title) = forum_title {
        let thread_id = next_id(&tx);
        tx.execute(
            "INSERT INTO channels(id, guild_id, name, kind, position, thread_parent_id, owner_id)
             VALUES(?1,?2,?3,'thread',0,?4,?5)",
            params![thread_id, target.guild_id, title, target.id, me],
        )?;
        destination = thread_id;
        new_thread = channel_by_id(&tx, thread_id);
    }

    let message_id = next_id(&tx);
    let kind = if reply_to.is_some() {
        "reply"
    } else {
        "default"
    };
    tx.execute(
        "INSERT INTO messages(id, channel_id, author_id, content, timestamp, kind, reply_to)
         VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            message_id,
            destination,
            me,
            content,
            now_ms(),
            kind,
            reply_to
        ],
    )?;

    for attachment in attachment_ids {
        tx.execute(
            "UPDATE attachments SET message_id=?1
             WHERE id=?2 AND uploader_id=?3 AND message_id IS NULL",
            params![message_id, attachment, me],
        )
        .ok();
    }

    tx.execute(
        "UPDATE channels
         SET last_message_id=?1,
             thread_message_count=thread_message_count + CASE WHEN kind='thread' THEN 1 ELSE 0 END
         WHERE id=?2",
        params![message_id, destination],
    )
    .ok();
    tx.execute(
        "INSERT INTO reads(user_id, channel_id, last_read_id) VALUES(?1,?2,?3)
         ON CONFLICT(user_id, channel_id) DO UPDATE SET last_read_id=excluded.last_read_id",
        params![me, destination, message_id],
    )
    .ok();

    let message = message_by_id(&tx, message_id, me);
    tx.commit()?;
    Ok((message.unwrap_or_default(), new_thread))
}

/// Creates a thread and optionally seeds it with the source message.
pub fn create_thread(
    conn: &Connection,
    me: Id,
    parent: &Channel,
    name: &str,
    source_message: Option<Id>,
) -> Result<Channel, rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;
    let thread_id = next_id(&tx);
    let source: Option<(Id, String, i64)> = source_message.and_then(|message| {
        tx.query_row(
            "SELECT author_id, content, timestamp FROM messages WHERE id=?1",
            params![message],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .ok()
        .flatten()
    });

    tx.execute(
        "INSERT INTO channels(id, guild_id, name, kind, position, thread_parent_id, owner_id, thread_message_count)
         VALUES(?1,?2,?3,'thread',0,?4,?5,?6)",
        params![
            thread_id,
            parent.guild_id,
            name,
            parent.id,
            me,
            i64::from(source.is_some())
        ],
    )?;

    if let Some((author, content, timestamp)) = source {
        let message_id = next_id(&tx);
        tx.execute(
            "INSERT INTO messages(id, channel_id, author_id, content, timestamp, kind)
             VALUES(?1,?2,?3,?4,?5,'default')",
            params![message_id, thread_id, author, content, timestamp],
        )
        .ok();
        tx.execute(
            "UPDATE channels SET last_message_id=?1 WHERE id=?2",
            params![message_id, thread_id],
        )
        .ok();
    }

    let thread = channel_by_id(&tx, thread_id).unwrap_or_default();
    tx.commit()?;
    Ok(thread)
}

pub fn search_message_ids(conn: &Connection, term: &str, limit: usize) -> Vec<(Id, Id)> {
    let mut rows = Vec::new();
    if let Ok(mut statement) = conn.prepare(
        "SELECT id, channel_id FROM messages
         WHERE (?1='' OR content LIKE ?1) ORDER BY id DESC LIMIT ?2",
    ) && let Ok(iter) =
        statement.query_map(params![term, limit], |row| Ok((row.get(0)?, row.get(1)?)))
    {
        rows.extend(iter.flatten());
    }
    rows
}

pub fn guild_name(conn: &Connection, guild: Id) -> Option<String> {
    conn.query_row(
        "SELECT name FROM guilds WHERE id=?1",
        params![guild],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}
