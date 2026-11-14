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
fn group_dms_keep_name_and_scope_messages_to_members() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    for id in 1..=4 {
        conn.execute(
            "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(?1,?2,?2,'x',1)",
            params![id, format!("user{id}")],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, topic, owner_id) VALUES(901,NULL,'Design Crew','group_dm','',1)",
        [],
    )
    .unwrap();
    for id in 1..=3 {
        conn.execute(
            "INSERT INTO dm_members(channel_id,user_id) VALUES(901,?1)",
            params![id],
        )
        .unwrap();
    }

    let channel = channel_by_id(&conn, 901).expect("group dm channel");
    assert_eq!(channel.kind, ChannelKind::GroupDm);
    assert_eq!(channel.name, "Design Crew");

    for id in 1..=3 {
        assert!(is_dm_member(&conn, 901, id));
        assert!(can_view(&conn, 901, id));
        assert!(can_send(&conn, 901, id));
    }
    assert!(!is_dm_member(&conn, 901, 4));
    assert!(!can_view(&conn, 901, 4));
    assert!(!can_send(&conn, 901, 4));

    let bootstrap = bootstrap(&conn, 1);
    let group = bootstrap
        .dms
        .iter()
        .find(|dm| dm.channel.id == 901)
        .expect("group dm in bootstrap");
    assert_eq!(group.channel.kind, ChannelKind::GroupDm);
    assert_eq!(group.channel.name, "Design Crew");
    let mut names: Vec<String> = group
        .recipients
        .iter()
        .map(|user| user.display_name.clone())
        .collect();
    names.sort();
    assert_eq!(names, vec!["user2", "user3"]);

    assert_eq!(dm_owner(&conn, 901), Some(1));
    assert!(dm_rename(&conn, 901, "Design Crew HQ"));
    assert!(dm_set_icon(&conn, 901, "/cdn/group.png"));
    let channel = channel_by_id(&conn, 901).expect("group dm channel");
    assert_eq!(channel.name, "Design Crew HQ");
    assert_eq!(channel.icon_url, "/cdn/group.png");

    assert!(dm_add_member(&conn, 901, 4));
    assert!(!dm_add_member(&conn, 901, 4));
    assert!(is_dm_member(&conn, 901, 4));
    assert!(can_send(&conn, 901, 4));
    assert_eq!(dm_members(&conn, 901), vec![1, 2, 3, 4]);

    assert!(dm_remove_member(&conn, 901, 4));
    assert!(!is_dm_member(&conn, 901, 4));
    assert!(!can_send(&conn, 901, 4));

    assert!(dm_transfer_owner(&conn, 901, 3));
    assert_eq!(dm_owner(&conn, 901), Some(3));
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
fn threads_inherit_parent_channel_permissions() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(1,'owner','Owner','x',1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(2,'member','Member','x',1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,2,'',1)",
        [],
    )
    .unwrap();
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
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, topic) VALUES(10,1,'parent','text','')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, thread_parent_id, owner_id) VALUES(11,1,'child','thread',10,1)",
        [],
    )
    .unwrap();
    assert!(has(channel_permissions(&conn, 11, 2), perm::SEND_MESSAGES));
    conn.execute(
        "INSERT INTO overwrites(channel_id, target_id, target_kind, allow, deny) VALUES(10,0,'role',0,?1)",
        params![perm::SEND_MESSAGES as i64],
    )
    .unwrap();
    assert!(!has(channel_permissions(&conn, 11, 2), perm::SEND_MESSAGES));
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
fn privacy_controls_new_dms_and_friend_requests() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    for id in 1..=3 {
        conn.execute(
            "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(?1,?2,?2,'x',1)",
            params![id, format!("user{id}")],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,1,'',1,0)",
        [],
    )
    .unwrap();
    for id in 1..=2 {
        conn.execute(
            "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,?1,'',1)",
            params![id],
        )
        .unwrap();
    }
    set_privacy(&conn, 2, PrivacyLevel::Nobody, PrivacyLevel::Nobody);
    assert!(!can_dm(&conn, 1, 2));
    assert!(!can_friend_request(&conn, 1, 2));
    set_privacy(
        &conn,
        2,
        PrivacyLevel::ServerMembers,
        PrivacyLevel::ServerMembers,
    );
    assert!(can_dm(&conn, 1, 2));
    assert!(can_friend_request(&conn, 1, 2));
    assert!(!can_dm(&conn, 3, 2));
    conn.execute(
        "INSERT INTO relationships(user_id, other_id, kind, since) VALUES(1,2,'friend',1)",
        [],
    )
    .unwrap();
    assert!(can_dm(&conn, 1, 2));
}

#[test]
fn discoverability_toggle_hides_server_from_explore() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(1,'me','Me','x',1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,1,'',1,0)",
        [],
    )
    .unwrap();
    assert_eq!(public_guilds(&conn, 1).len(), 1);
    assert!(set_guild_discoverable(&conn, 1, false));
    assert!(public_guilds(&conn, 1).is_empty());
    assert!(set_guild_discoverable(&conn, 1, true));
    assert_eq!(public_guilds(&conn, 1).len(), 1);
}

#[test]
fn public_directory_excludes_joined_servers() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(1,'me','Me','x',1)",
        [],
    )
    .unwrap();
    for (id, name) in [(1, "Joined"), (2, "Open"), (3, "Hidden")] {
        conn.execute(
            "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id, discoverable) VALUES(?1,?2,0,1,'',1,0,?3)",
            params![id, name, if id == 3 { 0 } else { 1 }],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,1,'',1)",
        [],
    )
    .unwrap();
    let open = public_guilds(&conn, 1);
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].name, "Open");
    assert!(join_public_guild(&conn, 2, 1));
    assert!(guild_member(&conn, 2, 1));
    assert!(public_guilds(&conn, 1).is_empty());
    assert!(!join_public_guild(&conn, 3, 1));
}

#[test]
fn profile_badges_and_decoration_round_trip() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(1,'nova','Nova','x',1)",
        [],
    )
    .unwrap();
    let user = user_by_id(&conn, 1).expect("user");
    assert!(user.badges.is_empty());
    assert!(user.avatar_decoration.is_empty());

    conn.execute(
        "UPDATE users SET badges='staff,nitro,early_supporter', avatar_decoration='aurora' WHERE id=1",
        [],
    )
    .unwrap();
    let user = user_by_id(&conn, 1).expect("user");
    assert_eq!(user.badges, vec!["staff", "nitro", "early_supporter"]);
    assert_eq!(user.avatar_decoration, "aurora");

    conn.execute(
        "UPDATE users SET badges='', avatar_decoration='' WHERE id=1",
        [],
    )
    .unwrap();
    let user = user_by_id(&conn, 1).expect("user");
    assert!(user.badges.is_empty());
    assert!(user.avatar_decoration.is_empty());
}

#[test]
fn user_profile_images_round_trip() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(1,'nova','Nova','x',1)",
        [],
    )
    .unwrap();
    let user = user_by_id(&conn, 1).expect("user");
    assert!(user.avatar_url.is_empty());
    assert!(user.banner_url.is_empty());

    conn.execute(
        "UPDATE users SET avatar_url='/cdn/avatar.png', banner_url='/cdn/banner.png' WHERE id=1",
        [],
    )
    .unwrap();
    let user = user_by_id(&conn, 1).expect("user");
    assert_eq!(user.avatar_url, "/cdn/avatar.png");
    assert_eq!(user.banner_url, "/cdn/banner.png");

    conn.execute("UPDATE users SET avatar_url='' WHERE id=1", [])
        .unwrap();
    let user = user_by_id(&conn, 1).expect("user");
    assert!(user.avatar_url.is_empty());
    assert_eq!(user.banner_url, "/cdn/banner.png");
}

#[test]
fn pronouns_and_shared_guilds_round_trip() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, created_at, pronouns) VALUES(1,'nova','Nova','x',1,'she/her')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(2,'maya','Maya','x',1)",
        [],
    )
    .unwrap();
    for guild in 1..=2 {
        conn.execute(
            "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(?1,?2,0,1,'',1,10)",
            params![guild, format!("Guild {guild}")],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(?1,1,'',1)",
            params![guild],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,2,'',1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(2,2,'',1)",
        [],
    )
    .unwrap();

    let user = user_by_id(&conn, 1).expect("user");
    assert_eq!(user.pronouns, "she/her");
    assert_eq!(user_by_id(&conn, 2).expect("user").pronouns, "");

    assert_eq!(shared_guilds(&conn, 1, 2), vec![1, 2]);
    assert!(shared_guilds(&conn, 1, 99).is_empty());
}

#[test]
fn banner_perk_unlocks_at_level_two() {
    assert!(!banner_unlocked(0));
    assert!(!banner_unlocked(1));
    assert!(banner_unlocked(2));
    assert!(banner_unlocked(3));
}

#[test]
fn guild_images_survive_and_reach_the_summary() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(1,'nova','Nova','x',1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,1,'',1,10)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,1,'',1)",
        [],
    )
    .unwrap();

    let (summary, _) = guild_summary_with_unread(&conn, 1, 1).expect("summary");
    assert!(summary.icon_url.is_empty());
    assert!(summary.banner_url.is_empty());

    conn.execute(
        "UPDATE guilds SET icon_url='/cdn/icon.png', banner_url='/cdn/banner.jpg' WHERE id=1",
        [],
    )
    .unwrap();
    let (summary, _) = guild_summary_with_unread(&conn, 1, 1).expect("summary");
    assert_eq!(summary.icon_url, "/cdn/icon.png");
    assert_eq!(summary.banner_url, "/cdn/banner.jpg");

    conn.execute("UPDATE guilds SET icon_url='' WHERE id=1", [])
        .unwrap();
    let (summary, _) = guild_summary_with_unread(&conn, 1, 1).expect("summary");
    assert!(summary.icon_url.is_empty());
    assert_eq!(summary.banner_url, "/cdn/banner.jpg");
}

#[test]
fn boost_perks_scale_with_level_and_reach_summary() {
    assert_eq!(upload_limit_bytes(0), 48 * 1024 * 1024);
    assert_eq!(upload_limit_bytes(1), 64 * 1024 * 1024);
    assert_eq!(upload_limit_bytes(2), 96 * 1024 * 1024);
    assert_eq!(upload_limit_bytes(3), 128 * 1024 * 1024);
    assert_eq!(upload_limit_bytes(9), 128 * 1024 * 1024);

    assert_eq!(voice_bitrate_kbps(0), 64);
    assert_eq!(voice_bitrate_kbps(1), 128);
    assert_eq!(voice_bitrate_kbps(2), 256);
    assert_eq!(voice_bitrate_kbps(3), 384);
    assert_eq!(voice_bitrate_kbps(9), 384);

    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    for id in 1..=3 {
        conn.execute(
            "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(?1,?2,?2,'x',1)",
            params![id, format!("user{id}")],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,1,'',1,10)",
        [],
    )
    .unwrap();
    for id in 1..=3 {
        conn.execute(
            "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,?1,'',1)",
            params![id],
        )
        .unwrap();
    }

    let (summary, _) = guild_summary_with_unread(&conn, 1, 1).expect("summary");
    assert_eq!(summary.boosts, 0);
    assert_eq!(summary.upload_limit_bytes, 48 * 1024 * 1024);
    assert_eq!(summary.voice_kbps, 64);

    boost_guild(&conn, 1, 1);
    boost_guild(&conn, 1, 2);
    let (summary, _) = guild_summary_with_unread(&conn, 1, 1).expect("summary");
    assert_eq!(summary.boost_level, 1);
    assert_eq!(summary.upload_limit_bytes, 64 * 1024 * 1024);
    assert_eq!(summary.voice_kbps, 128);
}

#[test]
fn boosts_persist_and_raise_levels() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    for id in 1..=3 {
        conn.execute(
            "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(?1,?2,?2,'x',1)",
            params![id, format!("user{id}")],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,1,'',1,10)",
        [],
    )
    .unwrap();
    for id in 1..=3 {
        conn.execute(
            "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,?1,'',1)",
            params![id],
        )
        .unwrap();
    }
    assert_eq!(boost_count(&conn, 1), 0);
    assert_eq!(boost_level(0), 0);
    assert_eq!(boost_level(2), 1);
    assert_eq!(boost_level(7), 2);
    assert_eq!(boost_level(14), 3);
    assert_eq!(boost_level(99), 3);

    assert!(boost_guild(&conn, 1, 1));
    assert!(!boost_guild(&conn, 1, 1), "a member boosts once");
    assert!(boost_guild(&conn, 1, 2));
    assert_eq!(boost_count(&conn, 1), 2);
    assert!(boosted_by(&conn, 1, 1));
    assert!(!boosted_by(&conn, 1, 3));
    let (summary, _) = guild_summary_with_unread(&conn, 1, 2).expect("summary");
    assert_eq!(summary.boosts, 2);
    assert_eq!(summary.boost_level, 1);
    assert!(summary.boosted_by_me);

    assert!(unboost_guild(&conn, 1, 2));
    assert_eq!(boost_count(&conn, 1), 1);
    assert!(!boosted_by(&conn, 1, 2));
    let (summary, _) = guild_summary_with_unread(&conn, 1, 2).expect("summary");
    assert_eq!(summary.boost_level, 0);
}

#[test]
fn ban_appeals_update_and_resolve() {
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
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,1,'',1,10)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,2,'',1)",
        [],
    )
    .unwrap();
    ban_member(&conn, 1, 2, 1, "spam");

    let first = create_appeal(&conn, 1, 2, "first reason");
    assert_eq!(pending_appeal(&conn, 1, 2), Some(first));
    let second = create_appeal(&conn, 1, 2, "updated reason");
    assert_eq!(first, second, "a pending appeal is updated, not duplicated");
    let listed = list_appeals(&conn, 1);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].message, "updated reason");
    assert_eq!(listed[0].user.id, 2);

    assert_eq!(resolve_appeal(&conn, 1, first, 1, false), Some(2));
    assert!(is_banned(&conn, 1, 2));
    assert!(list_appeals(&conn, 1).is_empty());

    let third = create_appeal(&conn, 1, 2, "please");
    assert_eq!(resolve_appeal(&conn, 1, third, 1, true), Some(2));
    assert!(!is_banned(&conn, 1, 2));
    assert!(list_appeals(&conn, 1).is_empty());
    assert!(resolve_appeal(&conn, 1, third, 1, true).is_none());
}

#[test]
fn prune_skips_owner_bots_recent_members_and_protected_roles() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    for id in 1..=7 {
        conn.execute(
            "INSERT INTO users(id, username, display_name, pass_hash, created_at, is_bot) VALUES(?1,?2,?2,'x',1,?3)",
            params![id, format!("user{id}"), if id == 7 { 1 } else { 0 }],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,1,'',1,10)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, topic) VALUES(20,1,'general','text','')",
        [],
    )
    .unwrap();
    let old = now_ms() - 40 * 86_400_000;
    for id in 1..=7 {
        conn.execute(
            "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,?1,'',?2)",
            params![id, if id == 6 { now_ms() } else { old }],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(10,1,'@everyone',0,0,0,0,0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(12,1,'Designer',0,0,1,1,1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(13,1,'Other',0,0,1,1,1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO member_roles(guild_id, user_id, role_id) VALUES(1,4,12)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO member_roles(guild_id, user_id, role_id) VALUES(1,5,13)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages(id, channel_id, author_id, content, timestamp) VALUES(100,20,6,'still here',?1)",
        params![now_ms()],
    )
    .unwrap();

    let mut candidates = prune_candidates(&conn, 1, 7, &[], 2);
    candidates.sort();
    assert_eq!(candidates, vec![3]);

    let mut with_designer = prune_candidates(&conn, 1, 7, &[12], 2);
    with_designer.sort();
    assert_eq!(with_designer, vec![3, 4]);

    let mut with_both = prune_candidates(&conn, 1, 7, &[12, 13], 2);
    with_both.sort();
    assert_eq!(with_both, vec![3, 4, 5]);

    let pruned = prune_members(&conn, 1, &candidates);
    assert_eq!(pruned, vec![3]);
    assert!(!guild_member(&conn, 1, 3));
    assert!(guild_member(&conn, 1, 4));
}

#[test]
fn moderation_kick_ban_timeout_and_hierarchy() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    for id in 1..=3 {
        conn.execute(
            "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(?1,?2,?2,'x',1)",
            params![id, format!("user{id}")],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,1,'',1,10)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(10,1,'@everyone',0,?1,0,0,0)",
        params![perm::EVERYONE as i64],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(11,1,'Moderator',0,?1,5,1,1)",
        params![(perm::KICK_MEMBERS | perm::BAN_MEMBERS | perm::MODERATE_MEMBERS) as i64],
    )
    .unwrap();
    for id in 1..=3 {
        conn.execute(
            "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,?1,'',1)",
            params![id],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO member_roles(guild_id, user_id, role_id) VALUES(1,2,11)",
        [],
    )
    .unwrap();

    assert!(can_manage_member(&conn, 1, 2, 3));
    assert!(!can_manage_member(&conn, 1, 3, 2));
    assert!(guild_member(&conn, 1, 3));
    assert!(kick_member(&conn, 1, 3));
    assert!(!guild_member(&conn, 1, 3));

    conn.execute(
        "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,3,'',1)",
        [],
    )
    .unwrap();
    assert!(ban_member(&conn, 1, 3, 2, "spam"));
    assert!(is_banned(&conn, 1, 3));
    assert!(!guild_member(&conn, 1, 3));
    assert!(unban_member(&conn, 1, 3));
    assert!(!is_banned(&conn, 1, 3));

    conn.execute(
        "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,3,'',1)",
        [],
    )
    .unwrap();
    let until = now_ms() + 60_000;
    assert!(timeout_member(&conn, 1, 3, until));
    assert_eq!(member_timeout(&conn, 1, 3), until);
    assert!(timeout_member(&conn, 1, 3, 0));
    assert_eq!(member_timeout(&conn, 1, 3), 0);
    let member = guild_member_detail(&conn, 1, 3).expect("member");
    assert_eq!(member.timeout_until, 0);
}

#[test]
fn guild_events_store_and_order_by_start() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    conn.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,1,'',1,10)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(1,'nova','Nova','x',1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, topic) VALUES(10,1,'general','text','')",
        [],
    )
    .unwrap();
    for (id, name, start) in [(10, "Later", 200), (11, "Sooner", 100)] {
        conn.execute(
            "INSERT INTO guild_events(id, guild_id, creator_id, name, description, start_at, channel_id, created_at)
             VALUES(?1,1,1,?2,'',?3,10,1)",
            params![id, name, start],
        )
        .unwrap();
    }
    let mut stmt = conn
        .prepare("SELECT name FROM guild_events WHERE guild_id=1 ORDER BY start_at")
        .unwrap();
    let names: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(names, vec!["Sooner", "Later"]);
}

#[test]
fn voice_channel_status_round_trips() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    conn.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,1,'',1,10)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, topic, status) VALUES(10,1,'general','voice','','Design critique at 4')",
        [],
    )
    .unwrap();
    let channel = channel_by_id(&conn, 10).expect("channel");
    assert_eq!(channel.status, "Design critique at 4");
    assert_eq!(channel.kind, ChannelKind::Voice);
}

#[test]
fn profile_activity_and_connections_round_trip() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(1,'nova','Nova','x',1)",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE users SET activity_kind='spotify', activity_title='Midnight City',
         activity_subtitle='M83', activity_state='Hurry Up', activity_url='https://example.com',
         activity_artwork_url='/cdn/art.png', activity_started_at=42,
         connections='[{\"kind\":\"github\",\"name\":\"nova\"}]' WHERE id=1",
        [],
    )
    .unwrap();
    let user = user_by_id(&conn, 1).expect("user");
    let activity = user.activity.expect("activity");
    assert_eq!(activity.kind, "spotify");
    assert_eq!(activity.title, "Midnight City");
    assert_eq!(activity.artwork_url, "/cdn/art.png");
    assert_eq!(activity.started_at, 42);
    assert_eq!(user.connections.len(), 1);
    assert_eq!(user.connections[0].kind, "github");
    assert_eq!(user.connections[0].name, "nova");

    conn.execute("UPDATE users SET activity_kind='' WHERE id=1", [])
        .unwrap();
    assert!(user_by_id(&conn, 1).unwrap().activity.is_none());
}

#[test]
fn role_icons_are_gated_and_role_links_round_trip() {
    use super::{role_from_row, role_icon_unlocked};
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    conn.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,1,'',1,1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(5,1,'Designer',0,0,1,1,1)",
        [],
    )
    .unwrap();

    assert!(!role_icon_unlocked(0));
    assert!(!role_icon_unlocked(1));
    assert!(role_icon_unlocked(2));
    assert!(role_icon_unlocked(3));

    conn.execute("UPDATE roles SET icon_url='/cdn/role.png' WHERE id=5", [])
        .unwrap();
    let role = conn
        .query_row("SELECT * FROM roles WHERE id=5", [], role_from_row)
        .unwrap();
    assert_eq!(role.icon_url, "/cdn/role.png");

    conn.execute(
        "INSERT INTO role_links(id, guild_id, role_id, label, url, created_at) VALUES(1,1,5,'Design','https://example.com',1)",
        [],
    )
    .unwrap();
    let (label, url): (String, String) = conn
        .query_row(
            "SELECT label, url FROM role_links WHERE role_id=5",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(label, "Design");
    assert_eq!(url, "https://example.com");
    conn.execute("DELETE FROM role_links WHERE role_id=5", [])
        .unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM role_links WHERE role_id=5", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn muted_channels_hide_unread_and_mentions_only_counts_mentions() {
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
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,2,'',1,1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(1,1,'@everyone',0,?1,0,0,1)",
        params![perm::EVERYONE as i64],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, topic) VALUES(10,1,'general','text','')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages(id, channel_id, author_id, content, timestamp) VALUES(100,10,2,'first',1)",
        [],
    )
    .unwrap();
    assert_eq!(unread_for(&conn, 10, 1), 1);
    set_channel_notification(&conn, 1, 10, NotificationLevel::None, false);
    assert!(guild_unread_channels(&conn, 1, 1).is_empty());
    set_channel_notification(&conn, 1, 10, NotificationLevel::Mentions, false);
    assert!(guild_unread_channels(&conn, 1, 1).is_empty());
    conn.execute(
        "INSERT INTO messages(id, channel_id, author_id, content, timestamp) VALUES(101,10,2,'hi <@1>',2)",
        [],
    )
    .unwrap();
    assert_eq!(
        guild_unread_channels(&conn, 1, 1).get(&10).copied(),
        Some(1)
    );
}

#[test]
fn suppress_everyone_hides_broadcast_mentions() {
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
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,2,'',1,10)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(10,1,'@everyone',0,?1,0,0,0)",
        params![perm::EVERYONE as i64],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, topic) VALUES(10,1,'general','text','')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages(id, channel_id, author_id, content, timestamp) VALUES(100,10,2,'@everyone hello',1)",
        [],
    )
    .unwrap();
    set_channel_notification(&conn, 1, 10, NotificationLevel::Mentions, false);
    assert_eq!(unread_mentions_for(&conn, 10, 1, false, false), 1);
    assert_eq!(unread_mentions_for(&conn, 10, 1, true, false), 0);
    set_channel_notification(&conn, 1, 10, NotificationLevel::Mentions, true);
    assert!(channel_suppressed(&conn, 1, 10));
    assert_eq!(guild_unread_channels(&conn, 1, 1).get(&10).copied(), None);
    conn.execute(
        "INSERT INTO messages(id, channel_id, author_id, content, timestamp) VALUES(101,10,2,'hi <@1>',2)",
        [],
    )
    .unwrap();
    assert_eq!(
        guild_unread_channels(&conn, 1, 1).get(&10).copied(),
        Some(1)
    );
}

#[test]
fn guild_notification_defaults_mute_suppression_and_overrides() {
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
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,2,'',1,10)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(10,1,'@everyone',0,?1,0,0,0)",
        params![perm::EVERYONE as i64],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO roles(id, guild_id, name, color, permissions, position, hoist, mentionable) VALUES(11,1,'Team',0,0,1,1,1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,1,'',1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,2,'',1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO member_roles(guild_id, user_id, role_id) VALUES(1,1,11)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, topic) VALUES(10,1,'general','text','')",
        [],
    )
    .unwrap();
    for (id, content) in [
        (100, "plain"),
        (101, "@everyone hello"),
        (102, "<@&11> role ping"),
        (103, "<@1> direct"),
    ] {
        conn.execute(
            "INSERT INTO messages(id, channel_id, author_id, content, timestamp) VALUES(?1,10,2,?2,?1)",
            params![id, content],
        )
        .unwrap();
    }

    assert_eq!(
        guild_unread_channels(&conn, 1, 1).get(&10).copied(),
        Some(4)
    );

    let mut settings = GuildNotificationSettings {
        level: NotificationLevel::Mentions,
        ..GuildNotificationSettings::default()
    };
    set_guild_notification_settings(&conn, 1, 1, &settings);
    assert_eq!(
        guild_unread_channels(&conn, 1, 1).get(&10).copied(),
        Some(3)
    );

    settings.suppress_everyone = true;
    set_guild_notification_settings(&conn, 1, 1, &settings);
    assert_eq!(
        guild_unread_channels(&conn, 1, 1).get(&10).copied(),
        Some(2)
    );

    settings.suppress_roles = true;
    set_guild_notification_settings(&conn, 1, 1, &settings);
    assert_eq!(
        guild_unread_channels(&conn, 1, 1).get(&10).copied(),
        Some(1)
    );

    settings.muted = true;
    settings.suppress_roles = false;
    settings.suppress_everyone = false;
    settings.level = NotificationLevel::All;
    set_guild_notification_settings(&conn, 1, 1, &settings);
    assert_eq!(
        guild_unread_channels(&conn, 1, 1).get(&10).copied(),
        Some(3)
    );

    settings.level = NotificationLevel::None;
    settings.muted = false;
    settings.mute_new_events = true;
    settings.mobile_push = true;
    set_guild_notification_settings(&conn, 1, 1, &settings);
    assert!(guild_unread_channels(&conn, 1, 1).is_empty());

    set_channel_notification(&conn, 1, 10, NotificationLevel::All, false);
    assert_eq!(
        guild_unread_channels(&conn, 1, 1).get(&10).copied(),
        Some(4)
    );

    set_channel_notification(&conn, 1, 10, NotificationLevel::None, false);
    assert!(guild_unread_channels(&conn, 1, 1).is_empty());
    assert_eq!(
        guild_notifications_for(&conn, 1)
            .get(&1)
            .cloned()
            .unwrap_or_default(),
        settings
    );
}

#[test]
fn channel_notifications_persist_and_reset() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    assert_eq!(channel_notification(&conn, 1, 10), NotificationLevel::All);
    set_channel_notification(&conn, 1, 10, NotificationLevel::Mentions, false);
    assert_eq!(
        channel_notification(&conn, 1, 10),
        NotificationLevel::Mentions
    );
    assert_eq!(
        channel_notifications_for(&conn, 1)
            .get(&10)
            .copied()
            .unwrap_or_default(),
        NotificationLevel::Mentions
    );
    set_channel_notification(&conn, 1, 10, NotificationLevel::All, false);
    assert_eq!(channel_notification(&conn, 1, 10), NotificationLevel::All);
}

#[test]
fn leaving_a_guild_removes_membership_state() {
    let conn = Connection::open_in_memory().expect("memory db");
    migrate(&conn).expect("migrate");
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, created_at) VALUES(2,'member','Member','x',1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO guilds(id, name, icon_color, owner_id, description, created_at, everyone_role_id) VALUES(1,'Guild',0,1,'',1,1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO channels(id, guild_id, name, kind, topic) VALUES(10,1,'general','text','')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(1,2,'',1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO member_roles(guild_id, user_id, role_id) VALUES(1,2,1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO reads(user_id, channel_id, last_read_id) VALUES(2,10,1)",
        [],
    )
    .unwrap();
    set_channel_notification(&conn, 2, 10, NotificationLevel::None, false);
    leave_guild(&conn, 1, 2);
    assert!(!guild_member(&conn, 1, 2));
    assert!(member_role_ids(&conn, 1, 2).is_empty());
    assert_eq!(channel_notification(&conn, 2, 10), NotificationLevel::All);
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
