CREATE TABLE battle_sessions_v29 (
    profile_id TEXT PRIMARY KEY REFERENCES profiles(id) ON DELETE CASCADE,
    chapter_id INTEGER NOT NULL CHECK (chapter_id > 0),
    copy_id INTEGER NOT NULL CHECK (copy_id > 0),
    current_fleet INTEGER NOT NULL CHECK (current_fleet > 0),
    state TEXT NOT NULL,
    started_at INTEGER NOT NULL CHECK (started_at >= 0),
    expires_at INTEGER NOT NULL CHECK (expires_at >= started_at),
    revision INTEGER NOT NULL CHECK (revision >= 0),
    attack_count INTEGER NOT NULL DEFAULT 0 CHECK (attack_count >= 0)
);

INSERT INTO battle_sessions_v29(
    profile_id, chapter_id, copy_id, current_fleet, state,
    started_at, expires_at, revision, attack_count
)
SELECT
    profile_id, chapter_id, copy_id, current_fleet, state,
    started_at, expires_at, revision, attack_count
FROM battle_sessions;

DROP TABLE battle_sessions;
ALTER TABLE battle_sessions_v29 RENAME TO battle_sessions;
CREATE INDEX IF NOT EXISTS idx_battle_sessions_expiry ON battle_sessions(expires_at);
