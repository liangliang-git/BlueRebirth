CREATE TABLE IF NOT EXISTS sea_difficulty (
    profile_id TEXT PRIMARY KEY REFERENCES profiles(id) ON DELETE CASCADE,
    difficulty INTEGER NOT NULL CHECK(difficulty BETWEEN 1 AND 7)
);
