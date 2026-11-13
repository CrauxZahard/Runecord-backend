ALTER TABLE channels ADD COLUMN icon_url TEXT NOT NULL DEFAULT '';
UPDATE channels SET owner_id = (SELECT MIN(m.user_id) FROM dm_members m WHERE m.channel_id = channels.id)
WHERE kind='group_dm' AND owner_id IS NULL;
