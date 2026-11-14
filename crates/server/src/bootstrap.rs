//! Server bootstrap and process wiring.
//!
//! This is the only module that knows how configuration, storage, caches,
//! background tasks, routers and the listener fit together.

use crate::AppState;
use crate::api;
use crate::cache;
use crate::compat;
use crate::config::Config;
use crate::crypto;
use crate::gateway_v10;
use crate::platform;
use crate::ratelimit;
use crate::store;
use axum::extract::DefaultBodyLimit;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

pub async fn run(config: Config) {
    let Config {
        host,
        port,
        data_dir,
        demo_seed,
    } = config;

    if let Err(err) = std::fs::create_dir_all(data_dir.join("uploads")) {
        eprintln!("runecord: could not create data folder: {err}");
        std::process::exit(1);
    }

    let db_path = data_dir.join("runecord.db.enc");
    let key = match crypto::load_key(&data_dir) {
        Ok(key) => key,
        Err(err) => {
            eprintln!("runecord: could not load encryption key: {err}");
            std::process::exit(1);
        }
    };

    let store = match store::Database::open(&db_path, key) {
        Ok(store) => store,
        Err(err) => {
            eprintln!("runecord: could not open {}: {err}", db_path.display());
            std::process::exit(1);
        }
    };

    if demo_seed {
        let _ = store::seed_demo(&store.conn(), Some("nova"));
        store::seed_demo_app(&store.conn());
        store::seed_public_guild(&store.conn());
        store.flush_now();
    }

    let (tx, _) = broadcast::channel(1024);
    let _ = std::fs::create_dir_all(data_dir.join("thumbs"));
    cache::prune_dir(&data_dir.join("thumbs"), cache::THUMB_DISK_BYTES);

    let media = crate::media::MediaStore::new(data_dir.clone(), store.key());

    let state = Arc::new(AppState {
        store,
        media,
        tx,
        online: tokio::sync::Mutex::new(HashMap::new()),
        data_dir: data_dir.clone(),
        limits: ratelimit::RateLimiter::new(),
        port,
        v10: gateway_v10::Sessions::new(),
        voice: std::sync::Mutex::new(HashMap::new()),
        voice_rooms: tokio::sync::Mutex::new(HashMap::new()),
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
    spawn_flush_task(state.clone());

    let app = api::router(state.clone())
        .nest("/api/v1", compat::router(state.clone()))
        .merge(platform::router(state.clone()))
        .layer(DefaultBodyLimit::max(128 * 1024 * 1024))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            ratelimit::middleware,
        ));

    let addr = std::net::SocketAddr::new(host, port);
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("runecord: port {port} is unavailable: {err}");
            std::process::exit(1);
        }
    };

    println!("runecord listening on http://{addr}");
    if let Err(err) = axum::serve(listener, app).await {
        eprintln!("runecord: server stopped with an error: {err}");
    }
}

fn spawn_flush_task(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut ticks = 0u32;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            state.store.flush_if_changed();
            ticks = ticks.wrapping_add(1);
            if ticks.is_multiple_of(150) {
                cache::prune_dir(
                    &state.data_dir.join("thumbs"),
                    cache::THUMB_DISK_BYTES,
                );
            }
        }
    });
}
