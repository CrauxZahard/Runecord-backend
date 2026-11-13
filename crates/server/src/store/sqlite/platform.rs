//! platform.

use super::*;
use rusqlite::{params, Connection, OptionalExtension, Row};

pub fn record_cdn(conn: &Connection, hash: &str, filename: &str, content_type: &str, size: u64) {
    conn.execute(
        "INSERT OR IGNORE INTO cdn_assets(hash, filename, content_type, size, created_at) VALUES(?1,?2,?3,?4,?5)",
        params![hash, filename, content_type, size as i64, now_ms()],
    ).ok();
}

pub fn create_application(conn: &Connection, owner: Id, name: &str, color: u32) -> Result<Application, String> {
    let (bot, _token) = create_bot(conn, owner, &format!("{name} Bot"))?;
    let id = next_id(conn);
    conn.execute(
        "INSERT INTO applications(id, owner_id, name, description, icon_color, bot_id, created_at) VALUES(?1,?2,?3,'',?4,?5,?6)",
        params![id, owner, name.trim(), color as i64, bot.id, now_ms()],
    ).map_err(|e| e.to_string())?;
    Ok(Application {
        id,
        owner_id: owner,
        name: name.trim().into(),
        icon_color: color,
        bot,
        scopes: vec!["bot".into(), "applications.commands".into()],
        permissions: 0,
        created_at: now_ms(),
        ..Application::default()
    })
}

pub(crate) fn application_from_row(r: &Row<'_>, conn: &Connection, with_bot: bool) -> rusqlite::Result<Application> {
    let bot_id: Id = r.get("bot_id")?;
    let bot = if with_bot { user_by_id(conn, bot_id).unwrap_or_default() } else { User { id: bot_id, ..User::default() } };
    Ok(Application {
        id: r.get("id")?,
        owner_id: r.get("owner_id")?,
        name: r.get("name")?,
        description: r.get("description")?,
        icon_color: r.get::<_, i64>("icon_color")? as u32,
        bot,
        scopes: r.get::<_, String>("scopes").unwrap_or_default().split_whitespace().map(str::to_string).collect(),
        permissions: r.get::<_, i64>("permissions").unwrap_or(0) as u64,
        created_at: r.get("created_at")?,
    })
}

pub fn applications_owned(conn: &Connection, owner: Id) -> Vec<Application> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT * FROM applications WHERE owner_id=?1 ORDER BY id") {
        if let Ok(rows) = stmt.query_map(params![owner], |r| application_from_row(r, conn, true)) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn application_by_id(conn: &Connection, id: Id) -> Option<Application> {
    conn.query_row("SELECT * FROM applications WHERE id=?1", params![id], |r| application_from_row(r, conn, true))
        .optional().ok().flatten()
}

pub fn delete_application(conn: &Connection, owner: Id, id: Id) -> bool {
    let Some(app) = application_by_id(conn, id) else { return false };
    if app.owner_id != owner { return false; }
    conn.execute("DELETE FROM application_commands WHERE app_id=?1", params![id]).ok();
    conn.execute("DELETE FROM app_installs WHERE app_id=?1", params![id]).ok();
    conn.execute("DELETE FROM applications WHERE id=?1", params![id]).ok();
    delete_bot(conn, owner, app.bot.id);
    true
}

pub fn create_install(conn: &Connection, app: Id) -> String {
    const ALPHA: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut bytes = [0u8; 8];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut bytes);
    let code: String = bytes.iter().map(|b| ALPHA[*b as usize % ALPHA.len()] as char).collect();
    conn.execute(
        "INSERT INTO app_installs(code, app_id, uses, created_at) VALUES(?1,?2,0,?3)",
        params![code, app, now_ms()],
    ).ok();
    code
}

pub fn install_application(conn: &Connection, code: &str, guild: Id) -> Option<Application> {
    let app_id: Id = conn.query_row("SELECT app_id FROM app_installs WHERE code=?1", params![code], |r| r.get(0)).optional().ok().flatten()?;
    let app = application_by_id(conn, app_id)?;
    add_bot_to_guild(conn, app.bot.id, guild);
    conn.execute("UPDATE app_installs SET uses=uses+1 WHERE code=?1", params![code]).ok();
    Some(app)
}

pub fn create_command(conn: &Connection, app: Id, name: &str, description: &str, options: &serde_json::Value) -> ApplicationCommand {
    let id = next_id(conn);
    conn.execute(
        "INSERT INTO application_commands(id, app_id, name, description, options_json, created_at) VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, app, name, description, options.to_string(), now_ms()],
    ).ok();
    ApplicationCommand { id, app_id: app, name: name.into(), description: description.into(), options: options.clone() }
}

pub fn commands_for(conn: &Connection, app: Id) -> Vec<ApplicationCommand> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT * FROM application_commands WHERE app_id=?1") {
        if let Ok(rows) = stmt.query_map(params![app], |r| {
            let raw: String = r.get("options_json")?;
            Ok(ApplicationCommand {
                id: r.get("id")?,
                app_id: r.get("app_id")?,
                name: r.get("name")?,
                description: r.get("description")?,
                options: serde_json::from_str(&raw).unwrap_or(serde_json::Value::Array(Vec::new())),
            })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn delete_command(conn: &Connection, app: Id, command: Id) -> bool {
    conn.execute("DELETE FROM application_commands WHERE id=?1 AND app_id=?2", params![command, app]).is_ok()
        && conn.changes() > 0
}

pub fn command_in_guild(conn: &Connection, name: &str, guild: Id) -> Option<(Application, ApplicationCommand)> {
    let mut stmt = conn.prepare(
        "SELECT a.*, c.id AS cmd_id, c.name AS cmd_name, c.description AS cmd_description, c.options_json
         FROM application_commands c JOIN applications a ON a.id=c.app_id
         WHERE c.name=?1
           AND EXISTS(SELECT 1 FROM guild_members m WHERE m.guild_id=?2 AND m.user_id=a.bot_id)
         LIMIT 1",
    ).ok()?;
    let row = stmt.query_row(params![name, guild], |r| {
        let app = application_from_row(r, conn, true)?;
        let raw: String = r.get("options_json")?;
        let command = ApplicationCommand {
            id: r.get("cmd_id")?,
            app_id: app.id,
            name: r.get("cmd_name")?,
            description: r.get("cmd_description")?,
            options: serde_json::from_str(&raw).unwrap_or(serde_json::Value::Array(Vec::new())),
        };
        Ok((app, command))
    }).optional().ok().flatten();
    row
}

pub fn create_webhook(conn: &Connection, guild: Id, channel: Id, creator: Id, name: &str, color: u32) -> (WebhookInfo, String) {
    let id = next_id(conn);
    let mut bytes = [0u8; 24];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut bytes);
    let token = format!("rcwh.{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
    conn.execute(
        "INSERT INTO webhooks(id, guild_id, channel_id, creator_id, name, avatar_color, token_hash, created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![id, guild, channel, creator, name, color as i64, hash_bot_token(&token), now_ms()],
    ).ok();
    (WebhookInfo { id, guild_id: guild, channel_id: channel, name: name.into(), avatar_color: color, token: Some(token.clone()) }, token)
}

pub fn webhooks_for_channel(conn: &Connection, channel: Id) -> Vec<WebhookInfo> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare("SELECT * FROM webhooks WHERE channel_id=?1") {
        if let Ok(rows) = stmt.query_map(params![channel], |r| {
            Ok(WebhookInfo {
                id: r.get("id")?,
                guild_id: r.get("guild_id")?,
                channel_id: r.get("channel_id")?,
                name: r.get("name")?,
                avatar_color: r.get::<_, i64>("avatar_color")? as u32,
                token: None,
            })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}

pub fn delete_webhook(conn: &Connection, guild: Id, id: Id) -> bool {
    conn.execute("DELETE FROM webhooks WHERE id=?1 AND guild_id=?2", params![id, guild]).ok();
    conn.changes() > 0
}

pub fn webhook_by_token(conn: &Connection, id: Id, token: &str) -> Option<WebhookInfo> {
    let hash = hash_bot_token(token);
    conn.query_row(
        "SELECT * FROM webhooks WHERE id=?1 AND token_hash=?2",
        params![id, hash],
        |r| Ok(WebhookInfo {
            id: r.get("id")?,
            guild_id: r.get("guild_id")?,
            channel_id: r.get("channel_id")?,
            name: r.get("name")?,
            avatar_color: r.get::<_, i64>("avatar_color")? as u32,
            token: None,
        }),
    ).optional().ok().flatten()
}

pub fn record_audit(conn: &Connection, guild: Id, actor: Id, action: i32, target: Option<Id>, changes: serde_json::Value) {
    let id = next_id(conn);
    conn.execute(
        "INSERT INTO audit_log(id, guild_id, actor_id, action_type, target_id, changes, created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![id, guild, actor, action, target, changes.to_string(), now_ms()],
    ).ok();
}

pub fn audit_entries(conn: &Connection, guild: Id, limit: u32, before: Option<Id>) -> Vec<AuditEntry> {
    let before = before.unwrap_or(i64::MAX);
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT * FROM audit_log WHERE guild_id=?1 AND id<?2 ORDER BY id DESC LIMIT ?3",
    ) {
        if let Ok(rows) = stmt.query_map(params![guild, before, limit.clamp(1, 100)], |r| {
            let raw: String = r.get("changes")?;
            Ok(AuditEntry {
                id: r.get("id")?,
                guild_id: r.get("guild_id")?,
                actor_id: r.get("actor_id")?,
                action_type: r.get::<_, i64>("action_type")? as i32,
                target_id: r.get("target_id")?,
                changes: serde_json::from_str(&raw).unwrap_or(serde_json::Value::Array(Vec::new())),
                created_at: r.get("created_at")?,
            })
        }) {
            out.extend(rows.flatten());
        }
    }
    out
}


