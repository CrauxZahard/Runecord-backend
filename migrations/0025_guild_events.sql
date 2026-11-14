CREATE TABLE IF NOT EXISTS guild_events (
    id INTEGER PRIMARY KEY,
    guild_id INTEGER NOT NULL,
    creator_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    start_at INTEGER NOT NULL,
    channel_id INTEGER,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_guild_events_guild ON guild_events(guild_id, start_at);

INSERT INTO guild_events(id, guild_id, creator_id, name, description, start_at, channel_id, created_at)
VALUES(
    1, 100, 1, 'Design review',
    'Walk through the latest interface audit and pick the next pass.',
    (CAST(strftime('%s','now') AS INTEGER) + 7200) * 1000,
    211,
    CAST(strftime('%s','now') AS INTEGER) * 1000
);
