ALTER TABLE users ADD COLUMN profile_effect TEXT NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN nameplate TEXT NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN profile_widget TEXT NOT NULL DEFAULT 'none';

UPDATE users SET profile_effect='aurora', nameplate='midnight', profile_widget='stats' WHERE id=2;
UPDATE users SET profile_effect='ember', nameplate='aurora', profile_widget='activity' WHERE id=1;
