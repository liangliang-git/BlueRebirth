use blueoath_domain::{
    AccountState, BattleSession, ChapterId, CopyId, DomainError, FleetId, HeroId, ResourceLedger,
};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GameServiceError {
    #[error("domain error: {0}")]
    Domain(#[from] DomainError),
    #[error("battle session already active")]
    BattleAlreadyActive,
    #[error("battle session is not active")]
    BattleNotActive,
    #[error("battle session does not match requested copy")]
    BattleCopyMismatch,
    #[error("hero is not owned by account: {0:?}")]
    HeroNotOwned(HeroId),
    #[error("fleet is not configured: {0:?}")]
    FleetNotConfigured(FleetId),
}

pub struct ResourceService;

impl ResourceService {
    pub fn credit(
        account: &mut AccountState,
        kind: blueoath_domain::CurrencyKind,
        amount: u64,
    ) -> Result<(), GameServiceError> {
        account.resources.credit(kind, amount)?;
        Ok(())
    }

    pub fn debit(
        account: &mut AccountState,
        kind: blueoath_domain::CurrencyKind,
        amount: u64,
    ) -> Result<(), GameServiceError> {
        account.resources.debit(kind, amount)?;
        Ok(())
    }

    pub fn snapshot(account: &AccountState) -> ResourceLedger {
        account.resources.clone()
    }
}

pub struct RewardService;

impl RewardService {
    pub fn grant(
        account: &mut AccountState,
        rewards: impl IntoIterator<Item = (blueoath_domain::CurrencyKind, u64)>,
    ) -> Result<(), GameServiceError> {
        for (kind, amount) in rewards {
            ResourceService::credit(account, kind, amount)?;
        }
        Ok(())
    }
}

pub struct ProgressService;

impl ProgressService {
    pub fn mark_copy_passed(account: &mut AccountState, copy_id: CopyId) -> bool {
        account.battle.passed_copies.insert(copy_id)
    }

    pub fn is_copy_passed(account: &AccountState, copy_id: CopyId) -> bool {
        account.battle.passed_copies.contains(&copy_id)
    }
}

pub struct BattleService;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BattleStartContext {
    pub chapter_id: ChapterId,
    pub copy_id: CopyId,
    pub fleet_id: FleetId,
    pub hero_ids: Vec<HeroId>,
    pub remaining_fleet_ids: Vec<u32>,
    pub started_at: u64,
    pub expires_at: u64,
}

impl BattleService {
    pub fn start(
        account: &mut AccountState,
        chapter_id: ChapterId,
        copy_id: CopyId,
        fleet_id: FleetId,
        now: u64,
    ) -> Result<(), GameServiceError> {
        let hero_ids = account
            .fleet
            .fleets
            .get(&fleet_id)
            .map(|fleet| fleet.members.clone())
            .ok_or(GameServiceError::FleetNotConfigured(fleet_id))?;
        Self::start_with_context(
            account,
            BattleStartContext {
                chapter_id,
                copy_id,
                fleet_id,
                hero_ids,
                remaining_fleet_ids: Vec::new(),
                started_at: now,
                expires_at: now,
            },
        )
    }

    pub fn start_with_context(
        account: &mut AccountState,
        context: BattleStartContext,
    ) -> Result<(), GameServiceError> {
        if account.battle.active.is_some() {
            return Err(GameServiceError::BattleAlreadyActive);
        }
        if !account.fleet.fleets.contains_key(&context.fleet_id) {
            return Err(GameServiceError::FleetNotConfigured(context.fleet_id));
        }
        if context
            .hero_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != context.hero_ids.len()
            || context.remaining_fleet_ids.contains(&0)
            || context
                .remaining_fleet_ids
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != context.remaining_fleet_ids.len()
            || context.expires_at < context.started_at
        {
            return Err(GameServiceError::Domain(DomainError::InvalidState(
                "battle session context is invalid",
            )));
        }
        for hero_id in &context.hero_ids {
            if !account.dock.heroes.contains_key(hero_id) {
                return Err(GameServiceError::HeroNotOwned(*hero_id));
            }
        }
        account.battle.active = Some(BattleSession {
            chapter_id: context.chapter_id,
            copy_id: context.copy_id,
            current_fleet: context.fleet_id.get() as u32,
            started_at: context.started_at,
            expires_at: context.expires_at,
            revision: 0,
            remaining_fleet_ids: context.remaining_fleet_ids,
            hero_ids: context.hero_ids,
            attack_count: 0,
        });
        Ok(())
    }

    pub fn settle(
        account: &mut AccountState,
        copy_id: CopyId,
        victory: bool,
    ) -> Result<bool, GameServiceError> {
        let Some(session) = account.battle.active.take() else {
            return Err(GameServiceError::BattleNotActive);
        };
        if session.copy_id != copy_id {
            account.battle.active = Some(session);
            return Err(GameServiceError::BattleCopyMismatch);
        }
        Ok(victory && account.battle.passed_copies.insert(copy_id))
    }

    pub fn record_attack(
        account: &mut AccountState,
        copy_id: CopyId,
        hero_ids: &[HeroId],
    ) -> Result<(), GameServiceError> {
        let Some(session) = account.battle.active.as_mut() else {
            return Err(GameServiceError::BattleNotActive);
        };
        if session.copy_id != copy_id {
            return Err(GameServiceError::BattleCopyMismatch);
        }
        if hero_ids.is_empty()
            || hero_ids
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != hero_ids.len()
            || hero_ids
                .iter()
                .any(|hero_id| !session.hero_ids.contains(hero_id))
        {
            return Err(GameServiceError::Domain(DomainError::InvalidState(
                "battle attack does not match active session",
            )));
        }
        session.attack_count = session.attack_count.saturating_add(1);
        session.revision = session.revision.saturating_add(1);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blueoath_domain::{
        AccountState, CurrencyKind, FleetRecord, HeroState, NewAccountFactory, ProfileId,
        TemplateId,
    };

    fn account() -> AccountState {
        let mut account = NewAccountFactory::create(ProfileId::new("game").unwrap(), "Game");
        let hero_id = HeroId::new(7).unwrap();
        account.dock.heroes.insert(
            hero_id,
            HeroState {
                id: hero_id,
                template_id: TemplateId::new(70).unwrap(),
                name: String::new(),
                change_name_time: 0,
                level: 1,
                exp: 0,
                mood: 1,
                affection: 0,
                hp: 1,
                locked: false,
                equip_slots: Vec::new(),
                pskills: std::collections::BTreeMap::new(),
            },
        );
        account.fleet.fleets.insert(
            FleetId::new(1).unwrap(),
            FleetRecord {
                members: vec![hero_id],
                ..FleetRecord::default()
            },
        );
        account
    }

    #[test]
    fn resource_and_reward_services_are_atomic_per_command() {
        let mut account = account();
        RewardService::grant(
            &mut account,
            [(CurrencyKind::Gold, 30), (CurrencyKind::Diamond, 2)],
        )
        .unwrap();
        ResourceService::debit(&mut account, CurrencyKind::Gold, 10).unwrap();
        assert_eq!(account.resources.amount(CurrencyKind::Gold).get(), 20);
        assert_eq!(
            account.resources.amount(CurrencyKind::Diamond).get(),
            10_002
        );
    }

    #[test]
    fn battle_service_is_idempotent_for_settlement() {
        let mut account = account();
        let chapter_id = ChapterId::new(1).unwrap();
        let copy_id = CopyId::new(2).unwrap();
        let fleet_id = FleetId::new(1).unwrap();
        BattleService::start(&mut account, chapter_id, copy_id, fleet_id, 10).unwrap();
        assert!(BattleService::settle(&mut account, copy_id, true).unwrap());
        assert!(ProgressService::is_copy_passed(&account, copy_id));
        assert_eq!(
            BattleService::settle(&mut account, copy_id, true),
            Err(GameServiceError::BattleNotActive)
        );
    }

    #[test]
    fn battle_service_persists_typed_session_context() {
        let mut account = account();
        let hero_id = HeroId::new(7).unwrap();
        BattleService::start_with_context(
            &mut account,
            BattleStartContext {
                chapter_id: ChapterId::new(3).unwrap(),
                copy_id: CopyId::new(4).unwrap(),
                fleet_id: FleetId::new(1).unwrap(),
                hero_ids: vec![hero_id],
                remaining_fleet_ids: vec![1, 2],
                started_at: 10,
                expires_at: 20,
            },
        )
        .unwrap();
        let session = account.battle.active.as_ref().unwrap();
        assert_eq!(session.hero_ids, vec![hero_id]);
        assert_eq!(session.remaining_fleet_ids, vec![1, 2]);
        assert_eq!(session.expires_at, 20);
    }

    #[test]
    fn battle_service_records_only_matching_attacks() {
        let mut account = account();
        let hero_id = HeroId::new(7).unwrap();
        BattleService::start(
            &mut account,
            ChapterId::new(1).unwrap(),
            CopyId::new(2).unwrap(),
            FleetId::new(1).unwrap(),
            10,
        )
        .unwrap();
        BattleService::record_attack(&mut account, CopyId::new(2).unwrap(), &[hero_id]).unwrap();
        assert_eq!(account.battle.active.as_ref().unwrap().attack_count, 1);
        assert!(matches!(
            BattleService::record_attack(&mut account, CopyId::new(3).unwrap(), &[hero_id]),
            Err(GameServiceError::BattleCopyMismatch)
        ));
    }
}
