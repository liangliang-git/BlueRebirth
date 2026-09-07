use blueoath_domain::{
    AccountRepository, AccountState, ActivityTowerState, ChapterId, CharacterState,
    ChatBarrageState, ChatMessageState, ConstructionJobState, ConstructionProjectState, CopyId,
    CurrencyKind, EquipId, EquipmentState, FleetId, FleetRecord, HeroId, HeroState,
    NewAccountFactory, PresetFleetState, ProfileId, ProfileState, RepositoryError, TemplateId,
    TowerRewardState,
};
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid profile id")]
    InvalidProfileId,
    #[error("account revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("invalid typed account: {0}")]
    InvalidTypedAccount(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredProfile {
    pub id: String,
    pub name: String,
    pub state: StoredProfileState,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StoredProfileState {
    pub level: i32,
    pub fuel: i64,
    pub coins: i64,
    pub ships: Vec<StoredShip>,
    pub formation_ship_ids: Vec<i32>,
    pub completed_stages: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredShip {
    pub id: i32,
    pub name: String,
    pub level: i32,
    pub power: i32,
}

/// SQLite persistence compatible with the C# `profiles` table.
#[derive(Debug, Clone)]
pub struct ProfileStore {
    db_path: PathBuf,
}

impl ProfileStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StorageError> {
        fs::create_dir_all(root.as_ref())?;
        let store = Self {
            db_path: root.as_ref().join("profiles.db"),
        };
        let connection = store.connection()?;
        run_migrations(&connection)?;
        Ok(store)
    }

    pub fn load(&self, profile_id: &str) -> Result<Option<StoredProfile>, StorageError> {
        let connection = self.connection()?;
        let row = connection
            .query_row(
                "SELECT id, name FROM profiles WHERE id = ?1",
                params![profile_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let Some((id, name)) = row else {
            return Ok(None);
        };
        let state = connection
            .query_row(
                "SELECT level, fuel, coins, completed_stages
                 FROM profile_runtime WHERE profile_id = ?1",
                params![profile_id],
                |row| {
                    Ok(StoredProfileState {
                        level: row.get(0)?,
                        fuel: row.get(1)?,
                        coins: row.get(2)?,
                        completed_stages: row.get(3)?,
                        ..StoredProfileState::default()
                    })
                },
            )
            .optional()?
            .unwrap_or_default();
        let mut state = state;
        let mut statement = connection.prepare(
            "SELECT ship_id, name, level, power
             FROM profile_ships WHERE profile_id = ?1 ORDER BY ship_id",
        )?;
        state.ships = statement
            .query_map(params![profile_id], |row| {
                Ok(StoredShip {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    level: row.get(2)?,
                    power: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut statement = connection.prepare(
            "SELECT ship_id FROM profile_formation
             WHERE profile_id = ?1 ORDER BY position",
        )?;
        state.formation_ship_ids = statement
            .query_map(params![profile_id], |row| row.get::<_, i32>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(StoredProfile { id, name, state }))
    }

    pub fn load_typed_account(
        &self,
        profile_id: &ProfileId,
    ) -> Result<Option<AccountState>, StorageError> {
        let connection = self.connection()?;
        let Some((name, revision)) = connection
            .query_row(
                "SELECT name, updated_utc FROM profiles WHERE id = ?1",
                params![profile_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .map(|(name, _updated)| (name, 0_u64))
        else {
            return Ok(None);
        };
        let revision = connection
            .query_row(
                "SELECT revision FROM account_revisions WHERE profile_id = ?1",
                params![profile_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(i64::try_from(revision).unwrap_or_default());
        let revision = u64::try_from(revision)
            .map_err(|_| StorageError::InvalidTypedAccount("negative revision".to_owned()))?;
        let has_typed_character = connection
            .query_row(
                "SELECT 1 FROM characters WHERE profile_id = ?1 LIMIT 1",
                params![profile_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_some();
        if !has_typed_character {
            return Ok(None);
        }
        let mut account = AccountState::new(ProfileState {
            id: profile_id.clone(),
            name,
            revision,
        });

        if let Some(row) = connection
            .query_row(
                "SELECT uid, name, level, exp, class_id, create_time, message,
                        secretary_id, head, head_frame, gold, diamond, supply, pve_pt
                 FROM characters WHERE profile_id = ?1",
                params![profile_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, i64>(10)?,
                        row.get::<_, i64>(11)?,
                        row.get::<_, i64>(12)?,
                        row.get::<_, i64>(13)?,
                    ))
                },
            )
            .optional()?
        {
            account.character = CharacterState {
                uid: positive_u64(row.0, "character uid")?,
                name: row.1,
                level: positive_u32(row.2, "character level")?,
                exp: non_negative_u64(row.3, "character exp")?,
                class_id: non_negative_u32(row.4, "character class")?,
                create_time: non_negative_u64(row.5, "character create time")?,
                message: row.6,
                secretary_id: (row.7 > 0)
                    .then(|| {
                        HeroId::new(row.7 as u64)
                            .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))
                    })
                    .transpose()?,
                head: non_negative_u32(row.8, "character head")?,
                head_frame: non_negative_u32(row.9, "character head frame")?,
                resources: account.character.resources.clone(),
            };
            for (kind, amount) in [
                (blueoath_domain::CurrencyKind::Gold, row.10),
                (blueoath_domain::CurrencyKind::Diamond, row.11),
                (blueoath_domain::CurrencyKind::Supply, row.12),
                (blueoath_domain::CurrencyKind::PvePoint, row.13),
            ] {
                account
                    .resources
                    .credit(kind, non_negative_u64(amount, "resource")?)
                    .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))?;
            }
        }

        let mut statement = connection.prepare(
            "SELECT hero_id, template_id, name, change_name_time, level, exp, mood, affection, hp, lock_state
             FROM heroes WHERE profile_id = ?1 ORDER BY hero_id",
        )?;
        let heroes = statement
            .query_map(params![profile_id.as_str()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for row in heroes {
            let id = positive_hero_id(row.0, "hero id")?;
            let template_id = positive_template_id(row.1, "hero template id")?;
            account.dock.heroes.insert(
                id,
                HeroState {
                    id,
                    template_id,
                    name: row.2,
                    change_name_time: non_negative_u64(row.3, "hero change name time")?,
                    level: positive_u32(row.4, "hero level")?,
                    exp: non_negative_u64(row.5, "hero exp")?,
                    mood: non_negative_u32(row.6, "hero mood")?,
                    affection: non_negative_u64(row.7, "hero affection")?,
                    hp: non_negative_u64(row.8, "hero hp")?,
                    locked: row.9 != 0,
                    equip_slots: Vec::new(),
                },
            );
        }

        let mut statement = connection.prepare(
            "SELECT equip_id, template_id, enhance_level, star, enhance_exp, hero_id
             FROM equipments WHERE profile_id = ?1 ORDER BY equip_id",
        )?;
        let equipments = statement
            .query_map(params![profile_id.as_str()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for row in equipments {
            let id = positive_equip_id(row.0, "equipment id")?;
            let template_id = positive_template_id(row.1, "equipment template id")?;
            let hero_id = row
                .5
                .map(|value| positive_hero_id(value, "equipment hero id"))
                .transpose()?;
            account.dock.equipments.insert(
                id,
                EquipmentState {
                    id,
                    template_id,
                    enhance_level: non_negative_u32(row.2, "equipment enhance level")?,
                    star: non_negative_u32(row.3, "equipment star")?,
                    enhance_exp: non_negative_u64(row.4, "equipment enhance exp")?,
                    hero_id,
                },
            );
        }

        let mut statement = connection.prepare(
            "SELECT template_id, amount
             FROM inventory WHERE profile_id = ?1 ORDER BY template_id",
        )?;
        let inventory = statement
            .query_map(params![profile_id.as_str()], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (template_value, amount) in inventory {
            let template_id = positive_template_id(template_value, "inventory template id")?;
            account
                .inventory
                .items
                .insert(template_id, non_negative_u64(amount, "inventory amount")?);
        }

        let mut statement = connection.prepare(
            "SELECT hero_id, slot_index, equip_id
             FROM hero_equip_slots WHERE profile_id = ?1 ORDER BY hero_id, slot_index",
        )?;
        let slots = statement
            .query_map(params![profile_id.as_str()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (hero_value, slot_value, equip_value) in slots {
            let hero_id = positive_hero_id(hero_value, "equipment slot hero id")?;
            let slot_index = usize::try_from(slot_value).map_err(|_| {
                StorageError::InvalidTypedAccount("equipment slot index is invalid".to_owned())
            })?;
            let equip_id = equip_value
                .map(|value| positive_equip_id(value, "equipment slot equipment id"))
                .transpose()?;
            let Some(hero) = account.dock.heroes.get_mut(&hero_id) else {
                return Err(StorageError::InvalidTypedAccount(
                    "equipment slot references missing hero".to_owned(),
                ));
            };
            if hero.equip_slots.len() <= slot_index {
                hero.equip_slots.resize(slot_index + 1, None);
            }
            hero.equip_slots[slot_index] = equip_id;
        }

        let mut statement = connection.prepare(
            "SELECT fleet_id, formation_id, tactic_id
             FROM fleets WHERE profile_id = ?1 ORDER BY fleet_id",
        )?;
        let fleets = statement
            .query_map(params![profile_id.as_str()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for row in fleets {
            let id = positive_fleet_id(row.0, "fleet id")?;
            account.fleet.fleets.insert(
                id,
                FleetRecord {
                    formation_id: non_negative_u32(row.1, "formation id")?,
                    tactic_id: non_negative_u32(row.2, "tactic id")?,
                    members: Vec::new(),
                },
            );
        }
        let mut statement = connection.prepare(
            "SELECT fleet_id, position, hero_id
             FROM fleet_members WHERE profile_id = ?1 ORDER BY fleet_id, position",
        )?;
        let members = statement
            .query_map(params![profile_id.as_str()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for row in members {
            let fleet_id = positive_fleet_id(row.0, "fleet member fleet id")?;
            let hero_id = positive_hero_id(row.2, "fleet member hero id")?;
            if let Some(fleet) = account.fleet.fleets.get_mut(&fleet_id) {
                let position = usize::try_from(row.1).map_err(|_| {
                    StorageError::InvalidTypedAccount("fleet member position is invalid".to_owned())
                })?;
                if fleet.members.len() <= position {
                    fleet.members.resize(position + 1, hero_id);
                }
                fleet.members[position] = hero_id;
            }
        }

        account.fleet.preset_name_num = connection
            .query_row(
                "SELECT name_num FROM preset_fleet_meta WHERE profile_id = ?1",
                params![profile_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .map(|value| non_negative_u32(value, "preset fleet name number"))
            .transpose()?
            .unwrap_or_default();
        account.fleet.preset_red_dot = connection
            .query_row(
                "SELECT red_dot FROM preset_fleet_meta WHERE profile_id = ?1",
                params![profile_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .map(|value| non_negative_u32(value, "preset fleet red dot"))
            .transpose()?
            .unwrap_or_default();
        let mut statement = connection.prepare(
            "SELECT slot, name, mode_id, strategy_id
             FROM preset_fleets WHERE profile_id = ?1 ORDER BY slot",
        )?;
        let presets = statement
            .query_map(params![profile_id.as_str()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (slot, name, mode_id, strategy_id) in presets {
            let mode_id = non_negative_u32(mode_id, "preset fleet mode")?;
            let strategy_id = non_negative_u32(strategy_id, "preset fleet strategy")?;
            let slot = usize::try_from(slot).map_err(|_| {
                StorageError::InvalidTypedAccount("preset fleet slot is invalid".to_owned())
            })?;
            if account.fleet.presets.len() <= slot {
                account
                    .fleet
                    .presets
                    .resize(slot + 1, PresetFleetState::default());
            }
            account.fleet.presets[slot] = PresetFleetState {
                name,
                hero_ids: Vec::new(),
                ex_hero_ids: Vec::new(),
                mode_id,
                strategy_id,
            };
        }
        let mut statement = connection.prepare(
            "SELECT slot, position, hero_id, is_ex
             FROM preset_fleet_members
             WHERE profile_id = ?1 ORDER BY slot, is_ex, position",
        )?;
        let preset_members = statement
            .query_map(params![profile_id.as_str()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (slot, position, hero_id, is_ex) in preset_members {
            let slot = usize::try_from(slot).map_err(|_| {
                StorageError::InvalidTypedAccount("preset fleet slot is invalid".to_owned())
            })?;
            let position = usize::try_from(position).map_err(|_| {
                StorageError::InvalidTypedAccount("preset fleet position is invalid".to_owned())
            })?;
            let hero_id = positive_hero_id(hero_id, "preset fleet hero id")?;
            let Some(preset) = account.fleet.presets.get_mut(slot) else {
                return Err(StorageError::InvalidTypedAccount(
                    "preset fleet member references missing preset".to_owned(),
                ));
            };
            let members = if is_ex == 0 {
                &mut preset.hero_ids
            } else {
                &mut preset.ex_hero_ids
            };
            if members.len() <= position {
                members.resize(
                    position + 1,
                    HeroId::new(1)
                        .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))?,
                );
            }
            members[position] = hero_id;
        }

        let mut statement = connection.prepare(
            "SELECT task_id, task_type, progress, completed
             FROM tasks WHERE profile_id = ?1 ORDER BY task_id",
        )?;
        let tasks = statement
            .query_map(params![profile_id.as_str()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (task_id, task_type, progress, completed) in tasks {
            let task_id = u64::try_from(task_id)
                .map_err(|_| StorageError::InvalidTypedAccount("task id is invalid".to_owned()))?;
            let task_type = u32::try_from(task_type).map_err(|_| {
                StorageError::InvalidTypedAccount("task type is invalid".to_owned())
            })?;
            account.tasks.task_types.insert(task_id, task_type);
            account
                .tasks
                .progress
                .insert(task_id, non_negative_u64(progress, "task progress")?);
            if completed != 0 {
                account.tasks.completed.insert(task_id);
            }
        }
        let mut statement = connection
            .prepare("SELECT task_id FROM task_claims WHERE profile_id = ?1 ORDER BY task_id")?;
        let claimed = statement
            .query_map(params![profile_id.as_str()], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for task_id in claimed {
            account
                .tasks
                .claimed
                .insert(u64::try_from(task_id).map_err(|_| {
                    StorageError::InvalidTypedAccount("claimed task id is invalid".to_owned())
                })?);
        }

        let mut statement = connection.prepare(
            "SELECT reset_day, chapter_id, challenge_times, select_ex
             FROM daily_copy_progress WHERE profile_id = ?1 ORDER BY chapter_id",
        )?;
        let daily_rows = statement
            .query_map(params![profile_id.as_str()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (reset_day, chapter_value, challenge_times, select_ex) in daily_rows {
            account.daily_copy.reset_day = non_negative_u32(reset_day, "daily reset day")?;
            let chapter_id = ChapterId::new(positive_u64(chapter_value, "daily chapter id")?)
                .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))?;
            account.daily_copy.challenge_times.insert(
                chapter_id,
                non_negative_u32(challenge_times, "daily challenge times")?,
            );
            account
                .daily_copy
                .select_ex
                .insert(chapter_id, select_ex != 0);
        }

        let mut statement = connection.prepare(
            "SELECT building_id, template_id, level, land_index,
                    production_status, recipe_id, item_count, product_count,
                    last_update_at, recipe_time, productivity, produce_speed
             FROM buildings WHERE profile_id = ?1 ORDER BY building_id",
        )?;
        let buildings = statement
            .query_map(params![profile_id.as_str()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (
            building_id,
            template_id,
            level,
            land_index,
            production_status,
            recipe_id,
            item_count,
            product_count,
            last_update_at,
            recipe_time,
            productivity,
            produce_speed,
        ) in buildings
        {
            let building_id = u64::try_from(building_id).map_err(|_| {
                StorageError::InvalidTypedAccount("building id is invalid".to_owned())
            })?;
            account
                .buildings
                .levels
                .insert(building_id, non_negative_u32(level, "building level")?);
            account.buildings.template_ids.insert(
                building_id,
                non_negative_u64(template_id, "building template id")?,
            );
            account.buildings.land_indices.insert(
                building_id,
                non_negative_u32(land_index, "building land index")?,
            );
            if production_status != 1
                || recipe_id != 0
                || item_count != 0
                || product_count != 0
                || last_update_at != 0
                || recipe_time != 0
                || productivity != 0
                || produce_speed != 0
            {
                account.buildings.productions.insert(
                    building_id,
                    blueoath_domain::BuildingProductionState {
                        status: non_negative_u32(production_status, "building production status")?,
                        recipe_id: non_negative_u32(recipe_id, "building recipe id")?,
                        item_count: non_negative_u32(item_count, "building item count")?,
                        product_count: non_negative_u32(product_count, "building product count")?,
                        last_update_at: non_negative_u64(
                            last_update_at,
                            "building production update time",
                        )?,
                        recipe_time: non_negative_u32(recipe_time, "building recipe time")?,
                        productivity: non_negative_u32(productivity, "building productivity")?,
                        produce_speed: non_negative_u32(produce_speed, "building produce speed")?,
                    },
                );
            }
        }
        let mut statement = connection.prepare(
            "SELECT building_id, position, hero_id
             FROM building_hero_assignments
             WHERE profile_id = ?1 ORDER BY building_id, position",
        )?;
        let assignments = statement
            .query_map(params![profile_id.as_str()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (building_value, position, hero_value) in assignments {
            let building_id = positive_u64(building_value, "building assignment id")?;
            let position = usize::try_from(position).map_err(|_| {
                StorageError::InvalidTypedAccount(
                    "building assignment position is invalid".to_owned(),
                )
            })?;
            let hero_id = positive_hero_id(hero_value, "building assignment hero id")?;
            if !account.buildings.levels.contains_key(&building_id) {
                return Err(StorageError::InvalidTypedAccount(
                    "building assignment references missing building".to_owned(),
                ));
            }
            let members = account
                .buildings
                .hero_assignments
                .entry(building_id)
                .or_default();
            if members.len() <= position {
                members.resize(
                    position + 1,
                    HeroId::new(1)
                        .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))?,
                );
            }
            members[position] = hero_id;
        }
        let mut statement = connection.prepare(
            "SELECT job_id, building_id, started_at, finish_at, state,
                    duration_seconds, project_gold, project_steel, project_aluminium, completed
             FROM construction_jobs WHERE profile_id = ?1 ORDER BY job_id",
        )?;
        let jobs = statement
            .query_map(params![profile_id.as_str()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (
            sequence,
            template_id,
            _started_at,
            finish_at,
            state,
            duration_seconds,
            project_gold,
            project_steel,
            project_aluminium,
            completed,
        ) in jobs
        {
            account
                .buildings
                .construction_jobs
                .push(ConstructionJobState {
                    sequence: positive_u64(sequence, "construction sequence")?,
                    template_id: positive_u64(template_id, "construction template id")?,
                    duration_seconds: non_negative_u32(duration_seconds, "construction duration")?,
                    end_at: non_negative_u64(finish_at, "construction finish time")?,
                    completed: completed != 0 || state == "completed",
                    project: ConstructionProjectState {
                        gold: non_negative_u32(project_gold, "construction gold")?,
                        steel: non_negative_u32(project_steel, "construction steel")?,
                        aluminium: non_negative_u32(project_aluminium, "construction aluminium")?,
                    },
                });
        }
        account.buildings.last_project = account
            .buildings
            .construction_jobs
            .last()
            .map(|job| job.project.clone());

        let mut statement = connection.prepare(
            "SELECT friend_profile_id, relation
             FROM friend_relations WHERE profile_id = ?1 ORDER BY friend_profile_id",
        )?;
        let relations = statement
            .query_map(params![profile_id.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (friend_profile_id, relation) in relations {
            let Ok(uid) = friend_profile_id.parse::<u64>() else {
                continue;
            };
            if uid == 0 {
                continue;
            }
            match relation.as_str() {
                "friend" => {
                    account.social.friends.insert(uid);
                }
                "pending" => {
                    account.social.pending.insert(uid);
                }
                "blacklist" => {
                    account.social.blacklist.insert(uid);
                }
                "applied" => {
                    account.social.applied.insert(uid);
                }
                _ => {}
            }
        }

        if let Some((
            chapter_value,
            copy_value,
            current_fleet,
            started_at,
            expires_at,
            revision,
            attack_count,
        )) = connection
            .query_row(
                "SELECT chapter_id, copy_id, current_fleet, started_at, expires_at, revision,
                        attack_count
                 FROM battle_sessions WHERE profile_id = ?1",
                params![profile_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .optional()?
        {
            let chapter_id = ChapterId::new(positive_u64(chapter_value, "battle chapter id")?)
                .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))?;
            let copy_id = CopyId::new(positive_u64(copy_value, "battle copy id")?)
                .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))?;
            account.battle.active = Some(blueoath_domain::BattleSession {
                chapter_id,
                copy_id,
                current_fleet: non_negative_u32(current_fleet, "battle current fleet")?,
                started_at: non_negative_u64(started_at, "battle start")?,
                expires_at: non_negative_u64(expires_at, "battle expiry")?,
                revision: non_negative_u64(revision, "battle revision")?,
                remaining_fleet_ids: Vec::new(),
                hero_ids: Vec::new(),
                attack_count: non_negative_u32(attack_count, "battle attack count")?,
            });
        }
        if let Some(active) = account.battle.active.as_mut() {
            let mut statement = connection.prepare(
                "SELECT fleet_id FROM battle_session_fleets
                 WHERE profile_id = ?1 ORDER BY position",
            )?;
            active.remaining_fleet_ids = statement
                .query_map(params![profile_id.as_str()], |row| row.get::<_, i64>(0))?
                .map(|value| {
                    value
                        .map_err(StorageError::from)
                        .and_then(|value| non_negative_u32(value, "battle fleet id"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut statement = connection.prepare(
                "SELECT hero_id FROM battle_session_heroes
                 WHERE profile_id = ?1 ORDER BY position",
            )?;
            active.hero_ids = statement
                .query_map(params![profile_id.as_str()], |row| row.get::<_, i64>(0))?
                .map(|value| {
                    value
                        .map_err(StorageError::from)
                        .and_then(|value| positive_hero_id(value, "battle hero id"))
                })
                .collect::<Result<Vec<_>, _>>()?;
        }

        let mut statement = connection.prepare(
            "SELECT copy_id, first_passed
             FROM copy_progress WHERE profile_id = ?1 ORDER BY copy_id",
        )?;
        let passed_copies = statement
            .query_map(params![profile_id.as_str()], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (copy_value, first_passed) in passed_copies {
            if first_passed != 0 {
                let copy_id = CopyId::new(positive_u64(copy_value, "passed copy id")?)
                    .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))?;
                account.battle.passed_copies.insert(copy_id);
            }
        }

        account.chat.channel = connection
            .query_row(
                "SELECT channel FROM chat_state WHERE profile_id = ?1",
                params![profile_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .map(|channel| non_negative_u32(channel, "chat channel"))
            .transpose()?
            .unwrap_or_default();
        let mut statement = connection.prepare(
            "SELECT message_id, channel, sender_uid, receive_uid, body,
                    message_type, voice, sent_at
             FROM chat_messages WHERE profile_id = ?1 ORDER BY sent_at, message_id",
        )?;
        for row in statement.query_map(params![profile_id.as_str()], |row| {
            Ok(ChatMessageState {
                id: row.get::<_, i64>(0)? as u64,
                channel: row.get::<_, i64>(1)? as u32,
                uid: row.get::<_, i64>(2)? as u64,
                receive_uid: row.get::<_, i64>(3)? as u64,
                message: row.get(4)?,
                message_type: row.get::<_, i64>(5)? as u32,
                voice: row.get(6)?,
                sent_at: row.get::<_, i64>(7)? as u64,
            })
        })? {
            account.chat.messages.push(row?);
        }
        let mut statement = connection.prepare(
            "SELECT barrage_id, offset_value, content, uid, sent_at
             FROM chat_barrages WHERE profile_id = ?1
             ORDER BY barrage_id, offset_value, sent_at",
        )?;
        for row in statement.query_map(params![profile_id.as_str()], |row| {
            Ok(ChatBarrageState {
                id: row.get::<_, i64>(0)? as u32,
                offset: row.get::<_, i64>(1)? as u32,
                content: row.get(2)?,
                uid: row.get::<_, i64>(3)? as u64,
                sent_at: row.get::<_, i64>(4)? as u64,
            })
        })? {
            account.chat.barrages.push(row?);
        }
        let mut statement = connection.prepare(
            "SELECT activity_id, progress_kind, value
             FROM activity_progress WHERE profile_id = ?1
             ORDER BY activity_id, progress_kind",
        )?;
        for row in statement.query_map(params![profile_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })? {
            let (activity_id, progress_kind, value) = row?;
            account.activities.progress.insert(
                format!("{activity_id}\u{1f}{progress_kind}"),
                non_negative_u64(value, "activity progress")?,
            );
        }

        if let Some(values) = connection
            .query_row(
                "SELECT chapter_id, area_index, copy_index, topic_index, daily_count,
                        reset_time, pass_last_chapter_id, is_reset, max_level, max_area,
                        max_copy, daily_count_ex, is_new_level
                 FROM tower_progress WHERE profile_id = ?1",
                params![profile_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, i64>(10)?,
                        row.get::<_, i64>(11)?,
                        row.get::<_, i64>(12)?,
                    ))
                },
            )
            .optional()?
        {
            account.tower.chapter_id = non_negative_u32(values.0, "tower chapter id")?;
            account.tower.area_index = non_negative_u32(values.1, "tower area index")?;
            account.tower.copy_index = non_negative_u32(values.2, "tower copy index")?;
            account.tower.topic_index = non_negative_u32(values.3, "tower topic index")?;
            account.tower.daily_count = non_negative_u32(values.4, "tower daily count")?;
            account.tower.reset_time = non_negative_u64(values.5, "tower reset time")?;
            account.tower.pass_last_chapter_id = non_negative_u32(values.6, "tower last chapter")?;
            account.tower.is_reset = values.7 != 0;
            account.tower.max_level = non_negative_u32(values.8, "tower max level")?;
            account.tower.max_area = non_negative_u32(values.9, "tower max area")?;
            account.tower.max_copy = non_negative_u32(values.10, "tower max copy")?;
            account.tower.daily_count_ex = non_negative_u32(values.11, "tower daily count ex")?;
            account.tower.is_new_level = values.12 != 0;
        }
        let mut statement = connection.prepare(
            "SELECT sf_id, count FROM tower_sf_counts
             WHERE profile_id = ?1 ORDER BY sf_id",
        )?;
        for row in statement.query_map(params![profile_id.as_str()], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })? {
            let (sf_id, count) = row?;
            account.tower.sf_id_counts.insert(
                positive_u64(sf_id, "tower sf id")?,
                non_negative_u32(count, "tower sf count")?,
            );
        }
        let mut statement = connection.prepare(
            "SELECT kind, value FROM tower_ids
             WHERE profile_id = ?1 ORDER BY kind, position",
        )?;
        for row in statement.query_map(params![profile_id.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })? {
            let (kind, value) = row?;
            match kind.as_str() {
                "hero" => account
                    .tower
                    .hero_ids
                    .push(positive_hero_id(value, "tower hero id")?),
                "lock_equip" => account
                    .tower
                    .lock_equip_ids
                    .push(positive_equip_id(value, "tower equipment id")?),
                "save_copy" => account.tower.save_pass_copy_ids.push(
                    CopyId::new(positive_u64(value, "tower saved copy id")?)
                        .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))?,
                ),
                _ => {}
            }
        }
        let mut statement = connection.prepare(
            "SELECT reward_type, config_id, amount, instance_id
             FROM tower_rewards WHERE profile_id = ?1 ORDER BY position",
        )?;
        for row in statement.query_map(params![profile_id.as_str()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })? {
            let (reward_type, config_id, amount, instance_id) = row?;
            account.tower.pending_rewards.push(TowerRewardState {
                reward_type: non_negative_u32(reward_type, "tower reward type")?,
                config_id: non_negative_u64(config_id, "tower reward config id")?,
                amount: non_negative_u64(amount, "tower reward amount")?,
                instance_id: non_negative_u64(instance_id, "tower reward instance id")?,
            });
        }
        if let Some(values) = connection
            .query_row(
                "SELECT activity_id, reset_time, small_reset_number, quick_number, history_max
                 FROM activity_tower_progress WHERE profile_id = ?1",
                params![profile_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?
        {
            account.activity_tower = ActivityTowerState {
                activity_id: non_negative_u32(values.0, "activity tower id")?,
                reset_time: non_negative_u64(values.1, "activity tower reset time")?,
                small_reset_number: non_negative_u32(values.2, "activity tower reset count")?,
                quick_number: non_negative_u32(values.3, "activity tower quick count")?,
                history_max: non_negative_u32(values.4, "activity tower history max")?,
                ..ActivityTowerState::default()
            };
        }
        let mut statement = connection.prepare(
            "SELECT kind, value FROM activity_tower_ids
             WHERE profile_id = ?1 ORDER BY kind, position",
        )?;
        for row in statement.query_map(params![profile_id.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })? {
            let (kind, value) = row?;
            let copy_id = || {
                CopyId::new(positive_u64(value, "activity tower copy id")?)
                    .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))
            };
            match kind.as_str() {
                "save_copy" => account.activity_tower.save_pass_copy_ids.push(copy_id()?),
                "pass_copy" => account.activity_tower.pass_copy_ids.push(copy_id()?),
                "lock_equip" => account
                    .activity_tower
                    .lock_equip_ids
                    .push(positive_equip_id(value, "activity tower equipment id")?),
                "hero" => account
                    .activity_tower
                    .hero_ids
                    .push(positive_hero_id(value, "activity tower hero id")?),
                "save_stage_copy" => account
                    .activity_tower
                    .save_pass_stage_copy_ids
                    .push(copy_id()?),
                _ => {}
            }
        }
        account
            .validate()
            .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))?;
        Ok(Some(account))
    }

    pub fn save_typed_account(&self, account: &mut AccountState) -> Result<(), StorageError> {
        account
            .validate()
            .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))?;
        let expected_revision = account.profile.as_ref().map(|profile| profile.revision);
        let next_revision = self.save_typed_account_with_revision(account, expected_revision)?;
        if let Some(profile) = account.profile.as_mut() {
            profile.revision = next_revision;
        }
        Ok(())
    }

    pub fn save(
        &self,
        profile_id: &str,
        name: &str,
        state: &StoredProfileState,
    ) -> Result<(), StorageError> {
        if !is_valid_profile_id(profile_id) {
            return Err(StorageError::InvalidProfileId);
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO profiles(id, name, updated_utc)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               updated_utc = excluded.updated_utc",
            params![profile_id, name, timestamp()],
        )?;
        transaction.execute(
            "DELETE FROM profile_formation WHERE profile_id = ?1",
            params![profile_id],
        )?;
        transaction.execute(
            "DELETE FROM profile_ships WHERE profile_id = ?1",
            params![profile_id],
        )?;
        transaction.execute(
            "DELETE FROM profile_runtime WHERE profile_id = ?1",
            params![profile_id],
        )?;
        transaction.execute(
            "INSERT INTO profile_runtime(profile_id, level, fuel, coins, completed_stages)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                profile_id,
                state.level,
                state.fuel,
                state.coins,
                state.completed_stages,
            ],
        )?;
        for ship in &state.ships {
            if ship.id <= 0 {
                continue;
            }
            transaction.execute(
                "INSERT INTO profile_ships(profile_id, ship_id, name, level, power)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                params![profile_id, ship.id, ship.name, ship.level, ship.power,],
            )?;
        }
        for (position, ship_id) in state.formation_ship_ids.iter().enumerate() {
            if *ship_id <= 0 {
                continue;
            }
            transaction.execute(
                "INSERT INTO profile_formation(profile_id, position, ship_id)
                     VALUES (?1, ?2, ?3)",
                params![profile_id, position as i64, ship_id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    fn save_typed_account_with_revision(
        &self,
        account: &AccountState,
        expected_revision: Option<u64>,
    ) -> Result<u64, StorageError> {
        let profile = account.profile.as_ref().ok_or_else(|| {
            StorageError::InvalidTypedAccount("account profile is required".to_owned())
        })?;
        if !is_valid_profile_id(profile.id.as_str()) {
            return Err(StorageError::InvalidProfileId);
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let actual = transaction
            .query_row(
                "SELECT revision FROM account_revisions WHERE profile_id = ?1",
                params![profile.id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or_default();
        let actual = u64::try_from(actual)
            .map_err(|_| StorageError::InvalidTypedAccount("negative revision".to_owned()))?;
        if expected_revision != Some(actual) && expected_revision.is_some() {
            return Err(StorageError::RevisionConflict {
                expected: expected_revision.unwrap_or_default(),
                actual,
            });
        }
        let next_revision = actual.checked_add(1).ok_or_else(|| {
            StorageError::InvalidTypedAccount("account revision overflow".to_owned())
        })?;
        let sql_revision = i64::try_from(next_revision).map_err(|_| {
            StorageError::InvalidTypedAccount("account revision exceeds SQLite range".to_owned())
        })?;

        transaction.execute(
            "INSERT INTO profiles(id, name, updated_utc)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               updated_utc = excluded.updated_utc",
            params![profile.id.as_str(), profile.name, timestamp()],
        )?;
        clear_normalized_account(&transaction, profile.id.as_str())?;

        let character = &account.character;
        transaction.execute(
            "INSERT INTO characters(
                profile_id, uid, name, level, exp, class_id, create_time, message,
                secretary_id, gold, diamond, supply, pve_pt, head, head_frame
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                profile.id.as_str(),
                typed_i64(character.uid, "character uid")?,
                character.name,
                typed_i64(character.level, "character level")?,
                typed_i64(character.exp, "character exp")?,
                typed_i64(character.class_id, "character class")?,
                typed_i64(character.create_time, "character create time")?,
                character.message,
                character
                    .secretary_id
                    .map(|id| typed_i64(id.get(), "secretary id"))
                    .transpose()?
                    .unwrap_or_default(),
                typed_i64(account.resources.amount(CurrencyKind::Gold).get(), "gold")?,
                typed_i64(
                    account.resources.amount(CurrencyKind::Diamond).get(),
                    "diamond"
                )?,
                typed_i64(
                    account.resources.amount(CurrencyKind::Supply).get(),
                    "supply"
                )?,
                typed_i64(
                    account.resources.amount(CurrencyKind::PvePoint).get(),
                    "pve point"
                )?,
                typed_i64(character.head, "character head")?,
                typed_i64(character.head_frame, "character head frame")?,
            ],
        )?;

        transaction.execute(
            "INSERT INTO chat_state(profile_id, channel)
             VALUES (?1, ?2)
             ON CONFLICT(profile_id) DO UPDATE SET channel = excluded.channel",
            params![
                profile.id.as_str(),
                typed_i64(account.chat.channel, "chat channel")?
            ],
        )?;
        for (template_id, amount) in &account.inventory.items {
            transaction.execute(
                "INSERT INTO inventory(profile_id, template_id, amount)
                 VALUES (?1, ?2, ?3)",
                params![
                    profile.id.as_str(),
                    typed_i64(template_id.get(), "inventory template id")?,
                    typed_i64(*amount, "inventory amount")?,
                ],
            )?;
        }
        for message in &account.chat.messages {
            transaction.execute(
                "INSERT INTO chat_messages(
                    profile_id, message_id, channel, sender_uid, body, sent_at,
                    receive_uid, message_type, voice
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    profile.id.as_str(),
                    typed_i64(message.id, "chat message id")?,
                    typed_i64(message.channel, "chat message channel")?,
                    typed_i64(message.uid, "chat sender uid")?,
                    message.message,
                    typed_i64(message.sent_at, "chat message time")?,
                    typed_i64(message.receive_uid, "chat receiver uid")?,
                    typed_i64(message.message_type, "chat message type")?,
                    message.voice,
                ],
            )?;
        }
        for barrage in &account.chat.barrages {
            transaction.execute(
                "INSERT INTO chat_barrages(
                    profile_id, barrage_id, offset_value, content, uid, sent_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    profile.id.as_str(),
                    typed_i64(barrage.id, "barrage id")?,
                    typed_i64(barrage.offset, "barrage offset")?,
                    barrage.content,
                    typed_i64(barrage.uid, "barrage uid")?,
                    typed_i64(barrage.sent_at, "barrage time")?,
                ],
            )?;
        }

        for hero in account.dock.heroes.values() {
            transaction.execute(
                "INSERT INTO heroes(
                profile_id, hero_id, template_id, name, change_name_time,
                    level, exp, mood, affection, hp, lock_state, created_utc
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    profile.id.as_str(),
                    typed_i64(hero.id.get(), "hero id")?,
                    typed_i64(hero.template_id.get(), "hero template id")?,
                    hero.name,
                    typed_i64(hero.change_name_time, "hero change name time")?,
                    typed_i64(hero.level, "hero level")?,
                    typed_i64(hero.exp, "hero exp")?,
                    typed_i64(hero.mood, "hero mood")?,
                    typed_i64(hero.affection, "hero affection")?,
                    typed_i64(hero.hp, "hero hp")?,
                    i64::from(hero.locked),
                    timestamp(),
                ],
            )?;
        }
        for equipment in account.dock.equipments.values() {
            transaction.execute(
                "INSERT INTO equipments(
                    profile_id, equip_id, template_id, enhance_level, star,
                    enhance_exp, hero_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    profile.id.as_str(),
                    typed_i64(equipment.id.get(), "equipment id")?,
                    typed_i64(equipment.template_id.get(), "equipment template id")?,
                    typed_i64(equipment.enhance_level, "equipment enhance level")?,
                    typed_i64(equipment.star, "equipment star")?,
                    typed_i64(equipment.enhance_exp, "equipment enhance exp")?,
                    equipment
                        .hero_id
                        .map(|id| typed_i64(id.get(), "equipment hero id"))
                        .transpose()?,
                ],
            )?;
        }
        for hero in account.dock.heroes.values() {
            for (slot_index, equip_id) in hero.equip_slots.iter().enumerate() {
                transaction.execute(
                    "INSERT INTO hero_equip_slots(
                        profile_id, hero_id, slot_index, equip_id
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        profile.id.as_str(),
                        typed_i64(hero.id.get(), "hero id")?,
                        typed_i64(slot_index, "equipment slot")?,
                        equip_id
                            .map(|id| typed_i64(id.get(), "slot equipment id"))
                            .transpose()?,
                    ],
                )?;
            }
        }
        for (fleet_id, fleet) in &account.fleet.fleets {
            transaction.execute(
                "INSERT INTO fleets(profile_id, fleet_id, formation_id, tactic_id)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    profile.id.as_str(),
                    typed_i64(fleet_id.get(), "fleet id")?,
                    typed_i64(fleet.formation_id, "formation id")?,
                    typed_i64(fleet.tactic_id, "tactic id")?,
                ],
            )?;
            for (position, hero_id) in fleet.members.iter().enumerate() {
                transaction.execute(
                    "INSERT INTO fleet_members(profile_id, fleet_id, position, hero_id)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        profile.id.as_str(),
                        typed_i64(fleet_id.get(), "fleet id")?,
                        typed_i64(position, "fleet member position")?,
                        typed_i64(hero_id.get(), "fleet member hero id")?,
                    ],
                )?;
            }
        }
        transaction.execute(
            "INSERT INTO preset_fleet_meta(profile_id, name_num, red_dot)
             VALUES (?1, ?2, ?3)",
            params![
                profile.id.as_str(),
                typed_i64(account.fleet.preset_name_num, "preset fleet name number")?,
                typed_i64(account.fleet.preset_red_dot, "preset fleet red dot")?,
            ],
        )?;
        for (slot, preset) in account.fleet.presets.iter().enumerate() {
            transaction.execute(
                "INSERT INTO preset_fleets(
                    profile_id, slot, name, mode_id, strategy_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    profile.id.as_str(),
                    typed_i64(slot, "preset fleet slot")?,
                    preset.name,
                    typed_i64(preset.mode_id, "preset fleet mode")?,
                    typed_i64(preset.strategy_id, "preset fleet strategy")?,
                ],
            )?;
            for (is_ex, members) in [(0_i64, &preset.hero_ids), (1_i64, &preset.ex_hero_ids)] {
                for (position, hero_id) in members.iter().enumerate() {
                    transaction.execute(
                        "INSERT INTO preset_fleet_members(
                            profile_id, slot, position, hero_id, is_ex
                         ) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            profile.id.as_str(),
                            typed_i64(slot, "preset fleet slot")?,
                            typed_i64(position, "preset fleet position")?,
                            typed_i64(hero_id.get(), "preset fleet hero id")?,
                            is_ex,
                        ],
                    )?;
                }
            }
        }
        for (task_id, progress) in &account.tasks.progress {
            transaction.execute(
                "INSERT INTO tasks(profile_id, task_id, task_type, progress, completed, reset_day)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0)",
                params![
                    profile.id.as_str(),
                    typed_i64(*task_id, "task id")?,
                    typed_i64(
                        account
                            .tasks
                            .task_types
                            .get(task_id)
                            .copied()
                            .unwrap_or_default(),
                        "task type",
                    )?,
                    typed_i64(*progress, "task progress")?,
                    i64::from(account.tasks.completed.contains(task_id)),
                ],
            )?;
        }
        for task_id in &account.tasks.claimed {
            transaction.execute(
                "INSERT INTO task_claims(profile_id, task_id, claimed_at)
                 VALUES (?1, ?2, ?3)",
                params![
                    profile.id.as_str(),
                    typed_i64(*task_id, "claimed task id")?,
                    timestamp(),
                ],
            )?;
        }
        for (chapter_id, challenge_times) in &account.daily_copy.challenge_times {
            transaction.execute(
                "INSERT INTO daily_copy_progress(
                    profile_id, reset_day, chapter_id, group_id, challenge_times,
                    success_times, select_ex, extra_group
                ) VALUES (?1, ?2, ?3, 1, ?4, 0, ?5, 0)",
                params![
                    profile.id.as_str(),
                    typed_i64(account.daily_copy.reset_day, "daily reset day")?,
                    typed_i64(chapter_id.get(), "daily chapter id")?,
                    typed_i64(*challenge_times, "daily challenge times")?,
                    i64::from(
                        account
                            .daily_copy
                            .select_ex
                            .get(chapter_id)
                            .copied()
                            .unwrap_or(false),
                    ),
                ],
            )?;
        }
        for copy_id in &account.battle.passed_copies {
            transaction.execute(
                "INSERT INTO copy_progress(
                    profile_id, copy_id, star_level, first_passed
                 ) VALUES (?1, ?2, 0, 1)",
                params![
                    profile.id.as_str(),
                    typed_i64(copy_id.get(), "passed copy id")?,
                ],
            )?;
        }
        for (building_id, level) in &account.buildings.levels {
            let production = account.buildings.productions.get(building_id);
            transaction.execute(
                "INSERT INTO buildings(
                    profile_id, building_id, level, land_index, template_id,
                    production_status, recipe_id, item_count, product_count,
                    last_update_at, recipe_time, productivity, produce_speed
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    profile.id.as_str(),
                    typed_i64(*building_id, "building id")?,
                    typed_i64(*level, "building level")?,
                    typed_i64(
                        account
                            .buildings
                            .land_indices
                            .get(building_id)
                            .copied()
                            .unwrap_or_default(),
                        "building land index",
                    )?,
                    typed_i64(
                        account
                            .buildings
                            .template_ids
                            .get(building_id)
                            .copied()
                            .unwrap_or(*building_id),
                        "building template id",
                    )?,
                    typed_i64(
                        production.map(|value| value.status).unwrap_or(1),
                        "building production status",
                    )?,
                    typed_i64(
                        production.map(|value| value.recipe_id).unwrap_or_default(),
                        "building recipe id",
                    )?,
                    typed_i64(
                        production.map(|value| value.item_count).unwrap_or_default(),
                        "building item count",
                    )?,
                    typed_i64(
                        production
                            .map(|value| value.product_count)
                            .unwrap_or_default(),
                        "building product count",
                    )?,
                    typed_i64(
                        production
                            .map(|value| value.last_update_at)
                            .unwrap_or_default(),
                        "building production update time",
                    )?,
                    typed_i64(
                        production
                            .map(|value| value.recipe_time)
                            .unwrap_or_default(),
                        "building recipe time",
                    )?,
                    typed_i64(
                        production
                            .map(|value| value.productivity)
                            .unwrap_or_default(),
                        "building productivity",
                    )?,
                    typed_i64(
                        production
                            .map(|value| value.produce_speed)
                            .unwrap_or_default(),
                        "building produce speed",
                    )?,
                ],
            )?;
        }
        for (building_id, hero_ids) in &account.buildings.hero_assignments {
            for (position, hero_id) in hero_ids.iter().enumerate() {
                transaction.execute(
                    "INSERT INTO building_hero_assignments(
                        profile_id, building_id, position, hero_id
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        profile.id.as_str(),
                        typed_i64(*building_id, "building assignment id")?,
                        typed_i64(position, "building assignment position")?,
                        typed_i64(hero_id.get(), "building assignment hero id")?,
                    ],
                )?;
            }
        }
        for job in &account.buildings.construction_jobs {
            transaction.execute(
                "INSERT INTO construction_jobs(
                    profile_id, job_id, building_id, started_at, finish_at, state,
                    duration_seconds, project_gold, project_steel, project_aluminium, completed
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    profile.id.as_str(),
                    typed_i64(job.sequence, "construction sequence")?,
                    typed_i64(job.template_id, "construction template id")?,
                    0_i64,
                    typed_i64(job.end_at, "construction finish time")?,
                    if job.completed {
                        "completed"
                    } else if job.end_at == 0 {
                        "waiting"
                    } else {
                        "active"
                    },
                    typed_i64(job.duration_seconds, "construction duration")?,
                    typed_i64(job.project.gold, "construction gold")?,
                    typed_i64(job.project.steel, "construction steel")?,
                    typed_i64(job.project.aluminium, "construction aluminium")?,
                    i64::from(job.completed),
                ],
            )?;
        }
        for (relation, ids) in [
            ("friend", &account.social.friends),
            ("pending", &account.social.pending),
            ("blacklist", &account.social.blacklist),
            ("applied", &account.social.applied),
        ] {
            for friend_profile_id in ids {
                transaction.execute(
                    "INSERT INTO friend_relations(
                        profile_id, friend_profile_id, relation, created_at
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        profile.id.as_str(),
                        friend_profile_id.to_string(),
                        relation,
                        timestamp(),
                    ],
                )?;
            }
        }
        if let Some(session) = &account.battle.active {
            transaction.execute(
                "INSERT INTO battle_sessions(
                    profile_id, chapter_id, copy_id, current_fleet, state,
                    started_at, expires_at, revision, attack_count
                 ) VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, ?7, ?8)",
                params![
                    profile.id.as_str(),
                    typed_i64(session.chapter_id.get(), "battle chapter id")?,
                    typed_i64(session.copy_id.get(), "battle copy id")?,
                    typed_i64(session.current_fleet, "battle current fleet")?,
                    typed_i64(session.started_at, "battle start")?,
                    typed_i64(session.expires_at, "battle expiry")?,
                    typed_i64(session.revision, "battle revision")?,
                    typed_i64(session.attack_count, "battle attack count")?,
                ],
            )?;
            for (position, fleet_id) in session.remaining_fleet_ids.iter().enumerate() {
                transaction.execute(
                    "INSERT INTO battle_session_fleets(profile_id, position, fleet_id)
                     VALUES (?1, ?2, ?3)",
                    params![
                        profile.id.as_str(),
                        typed_i64(position, "battle fleet position")?,
                        typed_i64(*fleet_id, "battle fleet id")?,
                    ],
                )?;
            }
            for (position, hero_id) in session.hero_ids.iter().enumerate() {
                transaction.execute(
                    "INSERT INTO battle_session_heroes(profile_id, position, hero_id)
                     VALUES (?1, ?2, ?3)",
                    params![
                        profile.id.as_str(),
                        typed_i64(position, "battle hero position")?,
                        typed_i64(hero_id.get(), "battle hero id")?,
                    ],
                )?;
            }
        }
        transaction.execute(
            "INSERT INTO tower_progress(
                profile_id, chapter_id, floor, reset_day, area_index, copy_index,
                topic_index, daily_count, reset_time, pass_last_chapter_id, is_reset,
                max_level, max_area, max_copy, daily_count_ex, is_new_level
             ) VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                profile.id.as_str(),
                typed_i64(account.tower.chapter_id, "tower chapter id")?,
                typed_i64(account.tower.area_index, "tower floor")?,
                typed_i64(account.tower.area_index, "tower area index")?,
                typed_i64(account.tower.copy_index, "tower copy index")?,
                typed_i64(account.tower.topic_index, "tower topic index")?,
                typed_i64(account.tower.daily_count, "tower daily count")?,
                typed_i64(account.tower.reset_time, "tower reset time")?,
                typed_i64(account.tower.pass_last_chapter_id, "tower last chapter")?,
                i64::from(account.tower.is_reset),
                typed_i64(account.tower.max_level, "tower max level")?,
                typed_i64(account.tower.max_area, "tower max area")?,
                typed_i64(account.tower.max_copy, "tower max copy")?,
                typed_i64(account.tower.daily_count_ex, "tower daily count ex")?,
                i64::from(account.tower.is_new_level),
            ],
        )?;
        for (sf_id, count) in &account.tower.sf_id_counts {
            transaction.execute(
                "INSERT INTO tower_sf_counts(profile_id, sf_id, count) VALUES (?1, ?2, ?3)",
                params![
                    profile.id.as_str(),
                    typed_i64(*sf_id, "tower sf id")?,
                    typed_i64(*count, "tower sf count")?,
                ],
            )?;
        }
        let insert_tower_id = |kind: &str, position: usize, value: u64| {
            transaction.execute(
                "INSERT INTO tower_ids(profile_id, kind, position, value)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    profile.id.as_str(),
                    kind,
                    typed_i64(position, "tower id position")?,
                    typed_i64(value, "tower id value")?,
                ],
            )?;
            Ok::<(), StorageError>(())
        };
        for (position, hero_id) in account.tower.hero_ids.iter().enumerate() {
            insert_tower_id("hero", position, hero_id.get())?;
        }
        for (position, equip_id) in account.tower.lock_equip_ids.iter().enumerate() {
            insert_tower_id("lock_equip", position, equip_id.get())?;
        }
        for (position, copy_id) in account.tower.save_pass_copy_ids.iter().enumerate() {
            insert_tower_id("save_copy", position, copy_id.get())?;
        }
        for (position, reward) in account.tower.pending_rewards.iter().enumerate() {
            transaction.execute(
                "INSERT INTO tower_rewards(
                    profile_id, position, reward_type, config_id, amount, instance_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    profile.id.as_str(),
                    typed_i64(position, "tower reward position")?,
                    typed_i64(reward.reward_type, "tower reward type")?,
                    typed_i64(reward.config_id, "tower reward config id")?,
                    typed_i64(reward.amount, "tower reward amount")?,
                    typed_i64(reward.instance_id, "tower reward instance id")?,
                ],
            )?;
        }
        transaction.execute(
            "INSERT INTO activity_tower_progress(
                profile_id, activity_id, reset_time, small_reset_number, quick_number, history_max
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                profile.id.as_str(),
                typed_i64(account.activity_tower.activity_id, "activity tower id")?,
                typed_i64(
                    account.activity_tower.reset_time,
                    "activity tower reset time"
                )?,
                typed_i64(
                    account.activity_tower.small_reset_number,
                    "activity tower reset count",
                )?,
                typed_i64(
                    account.activity_tower.quick_number,
                    "activity tower quick count"
                )?,
                typed_i64(
                    account.activity_tower.history_max,
                    "activity tower history max"
                )?,
            ],
        )?;
        let insert_activity_tower_id = |kind: &str, position: usize, value: u64| {
            transaction.execute(
                "INSERT INTO activity_tower_ids(profile_id, kind, position, value)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    profile.id.as_str(),
                    kind,
                    typed_i64(position, "activity tower id position")?,
                    typed_i64(value, "activity tower id value")?,
                ],
            )?;
            Ok::<(), StorageError>(())
        };
        for (position, copy_id) in account.activity_tower.save_pass_copy_ids.iter().enumerate() {
            insert_activity_tower_id("save_copy", position, copy_id.get())?;
        }
        for (position, copy_id) in account.activity_tower.pass_copy_ids.iter().enumerate() {
            insert_activity_tower_id("pass_copy", position, copy_id.get())?;
        }
        for (position, equip_id) in account.activity_tower.lock_equip_ids.iter().enumerate() {
            insert_activity_tower_id("lock_equip", position, equip_id.get())?;
        }
        for (position, hero_id) in account.activity_tower.hero_ids.iter().enumerate() {
            insert_activity_tower_id("hero", position, hero_id.get())?;
        }
        for (position, copy_id) in account
            .activity_tower
            .save_pass_stage_copy_ids
            .iter()
            .enumerate()
        {
            insert_activity_tower_id("save_stage_copy", position, copy_id.get())?;
        }
        for (key, value) in &account.activities.progress {
            let (activity_id, progress_kind) = key.split_once('\u{1f}').unwrap_or((key, "value"));
            transaction.execute(
                "INSERT INTO activity_progress(
                    profile_id, activity_id, progress_kind, value, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    profile.id.as_str(),
                    activity_id,
                    progress_kind,
                    typed_i64(*value, "activity progress")?,
                    timestamp(),
                ],
            )?;
        }
        transaction.execute(
            "INSERT INTO account_revisions(profile_id, revision, updated_utc)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(profile_id) DO UPDATE SET
               revision = excluded.revision,
               updated_utc = excluded.updated_utc",
            params![profile.id.as_str(), sql_revision, timestamp()],
        )?;
        transaction.commit()?;
        Ok(next_revision)
    }

    pub fn list(&self) -> Result<Vec<String>, StorageError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT id FROM profiles ORDER BY id")?;
        let profiles = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(profiles)
    }

    pub fn list_typed_accounts(&self) -> Result<Vec<AccountState>, StorageError> {
        let profile_ids = self.list()?;
        profile_ids
            .into_iter()
            .map(|profile_id| {
                let profile_id = ProfileId::new(profile_id)
                    .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))?;
                self.load_typed_account(&profile_id)
                    .map(|account| account.into_iter().collect::<Vec<_>>())
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|accounts| accounts.into_iter().flatten().collect())
    }

    pub fn reset(&self, profile_id: &str) -> Result<(), StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM profiles WHERE id = ?1", params![profile_id])?;
        transaction.execute(
            "DELETE FROM account_revisions WHERE profile_id = ?1",
            params![profile_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn connection(&self) -> Result<Connection, StorageError> {
        let connection = Connection::open(&self.db_path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        Ok(connection)
    }
}

fn positive_u64(value: i64, field: &str) -> Result<u64, StorageError> {
    u64::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| StorageError::InvalidTypedAccount(format!("{field} must be positive")))
}

fn typed_i64<T>(value: T, field: &str) -> Result<i64, StorageError>
where
    T: TryInto<i64>,
{
    value
        .try_into()
        .map_err(|_| StorageError::InvalidTypedAccount(format!("{field} exceeds SQLite range")))
}

fn clear_normalized_account(
    transaction: &Transaction<'_>,
    profile_id: &str,
) -> Result<(), StorageError> {
    for table in [
        "task_claims",
        "building_hero_assignments",
        "preset_fleet_members",
        "preset_fleets",
        "preset_fleet_meta",
        "construction_jobs",
        "buildings",
        "daily_copy_progress",
        "copy_progress",
        "sea_progress",
        "tower_progress",
        "activity_progress",
        "tasks",
        "chat_barrages",
        "chat_state",
        "battle_session_fleets",
        "battle_session_heroes",
        "battle_sessions",
        "activity_tower_ids",
        "activity_tower_progress",
        "tower_rewards",
        "tower_ids",
        "tower_sf_counts",
        "tower_progress",
        "fleet_members",
        "fleets",
        "hero_equip_slots",
        "equipments",
        "heroes",
        "inventory",
        "characters",
        "friend_relations",
        "chat_messages",
    ] {
        transaction.execute(
            &format!("DELETE FROM {table} WHERE profile_id = ?1"),
            params![profile_id],
        )?;
    }
    Ok(())
}

fn non_negative_u64(value: i64, field: &str) -> Result<u64, StorageError> {
    u64::try_from(value)
        .map_err(|_| StorageError::InvalidTypedAccount(format!("{field} must be non-negative")))
}

fn non_negative_u32(value: i64, field: &str) -> Result<u32, StorageError> {
    u32::try_from(value)
        .map_err(|_| StorageError::InvalidTypedAccount(format!("{field} is out of range")))
}

fn positive_u32(value: i64, field: &str) -> Result<u32, StorageError> {
    let value = non_negative_u32(value, field)?;
    (value > 0)
        .then_some(value)
        .ok_or_else(|| StorageError::InvalidTypedAccount(format!("{field} must be positive")))
}

fn positive_hero_id(value: i64, field: &str) -> Result<HeroId, StorageError> {
    HeroId::new(positive_u64(value, field)?)
        .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))
}

fn positive_equip_id(value: i64, field: &str) -> Result<EquipId, StorageError> {
    EquipId::new(positive_u64(value, field)?)
        .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))
}

fn positive_fleet_id(value: i64, field: &str) -> Result<FleetId, StorageError> {
    FleetId::new(positive_u64(value, field)?)
        .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))
}

fn positive_template_id(value: i64, field: &str) -> Result<TemplateId, StorageError> {
    TemplateId::new(positive_u64(value, field)?)
        .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))
}

impl AccountRepository for ProfileStore {
    fn load(&self, profile_id: &ProfileId) -> Result<Option<AccountState>, RepositoryError> {
        self.load_typed_account(profile_id)
            .map_err(|error| RepositoryError::Storage(error.to_string()))
    }

    fn create(&self, account: &AccountState) -> Result<(), RepositoryError> {
        if account.profile.is_none() {
            return Err(RepositoryError::Storage(
                "account profile is required for creation".to_owned(),
            ));
        }
        account.validate()?;
        self.save_typed_account_with_revision(account, None)
            .map(|_| ())
            .map_err(|error| RepositoryError::Storage(error.to_string()))
    }

    fn transact<F, T>(&self, profile_id: &ProfileId, operation: F) -> Result<T, RepositoryError>
    where
        F: FnOnce(&mut AccountState) -> Result<T, blueoath_domain::DomainError>,
    {
        let existing = self
            .load_typed_account(profile_id)
            .map_err(|error| RepositoryError::Storage(error.to_string()))?
            .unwrap_or_else(|| NewAccountFactory::create(profile_id.clone(), profile_id.as_str()));
        let expected_revision = existing.profile.as_ref().map(|profile| profile.revision);
        let mut account = existing;
        let result = operation(&mut account)?;
        account.validate()?;
        let next_revision = self
            .save_typed_account_with_revision(&account, expected_revision)
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        if let Some(profile) = account.profile.as_mut() {
            profile.revision = next_revision;
        }
        Ok(result)
    }
}

const MIGRATIONS: &[&str] = &[
    include_str!("../../../migrations/0001_schema_meta.sql"),
    include_str!("../../../migrations/0002_profiles_accounts.sql"),
    include_str!("../../../migrations/0003_account_revisions.sql"),
    include_str!("../../../migrations/0004_core_account.sql"),
    include_str!("../../../migrations/0005_core_indexes.sql"),
    include_str!("../../../migrations/0006_progress_social_activity.sql"),
    include_str!("../../../migrations/0007_normalized_profile_runtime.sql"),
    include_str!("../../../migrations/0008_character_profile_fields.sql"),
    include_str!("../../../migrations/0009_chat_state.sql"),
    include_str!("../../../migrations/0010_building_template_id.sql"),
    include_str!("../../../migrations/0011_preset_fleets.sql"),
    include_str!("../../../migrations/0012_building_hero_assignments.sql"),
    include_str!("../../../migrations/0013_hero_names.sql"),
    include_str!("../../../migrations/0014_building_production.sql"),
    include_str!("../../../migrations/0015_construction_typed_state.sql"),
    include_str!("../../../migrations/0016_battle_session_typed_state.sql"),
    include_str!("../../../migrations/0017_drop_json_accounts.sql"),
    include_str!("../../../migrations/0018_typed_tower_state.sql"),
];

fn run_migrations(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(MIGRATIONS[0])?;
    let current: usize =
        connection.query_row("SELECT version FROM schema_meta WHERE id = 1", [], |row| {
            row.get::<_, i64>(0)
        })? as usize;
    if current > MIGRATIONS.len() {
        return Err(StorageError::Sqlite(rusqlite::Error::InvalidQuery));
    }
    for (index, migration) in MIGRATIONS.iter().enumerate().skip(current) {
        let version = index + 1;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute_batch(migration)?;
        transaction.execute(
            "UPDATE schema_meta SET version = ?1, applied_at = ?2 WHERE id = 1",
            params![version as i64, timestamp()],
        )?;
        transaction.commit()?;
    }
    Ok(())
}

fn is_valid_profile_id(profile_id: &str) -> bool {
    !profile_id.is_empty()
        && profile_id.len() <= 64
        && profile_id.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.'
        })
}

fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::AutoSi, true)
}
