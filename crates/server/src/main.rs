//! Runecord's local server: SQLite, HTTP API, and the WebSocket gateway.

mod api;
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
    });

    let app = api::router(state)
        .nest_service("/uploads", tower_http::services::ServeDir::new(api::uploads_dir(&data)))
        .layer(DefaultBodyLimit::max(48 * 1024 * 1024));

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
