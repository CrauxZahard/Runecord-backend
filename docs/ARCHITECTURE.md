# Architecture

`Runecord-backend` is the standalone service behind the Rune client in
`..\Runecord`. It owns accounts, servers, channels, messages, roles, DMs,
applications and media, and exposes three surfaces over one axum server:

| Surface | Prefix | Purpose |
|---|---|---|
| Native API | `/api/*` | what the Rune client uses |
| Compatibility API | `/api/v1/*` | Discord-shaped routes for bots and tooling |
| Platform | `/api/applications`, `/api/webhooks`, `/interactions` | apps, webhooks, interactions |
| Gateway | `/gateway`, `/gateway/v1`, `/voice` | live events, Discord-style sessions, voice relay |

```text
crates/
  protocol/            wire + domain types, no axum or SQLite
  server/
    src/lib.rs         launcher, log setup
    src/bootstrap.rs   config → storage → state → routers → listener
    src/config.rs      CLI parsing (typed errors, tested)
    src/state.rs       AppState: concrete services shared by handlers
    src/api/           native HTTP + websocket surface, one file per feature
    src/compat.rs      Discord-compatible /api/v1 surface
    src/platform.rs    applications, webhooks, interactions, voice signaling
    src/gateway_v10.rs Discord-style gateway sessions and event pump
    src/media.rs       MediaStore: hashing, encryption, CDN and thumbnails
    src/cache.rs       bounded memory/disk caches and pruning
    src/crypto.rs      key load, seal/open
    src/ratelimit.rs   token-bucket middleware
    src/store/
      mod.rs           Database: connection, key, dirty tracking, flush
      sqlite/          SQL grouped by domain, plus the store test suite
migrations/            numbered SQL applied at startup
data/                  runtime database, uploads, thumbs (gitignored)
```

## Startup

`lib.rs` installs the tracing subscriber and builds the Tokio runtime.
`bootstrap.rs` then:

1. creates `uploads/` and `thumbs/`, loads or creates the encryption key;
2. opens the encrypted SQLite database;
3. optionally seeds demo data;
4. builds `AppState` (database, media store, broadcast channel, caches,
   rate limiter, gateway sessions, voice rooms, pending interactions);
5. builds the routers, applies the body limit and rate-limit middleware;
6. binds the listener and serves.

A background task flushes the database every two seconds when it is dirty and
prunes the thumbnail directory periodically.

## Request path

```text
request → ratelimit middleware → router (api / compat / platform)
        → auth helper (session token or bot token)
        → permission check
        → store function (SQL)
        → JSON response
        → optional broadcast::Sender<GatewayEvent>
```

Handlers own transport concerns, policy checks and event emission. They do not
embed SQL: persistence lives in `store/sqlite/<domain>.rs` and takes
`&rusqlite::Connection`. `AppState::store` is the concrete `Database`; its
`conn()` is used for connection-scoped work.

The gateway subscribes to the broadcast channel and filters events per
recipient using shared-guild and relationship checks, so each socket sees only
what its user may see.

## Storage

`store::Database` owns:

- the connection and the AES key used for the database and media;
- a dirty flag and `flush_if_changed`/`flush_now` used by the background task
  and shutdown paths;
- the schema entry point in `store/sqlite/mod.rs`.

SQL is grouped by domain: `auth`, `users`, `guilds`, `channels`, `messages`,
`roles`, `members`, `invites`, `events`, `inbox`, `social`, `dms`, `platform`,
`notifications`, `permissions`, `privacy`, `boosts`, `discovery`, `media`,
`moderation`, `model` (row mappers) and `seed`. `tests.rs` exercises these
against in-memory databases.

Media is content-addressed and encrypted at rest. `media::MediaStore` writes
sealed files under `data/cdn` and `data/thumbs`; handlers pass bytes and get a
`StoredAsset` with hash, size and URL. Metadata (`cdn_assets`, attachments,
icons) is still recorded through the store so lookups and cleanup stay
consistent.

## Protocol crate

`runecord-protocol` contains only data: users, sessions, guilds, channels,
messages, relationships, requests, gateway events and permission flags. It has
no axum, rusqlite or filesystem dependency. Both the server and the client
compile against it, so a payload change is visible to both at compile time.

## Adding a feature

1. Add or extend protocol types in `crates/protocol/src`.
2. Add SQL in `store/sqlite/<domain>.rs`; if the shape changes, add a numbered
   migration.
3. Add the handler in `src/api/<feature>.rs` (or `compat.rs`/`platform.rs`),
   and register the route.
4. Broadcast a `GatewayEvent` when other clients must update live.
5. Add a store test for new persistence behaviour and a config/parser test for
   new input handling.
6. Keep the compatibility surface's payloads Discord-shaped; do not reshape it
   around native API changes.

See `docs/DECISIONS.md` for boundary rationale and `docs/CONTRIBUTING.md` for
verification commands.
