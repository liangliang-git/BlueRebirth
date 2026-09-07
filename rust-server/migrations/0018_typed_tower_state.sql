ALTER TABLE tower_progress ADD COLUMN area_index INTEGER NOT NULL DEFAULT 0 CHECK (area_index >= 0);
ALTER TABLE tower_progress ADD COLUMN copy_index INTEGER NOT NULL DEFAULT 0 CHECK (copy_index >= 0);
ALTER TABLE tower_progress ADD COLUMN topic_index INTEGER NOT NULL DEFAULT 0 CHECK (topic_index >= 0);
ALTER TABLE tower_progress ADD COLUMN daily_count INTEGER NOT NULL DEFAULT 0 CHECK (daily_count >= 0);
ALTER TABLE tower_progress ADD COLUMN reset_time INTEGER NOT NULL DEFAULT 0 CHECK (reset_time >= 0);
ALTER TABLE tower_progress ADD COLUMN pass_last_chapter_id INTEGER NOT NULL DEFAULT 0 CHECK (pass_last_chapter_id >= 0);
ALTER TABLE tower_progress ADD COLUMN is_reset INTEGER NOT NULL DEFAULT 0 CHECK (is_reset IN (0, 1));
ALTER TABLE tower_progress ADD COLUMN max_level INTEGER NOT NULL DEFAULT 0 CHECK (max_level >= 0);
ALTER TABLE tower_progress ADD COLUMN max_area INTEGER NOT NULL DEFAULT 0 CHECK (max_area >= 0);
ALTER TABLE tower_progress ADD COLUMN max_copy INTEGER NOT NULL DEFAULT 0 CHECK (max_copy >= 0);
ALTER TABLE tower_progress ADD COLUMN daily_count_ex INTEGER NOT NULL DEFAULT 0 CHECK (daily_count_ex >= 0);
ALTER TABLE tower_progress ADD COLUMN is_new_level INTEGER NOT NULL DEFAULT 0 CHECK (is_new_level IN (0, 1));

CREATE TABLE IF NOT EXISTS tower_sf_counts (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    sf_id INTEGER NOT NULL CHECK (sf_id > 0),
    count INTEGER NOT NULL CHECK (count >= 0),
    PRIMARY KEY (profile_id, sf_id)
);

CREATE TABLE IF NOT EXISTS tower_ids (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    value INTEGER NOT NULL CHECK (value > 0),
    PRIMARY KEY (profile_id, kind, position),
    UNIQUE (profile_id, kind, value)
);

CREATE TABLE IF NOT EXISTS tower_rewards (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    reward_type INTEGER NOT NULL CHECK (reward_type >= 0),
    config_id INTEGER NOT NULL CHECK (config_id >= 0),
    amount INTEGER NOT NULL CHECK (amount >= 0),
    instance_id INTEGER NOT NULL CHECK (instance_id >= 0),
    PRIMARY KEY (profile_id, position)
);

CREATE TABLE IF NOT EXISTS activity_tower_progress (
    profile_id TEXT PRIMARY KEY REFERENCES profiles(id) ON DELETE CASCADE,
    activity_id INTEGER NOT NULL CHECK (activity_id >= 0),
    reset_time INTEGER NOT NULL CHECK (reset_time >= 0),
    small_reset_number INTEGER NOT NULL CHECK (small_reset_number >= 0),
    quick_number INTEGER NOT NULL CHECK (quick_number >= 0),
    history_max INTEGER NOT NULL CHECK (history_max >= 0)
);

CREATE TABLE IF NOT EXISTS activity_tower_ids (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    value INTEGER NOT NULL CHECK (value > 0),
    PRIMARY KEY (profile_id, kind, position),
    UNIQUE (profile_id, kind, value)
);
