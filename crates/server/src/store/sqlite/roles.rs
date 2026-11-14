//! Role storage.

use super::*;
use rusqlite::{Connection, OptionalExtension, params};

pub fn role_in_guild(conn: &Connection, guild: Id, role: Id) -> Option<Role> {
    conn.query_row(
        "SELECT * FROM roles WHERE id=?1 AND guild_id=?2",
        params![role, guild],
        role_from_row,
    )
    .optional()
    .ok()
    .flatten()
}

pub fn role_links(conn: &Connection, guild: Id, role: Id) -> Vec<RoleLink> {
    let mut out = Vec::new();
    if let Ok(mut statement) = conn.prepare(
        "SELECT id, guild_id, role_id, label, url FROM role_links
         WHERE guild_id=?1 AND role_id=?2 ORDER BY id",
    ) {
        if let Ok(rows) = statement.query_map(params![guild, role], |row| {
            Ok(RoleLink {
                id: row.get(0)?,
                guild_id: row.get(1)?,
                role_id: row.get(2)?,
                label: row.get(3)?,
                url: row.get(4)?,
            })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn add_role_link(
    conn: &Connection,
    guild: Id,
    role: Id,
    label: &str,
    url: &str,
) -> Option<RoleLink> {
    let id = next_id(conn);
    conn.execute(
        "INSERT INTO role_links(id, guild_id, role_id, label, url, created_at)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, guild, role, label, url, now_ms()],
    )
    .ok()?;
    Some(RoleLink {
        id,
        guild_id: guild,
        role_id: role,
        label: label.to_string(),
        url: url.to_string(),
    })
}

pub fn remove_role_link(conn: &Connection, guild: Id, role: Id, link: Id) -> bool {
    conn.execute(
        "DELETE FROM role_links WHERE id=?1 AND guild_id=?2 AND role_id=?3",
        params![link, guild, role],
    )
    .unwrap_or(0)
        > 0
}

pub fn set_role_icon(
    conn: &Connection,
    guild: Id,
    role: Id,
    url: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE roles SET icon_url=?1 WHERE id=?2 AND guild_id=?3",
        params![url, role, guild],
    )?;
    Ok(())
}

pub fn clear_role_icon(conn: &Connection, guild: Id, role: Id) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE roles SET icon_url='' WHERE id=?1 AND guild_id=?2",
        params![role, guild],
    )?;
    Ok(())
}

pub fn next_role_position(conn: &Connection, guild: Id) -> i32 {
    conn.query_row(
        "SELECT COALESCE(MAX(position),0)+1 FROM roles WHERE guild_id=?1",
        params![guild],
        |row| row.get(0),
    )
    .unwrap_or(1)
}

pub fn insert_role(
    conn: &Connection,
    guild: Id,
    name: &str,
    color: u32,
    permissions: u64,
    position: i32,
) -> Option<Role> {
    let id = next_id(conn);
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable)
         VALUES(?1,?2,?3,?4,?5,?6,0,1)",
        params![id, guild, name, color as i64, permissions as i64, position],
    )
    .ok()?;
    role_from_id(conn, id)
}

pub fn role_from_id(conn: &Connection, role: Id) -> Option<Role> {
    conn.query_row(
        "SELECT * FROM roles WHERE id=?1",
        params![role],
        role_from_row,
    )
    .optional()
    .ok()
    .flatten()
}

pub fn role_name(conn: &Connection, guild: Id, role: Id) -> String {
    conn.query_row(
        "SELECT name FROM roles WHERE id=?1 AND guild_id=?2",
        params![role, guild],
        |row| row.get(0),
    )
    .unwrap_or_default()
}

pub fn update_role_fields(
    conn: &Connection,
    guild: Id,
    role: Id,
    name: Option<&str>,
    color: Option<u32>,
    permissions: Option<u64>,
    hoist: Option<bool>,
    mentionable: Option<bool>,
    position: Option<i32>,
) -> Result<(), rusqlite::Error> {
    if let Some(name) = name {
        conn.execute(
            "UPDATE roles SET name=?1 WHERE id=?2 AND guild_id=?3 AND name<>'@everyone'",
            params![name, role, guild],
        )?;
    }
    if let Some(color) = color {
        conn.execute(
            "UPDATE roles SET color=?1 WHERE id=?2 AND guild_id=?3",
            params![color as i64, role, guild],
        )?;
    }
    if let Some(permissions) = permissions {
        conn.execute(
            "UPDATE roles SET permissions=?1 WHERE id=?2 AND guild_id=?3",
            params![permissions as i64, role, guild],
        )?;
    }
    if let Some(hoist) = hoist {
        conn.execute(
            "UPDATE roles SET hoist=?1 WHERE id=?2 AND guild_id=?3",
            params![hoist as i64, role, guild],
        )?;
    }
    if let Some(mentionable) = mentionable {
        conn.execute(
            "UPDATE roles SET mentionable=?1 WHERE id=?2 AND guild_id=?3",
            params![mentionable as i64, role, guild],
        )?;
    }
    if let Some(position) = position {
        conn.execute(
            "UPDATE roles SET position=?1 WHERE id=?2 AND guild_id=?3 AND name<>'@everyone'",
            params![position, role, guild],
        )?;
    }
    Ok(())
}

pub fn delete_role_cascade(conn: &Connection, guild: Id, role: Id) {
    conn.execute(
        "DELETE FROM roles WHERE id=?1 AND guild_id=?2 AND name<>'@everyone'",
        params![role, guild],
    )
    .ok();
    conn.execute(
        "DELETE FROM member_roles WHERE guild_id=?1 AND role_id=?2",
        params![guild, role],
    )
    .ok();
    conn.execute(
        "DELETE FROM overwrites
         WHERE target_id=?1 AND target_kind='role'
           AND channel_id IN (SELECT id FROM channels WHERE guild_id=?2)",
        params![role, guild],
    )
    .ok();
}
