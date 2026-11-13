//! invites.

use super::*;
use crate::{AppState, store};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use rusqlite::{OptionalExtension, params};
use std::sync::Arc;

pub(crate) fn invite_code() -> String {
    const ALPHA: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut bytes = [0u8; 8];
    rand::rng().fill_bytes(&mut bytes);
    bytes
        .iter()
        .map(|b| ALPHA[*b as usize % ALPHA.len()] as char)
        .collect()
}

pub(crate) async fn create_invite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(guild): Path<Id>,
    Json(body): Json<CreateInvite>,
) -> Result<Json<Invite>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    if !has(
        store::base_permissions(&conn, guild, me),
        perm::CREATE_INVITE,
    ) {
        return Err(forbidden());
    }
    let code = invite_code();
    conn.execute(
        "INSERT INTO invites(code, guild_id, inviter_id, uses, max_uses, created_at) VALUES(?1,?2,?3,0,?4,?5)",
        params![code, guild, me, body.max_uses, now_ms()],
    )
    .map_err(internal)?;
    let (name, color, count): (String, i64, i64) = conn
        .query_row("SELECT name, icon_color, (SELECT COUNT(*) FROM guild_members WHERE guild_id=?1) FROM guilds WHERE id=?1", params![guild], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(internal)?;
    Ok(Json(Invite {
        code,
        guild_id: guild,
        guild_name: name,
        guild_icon_color: color as u32,
        inviter_id: me,
        uses: 0,
        created_at: now_ms(),
        member_count: count as u32,
    }))
}

pub(crate) async fn invite_info(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<Json<Invite>, ApiError> {
    let conn = state.store.conn();
    let row = conn
        .query_row(
            "SELECT i.*, g.name, g.icon_color, (SELECT COUNT(*) FROM guild_members WHERE guild_id=g.id) AS member_count FROM invites i JOIN guilds g ON g.id=i.guild_id WHERE i.code=?1",
            params![code],
            |r| {
                Ok(Invite {
                    code: r.get("code")?,
                    guild_id: r.get("guild_id")?,
                    guild_name: r.get("name")?,
                    guild_icon_color: r.get::<_, i64>("icon_color")? as u32,
                    inviter_id: r.get("inviter_id")?,
                    uses: r.get::<_, i64>("uses")? as u32,
                    created_at: r.get("created_at")?,
                    member_count: r.get::<_, i64>("member_count")? as u32,
                })
            },
        )
        .optional()
        .map_err(internal)?;
    row.map(Json).ok_or_else(not_found)
}

pub(crate) async fn join_invite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> Result<Json<Guild>, ApiError> {
    let me = auth_user(&state, &headers)?;
    let conn = state.store.conn();
    let guild: Option<Id> = conn
        .query_row(
            "SELECT guild_id FROM invites WHERE code=?1",
            params![code],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?;
    let Some(guild) = guild else {
        return Err(not_found());
    };
    conn.execute("INSERT OR IGNORE INTO guild_members(guild_id, user_id, nickname, joined_at) VALUES(?1,?2,'',?3)", params![guild, me, now_ms()]).ok();
    conn.execute(
        "UPDATE invites SET uses=uses+1 WHERE code=?1",
        params![code],
    )
    .ok();
    let out = Guild {
        id: guild,
        ..Guild::default()
    };
    let count = conn
        .query_row(
            "SELECT COUNT(*) FROM guild_members WHERE guild_id=?1",
            params![guild],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as u32;
    let member = store::guild_member_detail(&conn, guild, me);
    drop(conn);
    let _ = state
        .tx
        .send(GatewayEvent::GuildCreate(Box::new(out.clone())));
    let _ = state.tx.send(GatewayEvent::MemberCountUpdate {
        guild_id: guild,
        count,
    });
    if let Some(member) = member {
        let _ = state.tx.send(GatewayEvent::MemberUpdate(Box::new(member)));
    }
    Ok(Json(out))
}

// ---------------------------------------------------------------------------
// Gateway
// ---------------------------------------------------------------------------
