ALTER TABLE applications ADD COLUMN scopes TEXT NOT NULL DEFAULT 'bot applications.commands';
ALTER TABLE applications ADD COLUMN permissions INTEGER NOT NULL DEFAULT 0;
