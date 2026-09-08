ALTER TABLE fleets ADD COLUMN tactic_name TEXT NOT NULL DEFAULT '';
ALTER TABLE fleets ADD COLUMN tactic_type INTEGER NOT NULL DEFAULT 1 CHECK (tactic_type > 0);

CREATE TABLE IF NOT EXISTS fleet_ex_members (
    profile_id TEXT NOT NULL,
    fleet_id INTEGER NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    hero_id INTEGER NOT NULL CHECK (hero_id > 0),
    PRIMARY KEY (profile_id, fleet_id, position),
    UNIQUE (profile_id, fleet_id, hero_id),
    FOREIGN KEY (profile_id, fleet_id) REFERENCES fleets(profile_id, fleet_id),
    FOREIGN KEY (profile_id, hero_id) REFERENCES heroes(profile_id, hero_id)
);

CREATE INDEX IF NOT EXISTS idx_fleet_ex_members_profile_hero
    ON fleet_ex_members(profile_id, hero_id);
