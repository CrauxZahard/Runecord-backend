ALTER TABLE guild_notifications ADD COLUMN mute_new_events INTEGER NOT NULL DEFAULT 0;
ALTER TABLE guild_notifications ADD COLUMN mobile_push INTEGER NOT NULL DEFAULT 0;
