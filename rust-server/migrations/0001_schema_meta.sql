CREATE TABLE IF NOT EXISTS schema_meta (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    version INTEGER NOT NULL CHECK (version >= 0),
    applied_at TEXT NOT NULL
);

INSERT INTO schema_meta(id, version, applied_at)
VALUES (1, 0, '1970-01-01T00:00:00Z')
ON CONFLICT(id) DO NOTHING;
