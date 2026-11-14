//! Voice audio relay.
//!
//! Clients connect with their session token and a channel id. Opus frames are
//! relayed to every other participant, prefixed with the sender's user id.

use super::*;
use crate::{AppState, store};
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Deserialize)]
pub(crate) struct VoiceQuery {
    token: String,
    channel: Id,
}

pub(crate) async fn voice_socket(
    State(state): State<Arc<AppState>>,
    Query(query): Query<VoiceQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let conn = state.store.conn();
    let me = store::session_user(&conn, query.token.trim()).ok_or_else(unauthorized)?;
    if !store::can_view(&conn, query.channel, me) {
        return Err(forbidden());
    }
    drop(conn);
    let channel = query.channel;
    Ok(ws.on_upgrade(move |socket| voice_loop(socket, state, me, channel)))
}

async fn voice_loop(socket: WebSocket, state: Arc<AppState>, me: Id, channel: Id) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    {
        let mut rooms = state.voice_rooms.lock().await;
        rooms.entry(channel).or_default().push((me, tx));
    }
    let sender = tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if sink.send(WsMessage::Binary(data.into())).await.is_err() {
                break;
            }
        }
    });
    while let Some(Ok(message)) = stream.next().await {
        let WsMessage::Binary(data) = message else {
            continue;
        };
        let mut frame = Vec::with_capacity(8 + data.len());
        frame.extend_from_slice(&me.to_le_bytes());
        frame.extend_from_slice(&data);
        let rooms = state.voice_rooms.lock().await;
        if let Some(peers) = rooms.get(&channel) {
            for (user, peer) in peers {
                if *user != me {
                    let _ = peer.send(frame.clone());
                }
            }
        }
    }
    sender.abort();
    let mut rooms = state.voice_rooms.lock().await;
    if let Some(peers) = rooms.get_mut(&channel) {
        peers.retain(|(user, _)| *user != me);
        if peers.is_empty() {
            rooms.remove(&channel);
        }
    }
}
