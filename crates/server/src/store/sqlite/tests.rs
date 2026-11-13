//! Store tests.

use super::*;

#[test]
fn dm_channels_carry_send_permissions() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    for id in 1..=2 {
        conn.execute(
            "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(?1,?2,?2,'x',1)",
            params![id, format!("user{id}")],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, topic) VALUES(900,NULL,'','dm','')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO dm_members(channel_id,user_id) VALUES(900,1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO dm_members(channel_id,user_id) VALUES(900,2)",
        [],
    )
    .unwrap();

    let channel = channel_by_id(&conn, 900).expect("dm channel");
    assert!(has(channel.my_permissions, perm::SEND_MESSAGES));
    assert!(can_send(&conn, 900, 1));
    assert!(can_send(&conn, 900, 2));
    assert!(
        bootstrap(&conn, 1)
            .dms
            .iter()
            .any(|dm| dm.channel.id == 900 && has(dm.channel.my_permissions, perm::SEND_MESSAGES))
    );
}

#[test]
fn channel_member_pages_hide_denied_members() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    conn.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,1,'',1,1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(1,1,'@everyone',0,?1,0,0,1)",
        params![perm::EVERYONE as i64],
    )
    .unwrap();
    for id in 1..=3 {
        conn.execute(
            "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(?1,?2,?2,'x',1)",
            params![id, format!("user{id}")],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,?1,'',?1)",
            params![id],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, topic) VALUES(10,1,'secret','text','')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO overwrites(channel_id, target_id, target_kind, allow, deny) VALUES(10,3,'member',0,?1)",
        params![perm::VIEW_CHANNEL as i64],
    )
    .unwrap();

    let channel_page =
        members_page(&conn, 1, 1, Some(10), 0, 100, "", 0, "all").expect("channel page");
    assert_eq!(channel_page.total, 2);
    assert_eq!(channel_page.members.len(), 2);
    assert!(
        !channel_page
            .members
            .iter()
            .any(|member| member.user.id == 3)
    );

    let guild_page = members_page(&conn, 1, 1, None, 0, 100, "", 0, "all").expect("guild page");
    assert_eq!(guild_page.total, 3);
}

#[test]
fn guild_commands_include_installed_app() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(1,'owner','Owner','x',1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,1,'',1,0)",
        [],
    )
    .unwrap();
    let app = create_application(&conn, 1, "Test App", 0x5865f2).expect("app");
    add_bot_to_guild(&conn, app.bot.id, 1);
    create_command(
        &conn,
        app.id,
        "ping",
        "Ping the app",
        &serde_json::json!({ "response": "Pong!" }),
    );
    let commands = commands_in_guild(&conn, 1);
    assert!(commands.iter().any(|command| command.name == "ping"));
}

#[test]
fn audit_pages_newest_first() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    for index in 0..60 {
        record_audit(
            &conn,
            1,
            1,
            11,
            None,
            serde_json::json!([{ "key": "step", "new_value": index }]),
        );
    }
    let first = audit_entries(&conn, 1, 50, None);
    assert_eq!(first.len(), 50);
    assert!(first[0].id > first[49].id);
    let older = audit_entries(&conn, 1, 50, Some(first[49].id));
    assert_eq!(older.len(), 10);
    assert!(older[0].id < first[49].id);
}

#[test]
fn member_pages_do_not_load_every_member() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    conn.execute(
            "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(1,'owner','Owner','x',1)",
            [],
        )
        .unwrap();
    conn.execute(
            "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Large',0,1,'',1,1)",
            [],
        )
        .unwrap();
    conn.execute(
            "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(1,1,'@everyone',0,?1,0,0,1)",
            params![perm::EVERYONE as i64],
        )
        .unwrap();
    conn.execute(
        "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,1,'',1)",
        [],
    )
    .unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    for id in 2..=10_001i64 {
        tx.execute(
                "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(?1,?2,?3,'x',1)",
                params![id, format!("user{id}"), format!("User {id}")],
            )
            .unwrap();
        tx.execute(
            "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,?1,'',?1)",
            params![id],
        )
        .unwrap();
    }
    tx.commit().unwrap();

    let started = std::time::Instant::now();
    let page = members_page(&conn, 1, 1, None, 0, 100, "", 0, "all").expect("page");
    let elapsed = started.elapsed();
    assert_eq!(page.members.len(), 100);
    assert_eq!(page.total, 10_001);
    assert_eq!(page.next_offset, Some(100));
    assert!(elapsed.as_millis() < 250, "page took {elapsed:?}");

    let searched = members_page(&conn, 1, 1, None, 0, 50, "User 9999", 0, "all").expect("search");
    assert_eq!(searched.members.len(), 1);

    let summary = guild_summary(&conn, 1, 1).expect("summary");
    assert_eq!(summary.member_count, 10_001);
    let bootstrap = bootstrap(&conn, 1);
    assert_eq!(bootstrap.guilds.len(), 1);
    assert_eq!(bootstrap.guilds[0].member_count, 10_001);
    assert!(bootstrap.users.len() <= 2);
    let encoded = serde_json::to_vec(&bootstrap).expect("bootstrap json");
    println!(
        "large_bootstrap_bytes={} page_elapsed={:?}",
        encoded.len(),
        elapsed
    );
    assert!(
        encoded.len() < 200_000,
        "bootstrap payload too large: {}",
        encoded.len()
    );
}
