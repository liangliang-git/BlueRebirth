CREATE TABLE IF NOT EXISTS sea_progress (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    copy_id INTEGER NOT NULL CHECK (copy_id > 0),
    star_level INTEGER NOT NULL CHECK (star_level >= 0),
    pass_count INTEGER NOT NULL CHECK (pass_count >= 0),
    PRIMARY KEY (profile_id, copy_id)
);

CREATE TABLE IF NOT EXISTS copy_progress (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    copy_id INTEGER NOT NULL CHECK (copy_id > 0),
    star_level INTEGER NOT NULL CHECK (star_level >= 0),
    first_passed INTEGER NOT NULL CHECK (first_passed IN (0, 1)),
    PRIMARY KEY (profile_id, copy_id)
);

CREATE TABLE IF NOT EXISTS daily_copy_progress (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    reset_day INTEGER NOT NULL CHECK (reset_day >= 0),
    chapter_id INTEGER NOT NULL CHECK (chapter_id > 0),
    group_id INTEGER NOT NULL CHECK (group_id > 0),
    challenge_times INTEGER NOT NULL CHECK (challenge_times >= 0),
    success_times INTEGER NOT NULL CHECK (success_times >= 0),
    select_ex INTEGER NOT NULL CHECK (select_ex IN (0, 1)),
    extra_group INTEGER NOT NULL CHECK (extra_group >= 0),
    PRIMARY KEY (profile_id, chapter_id)
);

CREATE TABLE IF NOT EXISTS buildings (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    building_id INTEGER NOT NULL CHECK (building_id > 0),
    level INTEGER NOT NULL CHECK (level >= 0),
    land_index INTEGER NOT NULL CHECK (land_index >= 0),
    PRIMARY KEY (profile_id, building_id)
);

CREATE TABLE IF NOT EXISTS construction_jobs (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    job_id INTEGER NOT NULL CHECK (job_id > 0),
    building_id INTEGER NOT NULL CHECK (building_id > 0),
    started_at INTEGER NOT NULL CHECK (started_at >= 0),
    finish_at INTEGER NOT NULL CHECK (finish_at >= started_at),
    state TEXT NOT NULL,
    PRIMARY KEY (profile_id, job_id)
);

CREATE TABLE IF NOT EXISTS tower_progress (
    profile_id TEXT PRIMARY KEY REFERENCES profiles(id) ON DELETE CASCADE,
    chapter_id INTEGER NOT NULL CHECK (chapter_id >= 0),
    floor INTEGER NOT NULL CHECK (floor >= 0),
    reset_day INTEGER NOT NULL CHECK (reset_day >= 0)
);

CREATE TABLE IF NOT EXISTS friend_relations (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    friend_profile_id TEXT NOT NULL,
    relation TEXT NOT NULL,
    created_at INTEGER NOT NULL CHECK (created_at >= 0),
    PRIMARY KEY (profile_id, friend_profile_id),
    CHECK (profile_id <> friend_profile_id)
);

CREATE TABLE IF NOT EXISTS chat_messages (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    message_id INTEGER NOT NULL CHECK (message_id > 0),
    channel INTEGER NOT NULL CHECK (channel >= 0),
    sender_uid INTEGER NOT NULL CHECK (sender_uid > 0),
    body TEXT NOT NULL,
    sent_at INTEGER NOT NULL CHECK (sent_at >= 0),
    PRIMARY KEY (profile_id, message_id)
);

CREATE TABLE IF NOT EXISTS activity_progress (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    activity_id TEXT NOT NULL,
    progress_kind TEXT NOT NULL,
    value INTEGER NOT NULL CHECK (value >= 0),
    updated_at INTEGER NOT NULL CHECK (updated_at >= 0),
    PRIMARY KEY (profile_id, activity_id, progress_kind)
);

CREATE INDEX IF NOT EXISTS idx_daily_copy_reset
    ON daily_copy_progress(profile_id, reset_day, chapter_id);
CREATE INDEX IF NOT EXISTS idx_construction_finish
    ON construction_jobs(profile_id, finish_at, state);
CREATE INDEX IF NOT EXISTS idx_chat_messages_time
    ON chat_messages(profile_id, sent_at);
CREATE INDEX IF NOT EXISTS idx_activity_progress_activity
    ON activity_progress(profile_id, activity_id);
