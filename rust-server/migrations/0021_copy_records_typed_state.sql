CREATE TABLE IF NOT EXISTS copy_records (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    record_index INTEGER NOT NULL CHECK(record_index >= 0),
    copy_id INTEGER NOT NULL CHECK(copy_id > 0),
    pass_time INTEGER NOT NULL CHECK(pass_time >= 0),
    secret_id INTEGER NOT NULL CHECK(secret_id >= 0),
    strategy_id INTEGER NOT NULL CHECK(strategy_id >= 0),
    power INTEGER NOT NULL CHECK(power >= 0),
    record_time INTEGER NOT NULL CHECK(record_time >= 0),
    PRIMARY KEY(profile_id, record_index)
);

CREATE TABLE IF NOT EXISTS copy_record_heroes (
    profile_id TEXT NOT NULL,
    record_index INTEGER NOT NULL,
    position INTEGER NOT NULL CHECK(position >= 0),
    hero_id INTEGER NOT NULL CHECK(hero_id > 0),
    PRIMARY KEY(profile_id, record_index, position),
    FOREIGN KEY(profile_id, record_index)
        REFERENCES copy_records(profile_id, record_index) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS copy_record_ex_buffs (
    profile_id TEXT NOT NULL,
    record_index INTEGER NOT NULL,
    position INTEGER NOT NULL CHECK(position >= 0),
    buff_id INTEGER NOT NULL CHECK(buff_id >= 0),
    PRIMARY KEY(profile_id, record_index, position),
    FOREIGN KEY(profile_id, record_index)
        REFERENCES copy_records(profile_id, record_index) ON DELETE CASCADE
);
