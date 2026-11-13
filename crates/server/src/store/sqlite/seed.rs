//! seed.

use super::*;
use rusqlite::{Connection, params};

pub fn seed_demo(conn: &Connection, me_hint: Option<&str>) -> Option<String> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))
        .unwrap_or(0);
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
    conn.execute(
        "UPDATE users SET custom_status='building tiny UIs' WHERE id=?1",
        params![maya],
    )
    .ok();

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
        conn.execute(
            "INSERT OR IGNORE INTO member_roles(guild_id, user_id, role_id) VALUES(?1,?2,?3)",
            params![guild_id, uid, role],
        )
        .ok();
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
    ch(
        202,
        "announcements",
        "announcement",
        2,
        Some(200),
        "News from the team",
    );
    ch(210, "Text Channels", "category", 3, None, "");
    ch(
        211,
        "general",
        "text",
        4,
        Some(210),
        "Talk about anything and everything",
    );
    ch(212, "design", "text", 5, Some(210), "Screens, type, motion");
    ch(213, "dev", "text", 6, Some(210), "Build logs and bugs");
    ch(214, "off-topic", "text", 7, Some(210), "Anything goes");
    ch(220, "Voice Channels", "category", 8, None, "");
    ch(221, "General", "voice", 9, Some(220), "");
    ch(222, "Focus Room", "voice", 10, Some(220), "");

    let mut mid = 5000;
    let msgs = [
        (maya, "morning! did the new sidebar land?", 310),
        (
            nova,
            "yeah — it's built in Rune now. 120 fps and the whole client is ~9 MB of RAM",
            308,
        ),
        (maya, "nine. megabytes.", 306),
        (maya, "electron could never", 305),
        (
            sam,
            "wait, with the virtualized message list? what does a 10k message channel cost?",
            300,
        ),
        (
            nova,
            "same. rows outside the viewport never get laid out or shaped",
            298,
        ),
        (ana, "the hover state on grouped messages is so clean", 240),
        (
            maya,
            "here's the row builder, if anyone wants to poke at it:\n```rust\nlet row = row().key(msg.id).pl(16.0).pr(48.0)\n    .hover(theme.chat_hover)\n    .child(avatar(&author, 40.0))\n    .child(body(msg, &author));\n```",
            210,
        ),
        (
            priya,
            "just joined — is there a channel for keyboard shortcuts?",
            90,
        ),
        (
            leo,
            "ctrl+k is the quick switcher. ctrl+shift+m toggles mute. esc closes whatever you opened",
            88,
        ),
        (nova, "pinned it in #welcome so it doesn't get lost", 60),
    ];
    for (author, content, mins) in msgs {
        insert_message(
            conn,
            &mut mid,
            211,
            author,
            content,
            t - mins * 60_000,
            Some(nova),
        );
    }
    insert_message(
        conn,
        &mut mid,
        201,
        bot,
        "Welcome to **Rune Lounge** — read the rules, pick a role, and say hi in #general.",
        t - 86_400_000,
        Some(nova),
    );
    insert_message(
        conn,
        &mut mid,
        202,
        nova,
        "Rune build 0.1 is live on the `dev` channel. Voice is next on the list.",
        t - 3_600_000,
        Some(nova),
    );

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
        conn.execute(
            "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(?1,?2,'',?3)",
            params![design, uid, t],
        )
        .ok();
    }
    conn.execute("INSERT INTO channels(id, guild_id, name, kind, position, topic) VALUES(301,?1,'general','text',0,'Say hello')", params![design]).ok();
    conn.execute("INSERT INTO channels(id, guild_id, name, kind, position, topic) VALUES(302,?1,'critique','text',1,'Post work, get notes')", params![design]).ok();
    insert_message(
        conn,
        &mut mid,
        301,
        maya,
        "welcome to the design guild",
        t - 7_200_000,
        Some(nova),
    );

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
        conn.execute(
            "INSERT INTO channels(id, guild_id, name, kind, topic) VALUES(?1,NULL,'','dm','')",
            params![cid],
        )
        .ok();
        conn.execute(
            "INSERT INTO dm_members(channel_id, user_id) VALUES(?1,?2)",
            params![cid, a],
        )
        .ok();
        conn.execute(
            "INSERT INTO dm_members(channel_id, user_id) VALUES(?1,?2)",
            params![cid, b],
        )
        .ok();
    };
    dm(900, nova, maya);
    dm(901, nova, leo);
    insert_message(
        conn,
        &mut mid,
        900,
        maya,
        "the app icon looks great at 16px",
        t - 7_200_000,
        Some(nova),
    );
    insert_message(
        conn,
        &mut mid,
        900,
        nova,
        "I redrew it as a vector path so it stays crisp",
        t - 7_100_000,
        Some(nova),
    );
    insert_message(
        conn,
        &mut mid,
        901,
        leo,
        "send me the crash log when you get a sec",
        t - 86_400_000,
        Some(nova),
    );

    conn.execute(
        "INSERT OR REPLACE INTO reads(user_id, channel_id, last_read_id) VALUES(?1,212,0)",
        params![nova],
    )
    .ok();
    conn.execute(
        "INSERT OR REPLACE INTO reads(user_id, channel_id, last_read_id) VALUES(?1,213,0)",
        params![nova],
    )
    .ok();

    conn.execute("UPDATE guilds SET tag='RUNE' WHERE id=100", [])
        .ok();
    conn.execute("UPDATE guilds SET tag='DSGN' WHERE id=300", [])
        .ok();
    conn.execute(
        "UPDATE users SET avatar_border=?1, banner_color=?2 WHERE id=?3",
        params![0x5865f2i64, 0x5865f2i64, nova],
    )
    .ok();
    conn.execute(
        "UPDATE users SET avatar_border=?1, banner_color=?2 WHERE id=?3",
        params![0xeb459ei64, 0x1e1f22i64, maya],
    )
    .ok();

    Some("nova".to_string())
}

pub(crate) fn insert_message(
    conn: &Connection,
    _mid: &mut Id,
    channel: Id,
    author: Id,
    content: &str,
    ts: i64,
    _owner: Option<Id>,
) -> Id {
    let id = next_id(conn);
    conn.execute(
        "INSERT INTO messages(id, channel_id, author_id, content, timestamp) VALUES(?1,?2,?3,?4,?5)",
        params![id, channel, author, content, ts],
    )
    .ok();
    conn.execute(
        "UPDATE channels SET last_message_id=?1 WHERE id=?2",
        params![id, channel],
    )
    .ok();
    if content.contains("```rust") || content.contains("clean") {
        for emoji in ["🔥", "👀"] {
            conn.execute(
                "INSERT OR IGNORE INTO reactions(message_id, emoji, user_id) VALUES(?1,?2,?3)",
                params![id, emoji, author],
            )
            .ok();
        }
    }
    id
}

// ---------------------------------------------------------------------------
// Demo application commands
// ---------------------------------------------------------------------------

pub fn seed_demo_app(conn: &Connection) {
    let Some(owner) = user_by_name(conn, "nova") else {
        return;
    };
    let existing: Option<Id> = conn
        .query_row(
            "SELECT id FROM applications WHERE owner_id=?1 AND name='Rune Tools'",
            params![owner.id],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten();
    let app = if let Some(id) = existing {
        let Some(app) = application_by_id(conn, id) else {
            return;
        };
        app
    } else {
        let Ok(app) = create_application(conn, owner.id, "Rune Tools", 0x5865f2) else {
            return;
        };
        app
    };
    add_bot_to_guild(conn, app.bot.id, 100);
    let commands = commands_for(conn, app.id);
    if !commands.iter().any(|command| command.name == "ping") {
        create_command(
            conn,
            app.id,
            "ping",
            "Check that the local app is awake.",
            &serde_json::json!({ "response": "Pong!" }),
        );
    }
    if !commands.iter().any(|command| command.name == "echo") {
        create_command(
            conn,
            app.id,
            "echo",
            "Echo the text after the command.",
            &serde_json::json!({ "response": "Echo!" }),
        );
    }
}

// ---------------------------------------------------------------------------
// Bots
// ---------------------------------------------------------------------------
