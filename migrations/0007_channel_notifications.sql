CREATE TABLE IF NOT EXISTS channel_notifications (
    user_id INTEGER NOT NULL,
    channel_id INTEGER NOT NULL,
    level TEXT NOT NULL DEFAULT 'all',
    PRIMARY KEY (user_id, channel_id)
);
