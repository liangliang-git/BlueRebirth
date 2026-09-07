CREATE TABLE IF NOT EXISTS chat_state (
    profile_id TEXT PRIMARY KEY REFERENCES profiles(id) ON DELETE CASCADE,
    channel INTEGER NOT NULL CHECK (channel >= 0)
);

ALTER TABLE chat_messages ADD COLUMN receive_uid INTEGER NOT NULL DEFAULT 0 CHECK (receive_uid >= 0);
ALTER TABLE chat_messages ADD COLUMN message_type INTEGER NOT NULL DEFAULT 0 CHECK (message_type >= 0);
ALTER TABLE chat_messages ADD COLUMN voice TEXT NOT NULL DEFAULT '';

CREATE TABLE IF NOT EXISTS chat_barrages (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    barrage_id INTEGER NOT NULL CHECK (barrage_id >= 0),
    offset_value INTEGER NOT NULL CHECK (offset_value >= 0),
    content TEXT NOT NULL,
    uid INTEGER NOT NULL CHECK (uid > 0),
    sent_at INTEGER NOT NULL CHECK (sent_at >= 0),
    PRIMARY KEY (profile_id, barrage_id, offset_value, uid, sent_at)
);

CREATE INDEX IF NOT EXISTS idx_chat_barrages_lookup
    ON chat_barrages(profile_id, barrage_id, offset_value);
