CREATE TABLE IF NOT EXISTS guild_boosts (
    guild_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (guild_id, user_id)
);
