//! Runecord's local server: SQLite, HTTP API, and the WebSocket gateway.

mod api;
mod compat;
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
    let db_path = data.join("runecord.db");
    let store = match store::SqliteStore::open(&db_path) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("runecord: could not open {}: {err}", db_path.display());
            std::process::exit(1);
        }
    };
    let _ = store::seed_demo(&store.conn(), Some("nova"));

    let (tx, _) = broadcast::channel(1024);
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
    });

    gateway_v10::spawn_pump(state.clone());

    let app = api::router(state.clone())
        .nest("/api/v10", compat::router(state.clone()))
        .merge(platform::router(state.clone()))
        .nest_service("/uploads", tower_http::services::ServeDir::new(api::uploads_dir(&data)))
        .nest_service("/cdn", tower_http::services::ServeDir::new(data.join("cdn")))
        .layer(DefaultBodyLimit::max(48 * 1024 * 1024))
        .layer(axum::middleware::from_fn_with_state(state.clone(), ratelimit::middleware));

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
