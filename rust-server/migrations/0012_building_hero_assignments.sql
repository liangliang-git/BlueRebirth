CREATE TABLE IF NOT EXISTS building_hero_assignments (
    profile_id TEXT NOT NULL,
    building_id INTEGER NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    hero_id INTEGER NOT NULL CHECK (hero_id > 0),
    PRIMARY KEY (profile_id, building_id, position),
    FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE,
    FOREIGN KEY (profile_id, building_id)
        REFERENCES buildings(profile_id, building_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_building_hero_assignments_profile
    ON building_hero_assignments(profile_id, building_id, position);
