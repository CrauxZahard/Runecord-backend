-- Applications, webhooks, audit log, CDN assets, custom emoji/stickers, webhook authors.

CREATE TABLE IF NOT EXISTS applications (
    id INTEGER PRIMARY KEY,
    owner_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    icon_color INTEGER NOT NULL DEFAULT 0,
    bot_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS application_commands (
    id INTEGER PRIMARY KEY,
    app_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    options_json TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS app_installs (
    code TEXT PRIMARY KEY,
    app_id INTEGER NOT NULL,
    uses INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS webhooks (
    id INTEGER PRIMARY KEY,
    guild_id INTEGER NOT NULL,
    channel_id INTEGER NOT NULL,
    creator_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    avatar_color INTEGER NOT NULL DEFAULT 0,
    token_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY,
    guild_id INTEGER NOT NULL,
    actor_id INTEGER NOT NULL,
    action_type INTEGER NOT NULL,
    target_id INTEGER,
    changes TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS cdn_assets (
    hash TEXT PRIMARY KEY,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS emojis (
    id INTEGER PRIMARY KEY,
    guild_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    sticker INTEGER NOT NULL DEFAULT 0,
    url TEXT NOT NULL,
    uploader_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

ALTER TABLE messages ADD COLUMN webhook_id INTEGER;
ALTER TABLE messages ADD COLUMN webhook_name TEXT;
ALTER TABLE messages ADD COLUMN webhook_color INTEGER;

CREATE INDEX IF NOT EXISTS idx_audit_guild ON audit_log(guild_id, id);
CREATE INDEX IF NOT EXISTS idx_commands_app ON application_commands(app_id);
