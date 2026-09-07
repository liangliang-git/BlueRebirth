use blueoath_domain::{
    AccountRepository, AccountState, CharacterState, EquipId, EquipmentState, FleetId, FleetRecord,
    HeroId, HeroState, NewAccountFactory, ProfileId, ProfileState, RepositoryError, TemplateId,
};
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::Value;
use std::collections::BTreeSet;
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
    pub state: Value,
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
                "SELECT id, name, state_json FROM profiles WHERE id = ?1",
                params![profile_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(id, name, state_json)| {
            Ok(StoredProfile {
                id,
                name,
                state: serde_json::from_str(&state_json)?,
            })
        })
        .transpose()
    }

    pub fn load_account(&self, profile_id: &str) -> Result<Option<Value>, StorageError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT account_json FROM accounts WHERE id = ?1",
                params![profile_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(StorageError::from)
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
        let mut account = AccountState::new(ProfileState {
            id: profile_id.clone(),
            name,
            revision,
        });

        if let Some(row) = connection
            .query_row(
                "SELECT uid, name, level, exp, secretary_id, head, head_frame,
                        gold, diamond, supply, pve_pt
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
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, i64>(10)?,
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
                secretary_id: (row.4 > 0)
                    .then(|| {
                        HeroId::new(row.4 as u64)
                            .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))
                    })
                    .transpose()?,
                head: non_negative_u32(row.5, "character head")?,
                head_frame: non_negative_u32(row.6, "character head frame")?,
                resources: account.character.resources.clone(),
            };
            for (kind, amount) in [
                (blueoath_domain::CurrencyKind::Gold, row.7),
                (blueoath_domain::CurrencyKind::Diamond, row.8),
                (blueoath_domain::CurrencyKind::Supply, row.9),
                (blueoath_domain::CurrencyKind::PvePoint, row.10),
            ] {
                account
                    .resources
                    .credit(kind, non_negative_u64(amount, "resource")?)
                    .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))?;
            }
        }

        let mut statement = connection.prepare(
            "SELECT hero_id, template_id, level, exp, mood, affection, hp, lock_state
             FROM heroes WHERE profile_id = ?1 ORDER BY hero_id",
        )?;
        let heroes = statement
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
                    level: positive_u32(row.2, "hero level")?,
                    exp: non_negative_u64(row.3, "hero exp")?,
                    mood: non_negative_u32(row.4, "hero mood")?,
                    affection: non_negative_u64(row.5, "hero affection")?,
                    hp: non_negative_u64(row.6, "hero hp")?,
                    locked: row.7 != 0,
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
        account
            .validate()
            .map_err(|error| StorageError::InvalidTypedAccount(error.to_string()))?;
        Ok(Some(account))
    }

    pub fn save(&self, profile_id: &str, name: &str, state: &Value) -> Result<(), StorageError> {
        if !is_valid_profile_id(profile_id) {
            return Err(StorageError::InvalidProfileId);
        }
        let state_json = serde_json::to_string(state)?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO profiles(id, name, state_json, updated_utc)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               state_json = excluded.state_json,
               updated_utc = excluded.updated_utc",
            params![profile_id, name, state_json, timestamp()],
        )?;
        Ok(())
    }

    pub fn save_account(&self, profile_id: &str, account: &Value) -> Result<(), StorageError> {
        self.save_account_with_revision(profile_id, account, None)
            .map(|_| ())
    }

    pub fn load_account_with_revision(
        &self,
        profile_id: &str,
    ) -> Result<Option<(Value, u64)>, StorageError> {
        let connection = self.connection()?;
        let account = connection
            .query_row(
                "SELECT account_json FROM accounts WHERE id = ?1",
                params![profile_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(account_json) = account else {
            return Ok(None);
        };
        let revision = connection
            .query_row(
                "SELECT revision FROM account_revisions WHERE profile_id = ?1",
                params![profile_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or_default();
        let revision = u64::try_from(revision)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, revision))?;
        Ok(Some((serde_json::from_str(&account_json)?, revision)))
    }

    pub fn save_account_with_revision(
        &self,
        profile_id: &str,
        account: &Value,
        expected_revision: Option<u64>,
    ) -> Result<u64, StorageError> {
        if !is_valid_profile_id(profile_id) {
            return Err(StorageError::InvalidProfileId);
        }
        let account_json = serde_json::to_string(account)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let actual = transaction
            .query_row(
                "SELECT revision FROM account_revisions WHERE profile_id = ?1",
                params![profile_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or_default();
        let actual = u64::try_from(actual)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, actual))?;
        if let Some(expected) = expected_revision {
            if expected != actual {
                return Err(StorageError::RevisionConflict { expected, actual });
            }
        }
        let next_revision = actual.checked_add(1).ok_or_else(|| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(
                "account revision overflow",
            )))
        })?;
        let sql_revision = i64::try_from(next_revision).map_err(|_| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(
                "account revision exceeds SQLite integer range",
            )))
        })?;
        transaction.execute(
            "INSERT INTO accounts(id, account_json, updated_utc)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
               account_json = excluded.account_json,
               updated_utc = excluded.updated_utc",
            params![profile_id, account_json, timestamp()],
        )?;
        project_normalized_core(&transaction, profile_id, account)?;
        transaction.execute(
            "INSERT INTO account_revisions(profile_id, revision, updated_utc)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(profile_id) DO UPDATE SET
               revision = excluded.revision,
               updated_utc = excluded.updated_utc",
            params![profile_id, sql_revision, timestamp()],
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

    pub fn list_accounts(&self) -> Result<Vec<(String, Value)>, StorageError> {
        let connection = self.connection()?;
        let mut statement =
            connection.prepare("SELECT id, account_json FROM accounts ORDER BY id")?;
        let accounts = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        accounts
            .into_iter()
            .map(|(id, account_json)| Ok((id, serde_json::from_str::<Value>(&account_json)?)))
            .collect()
    }

    pub fn reset(&self, profile_id: &str) -> Result<(), StorageError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM profiles WHERE id = ?1", params![profile_id])?;
        transaction.execute("DELETE FROM accounts WHERE id = ?1", params![profile_id])?;
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

fn project_normalized_core(
    transaction: &Transaction<'_>,
    profile_id: &str,
    account: &Value,
) -> Result<(), StorageError> {
    let character = account
        .get("character")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let profile_name = character
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .unwrap_or(profile_id);
    transaction.execute(
        "INSERT INTO profiles(id, name, state_json, updated_utc)
         VALUES (?1, ?2, '{}', ?3)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, updated_utc = excluded.updated_utc",
        params![profile_id, profile_name, timestamp()],
    )?;

    transaction.execute(
        "DELETE FROM task_claims WHERE profile_id = ?1",
        params![profile_id],
    )?;
    for table in [
        "construction_jobs",
        "buildings",
        "daily_copy_progress",
        "copy_progress",
        "sea_progress",
        "tower_progress",
        "activity_progress",
    ] {
        transaction.execute(
            &format!("DELETE FROM {table} WHERE profile_id = ?1"),
            params![profile_id],
        )?;
    }
    transaction.execute(
        "DELETE FROM tasks WHERE profile_id = ?1",
        params![profile_id],
    )?;
    transaction.execute(
        "DELETE FROM battle_sessions WHERE profile_id = ?1",
        params![profile_id],
    )?;
    transaction.execute(
        "DELETE FROM fleet_members WHERE profile_id = ?1",
        params![profile_id],
    )?;
    transaction.execute(
        "DELETE FROM fleets WHERE profile_id = ?1",
        params![profile_id],
    )?;
    transaction.execute(
        "DELETE FROM hero_equip_slots WHERE profile_id = ?1",
        params![profile_id],
    )?;
    transaction.execute(
        "DELETE FROM equipments WHERE profile_id = ?1",
        params![profile_id],
    )?;
    transaction.execute(
        "DELETE FROM heroes WHERE profile_id = ?1",
        params![profile_id],
    )?;
    transaction.execute(
        "DELETE FROM inventory WHERE profile_id = ?1",
        params![profile_id],
    )?;
    transaction.execute(
        "DELETE FROM characters WHERE profile_id = ?1",
        params![profile_id],
    )?;

    transaction.execute(
        "INSERT INTO characters(
            profile_id, uid, name, level, exp, secretary_id, gold, diamond,
            supply, pve_pt, head, head_frame
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            profile_id,
            positive_field(&character, "uid", 1),
            profile_name,
            positive_field(&character, "level", 1),
            non_negative_field(&character, "exp"),
            non_negative_field(&character, "secretaryId"),
            non_negative_field(&character, "gold"),
            non_negative_field(&character, "diamond"),
            non_negative_field(&character, "supply"),
            non_negative_field(&character, "pvePt"),
            non_negative_field(&character, "head"),
            non_negative_field(&character, "headFrame"),
        ],
    )?;

    let mut hero_ids = BTreeSet::new();
    let mut equipment_ids = BTreeSet::new();
    if let Some(heroes) = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
    {
        for hero in heroes {
            let Some(hero) = hero.as_object() else {
                continue;
            };
            let hero_id = positive_field(hero, "heroId", 0);
            let template_id = positive_field(hero, "templateId", 0);
            if hero_id == 0 || template_id == 0 {
                continue;
            }
            transaction.execute(
                "INSERT INTO heroes(
                    profile_id, hero_id, template_id, level, exp, mood,
                    affection, hp, lock_state, created_utc
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(profile_id, hero_id) DO UPDATE SET
                   template_id = excluded.template_id,
                   level = excluded.level,
                   exp = excluded.exp,
                   mood = excluded.mood,
                   affection = excluded.affection,
                   hp = excluded.hp,
                   lock_state = excluded.lock_state",
                params![
                    profile_id,
                    hero_id,
                    template_id,
                    positive_field(hero, "level", 1),
                    non_negative_field(hero, "exp"),
                    non_negative_field(hero, "mood"),
                    non_negative_field(hero, "affection"),
                    non_negative_field(hero, "curHp"),
                    bool_field(hero, "lock"),
                    timestamp(),
                ],
            )?;
            hero_ids.insert(hero_id);
        }
    }

    if let Some(equipments) = account
        .get("equip")
        .and_then(|equip| equip.get("items"))
        .and_then(Value::as_array)
    {
        for equipment in equipments {
            let Some(equipment) = equipment.as_object() else {
                continue;
            };
            let equip_id = positive_field(equipment, "equipId", 0);
            let template_id = positive_field(equipment, "templateId", 0);
            if equip_id == 0 || template_id == 0 {
                continue;
            }
            let hero_id = positive_field(equipment, "heroId", 0);
            let hero_id = hero_ids.contains(&hero_id).then_some(hero_id);
            transaction.execute(
                "INSERT INTO equipments(
                    profile_id, equip_id, template_id, enhance_level, star,
                    enhance_exp, hero_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(profile_id, equip_id) DO UPDATE SET
                   template_id = excluded.template_id,
                   enhance_level = excluded.enhance_level,
                   star = excluded.star,
                   enhance_exp = excluded.enhance_exp,
                   hero_id = excluded.hero_id",
                params![
                    profile_id,
                    equip_id,
                    template_id,
                    non_negative_field(equipment, "enhanceLv"),
                    non_negative_field(equipment, "star"),
                    non_negative_field(equipment, "enhanceExp"),
                    hero_id,
                ],
            )?;
            equipment_ids.insert(equip_id);
        }
    }

    if let Some(heroes) = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
    {
        for hero in heroes {
            let Some(hero) = hero.as_object() else {
                continue;
            };
            let hero_id = positive_field(hero, "heroId", 0);
            if hero_id == 0 || !hero_ids.contains(&hero_id) {
                continue;
            }
            if let Some(slots) = hero.get("equipSlots").and_then(Value::as_array) {
                for (slot_index, slot) in slots.iter().enumerate() {
                    let equip_id = slot.as_i64().unwrap_or_default();
                    let equip_id = equipment_ids.contains(&equip_id).then_some(equip_id);
                    transaction.execute(
                        "INSERT INTO hero_equip_slots(
                            profile_id, hero_id, slot_index, equip_id
                         ) VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT(profile_id, hero_id, slot_index) DO UPDATE SET
                           equip_id = excluded.equip_id",
                        params![profile_id, hero_id, slot_index as i64, equip_id],
                    )?;
                }
            }
        }
    }

    if let Some(items) = account
        .get("bag")
        .and_then(|bag| bag.get("items"))
        .and_then(Value::as_array)
    {
        for item in items {
            let Some(item) = item.as_object() else {
                continue;
            };
            let template_id = positive_field(item, "templateId", 0);
            if template_id == 0 {
                continue;
            }
            transaction.execute(
                "INSERT INTO inventory(profile_id, template_id, amount)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(profile_id, template_id) DO UPDATE SET
                   amount = inventory.amount + excluded.amount",
                params![profile_id, template_id, non_negative_field(item, "num")],
            )?;
        }
    }

    if let Some(tactics) = account
        .get("fleet")
        .and_then(|fleet| fleet.get("tactics"))
        .and_then(Value::as_array)
    {
        for (index, tactic) in tactics.iter().enumerate() {
            let Some(tactic) = tactic.as_object() else {
                continue;
            };
            let fleet_id = positive_field(tactic, "modeId", index as i64 + 1);
            transaction.execute(
                "INSERT INTO fleets(profile_id, fleet_id, formation_id, tactic_id)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(profile_id, fleet_id) DO UPDATE SET
                   formation_id = excluded.formation_id,
                   tactic_id = excluded.tactic_id",
                params![
                    profile_id,
                    fleet_id,
                    non_negative_field(tactic, "formationId"),
                    non_negative_field(tactic, "strategyId"),
                ],
            )?;
        }
    }

    if let Some(session) = account.get("battleSession").and_then(Value::as_object) {
        let copy_id = positive_field(session, "copyId", 0);
        if copy_id > 0 {
            let started_at = non_negative_field(session, "startedAt");
            transaction.execute(
                "INSERT INTO battle_sessions(
                    profile_id, chapter_id, copy_id, current_fleet, state,
                    started_at, expires_at, revision
                 ) VALUES (?1, 1, ?2, 0, 'active', ?3, ?3, 0)",
                params![profile_id, copy_id, started_at],
            )?;
        }
    }

    let task_reset_day = account
        .get("tasks")
        .and_then(Value::as_object)
        .map(|tasks| non_negative_field(tasks, "dailyResetDay"))
        .unwrap_or_default();
    if let Some(records) = account
        .get("tasks")
        .and_then(|tasks| tasks.get("records"))
        .and_then(Value::as_array)
    {
        for record in records {
            let Some(record) = record.as_object() else {
                continue;
            };
            let task_id = positive_field(record, "taskId", 0);
            if task_id == 0 {
                continue;
            }
            transaction.execute(
                "INSERT INTO tasks(
                    profile_id, task_id, task_type, progress, completed, reset_day
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(profile_id, task_id) DO UPDATE SET
                   task_type = excluded.task_type,
                   progress = excluded.progress,
                   completed = excluded.completed,
                   reset_day = excluded.reset_day",
                params![
                    profile_id,
                    task_id,
                    non_negative_field(record, "type"),
                    non_negative_field(record, "progress"),
                    bool_field(record, "completed") | bool_field(record, "isFinished"),
                    task_reset_day,
                ],
            )?;
        }
    }

    for (table, key) in [
        ("sea_progress", "seaProgress"),
        ("copy_progress", "copyProgress"),
    ] {
        if let Some(records) = account
            .get(key)
            .and_then(|state| state.get("records"))
            .and_then(Value::as_array)
        {
            for record in records {
                let Some(record) = record.as_object() else {
                    continue;
                };
                let copy_id = positive_field(record, "copyId", 0);
                if copy_id == 0 {
                    continue;
                }
                if table == "sea_progress" {
                    transaction.execute(
                        "INSERT INTO sea_progress(
                            profile_id, copy_id, star_level, pass_count
                         ) VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT(profile_id, copy_id) DO UPDATE SET
                           star_level = excluded.star_level,
                           pass_count = excluded.pass_count",
                        params![
                            profile_id,
                            copy_id,
                            non_negative_field(record, "starLevel"),
                            non_negative_field(record, "passCount"),
                        ],
                    )?;
                } else {
                    transaction.execute(
                        "INSERT INTO copy_progress(
                            profile_id, copy_id, star_level, first_passed
                         ) VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT(profile_id, copy_id) DO UPDATE SET
                           star_level = excluded.star_level,
                           first_passed = excluded.first_passed",
                        params![
                            profile_id,
                            copy_id,
                            non_negative_field(record, "starLevel"),
                            bool_field(record, "firstPass") | bool_field(record, "firstPassed"),
                        ],
                    )?;
                }
            }
        }
    }

    let daily_reset_day = account
        .get("dailyCopy")
        .and_then(Value::as_object)
        .map(|daily| non_negative_field(daily, "resetDay"))
        .unwrap_or_default();
    if let Some(chapters) = account
        .get("dailyCopy")
        .and_then(|state| state.get("chapters"))
        .and_then(Value::as_array)
    {
        for chapter in chapters {
            let Some(chapter) = chapter.as_object() else {
                continue;
            };
            let chapter_id = positive_field(chapter, "chapterId", 0);
            if chapter_id == 0 {
                continue;
            }
            transaction.execute(
                "INSERT INTO daily_copy_progress(
                    profile_id, reset_day, chapter_id, group_id, challenge_times,
                    success_times, select_ex, extra_group
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(profile_id, chapter_id) DO UPDATE SET
                   reset_day = excluded.reset_day,
                   group_id = excluded.group_id,
                   challenge_times = excluded.challenge_times,
                   success_times = excluded.success_times,
                   select_ex = excluded.select_ex,
                   extra_group = excluded.extra_group",
                params![
                    profile_id,
                    daily_reset_day,
                    chapter_id,
                    positive_field(chapter, "groupId", 1),
                    non_negative_field(chapter, "challengeTimes"),
                    non_negative_field(chapter, "successTimes"),
                    bool_field(chapter, "selectEx"),
                    non_negative_field(chapter, "extraGroup"),
                ],
            )?;
        }
    }

    if let Some(buildings) = account
        .get("building")
        .and_then(|state| state.get("buildings"))
        .and_then(Value::as_array)
    {
        for building in buildings {
            let Some(building) = building.as_object() else {
                continue;
            };
            let building_id = positive_field(building, "id", 0);
            if building_id == 0 {
                continue;
            }
            transaction.execute(
                "INSERT INTO buildings(profile_id, building_id, level, land_index)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(profile_id, building_id) DO UPDATE SET
                   level = excluded.level,
                   land_index = excluded.land_index",
                params![
                    profile_id,
                    building_id,
                    non_negative_field(building, "level"),
                    non_negative_field(building, "landIndex"),
                ],
            )?;
        }
    }

    if let Some(tower) = account.get("tower").and_then(Value::as_object) {
        transaction.execute(
            "INSERT INTO tower_progress(profile_id, chapter_id, floor, reset_day)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(profile_id) DO UPDATE SET
               chapter_id = excluded.chapter_id,
               floor = excluded.floor,
               reset_day = excluded.reset_day",
            params![
                profile_id,
                non_negative_field(tower, "chapterId"),
                non_negative_field(tower, "floor"),
                non_negative_field(tower, "resetDay"),
            ],
        )?;
    }
    Ok(())
}

fn non_negative_field(object: &serde_json::Map<String, Value>, key: &str) -> i64 {
    object
        .get(key)
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0)
}

fn positive_field(object: &serde_json::Map<String, Value>, key: &str, default: i64) -> i64 {
    object
        .get(key)
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn bool_field(object: &serde_json::Map<String, Value>, key: &str) -> i64 {
    object.get(key).and_then(Value::as_bool).unwrap_or(false) as i64
}

impl AccountRepository for ProfileStore {
    fn load(&self, profile_id: &ProfileId) -> Result<Option<AccountState>, RepositoryError> {
        if let Some(value) = self
            .load_account(profile_id.as_str())
            .map_err(|error| RepositoryError::Storage(error.to_string()))?
        {
            return serde_json::from_value(value)
                .map(Some)
                .map_err(|error| RepositoryError::Storage(error.to_string()));
        }
        self.load_typed_account(profile_id)
            .map_err(|error| RepositoryError::Storage(error.to_string()))
    }

    fn create(&self, account: &AccountState) -> Result<(), RepositoryError> {
        let profile = account.profile.as_ref().ok_or_else(|| {
            RepositoryError::Storage("account profile is required for creation".to_owned())
        })?;
        account.validate()?;
        let value = serde_json::to_value(account)
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        self.save_account(profile.id.as_str(), &value)
            .map_err(|error| RepositoryError::Storage(error.to_string()))
    }

    fn transact<F, T>(&self, profile_id: &ProfileId, operation: F) -> Result<T, RepositoryError>
    where
        F: FnOnce(&mut AccountState) -> Result<T, blueoath_domain::DomainError>,
    {
        let mut account = self
            .load_account(profile_id.as_str())
            .map_err(|error| RepositoryError::Storage(error.to_string()))?
            .map(|value| {
                serde_json::from_value(value)
                    .map_err(|error| RepositoryError::Storage(error.to_string()))
            })
            .transpose()?
            .unwrap_or_else(|| {
                let profile = profile_id.clone();
                NewAccountFactory::create(profile, profile_id.as_str())
            });
        let result = operation(&mut account)?;
        account.validate()?;
        let value = serde_json::to_value(&account)
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
        self.save_account(profile_id.as_str(), &value)
            .map_err(|error| RepositoryError::Storage(error.to_string()))?;
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
