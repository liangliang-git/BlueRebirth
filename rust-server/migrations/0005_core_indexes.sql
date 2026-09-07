CREATE INDEX IF NOT EXISTS idx_heroes_profile_template ON heroes(profile_id, template_id);
CREATE INDEX IF NOT EXISTS idx_equipments_profile_hero ON equipments(profile_id, hero_id);
CREATE INDEX IF NOT EXISTS idx_fleet_members_profile_hero ON fleet_members(profile_id, hero_id);
CREATE INDEX IF NOT EXISTS idx_battle_sessions_expiry ON battle_sessions(expires_at);
CREATE INDEX IF NOT EXISTS idx_tasks_reset ON tasks(profile_id, reset_day, task_type);
