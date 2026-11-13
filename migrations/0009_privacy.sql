ALTER TABLE users ADD COLUMN dm_privacy TEXT NOT NULL DEFAULT 'everyone';
ALTER TABLE users ADD COLUMN friend_privacy TEXT NOT NULL DEFAULT 'everyone';
