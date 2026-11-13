//! gateway.

use super::*;
use crate::{AppState, store};
use axum::extract::State;
use axum::http::HeaderMap;
use rusqlite::{OptionalExtension, params};
use std::sync::Arc;

pub(crate) async fn gateway(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let me = auth_user(&state, &headers)?;
    Ok(ws.on_upgrade(move |socket| gateway_loop(socket, state, me)))
}

pub(crate) fn related_user(conn: &rusqlite::Connection, a: Id, b: Id) -> bool {
    a == b
        || conn
            .query_row(
                "SELECT 1 FROM guild_members x JOIN guild_members y ON x.guild_id=y.guild_id WHERE x.user_id=?1 AND y.user_id=?2 LIMIT 1",
                params![a, b],
                |_| Ok(()),
            )
            .optional()
            .ok()
            .flatten()
            .is_some()
        || conn
            .query_row(
                "SELECT 1 FROM relationships WHERE (user_id=?1 AND other_id=?2) OR (user_id=?2 AND other_id=?1) LIMIT 1",
                params![a, b],
                |_| Ok(()),
            )
            .optional()
            .ok()
            .flatten()
            .is_some()
}

pub(crate) fn gateway_event_visible(state: &AppState, me: Id, event: &GatewayEvent) -> bool {
    let conn = state.store.conn();
    match event {
        GatewayEvent::Ready(_)
        | GatewayEvent::ReadyCheck
        | GatewayEvent::InteractionCreate { .. } => false,
        GatewayEvent::MessageCreate(m)
        | GatewayEvent::MessageUpdate(m)
        | GatewayEvent::ReactionUpdate(m) => store::can_view(&conn, m.channel_id, me),
        GatewayEvent::MessageDelete { channel_id, .. } => store::can_view(&conn, *channel_id, me),
        GatewayEvent::TypingStart { channel_id, .. } => store::can_view(&conn, *channel_id, me),
        GatewayEvent::PresenceUpdate { user_id, .. } => related_user(&conn, me, *user_id),
        GatewayEvent::DmCreate(dm) => store::can_view(&conn, dm.channel.id, me),
        GatewayEvent::ChannelCreate(c) | GatewayEvent::ChannelUpdate(c) => {
            store::can_view(&conn, c.id, me)
        }
        GatewayEvent::ChannelDelete {
            guild_id: Some(guild_id),
            ..
        } => store::guild_member(&conn, *guild_id, me),
        GatewayEvent::ChannelDelete {
            channel_id,
            guild_id: None,
        } => store::is_dm_member(&conn, *channel_id, me),
        GatewayEvent::GuildCreate(g) | GatewayEvent::GuildUpdate(g) => {
            store::guild_member(&conn, g.id, me)
        }
        GatewayEvent::GuildDelete { .. } => true,
        GatewayEvent::RoleCreate(role) | GatewayEvent::RoleUpdate(role) => {
            store::guild_member(&conn, role.guild_id, me)
        }
        GatewayEvent::RoleDelete { guild_id, .. } => store::guild_member(&conn, *guild_id, me),
        GatewayEvent::MemberUpdate(m) => store::guild_member(&conn, m.guild_id, me),
        GatewayEvent::MemberRemove { guild_id, .. }
        | GatewayEvent::MemberCountUpdate { guild_id, .. }
        | GatewayEvent::GuildMembers { guild_id, .. } => store::guild_member(&conn, *guild_id, me),
        GatewayEvent::RelationshipUpdate(_) | GatewayEvent::RelationshipRemove { .. } => true,
        GatewayEvent::UserUpdate(u) => related_user(&conn, me, u.id),
        GatewayEvent::VoiceStateUpdate(v) => store::guild_member(&conn, v.guild_id, me),
        GatewayEvent::ReadUpdate { channel_id, .. } => store::can_view(&conn, *channel_id, me),
    }
}

pub(crate) async fn gateway_loop(socket: WebSocket, state: Arc<AppState>, me: Id) {
    let (mut sink, mut stream) = socket.split();
    let mut rx = state.tx.subscribe();

    // Presence.
    let first = {
        let mut online = state.online.lock().await;
        let count = online.entry(me).or_insert(0);
        *count += 1;
        *count == 1
    };
    if first {
        let conn = state.store.conn();
        conn.execute(
            "UPDATE users SET status='online' WHERE id=?1 AND status='offline'",
            params![me],
        )
        .ok();
        let _ = state.tx.send(GatewayEvent::PresenceUpdate {
            user_id: me,
            status: Presence::Online,
        });
    }

    let bootstrap = {
        let conn = state.store.conn();
        store::bootstrap(&conn, me)
    };
    let ready = GatewayEvent::Ready(Box::new(bootstrap));
    if let Ok(text) = serde_json::to_string(&ready) {
        let _ = sink.send(WsMessage::Text(text.into())).await;
    }

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(event) => {
                        if !gateway_event_visible(&state, me, &event) {
                            continue;
                        }
                        if let Ok(text) = serde_json::to_string(&event) {
                            if sink.send(WsMessage::Text(text.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            incoming = stream.next() => {
                match incoming {
                    Some(Ok(WsMessage::Text(text))) => {
                        if let Ok(cmd) = serde_json::from_str::<GatewayCommand>(&text) {
                            match cmd {
                                GatewayCommand::Typing { channel_id } => {
                                    let _ = state.tx.send(GatewayEvent::TypingStart {
                                        channel_id,
                                        user_id: me,
                                        until_ms: now_ms() + 8000,
                                    });
                                }
                                GatewayCommand::Heartbeat => {}
                                GatewayCommand::Subscribe { .. } => {}
                            }
                        }
                    }
                    Some(Ok(WsMessage::Ping(bytes))) => {
                        let _ = sink.send(WsMessage::Pong(bytes)).await;
                    }
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }

    {
        let mut online = state.online.lock().await;
        if let Some(count) = online.get_mut(&me) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                online.remove(&me);
                let conn = state.store.conn();
                conn.execute("UPDATE users SET status='offline' WHERE id=?1", params![me])
                    .ok();
                let _ = state.tx.send(GatewayEvent::PresenceUpdate {
                    user_id: me,
                    status: Presence::Offline,
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Misc
// ---------------------------------------------------------------------------
