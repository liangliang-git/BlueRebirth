ALTER TABLE battle_sessions ADD COLUMN attack_count INTEGER NOT NULL DEFAULT 0 CHECK (attack_count >= 0);

CREATE TABLE IF NOT EXISTS battle_session_fleets (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    fleet_id INTEGER NOT NULL CHECK (fleet_id > 0),
    PRIMARY KEY (profile_id, position)
);

CREATE TABLE IF NOT EXISTS battle_session_heroes (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    hero_id INTEGER NOT NULL CHECK (hero_id > 0),
    PRIMARY KEY (profile_id, position)
);

CREATE INDEX IF NOT EXISTS idx_battle_session_fleets_profile
    ON battle_session_fleets(profile_id, position);
CREATE INDEX IF NOT EXISTS idx_battle_session_heroes_profile
    ON battle_session_heroes(profile_id, position);
