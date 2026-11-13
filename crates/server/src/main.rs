//! Runecord's local server: SQLite, HTTP API, and the WebSocket gateway.

mod api;
mod cache;
mod compat;
mod crypto;
mod gateway_v10;
mod platform;
mod ratelimit;
mod store;

use axum::extract::DefaultBodyLimit;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct AppState {
    pub store: store::SqliteStore,
    pub tx: broadcast::Sender<runecord_protocol::GatewayEvent>,
    pub online: tokio::sync::Mutex<HashMap<i64, usize>>,
    pub data_dir: PathBuf,
    pub limits: ratelimit::RateLimiter,
    pub port: u16,
    pub v10: gateway_v10::Sessions,
    pub voice: std::sync::Mutex<HashMap<i64, runecord_protocol::VoiceState>>,
    pub interactions: std::sync::Mutex<HashMap<String, PendingInteraction>>,
    pub guild_cache: std::sync::Mutex<
        cache::BoundedCache<(i64, i64, i64), std::sync::Arc<runecord_protocol::GuildDetail>>,
    >,
    pub thumb_cache: std::sync::Mutex<cache::BoundedCache<(String, u32), axum::body::Bytes>>,
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

#[tokio::main]
async fn main() {
    let mut port: u16 = 47_800;
    let mut data = PathBuf::from("data");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                if let Some(v) = args.next().and_then(|v| v.parse().ok()) {
                    port = v;
                }
            }
            "--data" => {
                if let Some(v) = args.next() {
                    data = PathBuf::from(v);
                }
            }
            "--help" | "-h" => {
                println!("runecord-server [--port 47800] [--data data]");
                return;
            }
            _ => {}
        }
    }

    if let Err(err) = std::fs::create_dir_all(data.join("uploads")) {
        eprintln!("runecord: could not create data folder: {err}");
        std::process::exit(1);
    }
    let db_path = data.join("runecord.db.enc");
    let key = match crypto::load_key(&data) {
        Ok(k) => k,
        Err(err) => {
            eprintln!("runecord: could not load encryption key: {err}");
            std::process::exit(1);
        }
    };
    let store = match store::SqliteStore::open(&db_path, key) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("runecord: could not open {}: {err}", db_path.display());
            std::process::exit(1);
        }
    };
    let _ = store::seed_demo(&store.conn(), Some("nova"));
    store::seed_demo_app(&store.conn());
    store.flush_now();

    let (tx, _) = broadcast::channel(1024);
    let _ = std::fs::create_dir_all(data.join("thumbs"));
    cache::prune_dir(&data.join("thumbs"), cache::THUMB_DISK_BYTES);
    let state = Arc::new(AppState {
        store,
        tx,
        online: tokio::sync::Mutex::new(HashMap::new()),
        data_dir: data.clone(),
        limits: ratelimit::RateLimiter::new(),
        port,
        v10: gateway_v10::Sessions::new(),
        voice: std::sync::Mutex::new(HashMap::new()),
        interactions: std::sync::Mutex::new(HashMap::new()),
        guild_cache: std::sync::Mutex::new(cache::BoundedCache::new(
            cache::GUILD_CACHE_ENTRIES,
            usize::MAX,
            |_| 1,
        )),
        thumb_cache: std::sync::Mutex::new(cache::BoundedCache::new(
            cache::THUMB_CACHE_ENTRIES,
            cache::THUMB_CACHE_BYTES,
            |bytes| bytes.len(),
        )),
    });

    gateway_v10::spawn_pump(state.clone());
    {
        let store = state.clone();
        tokio::spawn(async move {
            let mut ticks = 0u32;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                store.store.flush_if_changed();
                ticks = ticks.wrapping_add(1);
                if ticks % 150 == 0 {
                    cache::prune_dir(&store.data_dir.join("thumbs"), cache::THUMB_DISK_BYTES);
                }
            }
        });
    }

    let app = api::router(state.clone())
        .nest("/api/v1", compat::router(state.clone()))
        .merge(platform::router(state.clone()))
        .layer(DefaultBodyLimit::max(48 * 1024 * 1024))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            ratelimit::middleware,
        ));

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(err) => {
            eprintln!("runecord: port {port} is unavailable: {err}");
            std::process::exit(1);
        }
    };
    println!("runecord listening on http://{addr}");
    let _ = axum::serve(listener, app).await;
}
