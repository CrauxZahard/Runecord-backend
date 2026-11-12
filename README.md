# Runecord Backend

Standalone Rust service for Runecord: HTTP API, WebSocket gateway, SQLite storage and uploads. The Rune client in `..\Runecord` consumes `crates/protocol` by path.

## Layout

```
crates/protocol      wire + domain types shared with the client
crates/server        axum routes, gateway, workers
crates/server/src/store
  mod.rs             Store trait + SqliteStore boundary
  sqlite.rs          SQLite adapter, permissions, seed
migrations/          numbered SQL applied at startup
data/                runtime database + uploads (gitignored)
```

## Run

```bat
cargo run --release -p runecord-server -- --data data
```

Defaults to `127.0.0.1:47800`. `--port` and `--data` override.

The first start creates `data/runecord.db`, applies `migrations/`, and seeds the demo accounts (`nova`, `maya`, `leo`, `sam`, `ana`, `priya`, `tom`, `runebot`; password `runecord`).

## Storage

Phase 1 keeps SQLite (WAL) behind `store::Store`, with SQLite queries in `store::sqlite`. Phase 2 adds a Postgres adapter behind the same trait and moves the remaining direct `store.conn()` handler queries into trait methods. Postgres is the intended production adapter; SQLite stays the zero-install local adapter.

## API surface

`POST /api/register`, `/api/login`, `/api/logout` · `GET /api/bootstrap` · `PATCH /api/me` · guilds/channels/messages/reactions/pins/threads · friends/DMs · invites · roles · uploads · `GET /gateway` (WebSocket).

## Phase plan

1. Repo split, migrations, Store boundary, reseed. **(this commit)**
2. Postgres adapter, gateway RESUME + sequence numbers, full handler migration.
3. Postgres `LISTEN/NOTIFY` fan-out, Docker Compose topology (Postgres + two replicas + proxy), client disk/media caches.
