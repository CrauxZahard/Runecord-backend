ALTER TABLE guild_members ADD COLUMN timeout_until INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS bans (
    guild_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    actor_id INTEGER NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    PRIMARY KEY (guild_id, user_id)
);
