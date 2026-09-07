CREATE TABLE account_revisions_v32 (
    profile_id TEXT PRIMARY KEY REFERENCES profiles(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL CHECK (revision >= 0),
    updated_utc TEXT NOT NULL
);

INSERT INTO account_revisions_v32(profile_id, revision, updated_utc)
SELECT account_revisions.profile_id, account_revisions.revision, account_revisions.updated_utc
FROM account_revisions
JOIN profiles ON profiles.id = account_revisions.profile_id;

DROP TABLE account_revisions;
ALTER TABLE account_revisions_v32 RENAME TO account_revisions;
