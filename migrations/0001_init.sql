
PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v INTEGER NOT NULL);

CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name TEXT NOT NULL,
    pass_hash TEXT NOT NULL,
    avatar_color INTEGER NOT NULL DEFAULT 0,
    bio TEXT NOT NULL DEFAULT '',
    custom_status TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'offline',
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    token TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS guilds (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    icon_color INTEGER NOT NULL DEFAULT 0,
    owner_id INTEGER NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    everyone_role_id INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS guild_members (
    guild_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    nickname TEXT NOT NULL DEFAULT '',
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (guild_id, user_id)
);

CREATE TABLE IF NOT EXISTS roles (
    id INTEGER PRIMARY KEY,
    guild_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    color INTEGER NOT NULL DEFAULT 0,
    permissions INTEGER NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    hoist INTEGER NOT NULL DEFAULT 0,
    mentionable INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS member_roles (
    guild_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    role_id INTEGER NOT NULL,
    PRIMARY KEY (guild_id, user_id, role_id)
);

CREATE TABLE IF NOT EXISTS channels (
    id INTEGER PRIMARY KEY,
    guild_id INTEGER,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    parent_id INTEGER,
    topic TEXT NOT NULL DEFAULT '',
    nsfw INTEGER NOT NULL DEFAULT 0,
    slowmode INTEGER NOT NULL DEFAULT 0,
    last_message_id INTEGER,
    owner_id INTEGER,
    thread_parent_id INTEGER,
    thread_message_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS overwrites (
    channel_id INTEGER NOT NULL,
    target_id INTEGER NOT NULL,
    target_kind TEXT NOT NULL,
    allow INTEGER NOT NULL DEFAULT 0,
    deny INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (channel_id, target_id, target_kind)
);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY,
    channel_id INTEGER NOT NULL,
    author_id INTEGER NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    timestamp INTEGER NOT NULL,
    edited_at INTEGER,
    kind TEXT NOT NULL DEFAULT 'default',
    reply_to INTEGER,
    pinned INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS attachments (
    id INTEGER PRIMARY KEY,
    message_id INTEGER,
    uploader_id INTEGER NOT NULL,
    filename TEXT NOT NULL,
    size INTEGER NOT NULL,
    content_type TEXT NOT NULL,
    url TEXT NOT NULL,
    width INTEGER NOT NULL DEFAULT 0,
    height INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS reactions (
    message_id INTEGER NOT NULL,
    emoji TEXT NOT NULL,
    user_id INTEGER NOT NULL,
    PRIMARY KEY (message_id, emoji, user_id)
);

CREATE TABLE IF NOT EXISTS reads (
    user_id INTEGER NOT NULL,
    channel_id INTEGER NOT NULL,
    last_read_id INTEGER,
    PRIMARY KEY (user_id, channel_id)
);

CREATE TABLE IF NOT EXISTS relationships (
    user_id INTEGER NOT NULL,
    other_id INTEGER NOT NULL,
    kind TEXT NOT NULL,
    since INTEGER NOT NULL,
    PRIMARY KEY (user_id, other_id)
);

CREATE TABLE IF NOT EXISTS dm_members (
    channel_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    PRIMARY KEY (channel_id, user_id)
);

CREATE TABLE IF NOT EXISTS invites (
    code TEXT PRIMARY KEY,
    guild_id INTEGER NOT NULL,
    inviter_id INTEGER NOT NULL,
    uses INTEGER NOT NULL DEFAULT 0,
    max_uses INTEGER,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages(channel_id, id);
CREATE INDEX IF NOT EXISTS idx_members_user ON guild_members(user_id);
CREATE INDEX IF NOT EXISTS idx_reads_user ON reads(user_id);
