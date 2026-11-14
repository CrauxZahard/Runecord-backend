//! Shared application state.
//!
//! Handlers currently receive this concrete state. The architecture refactor
//! moves feature services behind it in stages; the type keeps its existing name
//! and fields during that migration so behavior stays unchanged.

use crate::cache;
use crate::gateway_v10;
use crate::ratelimit;
use crate::store;
use runecord_protocol::{GatewayEvent, GuildDetail, VoiceState};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::broadcast;

pub struct AppState {
    pub store: store::Database,
    pub media: crate::media::MediaStore,
    pub tx: broadcast::Sender<GatewayEvent>,
    pub online: tokio::sync::Mutex<HashMap<i64, usize>>,
    pub data_dir: PathBuf,
    pub limits: ratelimit::RateLimiter,
    pub port: u16,
    pub v10: gateway_v10::Sessions,
    pub voice: std::sync::Mutex<HashMap<i64, VoiceState>>,
    pub voice_rooms:
        tokio::sync::Mutex<HashMap<i64, Vec<(i64, tokio::sync::mpsc::UnboundedSender<Vec<u8>>)>>>,
    pub interactions: std::sync::Mutex<HashMap<String, PendingInteraction>>,
    pub guild_cache:
        std::sync::Mutex<cache::BoundedCache<(i64, i64, i64), std::sync::Arc<GuildDetail>>>,
    pub thumb_cache:
        std::sync::Mutex<cache::BoundedCache<(String, u32), axum::body::Bytes>>,
}

#[derive(Clone)]
pub struct PendingInteraction {
    pub app_id: i64,
    pub bot_id: i64,
    pub channel_id: i64,
    pub user_id: i64,
    pub token: String,
    pub created_at: i64,
}
