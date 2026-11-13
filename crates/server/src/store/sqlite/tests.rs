//! Store tests.

use super::*;

use super::*;

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
    let page = members_page(&conn, 1, 1, 0, 100, "", 0, "all").expect("page");
    let elapsed = started.elapsed();
    assert_eq!(page.members.len(), 100);
    assert_eq!(page.total, 10_001);
    assert_eq!(page.next_offset, Some(100));
    assert!(elapsed.as_millis() < 250, "page took {elapsed:?}");

    let searched = members_page(&conn, 1, 1, 0, 50, "User 9999", 0, "all").expect("search");
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
