//! SQLite adapter: queries, permissions and the demo seed.

use runecord_protocol::*;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::collections::{BTreeMap, HashMap, HashSet};

const SCHEMA: &str = include_str!("../../../../migrations/0001_init.sql");

const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", SCHEMA),
    ("0002_bots", include_str!("../../../../migrations/0002_bots.sql")),
];

/// Applies numbered migrations once each and returns the connection.
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version TEXT PRIMARY KEY,
            applied_at INTEGER NOT NULL
        );",
    )?;
    for (version, sql) in MIGRATIONS {
        let applied: bool = conn
            .query_row("SELECT 1 FROM schema_migrations WHERE version=?1", params![version], |_| Ok(()))
            .optional()?
            .is_some();
        if applied {
            continue;
        }
        conn.execute_batch(sql)?;
        conn.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES(?1,?2)",
            params![version, now_ms()],
        )?;
    }
    Ok(())
}

pub fn open(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn next_id(conn: &Connection) -> i64 {
    conn.query_row(
        "INSERT INTO meta(k, v) VALUES('id', 1000)
         ON CONFLICT(k) DO UPDATE SET v = v + 1
         RETURNING v",
        [],
        |r| r.get(0),
    )
    .unwrap_or(1000)
}

// ---------------------------------------------------------------------------
// Mappers
// ---------------------------------------------------------------------------

pub fn presence_from(s: &str) -> Presence {
    match s {
        "online" => Presence::Online,
        "idle" => Presence::Idle,
        "dnd" => Presence::Dnd,
        _ => Presence::Offline,
    }
}

pub fn presence_str(p: Presence) -> &'static str {
    match p {
        Presence::Online => "online",
        Presence::Idle => "idle",
        Presence::Dnd => "dnd",
        Presence::Offline => "offline",
    }
}

pub fn user_from_row(r: &Row<'_>) -> rusqlite::Result<User> {
    Ok(User {
        id: r.get("id")?,
        username: r.get("username")?,
        display_name: r.get("display_name")?,
        avatar_color: r.get::<_, i64>("avatar_color")? as u32,
        bio: r.get("bio")?,
        custom_status: r.get("custom_status")?,
        status: presence_from(&r.get::<_, String>("status")?),
        is_bot: r.get::<_, i64>("is_bot").unwrap_or(0) != 0,
        created_at: r.get("created_at")?,
    })
}

pub fn user_by_id(conn: &Connection, id: Id) -> Option<User> {
    conn.query_row("SELECT * FROM users WHERE id=?1", params![id], user_from_row).optional().ok().flatten()
}

pub fn user_by_name(conn: &Connection, name: &str) -> Option<User> {
    conn.query_row("SELECT * FROM users WHERE username=?1", params![name], user_from_row).optional().ok().flatten()
}

pub fn channel_from_row(r: &Row<'_>) -> rusqlite::Result<Channel> {
    let kind: String = r.get("kind")?;
    let kind = match kind.as_str() {
        "text" => ChannelKind::Text,
        "voice" => ChannelKind::Voice,
        "category" => ChannelKind::Category,
        "announcement" => ChannelKind::Announcement,
        "forum" => ChannelKind::Forum,
        "thread" => ChannelKind::Thread,
        "dm" => ChannelKind::Dm,
        "group_dm" => ChannelKind::GroupDm,
        _ => ChannelKind::Text,
    };
    let thread_parent: Option<Id> = r.get("thread_parent_id")?;
    let thread = if kind == ChannelKind::Thread {
        let parent = thread_parent.unwrap_or(0);
        Some(ThreadInfo {
            parent_id: parent,
            name: r.get("name")?,
            message_count: r.get::<_, i64>("thread_message_count")? as u32,
            archived: false,
        })
    } else {
        None
    };
    Ok(Channel {
        id: r.get("id")?,
        guild_id: r.get("guild_id")?,
        name: r.get("name")?,
        kind,
        position: r.get::<_, i64>("position")? as i32,
        parent_id: r.get("parent_id")?,
        topic: r.get("topic")?,
        nsfw: r.get::<_, i64>("nsfw")? != 0,
        slowmode: r.get::<_, i64>("slowmode")? as u32,
        last_message_id: r.get("last_message_id")?,
        owner_id: r.get("owner_id")?,
        thread,
    })
}

pub fn channel_by_id(conn: &Connection, id: Id) -> Option<Channel> {
    conn.query_row("SELECT * FROM channels WHERE id=?1", params![id], channel_from_row).optional().ok().flatten()
}

pub fn role_from_row(r: &Row<'_>) -> rusqlite::Result<Role> {
    Ok(Role {
        id: r.get("id")?,
        guild_id: r.get("guild_id")?,
        name: r.get("name")?,
        color: r.get::<_, i64>("color")? as u32,
        permissions: r.get::<_, i64>("permissions")? as u64,
        position: r.get::<_, i64>("position")? as i32,
        hoist: r.get::<_, i64>("hoist")? != 0,
        mentionable: r.get::<_, i64>("mentionable")? != 0,
    })
}

pub fn attachments(conn: &Connection, message_id: Id) -> Vec<Attachment> {
    let mut stmt = match conn.prepare("SELECT * FROM attachments WHERE message_id=?1 ORDER BY id") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map(params![message_id], |r| {
        Ok(Attachment {
            id: r.get("id")?,
            filename: r.get("filename")?,
            size: r.get::<_, i64>("size")? as u64,
            content_type: r.get("content_type")?,
            url: r.get("url")?,
            width: r.get::<_, i64>("width")? as u32,
            height: r.get::<_, i64>("height")? as u32,
        })
    })
    .map(|rows| rows.filter_map(Result::ok).collect())
    .unwrap_or_default()
}

pub fn reactions(conn: &Connection, message_id: Id, me: Id) -> Vec<Reaction> {
    let mut stmt = match conn.prepare("SELECT emoji, user_id FROM reactions WHERE message_id=?1 ORDER BY emoji, user_id") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut grouped: Vec<Reaction> = Vec::new();
    if let Ok(rows) = stmt.query_map(params![message_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Id>(1)?))) {
        for (emoji, user_id) in rows.flatten() {
            if let Some(last) = grouped.last_mut().filter(|x| x.emoji == emoji) {
                last.count += 1;
                last.user_ids.push(user_id);
                last.me |= user_id == me;
            } else {
                grouped.push(Reaction { emoji, count: 1, me: user_id == me, user_ids: vec![user_id] });
            }
        }
    }
    grouped
}

pub fn mentions_in(content: &str) -> Vec<Id> {
    let mut out = Vec::new();
    let mut rest = content;
    while let Some(i) = rest.find("<@") {
        let after = &rest[i + 2..];
        let Some(end) = after.find('>') else { break };
        if let Ok(id) = after[..end].parse::<Id>() {
            out.push(id);
        }
        rest = &after[end..];
    }
    out
}

pub fn message_by_id(conn: &Connection, id: Id, me: Id) -> Option<Message> {
    let base = conn
        .query_row(
            "SELECT m.*, u.display_name AS reply_author, rm.content AS reply_content, rm.author_id AS reply_author_id
             FROM messages m
             LEFT JOIN messages rm ON rm.id = m.reply_to
             LEFT JOIN users u ON u.id = rm.author_id
             WHERE m.id=?1",
            params![id],
            |r| {
                let content: String = r.get("content")?;
                let reply_to: Option<Id> = r.get("reply_to")?;
                Ok(Message {
                    id: r.get("id")?,
                    channel_id: r.get("channel_id")?,
                    author_id: r.get("author_id")?,
                    content: content.clone(),
                    timestamp: r.get("timestamp")?,
                    edited_at: r.get("edited_at")?,
                    kind: MessageKind::Default,
                    reply_to: reply_to.map(|rid| MessageReply {
                        message_id: rid,
                        author_id: r.get::<_, Option<Id>>("reply_author_id").ok().flatten().unwrap_or(0),
                        author_name: r.get::<_, Option<String>>("reply_author").ok().flatten().unwrap_or_default(),
                        content: r.get::<_, Option<String>>("reply_content").ok().flatten().unwrap_or_default(),
                    }),
                    attachments: Vec::new(),
                    reactions: Vec::new(),
                    pinned: r.get::<_, i64>("pinned")? != 0,
                    thread_id: None,
                    mentions: mentions_in(&content),
                    mention_everyone: content.contains("@everyone") || content.contains("@here"),
                    pending: false,
                })
            },
        )
        .optional()
        .ok()
        .flatten()?;
    let mut msg = base;
    msg.attachments = attachments(conn, msg.id);
    msg.reactions = reactions(conn, msg.id, me);
    Some(msg)
}

// ---------------------------------------------------------------------------
// Permissions
// ---------------------------------------------------------------------------

pub fn base_permissions(conn: &Connection, guild_id: Id, user_id: Id) -> u64 {
    let owner: Option<Id> = conn.query_row("SELECT owner_id FROM guilds WHERE id=?1", params![guild_id], |r| r.get(0)).optional().ok().flatten();
    if owner == Some(user_id) {
        return u64::MAX / 2;
    }
    let mut perms = 0u64;
    if let Ok(v) = conn.query_row(
        "SELECT permissions FROM roles WHERE guild_id=?1 AND name='@everyone' LIMIT 1",
        params![guild_id],
        |r| r.get::<_, i64>(0),
    ) {
        perms |= v as u64;
    }
    if let Ok(mut stmt) = conn.prepare(
        "SELECT r.permissions FROM roles r JOIN member_roles mr ON mr.role_id=r.id AND mr.user_id=?2 AND mr.guild_id=?1",
    ) {
        if let Ok(rows) = stmt.query_map(params![guild_id, user_id], |r| r.get::<_, i64>(0)) {
            for v in rows.flatten() {
                perms |= v as u64;
            }
        }
    }
    perms
}

pub fn channel_permissions(conn: &Connection, channel_id: Id, user_id: Id) -> u64 {
    let Some(channel) = channel_by_id(conn, channel_id) else { return 0 };
    let Some(guild_id) = channel.guild_id else { return u64::MAX / 2 };
    let mut perms = base_permissions(conn, guild_id, user_id);
    if perms & perm::ADMINISTRATOR != 0 {
        return perms;
    }
    let mut applies: Vec<(i64, u64, u64)> = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT target_id, allow, deny FROM overwrites WHERE channel_id=?1") {
        if let Ok(rows) = stmt.query_map(params![channel_id], |r| Ok((r.get::<_, Id>(0)?, r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)? as u64))) {
            for row in rows.flatten() {
                applies.push(row);
            }
        }
    }
    let role_ids: Vec<Id> = member_role_ids(conn, guild_id, user_id);
    for (target, allow, deny) in &applies {
        let is_everyone = *target == 0;
        let is_member = *target == user_id;
        let is_role = role_ids.contains(target);
        if is_everyone || is_member || is_role {
            perms &= !deny;
            perms |= allow;
        }
    }
    perms
}

pub fn member_role_ids(conn: &Connection, guild_id: Id, user_id: Id) -> Vec<Id> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT role_id FROM member_roles WHERE guild_id=?1 AND user_id=?2") {
        if let Ok(rows) = stmt.query_map(params![guild_id, user_id], |r| r.get::<_, Id>(0)) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn can_view(conn: &Connection, channel_id: Id, user_id: Id) -> bool {
    let Some(channel) = channel_by_id(conn, channel_id) else { return false };
    if channel.guild_id.is_none() {
        return is_dm_member(conn, channel_id, user_id);
    }
    has(channel_permissions(conn, channel_id, user_id), perm::VIEW_CHANNEL)
}

pub fn can_send(conn: &Connection, channel_id: Id, user_id: Id) -> bool {
    let Some(channel) = channel_by_id(conn, channel_id) else { return false };
    if channel.guild_id.is_none() {
        return is_dm_member(conn, channel_id, user_id);
    }
    has(channel_permissions(conn, channel_id, user_id), perm::SEND_MESSAGES)
}

pub fn is_dm_member(conn: &Connection, channel_id: Id, user_id: Id) -> bool {
    conn.query_row(
        "SELECT 1 FROM dm_members WHERE channel_id=?1 AND user_id=?2",
        params![channel_id, user_id],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

pub fn overwrites(conn: &Connection, channel_id: Id) -> Vec<PermissionOverwrite> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT * FROM overwrites WHERE channel_id=?1") {
        if let Ok(rows) = stmt.query_map(params![channel_id], |r| {
            let kind: String = r.get("target_kind")?;
            Ok(PermissionOverwrite {
                target_id: r.get("target_id")?,
                kind: if kind == "member" { OverwriteKind::Member } else { OverwriteKind::Role },
                allow: r.get::<_, i64>("allow")? as u64,
                deny: r.get::<_, i64>("deny")? as u64,
            })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn unread_for(conn: &Connection, channel_id: Id, me: Id) -> u32 {
    let last: Option<Id> = conn
        .query_row(
            "SELECT last_read_id FROM reads WHERE user_id=?1 AND channel_id=?2",
            params![me, channel_id],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten();
    let after = last.unwrap_or(0);
    conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE channel_id=?1 AND id>?2 AND author_id<>?3",
        params![channel_id, after, me],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0) as u32
}

pub fn bootstrap(conn: &Connection, me: Id) -> Bootstrap {
    let Some(user) = user_by_id(conn, me) else { return Bootstrap::default() };
    let mut guilds = Vec::new();
    let mut users: HashMap<Id, User> = HashMap::new();
    users.insert(user.id, user.clone());

    let guild_ids: Vec<Id> = {
        let mut out = Vec::new();
        if let Ok(mut stmt) = conn.prepare("SELECT guild_id FROM guild_members WHERE user_id=?1 ORDER BY joined_at") {
            if let Ok(rows) = stmt.query_map(params![me], |r| r.get::<_, Id>(0)) {
                out.extend(rows.flatten());
            }
        }
        out
    };

    for gid in &guild_ids {
        let Ok(guild_row) = conn.query_row("SELECT * FROM guilds WHERE id=?1", params![gid], |r| {
            Ok((r.get::<_, Id>("id")?, r.get::<_, String>("name")?, r.get::<_, i64>("icon_color")? as u32, r.get::<_, Id>("owner_id")?, r.get::<_, String>("description")?, r.get::<_, i64>("created_at")?))
        }) else {
            continue;
        };
        let mut channels: Vec<Channel> = Vec::new();
        if let Ok(mut stmt) = conn.prepare("SELECT * FROM channels WHERE guild_id=?1 ORDER BY position, id") {
            if let Ok(rows) = stmt.query_map(params![gid], channel_from_row) {
                channels.extend(rows.flatten().filter(|c| {
                    c.kind != ChannelKind::Thread || true
                }));
            }
        }
        let mut overwrite_list = Vec::new();
        for ch in &channels {
            overwrite_list.push(ChannelOverwrites { channel_id: ch.id, overwrites: overwrites(conn, ch.id) });
        }
        let roles = {
            let mut out = Vec::new();
            if let Ok(mut stmt) = conn.prepare("SELECT * FROM roles WHERE guild_id=?1 ORDER BY position DESC") {
                if let Ok(rows) = stmt.query_map(params![gid], role_from_row) {
                    out.extend(rows.flatten());
                }
            }
            out
        };
        let members = {
            let mut out = Vec::new();
            if let Ok(mut stmt) = conn.prepare(
                "SELECT u.*, gm.nickname, gm.joined_at FROM guild_members gm JOIN users u ON u.id=gm.user_id WHERE gm.guild_id=?1 ORDER BY gm.joined_at",
            ) {
                if let Ok(rows) = stmt.query_map(params![gid], |r| {
                    let user = user_from_row(r)?;
                    Ok(Member {
                        guild_id: *gid,
                        user,
                        nickname: r.get("nickname")?,
                        roles: Vec::new(),
                        joined_at: r.get("joined_at")?,
                    })
                }) {
                    out.extend(rows.flatten());
                }
                for m in out.iter_mut() {
                    m.roles = member_role_ids(conn, *gid, m.user.id);
                    users.insert(m.user.id, m.user.clone());
                }
            }
            out
        };
        let member_count = members.len() as u32;
        guilds.push(Guild {
            id: guild_row.0,
            name: guild_row.1,
            icon_color: guild_row.2,
            owner_id: guild_row.3,
            description: guild_row.4,
            member_count,
            created_at: guild_row.5,
            channels,
            roles,
            members,
            overwrites: overwrite_list,
        });
    }

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
                dms.push(DmChannel { channel, recipients });
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
    if let Ok(mut stmt) = conn.prepare("SELECT channel_id, last_read_id FROM reads WHERE user_id=?1") {
        if let Ok(rows) = stmt.query_map(params![me], |r| Ok(ReadState { channel_id: r.get(0)?, last_read_id: r.get(1)?, mention_count: 0 })) {
            reads.extend(rows.flatten());
        }
    }

    let mut unread = BTreeMap::new();
    for guild in &guilds {
        for ch in &guild.channels {
            if ch.kind.is_textual() {
                let n = unread_for(conn, ch.id, me);
                if n > 0 {
                    unread.insert(ch.id, n);
                }
            }
        }
    }
    for dm in &dms {
        let n = unread_for(conn, dm.channel.id, me);
        if n > 0 {
            unread.insert(dm.channel.id, n);
        }
    }

    let presences = users.iter().map(|(id, u)| (*id, u.status)).collect();
    Bootstrap {
        user,
        guilds,
        dms,
        relationships,
        reads,
        users: users.into_values().collect(),
        presences,
        unread,
    }
}

// ---------------------------------------------------------------------------
// Seed
// ---------------------------------------------------------------------------

fn hash_password(password: &str) -> String {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    let salt = SaltString::generate(&mut OsRng);
    let params = argon2::Params::new(8 * 1024, 1, 1, None).unwrap_or_default();
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    argon
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .unwrap_or_default()
}

fn insert_user(conn: &Connection, id: Id, username: &str, display_name: &str, color: u32, status: Presence) -> Id {
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, avatar_color, bio, custom_status, status, created_at)
         VALUES(?1,?2,?3,?4,?5,'','',?6,?7)",
        params![id, username, display_name, hash_password("runecord"), color as i64, presence_str(status), now_ms()],
    )
    .ok();
    id
}

pub fn seed_demo(conn: &Connection, me_hint: Option<&str>) -> Option<String> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)).unwrap_or(0);
    if count > 0 {
        return me_hint.map(str::to_string);
    }
    let t = now_ms();
    let nova = insert_user(conn, 1, "nova", "Nova", 0, Presence::Online);
    let maya = insert_user(conn, 2, "maya", "Maya Chen", 4, Presence::Online);
    let leo = insert_user(conn, 3, "leo", "Leo Park", 1, Presence::Dnd);
    let sam = insert_user(conn, 4, "sam", "Sam Rivera", 2, Presence::Idle);
    let ana = insert_user(conn, 5, "ana", "Ana Souza", 3, Presence::Online);
    let priya = insert_user(conn, 6, "priya", "Priya Nair", 5, Presence::Online);
    let tom = insert_user(conn, 7, "tom", "Tom Vale", 6, Presence::Offline);
    let bot = insert_user(conn, 8, "runebot", "Runebot", 9, Presence::Online);
    conn.execute("UPDATE users SET custom_status='building tiny UIs' WHERE id=?1", params![maya]).ok();

    // Guild 100: Rune Lounge
    let guild_id = 100;
    let everyone_role = 900;
    conn.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(?1,'Rune Lounge',?2,?3,'A home for interface obsessives.',?4,?5)",
        params![guild_id, 0x5865f2i64, nova, t, everyone_role],
    )
    .ok();
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(?1,?2,'@everyone',0,?3,0,0,0)",
        params![everyone_role, guild_id, (perm::EVERYONE | perm::CREATE_INVITE) as i64],
    )
    .ok();
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(901,?1,'Moderator',?2,?3,2,1,1)",
        params![guild_id, 0x3ba55di64, (perm::EVERYONE | perm::MANAGE_MESSAGES | perm::KICK_MEMBERS | perm::CREATE_INVITE) as i64],
    )
    .ok();
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(902,?1,'Designer',?2,?3,1,1,1)",
        params![guild_id, 0xeb459ei64, perm::EVERYONE as i64],
    )
    .ok();

    for uid in [nova, maya, leo, sam, ana, priya, tom, bot] {
        conn.execute(
            "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(?1,?2,'',?3)",
            params![guild_id, uid, t],
        )
        .ok();
    }
    for (uid, role) in [(nova, 901), (maya, 901), (ana, 902)] {
        conn.execute("INSERT OR IGNORE INTO member_roles(guild_id, user_id, role_id) VALUES(?1,?2,?3)", params![guild_id, uid, role]).ok();
    }

    let ch = |id: Id, name: &str, kind: &str, pos: i32, parent: Option<Id>, topic: &str| {
        conn.execute(
            "INSERT INTO channels(id, guild_id, name, kind, position, parent_id, topic) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![id, guild_id, name, kind, pos, parent, topic],
        )
        .ok();
    };
    ch(200, "Information", "category", 0, None, "");
    ch(201, "welcome", "text", 1, Some(200), "Start here");
    ch(202, "announcements", "announcement", 2, Some(200), "News from the team");
    ch(210, "Text Channels", "category", 3, None, "");
    ch(211, "general", "text", 4, Some(210), "Talk about anything and everything");
    ch(212, "design", "text", 5, Some(210), "Screens, type, motion");
    ch(213, "dev", "text", 6, Some(210), "Build logs and bugs");
    ch(214, "off-topic", "text", 7, Some(210), "Anything goes");
    ch(220, "Voice Channels", "category", 8, None, "");
    ch(221, "General", "voice", 9, Some(220), "");
    ch(222, "Focus Room", "voice", 10, Some(220), "");

    let mut mid = 5000;
    let msgs = [
        (maya, "morning! did the new sidebar land?", 310),
        (nova, "yeah — it's built in Rune now. 120 fps and the whole client is ~9 MB of RAM", 308),
        (maya, "nine. megabytes.", 306),
        (maya, "electron could never", 305),
        (sam, "wait, with the virtualized message list? what does a 10k message channel cost?", 300),
        (nova, "same. rows outside the viewport never get laid out or shaped", 298),
        (ana, "the hover state on grouped messages is so clean", 240),
        (maya, "here's the row builder, if anyone wants to poke at it:\n```rust\nlet row = row().key(msg.id).pl(16.0).pr(48.0)\n    .hover(theme.chat_hover)\n    .child(avatar(&author, 40.0))\n    .child(body(msg, &author));\n```", 210),
        (priya, "just joined — is there a channel for keyboard shortcuts?", 90),
        (leo, "ctrl+k is the quick switcher. ctrl+shift+m toggles mute. esc closes whatever you opened", 88),
        (nova, "pinned it in #welcome so it doesn't get lost", 60),
    ];
    for (author, content, mins) in msgs {
        insert_message(conn, &mut mid, 211, author, content, t - mins * 60_000, Some(nova));
    }
    insert_message(conn, &mut mid, 201, bot, "Welcome to **Rune Lounge** — read the rules, pick a role, and say hi in #general.", t - 86_400_000, Some(nova));
    insert_message(conn, &mut mid, 202, nova, "Rune build 0.1 is live on the `dev` channel. Voice is next on the list.", t - 3_600_000, Some(nova));

    // A second guild so the rail has somewhere to go.
    let design = 300;
    conn.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(?1,'Design Guild',?2,?3,'Critique, type, and pixels.',?4,903)",
        params![design, 0xeb459ei64, maya, t],
    )
    .ok();
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(903,?1,'@everyone',0,?2,0,0,0)",
        params![design, perm::EVERYONE as i64],
    )
    .ok();
    for uid in [nova, maya, ana] {
        conn.execute("INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(?1,?2,'',?3)", params![design, uid, t]).ok();
    }
    conn.execute("INSERT INTO channels(id, guild_id, name, kind, position, topic) VALUES(301,?1,'general','text',0,'Say hello')", params![design]).ok();
    conn.execute("INSERT INTO channels(id, guild_id, name, kind, position, topic) VALUES(302,?1,'critique','text',1,'Post work, get notes')", params![design]).ok();
    insert_message(conn, &mut mid, 301, maya, "welcome to the design guild", t - 7_200_000, Some(nova));

    // Friends and DMs.
    let friend = |a: Id, b: Id| {
        conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'friend',?3)", params![a, b, t]).ok();
        conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'friend',?3)", params![b, a, t]).ok();
    };
    for uid in [maya, leo, ana] {
        friend(nova, uid);
    }
    conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'incoming',?3)", params![nova, sam, t]).ok();
    conn.execute("INSERT OR REPLACE INTO relationships(user_id, other_id, kind, since) VALUES(?1,?2,'outgoing',?3)", params![nova, priya, t]).ok();
    let dm = |cid: Id, a: Id, b: Id| {
        conn.execute("INSERT INTO channels(id, guild_id, name, kind, topic) VALUES(?1,NULL,'','dm','')", params![cid]).ok();
        conn.execute("INSERT INTO dm_members(channel_id, user_id) VALUES(?1,?2)", params![cid, a]).ok();
        conn.execute("INSERT INTO dm_members(channel_id, user_id) VALUES(?1,?2)", params![cid, b]).ok();
    };
    dm(900, nova, maya);
    dm(901, nova, leo);
    insert_message(conn, &mut mid, 900, maya, "the app icon looks great at 16px", t - 7_200_000, Some(nova));
    insert_message(conn, &mut mid, 900, nova, "I redrew it as a vector path so it stays crisp", t - 7_100_000, Some(nova));
    insert_message(conn, &mut mid, 901, leo, "send me the crash log when you get a sec", t - 86_400_000, Some(nova));

    conn.execute("INSERT OR REPLACE INTO reads(user_id, channel_id, last_read_id) VALUES(?1,212,0)", params![nova]).ok();
    conn.execute("INSERT OR REPLACE INTO reads(user_id, channel_id, last_read_id) VALUES(?1,213,0)", params![nova]).ok();

    Some("nova".to_string())
}

fn insert_message(conn: &Connection, _mid: &mut Id, channel: Id, author: Id, content: &str, ts: i64, _owner: Option<Id>) -> Id {
    let id = next_id(conn);
    conn.execute(
        "INSERT INTO messages(id, channel_id, author_id, content, timestamp) VALUES(?1,?2,?3,?4,?5)",
        params![id, channel, author, content, ts],
    )
    .ok();
    conn.execute("UPDATE channels SET last_message_id=?1 WHERE id=?2", params![id, channel]).ok();
    if content.contains("```rust") || content.contains("clean") {
        for emoji in ["🔥", "👀"] {
            conn.execute("INSERT OR IGNORE INTO reactions(message_id, emoji, user_id) VALUES(?1,?2,?3)", params![id, emoji, author]).ok();
        }
    }
    id
}

pub fn share_users(conn: &Connection, a: Id, b: Id) -> HashSet<Id> {
    let mut out = HashSet::new();
    out.insert(a);
    out.insert(b);
    out
}

// ---------------------------------------------------------------------------
// Bots
// ---------------------------------------------------------------------------

pub fn hash_bot_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn bot_for_token(conn: &Connection, token: &str) -> Option<Id> {
    let hash = hash_bot_token(token);
    conn.query_row(
        "SELECT bt.bot_id FROM bot_tokens bt JOIN users u ON u.id=bt.bot_id
         WHERE bt.token_hash=?1 AND u.is_bot=1",
        params![hash],
        |r| r.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn create_bot(conn: &Connection, owner: Id, display_name: &str) -> Result<(User, String), String> {
    use rand::RngCore;
    let name = display_name.trim();
    if name.chars().count() < 2 || name.chars().count() > 32 {
        return Err("Bot name must be 2-32 characters.".into());
    }
    let base: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let mut username = String::new();
    for _ in 0..10 {
        let suffix: u16 = rand::random();
        let candidate = format!("{base}_{:04x}", suffix);
        if user_by_name(conn, &candidate).is_none() {
            username = candidate;
            break;
        }
    }
    if username.is_empty() {
        return Err("Could not allocate a bot username.".into());
    }
    let id = next_id(conn);
    let color = (id.unsigned_abs() % 10) as i64;
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, avatar_color, bio, custom_status, status, is_bot, created_at)
         VALUES(?1,?2,?3,'',?4,'','','offline',1,?5)",
        params![id, username, name, color, now_ms()],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO bot_owners(owner_id, bot_id, created_at) VALUES(?1,?2,?3)",
        params![owner, id, now_ms()],
    )
    .map_err(|e| e.to_string())?;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let mut token = String::from("rcbot.");
    for b in bytes {
        token.push_str(&format!("{b:02x}"));
    }
    conn.execute(
        "INSERT INTO bot_tokens(bot_id, token_hash, label, created_at) VALUES(?1,?2,?3,?4)",
        params![id, hash_bot_token(&token), name, now_ms()],
    )
    .map_err(|e| e.to_string())?;
    let user = user_by_id(conn, id).ok_or_else(|| "bot row missing".to_string())?;
    Ok((user, token))
}

pub fn bots_owned(conn: &Connection, owner: Id) -> Vec<User> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT u.* FROM users u JOIN bot_owners b ON b.bot_id=u.id
         WHERE b.owner_id=?1 AND u.is_bot=1 ORDER BY u.created_at",
    ) {
        if let Ok(rows) = stmt.query_map(params![owner], user_from_row) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn delete_bot(conn: &Connection, owner: Id, bot: Id) -> bool {
    let owned: bool = conn
        .query_row(
            "SELECT 1 FROM bot_owners WHERE owner_id=?1 AND bot_id=?2",
            params![owner, bot],
            |_| Ok(()),
        )
        .optional()
        .ok()
        .flatten()
        .is_some();
    if !owned {
        return false;
    }
    conn.execute("DELETE FROM bot_tokens WHERE bot_id=?1", params![bot]).ok();
    conn.execute("DELETE FROM bot_owners WHERE bot_id=?1", params![bot]).ok();
    conn.execute("DELETE FROM guild_members WHERE user_id=?1", params![bot]).ok();
    conn.execute("DELETE FROM member_roles WHERE user_id=?1", params![bot]).ok();
    conn.execute("DELETE FROM relationships WHERE user_id=?1 OR other_id=?1", params![bot]).ok();
    conn.execute("DELETE FROM dm_members WHERE user_id=?1", params![bot]).ok();
    // Keep the user row so its message history still resolves an author; revoking the token
    // makes further bot logins impossible.
    true
}

pub fn add_bot_to_guild(conn: &Connection, bot: Id, guild: Id) -> bool {
    conn.execute(
        "INSERT OR IGNORE INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(?1,?2,'',?3)",
        params![guild, bot, now_ms()],
    )
    .is_ok()
}

// ---------------------------------------------------------------------------
// Message windows (Discord `before` / `after` / `around` semantics)
// ---------------------------------------------------------------------------

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
            if let Ok(rows) = stmt.query_map(params![channel, after, limit], |r| r.get::<_, Id>(0)) {
                ids.extend(rows.flatten());
            }
        }
    } else if let Some(around) = around {
        let half = limit / 2 + 1;
        let mut older: Vec<Id> = Vec::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id FROM messages WHERE channel_id=?1 AND id<=?2 ORDER BY id DESC LIMIT ?3",
        ) {
            if let Ok(rows) = stmt.query_map(params![channel, around, half], |r| r.get::<_, Id>(0)) {
                older.extend(rows.flatten());
            }
        }
        older.reverse();
        let mut newer: Vec<Id> = Vec::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id FROM messages WHERE channel_id=?1 AND id>?2 ORDER BY id ASC LIMIT ?3",
        ) {
            if let Ok(rows) = stmt.query_map(params![channel, around, limit / 2], |r| r.get::<_, Id>(0)) {
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
            if let Ok(rows) = stmt.query_map(params![channel, before, limit], |r| r.get::<_, Id>(0)) {
                ids.extend(rows.flatten());
            }
        }
        ids.reverse();
    }
    ids.into_iter().filter_map(|id| message_by_id(conn, id, me)).collect()
}

pub fn list_children(conn: &Connection, guild: Id) -> Vec<Channel> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT * FROM channels WHERE guild_id=?1 ORDER BY position, id") {
        if let Ok(rows) = stmt.query_map(params![guild], channel_from_row) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn guild_members(conn: &Connection, guild: Id) -> Vec<Member> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT u.*, gm.nickname, gm.joined_at FROM guild_members gm JOIN users u ON u.id=gm.user_id WHERE gm.guild_id=?1",
    ) {
        if let Ok(rows) = stmt.query_map(params![guild], |r| {
            let user = user_from_row(r)?;
            Ok(Member {
                guild_id: guild,
                user,
                nickname: r.get("nickname")?,
                roles: Vec::new(),
                joined_at: r.get("joined_at")?,
            })
        }) {
            out.extend(rows.flatten());
        }
    }
    for m in out.iter_mut() {
        m.roles = member_role_ids(conn, guild, m.user.id);
    }
    out
}
