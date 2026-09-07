CREATE TABLE IF NOT EXISTS account_revisions (
    profile_id TEXT PRIMARY KEY,
    revision INTEGER NOT NULL CHECK (revision >= 0),
    updated_utc TEXT NOT NULL
);
