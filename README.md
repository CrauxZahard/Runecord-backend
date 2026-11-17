# Runecord Backend

This Rust service runs the Runecord HTTP API, WebSocket gateway, and SQLite store. The Rune client in `..\Runecord` uses `crates/protocol` by path.

## Layout

```
crates\protocol   Shared wire and domain types.
crates\server     axum routes, gateway, workers, and store.
migrations\       Numbered SQL files. The server applies them at startup.
data\             Runtime database and media. Gitignored.
```

`docs/ARCHITECTURE.md` maps the modules. Handlers call `store/sqlite/*` functions. They do not embed SQL. `MediaStore` owns file hashing, encryption, and thumbnails.

## Run

```
cargo run --release -p runecord-server -- --data data
```

The server listens on `127.0.0.1:47800`. Use `--host`, `--port`, and `--data` to change these values. Set `RUST_LOG=debug` for more log detail. The default is `info`.

The first start creates `data/key.bin` and `data/runecord.db.enc`. It then seeds demo accounts. Use password `runecord`. Add `--no-demo` to skip the seed.

## Self-hosting

```
runecord-server --host 0.0.0.0 --data data --no-demo
```

Put a TLS reverse proxy in front of the port. Forward HTTP and the WebSocket upgrades for `/gateway` and `/voice`. Register the first account after the seed is off.

Gate signup before you expose the port:

```
runecord-server --host 0.0.0.0 --signup code --signup-code CODE
runecord-server --host 0.0.0.0 --signup closed
```

Login and registration allow 5 attempts per minute per IP. Add `--trust-proxy` only behind a trusted proxy.

## Encryption at rest

- `data/key.bin`: 32-byte key. Back it up. Lost keys make data unreadable.
- `data/runecord.db.enc`: AES-256-GCM encrypted database. SQLite runs in memory. No plaintext file is written.
- CDN files: AES-256-GCM encrypted on disk.
- Tokens: stored as SHA-256 hashes.
- The design protects a stolen disk or backup. A process running as your user can read the key.

## Caching

There is no Redis. One process serves users on loopback.

- Guild detail: LRU of 256 entries keyed by `(guild, user, generation)`. Any write invalidates all entries.
- Thumbnails: 32 MB / 256-entry LRU of decoded 480 px PNG bytes.
- Disk thumbnails: `data/thumbs` is pruned to 512 MB.
- CDN originals: content-addressed and immutable.

`GET /api/cache` reports entries, weight, hits, and misses.

## API

The native API uses `POST /api/register`, `/api/login`, and `/api/logout`. It also serves `/api/bootstrap`, `/api/me`, guilds, channels, messages, roles, events, invites, social, DMs, bots, interactions, emojis, audit logs, and uploads. WebSockets live at `/gateway` and `/voice`.

Bot tokens use `rcbot.<64 hex>` with `Authorization: Bot`. Human sessions manage bots. Tokens are shown once and stored as hashes.

Rate limits use route buckets and a 50 req/s global bucket. Each response carries `X-RateLimit-*` headers. A limited request returns `429` with `Retry-After` and a JSON body.

`/api/v1` and `/api/gateway/v1` offer compatibility payloads and opcodes for external clients. Errors use `{code, message}`.

## Develop

Toolchain 1.99.1 is pinned. Run these commands here and in `..\Runecord`:

```
cargo fmt --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace
```

Read `docs/ARCHITECTURE.md`, `docs/CONTRIBUTING.md`, and `docs/DECISIONS.md` for details.
