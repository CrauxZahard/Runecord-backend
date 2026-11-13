CREATE TABLE IF NOT EXISTS ban_appeals (
    id INTEGER PRIMARY KEY,
    guild_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    message TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    resolved_at INTEGER,
    resolver_id INTEGER
);
