CREATE TABLE IF NOT EXISTS local_runtime (
    profile_id TEXT PRIMARY KEY REFERENCES profiles(id) ON DELETE CASCADE,
    level INTEGER NOT NULL CHECK (level >= 0),
    fuel INTEGER NOT NULL CHECK (fuel >= 0),
    coins INTEGER NOT NULL CHECK (coins >= 0),
    completed_stages INTEGER NOT NULL CHECK (completed_stages >= 0)
);

CREATE TABLE IF NOT EXISTS local_ships (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    ship_id INTEGER NOT NULL CHECK (ship_id > 0),
    name TEXT NOT NULL,
    level INTEGER NOT NULL CHECK (level >= 0),
    power INTEGER NOT NULL CHECK (power >= 0),
    PRIMARY KEY (profile_id, ship_id)
);

CREATE TABLE IF NOT EXISTS local_formation (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    ship_id INTEGER NOT NULL CHECK (ship_id > 0),
    PRIMARY KEY (profile_id, position),
    FOREIGN KEY (profile_id, ship_id)
        REFERENCES local_ships(profile_id, ship_id)
);

DROP TABLE IF EXISTS profile_formation;
DROP TABLE IF EXISTS profile_ships;
DROP TABLE IF EXISTS profile_runtime;
