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
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetRecord {
    pub formation_id: u32,
    pub tactic_id: u32,
    pub members: Vec<HeroId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleSession {
    pub chapter_id: ChapterId,
    pub copy_id: CopyId,
    pub current_fleet: u32,
    pub started_at: u64,
    pub expires_at: u64,
    pub revision: u64,
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
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskState {
    pub progress: BTreeMap<u64, u64>,
    pub completed: BTreeSet<u64>,
    pub claimed: BTreeSet<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingState {
    pub levels: BTreeMap<u64, u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialState {
    pub friends: BTreeSet<ProfileId>,
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
    }
}
