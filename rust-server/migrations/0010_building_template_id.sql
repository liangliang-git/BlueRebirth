ALTER TABLE buildings ADD COLUMN template_id INTEGER NOT NULL DEFAULT 0 CHECK (template_id >= 0);
