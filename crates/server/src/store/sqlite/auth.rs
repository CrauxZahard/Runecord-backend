//! auth.

use super::*;
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) fn hash_password(password: &str) -> String {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    let salt = SaltString::generate(&mut OsRng);
    let params = argon2::Params::new(8 * 1024, 1, 1, None).unwrap_or_default();
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    argon
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .unwrap_or_default()
}

pub(crate) fn insert_user(conn: &Connection, id: Id, username: &str, display_name: &str, color: u32, status: Presence) -> Id {
    conn.execute(
        "INSERT INTO users(id, username, display_name, pass_hash, avatar_color, bio, custom_status, status, created_at)
         VALUES(?1,?2,?3,?4,?5,'','',?6,?7)",
        params![id, username, display_name, hash_password("runecord"), color as i64, presence_str(status), now_ms()],
    )
    .ok();
    id
}


pub fn hash_bot_token(token: &str) -> String {
    hash_bytes(token.as_bytes())
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
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


pub fn regenerate_bot_token(conn: &Connection, bot: Id) -> String {
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut bytes);
    let mut token = String::from("rcbot.");
    for b in bytes {
        token.push_str(&format!("{b:02x}"));
    }
    conn.execute(
        "INSERT INTO bot_tokens(bot_id, token_hash, label, created_at) VALUES(?1,?2,'',?3)
         ON CONFLICT(bot_id) DO UPDATE SET token_hash=excluded.token_hash",
        params![bot, hash_bot_token(&token), now_ms()],
    ).ok();
    token
}

