use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("{0} must be positive")]
    NonPositiveId(&'static str),
    #[error("profile id is empty")]
    EmptyProfileId,
    #[error("profile id is too long")]
    ProfileIdTooLong,
    #[error("profile id contains unsupported characters")]
    InvalidProfileId,
    #[error("resource amount cannot be negative")]
    NegativeResource,
    #[error("resource amount overflow")]
    ResourceOverflow,
    #[error("invalid state: {0}")]
    InvalidState(&'static str),
    #[error("insufficient {kind:?}: required {required}, available {available}")]
    InsufficientResource {
        kind: CurrencyKind,
        required: u64,
        available: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProfileId(String);

impl ProfileId {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.is_empty() {
            return Err(DomainError::EmptyProfileId);
        }
        if value.len() > 64 {
            return Err(DomainError::ProfileIdTooLong);
        }
        if !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.'
        }) {
            return Err(DomainError::InvalidProfileId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

macro_rules! positive_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        pub struct $name(u64);

        impl $name {
            pub fn new(value: u64) -> Result<Self, DomainError> {
                if value == 0 {
                    return Err(DomainError::NonPositiveId(stringify!($name)));
                }
                Ok(Self(value))
            }

            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

positive_id!(ChapterId);
positive_id!(CopyId);
positive_id!(FleetId);
positive_id!(HeroId);
positive_id!(EquipId);
positive_id!(TemplateId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CurrencyKind {
    Gold,
    Diamond,
    Supply,
    PvePoint,
    Oil,
    BuildMaterial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ResourceAmount(u64);

impl ResourceAmount {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub fn checked_add(self, amount: u64) -> Result<Self, DomainError> {
        self.0
            .checked_add(amount)
            .map(Self)
            .ok_or(DomainError::ResourceOverflow)
    }

    pub fn checked_sub(self, kind: CurrencyKind, amount: u64) -> Result<Self, DomainError> {
        self.0
            .checked_sub(amount)
            .map(Self)
            .ok_or(DomainError::InsufficientResource {
                kind,
                required: amount,
                available: self.0,
            })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLedger {
    amounts: BTreeMap<CurrencyKind, ResourceAmount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterState {
    pub uid: u64,
    pub name: String,
    pub level: u32,
    pub exp: u64,
    pub class_id: u32,
    pub create_time: u64,
    pub message: String,
    pub secretary_id: Option<HeroId>,
    pub head: u32,
    pub head_frame: u32,
    pub resources: ResourceLedger,
}

impl Default for CharacterState {
    fn default() -> Self {
        Self {
            uid: 1,
            name: "Commander".to_owned(),
            level: 1,
            exp: 0,
            class_id: 1,
            create_time: 0,
            message: String::new(),
            secretary_id: None,
            head: 0,
            head_frame: 0,
            resources: ResourceLedger::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeroState {
    pub id: HeroId,
    pub template_id: TemplateId,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub change_name_time: u64,
    pub level: u32,
    pub exp: u64,
    pub mood: u32,
    pub affection: u64,
    pub hp: u64,
    pub locked: bool,
    pub equip_slots: Vec<Option<EquipId>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockState {
    pub heroes: BTreeMap<HeroId, HeroState>,
    pub equipments: BTreeMap<EquipId, EquipmentState>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryState {
    pub items: BTreeMap<TemplateId, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquipmentState {
    pub id: EquipId,
    pub template_id: TemplateId,
    pub enhance_level: u32,
    pub star: u32,
    pub enhance_exp: u64,
    pub hero_id: Option<HeroId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetState {
    pub fleets: BTreeMap<FleetId, FleetRecord>,
    #[serde(default)]
    pub presets: Vec<PresetFleetState>,
    #[serde(default)]
    pub preset_name_num: u32,
    #[serde(default)]
    pub preset_red_dot: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetRecord {
    pub formation_id: u32,
    pub tactic_id: u32,
    pub members: Vec<HeroId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetFleetState {
    pub name: String,
    pub hero_ids: Vec<HeroId>,
    pub ex_hero_ids: Vec<HeroId>,
    pub mode_id: u32,
    pub strategy_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleSession {
    pub chapter_id: ChapterId,
    pub copy_id: CopyId,
    pub current_fleet: u32,
    pub started_at: u64,
    pub expires_at: u64,
    pub revision: u64,
    #[serde(default)]
    pub remaining_fleet_ids: Vec<u32>,
    #[serde(default)]
    pub hero_ids: Vec<HeroId>,
    #[serde(default)]
    pub attack_count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleProgressState {
    pub active: Option<BattleSession>,
    pub passed_copies: BTreeSet<CopyId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyCopyState {
    pub reset_day: u32,
    pub challenge_times: BTreeMap<ChapterId, u32>,
    #[serde(default)]
    pub select_ex: BTreeMap<ChapterId, bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskState {
    pub progress: BTreeMap<u64, u64>,
    #[serde(default)]
    pub task_types: BTreeMap<u64, u32>,
    pub completed: BTreeSet<u64>,
    pub claimed: BTreeSet<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingState {
    pub levels: BTreeMap<u64, u32>,
    #[serde(default)]
    pub template_ids: BTreeMap<u64, u64>,
    #[serde(default)]
    pub land_indices: BTreeMap<u64, u32>,
    #[serde(default)]
    pub hero_assignments: BTreeMap<u64, Vec<HeroId>>,
    #[serde(default)]
    pub productions: BTreeMap<u64, BuildingProductionState>,
    #[serde(default)]
    pub construction_jobs: Vec<ConstructionJobState>,
    #[serde(default)]
    pub last_project: Option<ConstructionProjectState>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingProductionState {
    pub status: u32,
    pub recipe_id: u32,
    pub item_count: u32,
    pub product_count: u32,
    pub last_update_at: u64,
    pub recipe_time: u32,
    pub productivity: u32,
    pub produce_speed: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstructionProjectState {
    pub gold: u32,
    pub steel: u32,
    pub aluminium: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstructionJobState {
    pub sequence: u64,
    pub template_id: u64,
    pub duration_seconds: u32,
    pub end_at: u64,
    pub completed: bool,
    pub project: ConstructionProjectState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialState {
    #[serde(default)]
    pub friends: BTreeSet<u64>,
    #[serde(default)]
    pub pending: BTreeSet<u64>,
    #[serde(default)]
    pub blacklist: BTreeSet<u64>,
    #[serde(default)]
    pub applied: BTreeSet<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessageState {
    pub id: u64,
    pub uid: u64,
    pub channel: u32,
    pub receive_uid: u64,
    pub message: String,
    pub message_type: u32,
    pub voice: String,
    pub sent_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatBarrageState {
    pub id: u32,
    pub offset: u32,
    pub content: String,
    pub uid: u64,
    pub sent_at: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatState {
    pub channel: u32,
    pub messages: Vec<ChatMessageState>,
    pub barrages: Vec<ChatBarrageState>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityState {
    pub progress: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InviteScoreState {
    pub have_got_ssr: u64,
    pub have_got_fashion: u64,
    pub have_first_battle_win: u64,
    pub record_version: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TalentState {
    pub active: BTreeMap<u64, u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TowerRewardState {
    pub reward_type: u32,
    pub config_id: u64,
    pub amount: u64,
    pub instance_id: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TowerState {
    pub chapter_id: u32,
    pub area_index: u32,
    pub copy_index: u32,
    pub topic_index: u32,
    pub daily_count: u32,
    pub reset_time: u64,
    pub sf_id_counts: BTreeMap<u64, u32>,
    pub hero_ids: Vec<HeroId>,
    pub lock_equip_ids: Vec<EquipId>,
    pub pass_last_chapter_id: u32,
    pub is_reset: bool,
    pub max_level: u32,
    pub max_area: u32,
    pub max_copy: u32,
    pub daily_count_ex: u32,
    pub is_new_level: bool,
    pub save_pass_copy_ids: Vec<CopyId>,
    pub pending_rewards: Vec<TowerRewardState>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityTowerState {
    pub activity_id: u32,
    pub reset_time: u64,
    pub small_reset_number: u32,
    pub quick_number: u32,
    pub history_max: u32,
    pub save_pass_copy_ids: Vec<CopyId>,
    pub pass_copy_ids: Vec<CopyId>,
    pub lock_equip_ids: Vec<EquipId>,
    pub hero_ids: Vec<HeroId>,
    pub save_pass_stage_copy_ids: Vec<CopyId>,
}

impl ResourceLedger {
    pub fn amount(&self, kind: CurrencyKind) -> ResourceAmount {
        self.amounts
            .get(&kind)
            .copied()
            .unwrap_or(ResourceAmount::ZERO)
    }

    pub fn credit(&mut self, kind: CurrencyKind, amount: u64) -> Result<(), DomainError> {
        let current = self.amount(kind);
        self.amounts.insert(kind, current.checked_add(amount)?);
        Ok(())
    }

    pub fn debit(&mut self, kind: CurrencyKind, amount: u64) -> Result<(), DomainError> {
        let current = self.amount(kind);
        self.amounts
            .insert(kind, current.checked_sub(kind, amount)?);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileState {
    pub id: ProfileId,
    pub name: String,
    pub revision: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountState {
    pub profile: Option<ProfileState>,
    pub resources: ResourceLedger,
    #[serde(default)]
    pub inventory: InventoryState,
    #[serde(default)]
    pub character: CharacterState,
    #[serde(default)]
    pub dock: DockState,
    #[serde(default)]
    pub fleet: FleetState,
    #[serde(default)]
    pub battle: BattleProgressState,
    #[serde(default)]
    pub daily_copy: DailyCopyState,
    #[serde(default)]
    pub tasks: TaskState,
    #[serde(default)]
    pub buildings: BuildingState,
    #[serde(default)]
    pub social: SocialState,
    #[serde(default)]
    pub chat: ChatState,
    #[serde(default)]
    pub activities: ActivityState,
    #[serde(default)]
    pub invite_score: InviteScoreState,
    #[serde(default)]
    pub talents: TalentState,
    #[serde(default)]
    pub tower: TowerState,
    #[serde(default)]
    pub activity_tower: ActivityTowerState,
}

impl AccountState {
    pub fn new(profile: ProfileState) -> Self {
        Self {
            profile: Some(profile),
            ..Self::default()
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        if self.character.uid == 0 {
            return Err(DomainError::InvalidState("character uid must be positive"));
        }
        if self.character.level == 0 {
            return Err(DomainError::InvalidState(
                "character level must be positive",
            ));
        }
        if self.social.friends.contains(&self.character.uid)
            || self.social.pending.contains(&self.character.uid)
            || self.social.blacklist.contains(&self.character.uid)
            || self.social.applied.contains(&self.character.uid)
        {
            return Err(DomainError::InvalidState(
                "social relation cannot target current character",
            ));
        }
        if self
            .social
            .friends
            .iter()
            .any(|uid| self.social.blacklist.contains(uid))
        {
            return Err(DomainError::InvalidState(
                "friend cannot also be blacklisted",
            ));
        }
        for hero in self.dock.heroes.values() {
            if hero.level == 0 {
                return Err(DomainError::InvalidState("hero level must be positive"));
            }
            if hero
                .equip_slots
                .iter()
                .flatten()
                .any(|id| !self.dock.equipments.contains_key(id))
            {
                return Err(DomainError::InvalidState(
                    "hero references missing equipment",
                ));
            }
        }
        for equipment in self.dock.equipments.values() {
            if let Some(hero_id) = equipment.hero_id {
                if !self.dock.heroes.contains_key(&hero_id) {
                    return Err(DomainError::InvalidState(
                        "equipment references missing hero",
                    ));
                }
            }
        }
        for (building_id, hero_ids) in &self.buildings.hero_assignments {
            if !self.buildings.levels.contains_key(building_id) {
                return Err(DomainError::InvalidState(
                    "building assignment references missing building",
                ));
            }
            if hero_ids
                .iter()
                .any(|hero_id| !self.dock.heroes.contains_key(hero_id))
            {
                return Err(DomainError::InvalidState(
                    "building assignment references missing hero",
                ));
            }
        }
        if self
            .buildings
            .productions
            .keys()
            .any(|building_id| !self.buildings.levels.contains_key(building_id))
        {
            return Err(DomainError::InvalidState(
                "building production references missing building",
            ));
        }
        let mut construction_sequences = BTreeSet::new();
        for job in &self.buildings.construction_jobs {
            if job.sequence == 0
                || job.template_id == 0
                || !construction_sequences.insert(job.sequence)
            {
                return Err(DomainError::InvalidState("construction job is invalid"));
            }
        }
        for hero_id in self
            .tower
            .hero_ids
            .iter()
            .chain(self.activity_tower.hero_ids.iter())
        {
            if !self.dock.heroes.contains_key(hero_id) {
                return Err(DomainError::InvalidState("tower references missing hero"));
            }
        }
        for equip_id in self
            .tower
            .lock_equip_ids
            .iter()
            .chain(self.activity_tower.lock_equip_ids.iter())
        {
            if !self.dock.equipments.contains_key(equip_id) {
                return Err(DomainError::InvalidState(
                    "tower references missing equipment",
                ));
            }
        }
        if self.tower.hero_ids.iter().collect::<BTreeSet<_>>().len() != self.tower.hero_ids.len()
            || self
                .activity_tower
                .hero_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != self.activity_tower.hero_ids.len()
        {
            return Err(DomainError::InvalidState(
                "tower hero list contains duplicates",
            ));
        }
        Ok(())
    }
}

pub struct NewAccountFactory;

impl NewAccountFactory {
    pub fn create(profile_id: ProfileId, name: impl Into<String>) -> AccountState {
        let name = name.into();
        let mut account = AccountState::new(ProfileState {
            id: profile_id,
            name: name.clone(),
            revision: 0,
        });
        account.character.name = name;
        account.character.class_id = 1;
        account.character.head = 1021051;
        account.character.secretary_id = HeroId::new(1).ok();
        account
            .resources
            .credit(CurrencyKind::Diamond, 10_000)
            .expect("starter diamond balance is bounded");
        account
            .resources
            .credit(CurrencyKind::Supply, 10_000)
            .expect("starter supply balance is bounded");

        let starter_hero_id = HeroId::new(1).expect("starter hero id is positive");
        account.dock.heroes.insert(
            starter_hero_id,
            HeroState {
                id: starter_hero_id,
                template_id: TemplateId::new(10_210_511)
                    .expect("starter hero template is positive"),
                name: String::new(),
                change_name_time: 0,
                level: 1,
                exp: 0,
                mood: 100,
                affection: 500_000,
                hp: 10_000_000_000,
                locked: true,
                equip_slots: vec![
                    Some(EquipId::new(1).expect("starter equipment id is positive")),
                    None,
                    Some(EquipId::new(2).expect("starter equipment id is positive")),
                    None,
                    None,
                    None,
                ],
            },
        );
        for (id, template_id) in [(1, 30_091), (2, 30_221)] {
            let id = EquipId::new(id).expect("starter equipment id is positive");
            account.dock.equipments.insert(
                id,
                EquipmentState {
                    id,
                    template_id: TemplateId::new(template_id)
                        .expect("starter equipment template is positive"),
                    enhance_level: 0,
                    star: 0,
                    enhance_exp: 0,
                    hero_id: Some(starter_hero_id),
                },
            );
        }
        for (template_id, amount) in [
            (60_000, 1_000),
            (60_001, 1_000),
            (60_002, 1_000),
            (60_003, 1_000),
            (10_182, 100),
            (10_185, 100),
            (10_187, 100),
            (10_007, 100),
            (10_181, 100),
            (12_201, 100),
            (10_029, 1_000),
            (10_030, 1_000),
            (10_031, 100),
        ] {
            account.inventory.items.insert(
                TemplateId::new(template_id).expect("starter item id is positive"),
                amount,
            );
        }
        for fleet_id in 1..=5u64 {
            account.fleet.fleets.insert(
                FleetId::new(fleet_id).expect("starter fleet id is positive"),
                FleetRecord {
                    formation_id: 2,
                    tactic_id: 0,
                    members: Vec::new(),
                },
            );
        }
        account.buildings.levels.insert(1, 2);
        account.buildings.template_ids.insert(1, 2);
        account.buildings.land_indices.insert(1, 1);
        account.buildings.levels.insert(2, 1);
        account.buildings.template_ids.insert(2, 41);
        account.buildings.land_indices.insert(2, 6);
        account
    }
}

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("storage failure: {0}")]
    Storage(String),
    #[error("account revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("domain error: {0}")]
    Domain(#[from] DomainError),
}

pub trait AccountRepository {
    fn load(&self, profile_id: &ProfileId) -> Result<Option<AccountState>, RepositoryError>;
    fn create(&self, account: &AccountState) -> Result<(), RepositoryError>;

    fn transact<F, T>(&self, profile_id: &ProfileId, operation: F) -> Result<T, RepositoryError>
    where
        F: FnOnce(&mut AccountState) -> Result<T, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::{CurrencyKind, DomainError, NewAccountFactory, ProfileId, ResourceLedger};

    #[test]
    fn validates_profile_ids_at_domain_boundary() {
        assert!(ProfileId::new("jp.v1").is_ok());
        assert!(matches!(
            ProfileId::new("bad/id"),
            Err(DomainError::InvalidProfileId)
        ));
    }

    #[test]
    fn resource_ledger_prevents_negative_balances_and_overflow() {
        let mut ledger = ResourceLedger::default();
        assert!(matches!(
            ledger.debit(CurrencyKind::Gold, 1),
            Err(DomainError::InsufficientResource { .. })
        ));
        ledger.credit(CurrencyKind::Gold, 10).unwrap();
        ledger.debit(CurrencyKind::Gold, 4).unwrap();
        assert_eq!(ledger.amount(CurrencyKind::Gold).get(), 6);
    }

    #[test]
    fn new_account_factory_creates_valid_typed_state() {
        let account = NewAccountFactory::create(ProfileId::new("captain").unwrap(), "Captain");
        assert!(account.validate().is_ok());
        assert_eq!(account.character.level, 1);
        assert!(account.profile.is_some());
        assert_eq!(account.dock.heroes.len(), 1);
        assert_eq!(account.dock.equipments.len(), 2);
        assert_eq!(account.inventory.items.len(), 13);
        assert_eq!(account.fleet.fleets.len(), 5);
        assert_eq!(account.buildings.levels.get(&1), Some(&2));
    }

    #[test]
    fn validates_social_relation_invariants() {
        let mut account = NewAccountFactory::create(ProfileId::new("social").unwrap(), "Captain");
        account.social.friends.insert(account.character.uid);
        assert!(matches!(
            account.validate(),
            Err(DomainError::InvalidState(
                "social relation cannot target current character"
            ))
        ));
    }
}
