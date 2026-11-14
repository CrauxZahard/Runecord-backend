# Runecord Backend

Standalone Rust service for Runecord: HTTP API, WebSocket gateway, SQLite storage and uploads. The Rune client in `..\Runecord` consumes `crates/protocol` by path.

## Layout

```
crates/protocol      wire + domain types shared with the client
crates/server        axum routes, gateway, workers
crates/server/src
  lib.rs             launcher and tracing setup
  bootstrap.rs       config → storage → state → routers → listener
  config.rs          CLI parsing (typed errors, unit tested)
  state.rs           AppState shared by handlers
  media.rs           MediaStore: hashing, encryption, CDN and thumbnails
  compat.rs          Discord-compatible /api/v1 surface
  platform.rs        applications, webhooks, interactions, voice signaling
  gateway_v10.rs     Discord-style gateway sessions and event pump
  cache.rs           bounded memory/disk caches and pruning
  crypto.rs          key load, seal/open
  ratelimit.rs       token-bucket middleware
  api/
    mod.rs           router, auth helpers, errors
    auth.rs          register/login/logout
    users.rs         profiles, activity, effects, nameplates, widgets, read state
    guilds.rs        guild detail and guild CRUD
    members.rs       paginated members and member updates
    channels.rs      channels, overwrites, threads
    messages.rs      messages, reactions, pins, search
    media.rs         uploads and the CDN/thumbnail path
    roles.rs         role CRUD, icons and links
    events.rs        server scheduled events
    invites.rs       invite create/info/join
    social.rs        friends and DMs
    bots.rs          bot account CRUD
    gateway.rs       websocket loop and event filtering
    misc.rs          cache stats and small helpers
  store/
    mod.rs           Database: connection, key, dirty tracking, flush
    sqlite/mod.rs    schema and connection setup
    sqlite/model.rs  row mappers and message assembly
    sqlite/*.rs      SQL grouped by domain: guilds, messages, roles, members,
                     channels, invites, events, inbox, social, dms, users,
                     platform, notifications, permissions, privacy, boosts,
                     discovery, media, moderation, auth, seed
    sqlite/tests.rs  behavioural store suite
migrations/          numbered SQL applied at startup
data/                runtime database, cdn and thumbs (gitignored)
```

See `docs/ARCHITECTURE.md` for the request path and module responsibilities.

## Run

```bat
cargo run --release -p runecord-server -- --data data
```

Defaults to `127.0.0.1:47800`. `--host`, `--port` and `--data` override. Logging uses
`tracing`; set `RUST_LOG` for more detail (`RUST_LOG=debug`), the default is `info`.

The first start creates `data/key.bin` and `data/runecord.db.enc`, applies `migrations/`, and seeds the demo accounts (`nova`, `maya`, `leo`, `sam`, `ana`, `priya`, `tom`, `runebot`; password `runecord`). Pass `--no-demo` on a public deployment to skip those accounts and start with an empty database.

## Self-hosting

```bat
runecord-server --host 0.0.0.0 --port 47800 --data data --no-demo
```

`--host 0.0.0.0` listens on every interface. Put a TLS reverse proxy in front of the port and point the client's **User Settings → Application → Backend Server** at the public `https://` URL. The reverse proxy must forward ordinary HTTP and the WebSocket upgrade for `/gateway` and `/voice` on the same host and port; the client derives `ws://` or `wss://` from the configured URL. The demo seed is off with `--no-demo`, so register the first account against the empty server instead of shipping known credentials.

## Encryption at rest

- `data/key.bin`: 32-byte master key generated on first run. Keep it — losing it makes the data unreadable.
- `data/runecord.db.enc`: the whole database is serialized and AES-256-GCM encrypted, then rewritten within two seconds of a write (and immediately on seed and shutdown paths). SQLite runs in memory; no plaintext database file is written.
- CDN files are AES-256-GCM encrypted on disk and decrypted by `GET /cdn/{file}`.
- Session tokens, bot tokens and webhook tokens are stored as SHA-256 hashes.
- Threat model: protects data at rest (stolen disk or backup). A process running as your user can read `key.bin`; wrap it with DPAPI/OS keychain for stronger isolation.

## Storage

`store::Database` owns the in-memory SQLite connection, the encryption key, dirty tracking and the flush path. SQL lives in `store/sqlite/*`, grouped by domain; API handlers call those functions instead of embedding queries. There is no storage trait: a second backend is not implemented, so the concrete type is the honest boundary. See `docs/DECISIONS.md`.

## Development

Toolchain `1.99.1` is pinned. Both this workspace and `..\Runecord` must pass:

```bat
cargo fmt --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace
```

Startup/config tests live beside their modules; the behavioural store suite is `crates/server/src/store/sqlite/tests.rs`. See `docs/ARCHITECTURE.md` for the module map, `docs/CONTRIBUTING.md` for workflow and `docs/DECISIONS.md` for boundary rationale.

## Caching

There is no external cache and no Redis. One process serves accounts on loopback, so the cache is an in-process, bounded map (`src/cache.rs`), and the database already lives in memory.

- **Guild detail** (`GET /api/guilds/{id}`): LRU of 256 entries keyed by `(guild, user, generation)`, where generation is SQLite's monotonic `total_changes()`. Any write invalidates every entry by construction; no timer or manual invalidation path exists.
- **Thumbnails**: decoded 480 px PNG bytes are kept in a 32 MB / 256-entry LRU, so repeated `?w=480` reads skip the disk read, AES decrypt and image decode.
- **Disk thumbnails**: `data/thumbs` is pruned to 512 MB by age at startup and every five minutes.
- **CDN originals**: content-addressed and immutable, served with `Cache-Control: max-age=31536000, immutable`.

`GET /api/cache` returns entries, weight, hits and misses for both caches. Redis would only become necessary when more than one server process must share cache state; that is the multi-node phase, not the local design.

## API surface

`POST /api/register`, `/api/login`, `/api/logout` · `GET /api/bootstrap` · `PATCH /api/me` · profile activity, artwork, connections, effects, nameplates and widgets · `POST /api/me/password` · `POST /api/me/privacy` · `GET /api/users/lookup?q=` · `GET /api/users/{id}/mutuals` · `POST/DELETE /api/me/avatar` · `POST/DELETE /api/me/banner` · guilds/channels/messages/reactions/pins/threads · server overview edit · server icon and banner upload · server boosts with level-based upload and voice limits · kick/ban/timeout, ban list, unban and ban appeals · prune preview and apply · leave server · channel notification levels and @everyone suppression · server notification settings and overrides · public server directory and join · server discoverability · friends/DMs · inbox mentions and unreads · server events with create/list/delete · group DMs with rename, icon, member add/remove and leave · invites · roles with icons and links · bots · applications/install links/commands · guild commands · interactions · guild emojis · audit logs · uploads · `GET /gateway` and `/voice` (WebSockets).

### Bot accounts

Human sessions manage bots; bots authenticate with their own token.

| Method | Path | Who | Purpose |
|---|---|---|---|
| `POST` | `/api/bots` | human session | `{name, guild_id?}` → `{bot, token}`; token is shown once |
| `GET` | `/api/bots` | human session | bots you own |
| `DELETE` | `/api/bots/{id}` | human session | revoke the token and remove the bot from guilds |

Bot token format: `rcbot.<64 hex>`. Send it as:

```
Authorization: Bot rcbot.…
```

Bots are normal `users` rows with `is_bot=1`, so permissions, mentions and message authors work unchanged. Tokens are stored as SHA-256 hashes. `POST /api/bots` with `guild_id` requires `MANAGE_GUILD` and adds the bot as a member.

### Rate limits

Discord-style buckets per actor (token) + route + major id, plus a 50 req/s global bucket.
Defaults: 5/60s login/register, 5/5s message sends and edits, 5/5s reactions, 10/60s
uploads, 2/60s bot creation, 60/60s reads.

Every response carries:

```
X-RateLimit-Limit, X-RateLimit-Remaining, X-RateLimit-Reset,
X-RateLimit-Reset-After, X-RateLimit-Bucket
```

A limited request returns `429` with `Retry-After`, `X-RateLimit-Global` (when global) and:

```json
{ "code": 0, "message": "You are being rate limited.", "retry_after": 4.746, "global": false }
```

### Discord-compatible `/api/v1`

Mounted alongside the native API; same auth (`Bot` or `Bearer`), Discord-shaped payloads
(string IDs, nested `author`, ISO timestamps, `type`, `reactions[].emoji`):

```
GET    /api/v1/users/@me
GET    /api/v1/users/@me/guilds
GET    /api/v1/users/{id}
GET    /api/v1/guilds/{guild_id}
GET    /api/v1/guilds/{guild_id}/channels
GET    /api/v1/guilds/{guild_id}/members
PUT    /api/v1/guilds/{guild_id}/members/{user_id}/roles/{role_id}
DELETE /api/v1/guilds/{guild_id}/members/{user_id}/roles/{role_id}
GET    /api/v1/channels/{channel_id}
GET    /api/v1/channels/{channel_id}/messages?before=&after=&around=&limit=
POST   /api/v1/channels/{channel_id}/messages
GET    /api/v1/channels/{channel_id}/messages/{message_id}
PATCH  /api/v1/channels/{channel_id}/messages/{message_id}
DELETE /api/v1/channels/{channel_id}/messages/{message_id}
PUT    /api/v1/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/@me
DELETE /api/v1/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/@me
GET    /api/v1/channels/{channel_id}/pins
PUT    /api/v1/channels/{channel_id}/pins/{message_id}
DELETE /api/v1/channels/{channel_id}/pins/{message_id}
POST   /api/v1/channels/{channel_id}/typing
GET    /api/v1/gateway
GET    /api/v1/gateway/bot
```

`POST .../messages` accepts `{content, nonce?, message_reference?: {message_id}}`.
Errors use `{code, message}` with Discord codes (`10003 Unknown Channel`, `10008 Unknown Message`,
`50001 Missing Access`, `50035 Invalid Form Body`).

The gateway URL still speaks Runecord's native `{op, d}` envelope at `/gateway`. Discord
opcodes live at `/gateway/v1`.

### Gateway v10

`ws://127.0.0.1:47800/gateway/v1` implements the Discord handshake:

| Op | Direction | Payload |
|---|---|---|
| `10` HELLO | server | `{heartbeat_interval: 41250}` |
| `2` IDENTIFY | client | `{token: "Bot rcbot.…", intents}` |
| `1` HEARTBEAT | client | sequence number or `null` → `11 HEARTBEAT_ACK` |
| `6` RESUME | client | `{token, session_id, seq}` → replay + `RESUMED` |
| `0` DISPATCH | server | `{t, s, d}` events |
| `9` INVALID_SESSION | server | `false` |

Intents: `GUILDS`, `GUILD_MEMBERS`, `GUILD_PRESENCES`, `GUILD_MESSAGES`,
`DIRECT_MESSAGES`, `MESSAGE_CONTENT` (omit `intents` to receive all).
Sessions live in memory for 5 minutes with a 1024-event replay buffer; a central pump
buffers events even while the socket is closed. Event payloads use the same
Discord shapes as `/api/v1`.

Probe it with:

```bat
node scripts\gw_probe.mjs <bot-token> 100 211
```

It checks HELLO, READY, heartbeat ACK, a live dispatch, and a resume that replays a
message sent while the socket was offline.

## Phase plan

1. Repo split, migrations, Store boundary, reseed. ✅
2. **Phase A (done):** Discord /gateway/v1 with HELLO/IDENTIFY/READY/HEARTBEAT/RESUME, intents, replay buffer, bot presence.
3. **Phase B (done, API):** applications + install links, application commands/interactions, webhooks, audit log, content-addressed CDN.
4. **Phase C (partial):** emoji/sticker backend, thread/forum APIs and client thread browser, voice signaling + participant panel. Remaining: bots/apps/audit settings UI, custom-emoji picker/rendering, slash-command invocation UI, multiline composer.
