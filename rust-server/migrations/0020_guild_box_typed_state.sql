CREATE TABLE IF NOT EXISTS guild_box_items (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    box_kind TEXT NOT NULL CHECK (box_kind IN ('share', 'task')),
    box_id INTEGER NOT NULL CHECK (box_id > 0),
    end_time INTEGER NOT NULL CHECK (end_time >= 0),
    box_uid INTEGER NOT NULL CHECK (box_uid >= 0),
    is_picked INTEGER NOT NULL CHECK (is_picked IN (0, 1)),
    recharge_id INTEGER NOT NULL CHECK (recharge_id >= 0),
    recharge_name TEXT NOT NULL,
    PRIMARY KEY(profile_id, box_kind, box_id)
);
