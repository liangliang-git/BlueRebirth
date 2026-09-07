CREATE TABLE IF NOT EXISTS characters (
    profile_id TEXT PRIMARY KEY REFERENCES profiles(id) ON DELETE CASCADE,
    uid INTEGER NOT NULL CHECK (uid > 0),
    name TEXT NOT NULL,
    level INTEGER NOT NULL CHECK (level > 0),
    exp INTEGER NOT NULL CHECK (exp >= 0),
    secretary_id INTEGER NOT NULL CHECK (secretary_id >= 0),
    gold INTEGER NOT NULL CHECK (gold >= 0),
    diamond INTEGER NOT NULL CHECK (diamond >= 0),
    supply INTEGER NOT NULL CHECK (supply >= 0),
    pve_pt INTEGER NOT NULL CHECK (pve_pt >= 0),
    head INTEGER NOT NULL CHECK (head >= 0),
    head_frame INTEGER NOT NULL CHECK (head_frame >= 0)
);

CREATE TABLE IF NOT EXISTS heroes (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    hero_id INTEGER NOT NULL CHECK (hero_id > 0),
    template_id INTEGER NOT NULL CHECK (template_id > 0),
    level INTEGER NOT NULL CHECK (level > 0),
    exp INTEGER NOT NULL CHECK (exp >= 0),
    mood INTEGER NOT NULL CHECK (mood >= 0),
    affection INTEGER NOT NULL CHECK (affection >= 0),
    hp INTEGER NOT NULL CHECK (hp >= 0),
    lock_state INTEGER NOT NULL CHECK (lock_state >= 0),
    created_utc TEXT NOT NULL,
    PRIMARY KEY (profile_id, hero_id)
);

CREATE TABLE IF NOT EXISTS equipments (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    equip_id INTEGER NOT NULL CHECK (equip_id > 0),
    template_id INTEGER NOT NULL CHECK (template_id > 0),
    enhance_level INTEGER NOT NULL CHECK (enhance_level >= 0),
    star INTEGER NOT NULL CHECK (star >= 0),
    enhance_exp INTEGER NOT NULL CHECK (enhance_exp >= 0),
    hero_id INTEGER,
    PRIMARY KEY (profile_id, equip_id),
    FOREIGN KEY (profile_id, hero_id) REFERENCES heroes(profile_id, hero_id)
);

CREATE TABLE IF NOT EXISTS hero_equip_slots (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    hero_id INTEGER NOT NULL,
    slot_index INTEGER NOT NULL CHECK (slot_index >= 0),
    equip_id INTEGER,
    PRIMARY KEY (profile_id, hero_id, slot_index),
    FOREIGN KEY (profile_id, hero_id) REFERENCES heroes(profile_id, hero_id),
    FOREIGN KEY (profile_id, equip_id) REFERENCES equipments(profile_id, equip_id)
);

CREATE TABLE IF NOT EXISTS fleets (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    fleet_id INTEGER NOT NULL CHECK (fleet_id > 0),
    formation_id INTEGER NOT NULL CHECK (formation_id >= 0),
    tactic_id INTEGER NOT NULL CHECK (tactic_id >= 0),
    PRIMARY KEY (profile_id, fleet_id)
);

CREATE TABLE IF NOT EXISTS fleet_members (
    profile_id TEXT NOT NULL,
    fleet_id INTEGER NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    hero_id INTEGER NOT NULL,
    PRIMARY KEY (profile_id, fleet_id, position),
    UNIQUE (profile_id, fleet_id, hero_id),
    FOREIGN KEY (profile_id, fleet_id) REFERENCES fleets(profile_id, fleet_id),
    FOREIGN KEY (profile_id, hero_id) REFERENCES heroes(profile_id, hero_id)
);

CREATE TABLE IF NOT EXISTS inventory (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    template_id INTEGER NOT NULL CHECK (template_id > 0),
    amount INTEGER NOT NULL CHECK (amount >= 0),
    PRIMARY KEY (profile_id, template_id)
);

CREATE TABLE IF NOT EXISTS battle_sessions (
    profile_id TEXT PRIMARY KEY REFERENCES profiles(id) ON DELETE CASCADE,
    chapter_id INTEGER NOT NULL CHECK (chapter_id > 0),
    copy_id INTEGER NOT NULL CHECK (copy_id > 0),
    current_fleet INTEGER NOT NULL CHECK (current_fleet >= 0),
    state TEXT NOT NULL,
    started_at INTEGER NOT NULL CHECK (started_at >= 0),
    expires_at INTEGER NOT NULL CHECK (expires_at >= started_at),
    revision INTEGER NOT NULL CHECK (revision >= 0)
);

CREATE TABLE IF NOT EXISTS tasks (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    task_id INTEGER NOT NULL CHECK (task_id > 0),
    task_type INTEGER NOT NULL CHECK (task_type >= 0),
    progress INTEGER NOT NULL CHECK (progress >= 0),
    completed INTEGER NOT NULL CHECK (completed IN (0, 1)),
    reset_day INTEGER NOT NULL CHECK (reset_day >= 0),
    PRIMARY KEY (profile_id, task_id)
);

CREATE TABLE IF NOT EXISTS task_claims (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    task_id INTEGER NOT NULL,
    claimed_at INTEGER NOT NULL CHECK (claimed_at >= 0),
    PRIMARY KEY (profile_id, task_id),
    FOREIGN KEY (profile_id, task_id) REFERENCES tasks(profile_id, task_id)
);
