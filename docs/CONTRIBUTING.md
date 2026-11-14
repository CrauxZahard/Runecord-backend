# Contributing

This repository is the Runecord server and the shared protocol crate. The Rune
client lives in `..\Runecord` and consumes `crates/protocol` by path, so keep
both repositories side by side.

## Toolchain

The workspace pins Rust `1.99.1` in `rust-toolchain.toml` with `rustfmt` and
`clippy`:

```bat
rustup toolchain install 1.99.1 --component rustfmt clippy
```

## Run it

```bat
run-server.bat
cargo run -p runecord-server -- --host 127.0.0.1 --port 47800 --data data
cargo run -p runecord-server -- --help
```

Flags: `--host`, `--port`, `--data`, `--no-demo`, `--help`. Logging uses
`tracing`; set `RUST_LOG` to raise verbosity (`RUST_LOG=debug`), the default is
`info`.

The first run creates `data/`, an encryption key and an encrypted SQLite
database, and seeds demo accounts unless `--no-demo` is passed. Do not commit
`data/`.

## Verify before committing

```bat
cargo fmt --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace
```

`crates/server/src/store/sqlite/tests.rs` is the behavioural suite: it builds
in-memory databases and covers permissions, threads, moderation, invites,
notifications, discovery, privacy, profile data and pagination. Add a test
there when you change SQL or permission logic. Add a unit test in the relevant
module for parsing and pure helpers. Do not delete or weaken tests to make a
change pass.

## Style

- `cargo fmt` is the formatter; do not hand-format.
- Keep the tree warning-free. Fix lints instead of suppressing them. If an item
  is intentionally unused (a protocol field kept for compatibility, a reserved
  variant), add a specific `#[allow(...)]` with a comment.
- Handlers stay small: validate, check permissions, call store functions,
  emit events. SQL belongs in `store/sqlite/*`.
- Return `ApiError` (status + message) from native handlers and `DiscordError`
  (status + code + message) from the compatibility surface. Do not invent a
  third error shape.
- Preserve routes, payloads, database schema and file formats. Schema changes
  need a numbered migration and a note in `docs/DECISIONS.md`.
- Media writes go through `MediaStore`; do not reimplement hashing, directory
  creation or sealing in handlers.

## Adding an endpoint

1. Add request/response types to `crates/protocol/src`.
2. Add the store function in `store/sqlite/<domain>.rs`.
3. Add the handler in `src/api/<feature>.rs` and register the route in
   `src/api/mod.rs` (or in `compat.rs`/`platform.rs` for those surfaces).
4. If clients must update live, send the appropriate `GatewayEvent`.
5. Add a store test and run the full verification set.

## Schema migrations

`migrations/` holds numbered SQL files applied at startup. Append a new file,
never edit an applied one; existing installations only run files they have not
seen. Add the corresponding column to row mappers and model structs, and cover
the change in the store tests.

## Commits

One focused commit per logical change, imperative subject, verification noted
in the body when it is not the standard set. Keep pure moves separate from
behaviour changes so review and bisection stay possible.

## Documentation

- `docs/ARCHITECTURE.md` — startup, request path, storage and module layout.
- `docs/DECISIONS.md` — why the boundaries are where they are.
- `docs/CONTRIBUTING.md` — this file.
- `docs/` also hosts the published API reference site; Markdown files here are
  documentation, not part of the site navigation.
