CREATE TABLE IF NOT EXISTS guilds (
    profile_id TEXT PRIMARY KEY REFERENCES profiles(id) ON DELETE CASCADE,
    guild_id INTEGER NOT NULL CHECK (guild_id > 0),
    name TEXT NOT NULL,
    emblem INTEGER NOT NULL CHECK (emblem >= 0),
    frame INTEGER NOT NULL CHECK (frame >= 0),
    enounce TEXT NOT NULL,
    notice TEXT NOT NULL,
    level INTEGER NOT NULL CHECK (level >= 0),
    exp INTEGER NOT NULL CHECK (exp >= 0),
    member_num INTEGER NOT NULL CHECK (member_num >= 0),
    leader_id INTEGER NOT NULL CHECK (leader_id >= 0),
    leader_name TEXT NOT NULL,
    limit_level INTEGER NOT NULL CHECK (limit_level >= 0),
    power INTEGER NOT NULL CHECK (power >= 0),
    honor INTEGER NOT NULL CHECK (honor >= 0),
    create_time INTEGER NOT NULL CHECK (create_time >= 0),
    chat_room TEXT NOT NULL,
    my_post INTEGER NOT NULL CHECK (my_post >= 0),
    join_time INTEGER NOT NULL CHECK (join_time >= 0),
    apply_num INTEGER NOT NULL CHECK (apply_num >= 0)
);

CREATE TABLE IF NOT EXISTS guild_members (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    uid INTEGER NOT NULL CHECK (uid > 0),
    name TEXT NOT NULL,
    post INTEGER NOT NULL CHECK (post >= 0),
    contribute INTEGER NOT NULL CHECK (contribute >= 0),
    today_contribute INTEGER NOT NULL CHECK (today_contribute >= 0),
    power INTEGER NOT NULL CHECK (power >= 0),
    PRIMARY KEY(profile_id, uid)
);

CREATE TABLE IF NOT EXISTS guild_applications (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    uid INTEGER NOT NULL CHECK (uid > 0),
    name TEXT NOT NULL,
    applied_at INTEGER NOT NULL CHECK (applied_at >= 0),
    quality INTEGER NOT NULL CHECK (quality >= 0),
    PRIMARY KEY(profile_id, uid)
);
