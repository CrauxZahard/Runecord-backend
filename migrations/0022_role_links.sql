CREATE TABLE IF NOT EXISTS role_links (
    id INTEGER PRIMARY KEY,
    guild_id INTEGER NOT NULL,
    role_id INTEGER NOT NULL,
    label TEXT NOT NULL,
    url TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_role_links_role ON role_links(role_id);
