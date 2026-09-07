CREATE TABLE IF NOT EXISTS supply_heroes (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK(position >= 0),
    hero_id INTEGER NOT NULL CHECK(hero_id > 0),
    PRIMARY KEY(profile_id, position),
    UNIQUE(profile_id, hero_id)
);
