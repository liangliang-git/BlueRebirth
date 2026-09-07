use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    #[error("insufficient {kind:?}: required {required}, available {available}")]
    InsufficientResource {
        kind: CurrencyKind,
        required: u64,
        available: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    use super::{CurrencyKind, DomainError, ProfileId, ResourceLedger};

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
}
