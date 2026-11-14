CREATE TABLE IF NOT EXISTS guild_notifications (
    user_id INTEGER NOT NULL,
    guild_id INTEGER NOT NULL,
    level TEXT NOT NULL DEFAULT 'all',
    muted INTEGER NOT NULL DEFAULT 0,
    suppress_everyone INTEGER NOT NULL DEFAULT 0,
    suppress_roles INTEGER NOT NULL DEFAULT 0,
    suppress_highlights INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, guild_id)
);
