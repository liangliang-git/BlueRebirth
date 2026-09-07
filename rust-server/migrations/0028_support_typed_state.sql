CREATE TABLE IF NOT EXISTS support_entries (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    entry_id INTEGER NOT NULL CHECK(entry_id > 0),
    support_id INTEGER NOT NULL CHECK(support_id > 0),
    start_time INTEGER NOT NULL CHECK(start_time >= 0),
    PRIMARY KEY(profile_id, entry_id)
);

CREATE TABLE IF NOT EXISTS support_entry_heroes (
    profile_id TEXT NOT NULL,
    entry_id INTEGER NOT NULL,
    position INTEGER NOT NULL CHECK(position >= 0),
    hero_id INTEGER NOT NULL CHECK(hero_id > 0),
    PRIMARY KEY(profile_id, entry_id, position),
    UNIQUE(profile_id, entry_id, hero_id),
    FOREIGN KEY(profile_id, entry_id)
        REFERENCES support_entries(profile_id, entry_id) ON DELETE CASCADE
);
