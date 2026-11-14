//! User profile, credentials and read-state storage.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};

pub fn mark_online(conn: &Connection, user: Id) {
    conn.execute(
        "UPDATE users SET status='online' WHERE id=?1",
        params![user],
    )
    .ok();
}

pub fn mark_online_if_offline(conn: &Connection, user: Id) {
    conn.execute(
        "UPDATE users SET status='online' WHERE id=?1 AND status='offline'",
        params![user],
    )
    .ok();
}

pub fn mark_offline(conn: &Connection, user: Id) {
    conn.execute(
        "UPDATE users SET status='offline' WHERE id=?1",
        params![user],
    )
    .ok();
}

pub fn lookup_ids(conn: &Connection, me: Id, needle: &str) -> Vec<Id> {
    let like = format!("%{needle}%");
    let mut ids = Vec::new();
    if let Ok(mut statement) = conn.prepare(
        "SELECT id FROM users
         WHERE id<>?1 AND (lower(username) LIKE ?2 OR lower(display_name) LIKE ?2)
         ORDER BY username LIMIT 10",
    ) && let Ok(rows) = statement.query_map(params![me, like], |row| row.get::<_, Id>(0))
    {
        ids.extend(rows.flatten());
    }
    ids
}

pub fn password_hash(conn: &Connection, user: Id) -> Option<String> {
    conn.query_row(
        "SELECT pass_hash FROM users WHERE id=?1",
        params![user],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn set_password_hash(conn: &Connection, user: Id, hash: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET pass_hash=?1 WHERE id=?2",
        params![hash, user],
    )?;
    Ok(())
}

pub fn delete_other_sessions(conn: &Connection, user: Id, current_hash: &str) {
    conn.execute(
        "DELETE FROM sessions WHERE user_id=?1 AND token<>?2",
        params![user, current_hash],
    )
    .ok();
}

pub fn set_display_name(conn: &Connection, user: Id, name: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET display_name=?1 WHERE id=?2",
        params![name, user],
    )?;
    Ok(())
}

pub fn set_avatar_color(conn: &Connection, user: Id, color: u32) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET avatar_color=?1 WHERE id=?2",
        params![color as i64, user],
    )?;
    Ok(())
}

pub fn set_avatar_border(conn: &Connection, user: Id, color: u32) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET avatar_border=?1 WHERE id=?2",
        params![color as i64, user],
    )?;
    Ok(())
}

pub fn set_badges(conn: &Connection, user: Id, badges: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET badges=?1 WHERE id=?2",
        params![badges, user],
    )?;
    Ok(())
}

pub fn set_avatar_decoration(
    conn: &Connection,
    user: Id,
    decoration: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET avatar_decoration=?1 WHERE id=?2",
        params![decoration, user],
    )?;
    Ok(())
}

pub fn set_pronouns(conn: &Connection, user: Id, pronouns: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET pronouns=?1 WHERE id=?2",
        params![pronouns, user],
    )?;
    Ok(())
}

pub fn set_banner_color(conn: &Connection, user: Id, color: u32) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET banner_color=?1 WHERE id=?2",
        params![color as i64, user],
    )?;
    Ok(())
}

pub fn set_bio(conn: &Connection, user: Id, bio: &str) -> Result<(), rusqlite::Error> {
    conn.execute("UPDATE users SET bio=?1 WHERE id=?2", params![bio, user])?;
    Ok(())
}

pub fn set_custom_status(conn: &Connection, user: Id, status: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET custom_status=?1 WHERE id=?2",
        params![status, user],
    )?;
    Ok(())
}

pub fn clear_activity(conn: &Connection, user: Id) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET activity_kind='', activity_title='', activity_subtitle='',
         activity_state='', activity_url='', activity_artwork_url='', activity_started_at=0
         WHERE id=?1",
        params![user],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)] // plain SQL parameter mapping
pub fn set_activity(
    conn: &Connection,
    user: Id,
    kind: &str,
    title: &str,
    subtitle: &str,
    state: &str,
    url: &str,
    started_at: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET activity_kind=?1, activity_title=?2, activity_subtitle=?3,
         activity_state=?4, activity_url=?5, activity_started_at=?6 WHERE id=?7",
        params![kind, title, subtitle, state, url, started_at, user],
    )?;
    Ok(())
}

pub fn set_profile_effect(
    conn: &Connection,
    user: Id,
    effect: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET profile_effect=?1 WHERE id=?2",
        params![effect, user],
    )?;
    Ok(())
}

pub fn set_nameplate(conn: &Connection, user: Id, nameplate: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET nameplate=?1 WHERE id=?2",
        params![nameplate, user],
    )?;
    Ok(())
}

pub fn set_profile_widget(
    conn: &Connection,
    user: Id,
    widget: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET profile_widget=?1 WHERE id=?2",
        params![widget, user],
    )?;
    Ok(())
}

pub fn set_connections(conn: &Connection, user: Id, encoded: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET connections=?1 WHERE id=?2",
        params![encoded, user],
    )?;
    Ok(())
}

pub fn update_read(
    conn: &Connection,
    user: Id,
    channel: Id,
    last_read: Option<Id>,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO reads(user_id, channel_id, last_read_id) VALUES(?1,?2,?3)
         ON CONFLICT(user_id, channel_id) DO UPDATE SET last_read_id=excluded.last_read_id",
        params![user, channel, last_read],
    )?;
    Ok(())
}

pub fn activity_kind(conn: &Connection, user: Id) -> String {
    conn.query_row(
        "SELECT activity_kind FROM users WHERE id=?1",
        params![user],
        |row| row.get(0),
    )
    .unwrap_or_default()
}

pub fn set_activity_artwork(conn: &Connection, user: Id, url: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET activity_artwork_url=?1 WHERE id=?2",
        params![url, user],
    )?;
    Ok(())
}

pub fn clear_activity_artwork(conn: &Connection, user: Id) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET activity_artwork_url='' WHERE id=?1",
        params![user],
    )?;
    Ok(())
}

pub fn set_user_image(
    conn: &Connection,
    user: Id,
    banner: bool,
    url: &str,
) -> Result<(), rusqlite::Error> {
    let column = if banner { "banner_url" } else { "avatar_url" };
    conn.execute(
        &format!("UPDATE users SET {column}=?1 WHERE id=?2"),
        params![url, user],
    )?;
    Ok(())
}

pub fn clear_user_image(conn: &Connection, user: Id, banner: bool) -> Result<(), rusqlite::Error> {
    let column = if banner { "banner_url" } else { "avatar_url" };
    conn.execute(
        &format!("UPDATE users SET {column}='' WHERE id=?1"),
        params![user],
    )?;
    Ok(())
}
