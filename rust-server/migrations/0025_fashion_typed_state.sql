CREATE TABLE IF NOT EXISTS fashion_entries (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    sf_id INTEGER NOT NULL CHECK(sf_id > 0),
    fashion_tid INTEGER NOT NULL CHECK(fashion_tid > 0),
    PRIMARY KEY(profile_id, sf_id, fashion_tid)
);
