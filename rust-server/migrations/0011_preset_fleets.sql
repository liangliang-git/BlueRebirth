CREATE TABLE IF NOT EXISTS preset_fleet_meta (
    profile_id TEXT PRIMARY KEY REFERENCES profiles(id) ON DELETE CASCADE,
    name_num INTEGER NOT NULL CHECK (name_num >= 0),
    red_dot INTEGER NOT NULL CHECK (red_dot >= 0)
);

CREATE TABLE IF NOT EXISTS preset_fleets (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    slot INTEGER NOT NULL CHECK (slot >= 0),
    name TEXT NOT NULL,
    mode_id INTEGER NOT NULL CHECK (mode_id >= 0),
    strategy_id INTEGER NOT NULL CHECK (strategy_id >= 0),
    PRIMARY KEY (profile_id, slot)
);

CREATE TABLE IF NOT EXISTS preset_fleet_members (
    profile_id TEXT NOT NULL,
    slot INTEGER NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    hero_id INTEGER NOT NULL CHECK (hero_id > 0),
    is_ex INTEGER NOT NULL CHECK (is_ex IN (0, 1)),
    PRIMARY KEY (profile_id, slot, position, is_ex),
    FOREIGN KEY (profile_id, slot)
        REFERENCES preset_fleets(profile_id, slot) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_preset_fleet_members_profile
    ON preset_fleet_members(profile_id, slot, is_ex, position);
