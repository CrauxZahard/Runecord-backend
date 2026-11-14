//! Discord-compatible gateway: HELLO / IDENTIFY / READY / HEARTBEAT / RESUME.
//! A central pump buffers events per session, so missed events replay even while the
//! socket is gone. The native `/gateway` stays as-is; both share the same bus.

use crate::AppState;
use crate::compat::{channel_json, guild_json, iso, message_json, user_json};
use crate::store;
use axum::extract::State;
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use runecord_protocol::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

const OP_DISPATCH: u8 = 0;
const OP_HEARTBEAT: u8 = 1;
const OP_IDENTIFY: u8 = 2;
const OP_RESUME: u8 = 6;
const OP_INVALID_SESSION: u8 = 9;
const OP_HELLO: u8 = 10;
const OP_HEARTBEAT_ACK: u8 = 11;

const INTENT_GUILDS: u64 = 1 << 0;
const INTENT_GUILD_MEMBERS: u64 = 1 << 1;
const INTENT_GUILD_PRESENCES: u64 = 1 << 8;
const INTENT_GUILD_MESSAGES: u64 = 1 << 9;
const INTENT_DIRECT_MESSAGES: u64 = 1 << 12;
const INTENT_MESSAGE_CONTENT: u64 = 1 << 15;
const ALL_INTENTS: u64 = u64::MAX >> 2;

const SESSION_TTL: Duration = Duration::from_secs(300);
const BUFFER_CAP: usize = 1024;

#[derive(Clone, Debug)]
struct Buffered {
    seq: u64,
    event: String,
    data: Value,
}

struct Session {
    user: Id,
    intents: u64,
    seq: u64,
    buffer: VecDeque<Buffered>,
    seen: Instant,
    live: broadcast::Sender<Buffered>,
}

pub struct Sessions {
    inner: Mutex<HashMap<String, Session>>,
    counter: AtomicU64,
}

impl Sessions {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            counter: AtomicU64::new(1),
        }
    }

    fn next_id(&self) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("rcs_{:x}_{}", n, now_ms())
    }

    fn prune(&self) {
        let mut map = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.retain(|_, s| s.seen.elapsed() < SESSION_TTL);
    }

    fn create(&self, user: Id, intents: u64) -> (String, broadcast::Receiver<Buffered>) {
        self.prune();
        let id = self.next_id();
        let (live, rx) = broadcast::channel(1024);
        let mut map = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.insert(
            id.clone(),
            Session {
                user,
                intents,
                seq: 0,
                buffer: VecDeque::new(),
                seen: Instant::now(),
                live,
            },
        );
        (id, rx)
    }

    fn subscribe(&self, session_id: &str) -> Option<broadcast::Receiver<Buffered>> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(session_id)
            .map(|s| s.live.subscribe())
    }

    fn exists_for(&self, session_id: &str, user: Id) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(session_id)
            .is_some_and(|s| s.user == user && s.seen.elapsed() < SESSION_TTL)
    }

    fn since(&self, session_id: &str, after: u64) -> Vec<Buffered> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(session_id)
            .map(|s| s.buffer.iter().filter(|b| b.seq > after).cloned().collect())
            .unwrap_or_default()
    }

    /// Appends to the replay buffer and fans out to a live socket if one is attached.
    fn publish(&self, session_id: &str, event: String, data: Value) -> Option<u64> {
        let mut map = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let session = map.get_mut(session_id)?;
        session.seq += 1;
        session.seen = Instant::now();
        let seq = session.seq;
        let item = Buffered { seq, event, data };
        session.buffer.push_back(item.clone());
        while session.buffer.len() > BUFFER_CAP {
            session.buffer.pop_front();
        }
        let _ = session.live.send(item);
        Some(seq)
    }

    fn touch(&self, session_id: &str) {
        if let Some(s) = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_mut(session_id)
        {
            s.seen = Instant::now();
        }
    }

    fn current_seq(&self, session_id: &str) -> u64 {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(session_id)
            .map(|s| s.seq)
            .unwrap_or(0)
    }

    fn snapshot(&self) -> Vec<(String, Id, u64)> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|(_, s)| s.seen.elapsed() < SESSION_TTL)
            .map(|(id, s)| (id.clone(), s.user, s.intents))
            .collect()
    }
}

#[derive(Deserialize)]
struct Identify {
    token: String,
    #[serde(default)]
    intents: u64,
}

#[derive(Deserialize)]
struct Resume {
    token: String,
    session_id: String,
    #[serde(default)]
    seq: u64,
}

#[derive(Deserialize)]
struct VoiceIn {
    guild_id: Id,
    #[serde(default)]
    channel_id: Option<Id>,
    #[serde(default)]
    self_mute: bool,
    #[serde(default)]
    self_deaf: bool,
}

/// Buffers every broadcast event for every live session, connected or not.
pub fn spawn_pump(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut rx = state.tx.subscribe();
        loop {
            let Ok(event) = rx.recv().await else { continue };
            for (id, user, intents) in state.v10.snapshot() {
                if let Some((name, data)) = dispatch(&state, user, intents, &event) {
                    state.v10.publish(&id, name, data);
                }
            }
        }
    });
}

pub async fn handler(State(state): State<Arc<AppState>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| run(socket, state))
}

async fn run(socket: WebSocket, state: Arc<AppState>) {
    let (mut sink, mut stream) = socket.split();
    if send_json(
        &mut sink,
        json!({ "op": OP_HELLO, "d": { "heartbeat_interval": 41250 } }),
    )
    .await
    .is_err()
    {
        return;
    }

    // Handshake: IDENTIFY or RESUME.
    let (sid, user, live, last_seq) = loop {
        let Some(Ok(msg)) = stream.next().await else {
            return;
        };
        let WsMessage::Text(text) = msg else {
            if matches!(msg, WsMessage::Close(_)) {
                return;
            }
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let op = value.get("op").and_then(|o| o.as_u64()).unwrap_or(0) as u8;
        let d = value.get("d").cloned().unwrap_or(Value::Null);
        match op {
            OP_HEARTBEAT => {
                let _ = send_json(
                    &mut sink,
                    json!({ "op": OP_HEARTBEAT_ACK, "d": Value::Null }),
                )
                .await;
            }
            OP_IDENTIFY => {
                let Ok(ident) = serde_json::from_value::<Identify>(d) else {
                    let _ =
                        send_json(&mut sink, json!({ "op": OP_INVALID_SESSION, "d": false })).await;
                    return;
                };
                let Some((id, _is_bot)) = auth_token(&state, &ident.token) else {
                    let _ =
                        send_json(&mut sink, json!({ "op": OP_INVALID_SESSION, "d": false })).await;
                    return;
                };
                let intents = if ident.intents == 0 {
                    ALL_INTENTS
                } else {
                    ident.intents
                };
                let (sid, live) = state.v10.create(id, intents);
                mark_online(&state, id).await;
                let ready = ready_payload(&state, id, &sid);
                let _ = send_json(
                    &mut sink,
                    json!({ "op": OP_DISPATCH, "t": "READY", "s": 0, "d": ready }),
                )
                .await;
                break (sid, id, live, 0u64);
            }
            OP_RESUME => {
                let Ok(res) = serde_json::from_value::<Resume>(d) else {
                    let _ =
                        send_json(&mut sink, json!({ "op": OP_INVALID_SESSION, "d": false })).await;
                    return;
                };
                let Some((id, _is_bot)) = auth_token(&state, &res.token) else {
                    let _ =
                        send_json(&mut sink, json!({ "op": OP_INVALID_SESSION, "d": false })).await;
                    return;
                };
                if !state.v10.exists_for(&res.session_id, id) {
                    let _ =
                        send_json(&mut sink, json!({ "op": OP_INVALID_SESSION, "d": false })).await;
                    return;
                }
                let Some(live) = state.v10.subscribe(&res.session_id) else {
                    let _ =
                        send_json(&mut sink, json!({ "op": OP_INVALID_SESSION, "d": false })).await;
                    return;
                };
                for item in state.v10.since(&res.session_id, res.seq) {
                    let _ = send_json(&mut sink, json!({ "op": OP_DISPATCH, "t": item.event, "s": item.seq, "d": item.data })).await;
                }
                let _ = send_json(&mut sink, json!({ "op": OP_DISPATCH, "t": "RESUMED", "s": state.v10.current_seq(&res.session_id), "d": {} })).await;
                mark_online(&state, id).await;
                break (res.session_id, id, live, res.seq);
            }
            _ => {}
        }
    };

    let mut live = live;
    let mut last_sent = last_seq;
    let mut online = true;

    loop {
        tokio::select! {
            incoming = stream.next() => {
                let Some(Ok(msg)) = incoming else { break };
                match msg {
                    WsMessage::Text(text) => {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            let op = value.get("op").and_then(|o| o.as_u64()).unwrap_or(0) as u8;
                            let d = value.get("d").cloned().unwrap_or(Value::Null);
                            if op == OP_HEARTBEAT {
                                let _ = send_json(&mut sink, json!({ "op": OP_HEARTBEAT_ACK, "d": Value::Null })).await;
                                state.v10.touch(&sid);
                            } else if op == 4 {
                                if let Ok(v) = serde_json::from_value::<VoiceIn>(d) {
                                    let _ = crate::platform::set_voice(&state, user, v.guild_id, v.channel_id, v.self_mute, v.self_deaf);
                                }
                            } else if op == 6 {
                                let _ = send_json(&mut sink, json!({ "op": OP_INVALID_SESSION, "d": false })).await;
                            }
                        }
                    }
                    WsMessage::Ping(bytes) => { let _ = sink.send(WsMessage::Pong(bytes)).await; }
                    WsMessage::Close(_) => break,
                    _ => {}
                }
            }
            event = live.recv() => {
                match event {
                    Ok(item) if item.seq > last_sent => {
                        last_sent = item.seq;
                        let _ = send_json(&mut sink, json!({ "op": OP_DISPATCH, "t": item.event, "s": item.seq, "d": item.data })).await;
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        }
    }

    if online {
        online = false;
        mark_offline(&state, user).await;
    }
    let _ = online;
}

async fn send_json(
    sink: &mut futures_util::stream::SplitSink<WebSocket, WsMessage>,
    value: Value,
) -> Result<(), ()> {
    sink.send(WsMessage::Text(value.to_string().into()))
        .await
        .map_err(|_| ())
}

fn auth_token(state: &AppState, raw: &str) -> Option<(Id, bool)> {
    let token = raw.trim();
    let conn = state.store.conn();
    if let Some(bot) = token.strip_prefix("Bot ") {
        let id = store::bot_for_token(&conn, bot.trim())?;
        if !store::bot_scopes(&conn, id).iter().any(|s| s == "bot") {
            return None;
        }
        return Some((id, true));
    }
    store::session_user(&conn, token).map(|id| (id, false))
}

fn ready_payload(state: &AppState, user_id: Id, session_id: &str) -> Value {
    let conn = state.store.conn();
    let user = store::user_by_id(&conn, user_id).unwrap_or_default();
    let guilds: Vec<Value> = store::guild_ids_for_user(&conn, user_id)
        .into_iter()
        .map(|id| json!({ "id": id.to_string(), "unavailable": true }))
        .collect();
    json!({
        "v": 1,
        "user": user_json(&user),
        "guilds": guilds,
        "session_id": session_id,
        "resume_gateway_url": format!("ws://127.0.0.1:{}/gateway/v1", state.port),
        "application": { "id": "0", "flags": 0 }
    })
}

async fn mark_online(state: &AppState, user: Id) {
    let first = {
        let mut online = state.online.lock().await;
        let count = online.entry(user).or_insert(0);
        *count += 1;
        *count == 1
    };
    if first {
        let conn = state.store.conn();
        store::mark_online(&conn, user);
    }
    let _ = state.tx.send(GatewayEvent::PresenceUpdate {
        user_id: user,
        status: Presence::Online,
    });
}

async fn mark_offline(state: &AppState, user: Id) {
    let last = {
        let mut online = state.online.lock().await;
        if let Some(count) = online.get_mut(&user) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                online.remove(&user);
                true
            } else {
                false
            }
        } else {
            false
        }
    };
    if last {
        let conn = state.store.conn();
        store::mark_offline(&conn, user);
        let _ = state.tx.send(GatewayEvent::PresenceUpdate {
            user_id: user,
            status: Presence::Offline,
        });
    }
}

fn dispatch(
    state: &AppState,
    user: Id,
    intents: u64,
    event: &GatewayEvent,
) -> Option<(String, Value)> {
    let conn = state.store.conn();
    let allow_content = intents & INTENT_MESSAGE_CONTENT != 0;
    match event {
        GatewayEvent::MessageCreate(m)
        | GatewayEvent::MessageUpdate(m)
        | GatewayEvent::ReactionUpdate(m) => {
            let channel = store::channel_by_id(&conn, m.channel_id)?;
            let (needed, is_dm) = if channel.guild_id.is_some() {
                (INTENT_GUILD_MESSAGES, false)
            } else {
                (INTENT_DIRECT_MESSAGES, true)
            };
            if intents & needed == 0 || !visible(&conn, m.channel_id, user) {
                return None;
            }
            let mut json = message_json(&conn, m);
            if !allow_content && !is_dm {
                if let Some(obj) = json.as_object_mut() {
                    obj.insert("content".into(), Value::String(String::new()));
                }
            }
            let name = if matches!(event, GatewayEvent::MessageCreate(_)) {
                "MESSAGE_CREATE"
            } else {
                "MESSAGE_UPDATE"
            };
            Some((name.into(), json))
        }
        GatewayEvent::MessageDelete {
            channel_id,
            message_id,
        } => {
            let channel = store::channel_by_id(&conn, *channel_id)?;
            let needed = if channel.guild_id.is_some() {
                INTENT_GUILD_MESSAGES
            } else {
                INTENT_DIRECT_MESSAGES
            };
            if intents & needed == 0 || !visible(&conn, *channel_id, user) {
                return None;
            }
            Some((
                "MESSAGE_DELETE".into(),
                json!({ "id": message_id.to_string(), "channel_id": channel_id.to_string(), "guild_id": channel.guild_id.map(|g| g.to_string()) }),
            ))
        }
        GatewayEvent::TypingStart {
            channel_id,
            user_id,
            ..
        } => {
            let channel = store::channel_by_id(&conn, *channel_id)?;
            let needed = if channel.guild_id.is_some() {
                INTENT_GUILD_MESSAGES
            } else {
                INTENT_DIRECT_MESSAGES
            };
            if intents & needed == 0 {
                return None;
            }
            Some((
                "TYPING_START".into(),
                json!({ "channel_id": channel_id.to_string(), "user_id": user_id.to_string(), "timestamp": iso(now_ms()) }),
            ))
        }
        GatewayEvent::PresenceUpdate { user_id, status } => {
            if intents & INTENT_GUILD_PRESENCES == 0 {
                return None;
            }
            let u = store::user_by_id(&conn, *user_id).unwrap_or_default();
            let status = match status {
                Presence::Online => "online",
                Presence::Idle => "idle",
                Presence::Dnd => "dnd",
                Presence::Offline => "offline",
            };
            Some((
                "PRESENCE_UPDATE".into(),
                json!({
                    "user": user_json(&u),
                    "status": status,
                    "activities": [],
                    "client_status": { "desktop": status }
                }),
            ))
        }
        GatewayEvent::ChannelCreate(c) | GatewayEvent::ChannelUpdate(c) => {
            if intents & INTENT_GUILDS == 0 {
                return None;
            }
            if let Some(g) = c.guild_id {
                if !member(&conn, g, user) {
                    return None;
                }
            }
            let name = if matches!(event, GatewayEvent::ChannelCreate(_)) {
                "CHANNEL_CREATE"
            } else {
                "CHANNEL_UPDATE"
            };
            Some((name.into(), channel_json(c)))
        }
        GatewayEvent::ChannelDelete {
            channel_id,
            guild_id,
        } => {
            if intents & INTENT_GUILDS == 0 {
                return None;
            }
            if let Some(g) = guild_id {
                if !member(&conn, *g, user) {
                    return None;
                }
            }
            Some((
                "CHANNEL_DELETE".into(),
                json!({ "id": channel_id.to_string(), "guild_id": guild_id.map(|g| g.to_string()) }),
            ))
        }
        GatewayEvent::GuildCreate(g) | GatewayEvent::GuildUpdate(g) => {
            if intents & INTENT_GUILDS == 0 || !member(&conn, g.id, user) {
                return None;
            }
            let name = if matches!(event, GatewayEvent::GuildCreate(_)) {
                "GUILD_CREATE"
            } else {
                "GUILD_UPDATE"
            };
            Some((name.into(), guild_json(&conn, g, user)))
        }
        GatewayEvent::GuildDelete { guild_id } => {
            if intents & INTENT_GUILDS == 0 {
                return None;
            }
            Some(("GUILD_DELETE".into(), json!({ "id": guild_id.to_string() })))
        }
        GatewayEvent::RoleCreate(role) | GatewayEvent::RoleUpdate(role) => {
            if intents & INTENT_GUILDS == 0 || !member(&conn, role.guild_id, user) {
                return None;
            }
            let name = if matches!(event, GatewayEvent::RoleCreate(_)) {
                "GUILD_ROLE_CREATE"
            } else {
                "GUILD_ROLE_UPDATE"
            };
            Some((name.into(), serde_json::to_value(role).ok()?))
        }
        GatewayEvent::RoleDelete { guild_id, role_id } => {
            if intents & INTENT_GUILDS == 0 || !member(&conn, *guild_id, user) {
                return None;
            }
            Some((
                "GUILD_ROLE_DELETE".into(),
                json!({
                    "guild_id": guild_id.to_string(),
                    "role_id": role_id.to_string(),
                }),
            ))
        }
        GatewayEvent::MemberUpdate(m) => {
            if intents & INTENT_GUILD_MEMBERS == 0 || !member(&conn, m.guild_id, user) {
                return None;
            }
            Some((
                "GUILD_MEMBER_UPDATE".into(),
                json!({
                    "guild_id": m.guild_id.to_string(),
                    "user": user_json(&m.user),
                    "roles": m.roles.iter().map(|r| r.to_string()).collect::<Vec<_>>(),
                    "nick": m.nickname,
                    "joined_at": iso(m.joined_at)
                }),
            ))
        }
        GatewayEvent::UserUpdate(u) => {
            if u.id != user {
                return None;
            }
            Some(("USER_UPDATE".into(), user_json(u)))
        }
        GatewayEvent::VoiceStateUpdate(v) => {
            if intents & (1 << 7) == 0 || !member(&conn, v.guild_id, user) {
                return None;
            }
            Some((
                "VOICE_STATE_UPDATE".into(),
                json!({
                    "guild_id": v.guild_id.to_string(),
                    "channel_id": v.channel_id.map(|c| c.to_string()),
                    "user_id": v.user_id.to_string(),
                    "self_mute": v.self_mute,
                    "self_deaf": v.self_deaf,
                    "session_id": "local"
                }),
            ))
        }
        GatewayEvent::InteractionCreate { app_id, payload } => {
            let app = store::application_by_id(&conn, *app_id)?;
            if app.bot.id != user {
                return None;
            }
            Some(("INTERACTION_CREATE".into(), payload.clone()))
        }
        GatewayEvent::RelationshipUpdate(r) => Some((
            "RELATIONSHIP_ADD".into(),
            json!({ "id": r.user.id.to_string(), "type": 1, "user": user_json(&r.user) }),
        )),
        GatewayEvent::RelationshipRemove { user_id } => Some((
            "RELATIONSHIP_REMOVE".into(),
            json!({ "id": user_id.to_string() }),
        )),
        GatewayEvent::DmCreate(_) | GatewayEvent::DmUpdate(_) | GatewayEvent::DmRemove { .. } => {
            None
        }
        GatewayEvent::ReadUpdate { .. }
        | GatewayEvent::GuildMembers { .. }
        | GatewayEvent::MemberRemove { .. }
        | GatewayEvent::MemberCountUpdate { .. }
        | GatewayEvent::ReadyCheck
        | GatewayEvent::Ready(_) => None,
    }
}

fn member(conn: &rusqlite::Connection, guild: Id, user: Id) -> bool {
    store::guild_member(conn, guild, user)
}

fn visible(conn: &rusqlite::Connection, channel: Id, user: Id) -> bool {
    let Some(channel) = store::channel_by_id(conn, channel) else {
        return false;
    };
    if channel.guild_id.is_some() {
        store::can_view(conn, channel.id, user)
    } else {
        store::is_dm_member(conn, channel.id, user)
    }
}
