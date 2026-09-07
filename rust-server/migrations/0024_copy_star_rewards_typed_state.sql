CREATE TABLE IF NOT EXISTS copy_star_rewards (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    chapter_id INTEGER NOT NULL CHECK(chapter_id > 0),
    reward_index INTEGER NOT NULL CHECK(reward_index > 0),
    PRIMARY KEY(profile_id, chapter_id, reward_index)
);
