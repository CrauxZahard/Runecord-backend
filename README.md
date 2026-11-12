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

### Discord-compatible `/api/v10`

Mounted alongside the native API; same auth (`Bot` or `Bearer`), Discord-shaped payloads
(string IDs, nested `author`, ISO timestamps, `type`, `reactions[].emoji`):

```
GET    /api/v10/users/@me
GET    /api/v10/users/@me/guilds
GET    /api/v10/users/{id}
GET    /api/v10/guilds/{guild_id}
GET    /api/v10/guilds/{guild_id}/channels
GET    /api/v10/guilds/{guild_id}/members
PUT    /api/v10/guilds/{guild_id}/members/{user_id}/roles/{role_id}
DELETE /api/v10/guilds/{guild_id}/members/{user_id}/roles/{role_id}
GET    /api/v10/channels/{channel_id}
GET    /api/v10/channels/{channel_id}/messages?before=&after=&around=&limit=
POST   /api/v10/channels/{channel_id}/messages
GET    /api/v10/channels/{channel_id}/messages/{message_id}
PATCH  /api/v10/channels/{channel_id}/messages/{message_id}
DELETE /api/v10/channels/{channel_id}/messages/{message_id}
PUT    /api/v10/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/@me
DELETE /api/v10/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/@me
GET    /api/v10/channels/{channel_id}/pins
PUT    /api/v10/channels/{channel_id}/pins/{message_id}
DELETE /api/v10/channels/{channel_id}/pins/{message_id}
POST   /api/v10/channels/{channel_id}/typing
GET    /api/v10/gateway
GET    /api/v10/gateway/bot
```

`POST .../messages` accepts `{content, nonce?, message_reference?: {message_id}}`.
Errors use `{code, message}` with Discord codes (`10003 Unknown Channel`, `10008 Unknown Message`,
`50001 Missing Access`, `50035 Invalid Form Body`).

The gateway URL still speaks Runecord's native `{op, d}` envelope. Discord gateway opcodes,
intents and RESUME are the Phase 2 item.

## Phase plan

1. Repo split, migrations, Store boundary, reseed. **(this commit)**
2. Postgres adapter, gateway RESUME + sequence numbers, full handler migration.
3. Postgres `LISTEN/NOTIFY` fan-out, Docker Compose topology (Postgres + two replicas + proxy), client disk/media caches.
