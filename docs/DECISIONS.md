# Decisions

Short records of boundaries that are easy to undo by accident. Client-side
decisions (UI layering, cache, voice) are recorded in
`..\Runecord\docs\DECISIONS.md`.

## 1. `runecord-protocol` is a data-only crate

**Context.** Wire types and server-only code were entangled, and the client
had to depend on the server crate to know a payload.

**Decision.** Keep protocol types in their own crate with no axum, rusqlite or
filesystem dependency. The client consumes it by sibling path.

**Consequences.** Payload changes are compile-time visible to both processes.
The crate must stay free of behaviour; helper functions on types are allowed,
I/O is not.

## 2. Two crates, module boundaries inside the server

**Context.** Splitting the server into domain/application/infrastructure
crates would add a dependency graph for a single-process local service.

**Decision.** `runecord-protocol` + `runecord-server`, with internal modules:
`bootstrap`, `config`, `state`, `api`, `compat`, `platform`, `gateway_v10`,
`store`, `media`, `cache`, `crypto`, `ratelimit`.

**Consequences.** Refactoring and compile times stay cheap. The store and media
seams already exist; promote them to crates if a second backend or transport
appears.

## 3. A concrete `Database`, not a storage trait

**Context.** An unused `Store` trait advertised a future Postgres adapter with
no second implementation and no callers beyond a shim.

**Decision.** Delete the trait. `store::Database` is concrete: connection, key,
dirty tracking and flush. SQL lives in domain-grouped `store/sqlite/*`
functions over `&Connection`.

**Consequences.** One real API, enforced by the compiler. Add a trait when a
second implementation exists, not before.

## 4. SQL belongs to the store

**Context.** Handlers embedded queries, duplicating row mapping and mixing
policy with persistence.

**Decision.** Handlers keep HTTP, permissions and gateway broadcasts; SQL moves
to `store/sqlite/<domain>.rs`.

**Consequences.** Queries are testable without an HTTP server, and changes
happen once. A handler needing a new query adds a named store function.

## 5. `MediaStore` owns encrypted files

**Context.** Uploads, emoji, role icons, guild and user images each repeated
hashing, extension handling, directory creation and AES sealing.

**Decision.** `media::MediaStore` provides `save`, `save_with_ext`, `read`,
`read_thumb` and `write_thumb`; handlers pass bytes and receive a
`StoredAsset`.

**Consequences.** Encrypt-on-write cannot drift between features. Metadata
writes (`cdn_assets`, attachments) remain explicit store calls.

## 6. Durability is explicit, not implicit

**Context.** The database is encrypted and flushed by a background task; a
naive per-request flush would stall handlers, and no flush would lose data on
crash.

**Decision.** Keep the dirty flag and two-second flush loop, plus `flush_now`
for paths that must persist immediately (seeding, shutdown). Request handlers
mutate through the shared connection; the flush task serialises when the data
changes.

**Consequences.** Request latency stays low while data lands on disk within a
bounded window. Any future change to flushing needs its own decision record and
a crash-safety review.

## 7. Typed errors at their own boundary

**Context.** HTTP handlers already return `ApiError` (status + message) and
the compatibility surface returns `DiscordError` (status + code + message),
while configuration parsed into `String` errors.

**Decision.** Add `thiserror` for `config::ConfigError`. Keep the two HTTP
error types; keep `rusqlite::Error` in the store.

**Consequences.** Each layer translates failures where it has the information
to do so. Do not collapse everything into one enum.

## 8. Structured logging with one subscriber

**Context.** Startup and fatal errors used `println!`/`eprintln!` with no
level control.

**Decision.** `lib.rs` installs a `tracing-subscriber` with an `EnvFilter`
(`RUST_LOG`, default `info`). Bootstrap failures log at `error`, the listening
line at `info`.

**Consequences.** Operators can raise verbosity without rebuilding, and logs
identify their module. Request-level spans can be added incrementally.

## 9. The compatibility surface is a facade

**Context.** Bots and tooling expect Discord-shaped routes and payloads, while
the native client uses a smaller, cleaner API.

**Decision.** Keep `/api/v1` and `compat.rs` as a translation layer over the
same store and gateway; do not reshape the native API to match Discord, and do
not let Discord response shapes leak into protocol types.

**Consequences.** Compatibility fixes stay local to `compat.rs`. Shared
semantics (permissions, message creation) live in the store so both surfaces
behave the same.

## 10. Clippy and tests are part of the build contract

**Context.** Warnings accumulated until `-D warnings` was unusable.

**Decision.** Both workspaces must pass
`cargo clippy --workspace --all-targets --all-features -- -D warnings`, and the
store suite must keep passing.

**Consequences.** Fix lints instead of ignoring them; annotate intentional dead
code and the few argument-heavy mapping functions with a specific reason.
Tests are extended, never removed or relaxed.

## 11. Preserve behaviour while restructuring

**Context.** The refactor touched every module.

**Decision.** Moves preserve routes, payloads, schema, file formats and
user-visible behaviour. Behaviour changes are separate commits with their own
tests.

**Consequences.** Structural commits can be reviewed as moves, and a regression
points at one behavioural change rather than a thousand-line diff.
