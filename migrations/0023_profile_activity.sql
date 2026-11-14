ALTER TABLE users ADD COLUMN activity_kind TEXT NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN activity_title TEXT NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN activity_subtitle TEXT NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN activity_state TEXT NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN activity_url TEXT NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN activity_artwork_url TEXT NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN activity_started_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN connections TEXT NOT NULL DEFAULT '[]';

UPDATE users SET
  activity_kind='spotify',
  activity_title='Midnight City',
  activity_subtitle='M83',
  activity_state='Hurry Up, We''re Dreaming',
  activity_url='https://open.spotify.com/track/6ac4NVYYl2U73QiTt11ZKd',
  connections='[{"kind":"github","name":"mayachen"},{"kind":"steam","name":"maya"}]'
WHERE id=2;

UPDATE users SET
  activity_kind='playing',
  activity_title='Hollow Knight: Silksong',
  activity_subtitle='Exploring Moss Grotto',
  connections='[{"kind":"github","name":"anasouza"},{"kind":"website","name":"anasouza.dev"}]'
WHERE id=5;
