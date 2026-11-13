//! bootstrap.

use super::*;
use rusqlite::{Connection, params};
use std::collections::{BTreeMap, HashMap};

pub fn bootstrap(conn: &Connection, me: Id) -> Bootstrap {
    let Some(user) = user_by_id(conn, me) else {
        return Bootstrap::default();
    };
    let mut users: HashMap<Id, User> = HashMap::new();
    users.insert(user.id, user.clone());

    let guild_ids: Vec<Id> = {
        let mut out = Vec::new();
        if let Ok(mut stmt) =
            conn.prepare("SELECT guild_id FROM guild_members WHERE user_id=?1 ORDER BY joined_at")
        {
            if let Ok(rows) = stmt.query_map(params![me], |r| r.get::<_, Id>(0)) {
                out.extend(rows.flatten());
            }
        }
        out
    };

    let mut unread = BTreeMap::new();
    let mut summaries = Vec::with_capacity(guild_ids.len());
    for guild in &guild_ids {
        if let Some((summary, map)) = guild_summary_with_unread(conn, *guild, me) {
            summaries.push(summary);
            unread.extend(map);
        }
    }
    let guilds = summaries;

    let mut dms = Vec::new();
    {
        let mut stmt = match conn.prepare(
            "SELECT c.* FROM channels c JOIN dm_members d ON d.channel_id=c.id WHERE d.user_id=?1 AND c.kind IN ('dm','group_dm') ORDER BY c.id",
        ) {
            Ok(s) => s,
            Err(_) => return Bootstrap { user, guilds, ..Bootstrap::default() },
        };
        if let Ok(rows) = stmt.query_map(params![me], channel_from_row) {
            for channel in rows.flatten() {
                let mut recipients = Vec::new();
                if let Ok(mut rs) = conn.prepare("SELECT u.* FROM users u JOIN dm_members d ON d.user_id=u.id WHERE d.channel_id=?1 AND u.id<>?2") {
                    if let Ok(us) = rs.query_map(params![channel.id, me], user_from_row) {
                        for u in us.flatten() {
                            users.insert(u.id, u.clone());
                            recipients.push(u);
                        }
                    }
                }
                dms.push(DmChannel {
                    channel,
                    recipients,
                });
            }
        }
    }

    let mut relationships = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT r.kind, r.since, u.* FROM relationships r JOIN users u ON u.id=r.other_id WHERE r.user_id=?1 ORDER BY r.kind") {
        if let Ok(rows) = stmt.query_map(params![me], |r| {
            let kind: String = r.get("kind")?;
            let kind = match kind.as_str() {
                "friend" => RelationshipKind::Friend,
                "incoming" => RelationshipKind::Incoming,
                "outgoing" => RelationshipKind::Outgoing,
                _ => RelationshipKind::Blocked,
            };
            Ok(Relationship { kind, user: user_from_row(r)?, since: r.get("since")? })
        }) {
            for rel in rows.flatten() {
                users.insert(rel.user.id, rel.user.clone());
                relationships.push(rel);
            }
        }
    }

    let mut reads = Vec::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT channel_id, last_read_id FROM reads WHERE user_id=?1")
    {
        if let Ok(rows) = stmt.query_map(params![me], |r| {
            Ok(ReadState {
                channel_id: r.get(0)?,
                last_read_id: r.get(1)?,
                mention_count: 0,
            })
        }) {
            reads.extend(rows.flatten());
        }
    }

    for dm in &dms {
        let n = unread_for(conn, dm.channel.id, me);
        if n > 0 {
            unread.insert(dm.channel.id, n);
        }
    }

    let presences = users.iter().map(|(id, u)| (*id, u.status)).collect();
    let emojis = guild_ids
        .iter()
        .flat_map(|g| guild_emojis(conn, *g))
        .collect();
    Bootstrap {
        user,
        guilds,
        dms,
        relationships,
        reads,
        users: users.into_values().collect(),
        presences,
        unread,
        emojis,
        voice_states: Vec::new(),
        channel_notifications: channel_notifications_for(conn, me),
        channel_suppress: channel_suppress_for(conn, me).into_keys().collect(),
    }
}

// ---------------------------------------------------------------------------
// Seed
// ---------------------------------------------------------------------------
