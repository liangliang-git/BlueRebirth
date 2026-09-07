ALTER TABLE daily_copy_progress
    ADD COLUMN ex_star INTEGER NOT NULL DEFAULT 0 CHECK (ex_star >= 0);
