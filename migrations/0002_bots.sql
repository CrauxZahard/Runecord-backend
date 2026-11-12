-- Bot accounts, ownership and hashed bot tokens.
ALTER TABLE users ADD COLUMN is_bot INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS bot_owners (
    owner_id INTEGER NOT NULL,
    bot_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (owner_id, bot_id)
);

CREATE TABLE IF NOT EXISTS bot_tokens (
    bot_id INTEGER PRIMARY KEY,
    token_hash TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_bot_owners_owner ON bot_owners(owner_id);
