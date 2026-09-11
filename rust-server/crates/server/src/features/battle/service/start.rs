use super::super::*;

use super::context::TypedBattleContext;
use super::payload::daily_copy_enter_payload;

use crate::common::error::GameError;
use crate::common::response::{HandlerResult, Response};

pub(crate) fn handle(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    context: TypedBattleContext<'_>,
) -> HandlerResult {
    let TypedBattleContext {
        battle_catalog,
        fashion_catalog: _,
        hero_level_catalog: _,
        drop_multiplier: _,
        ship_stat_multiplier,
        commander_exp_multiplier: _,
        ship_exp_multiplier: _,
        affection_multiplier: _,
        server_state: _,
        effects: _,
    } = context;

    match method {
        "copy.StartBase" | "copy.PvpStartBase" => {
            let Ok(request) = CopyStartRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "copy start request is invalid",
                ));
            };
            let Some(copy_id) = blueoath_domain::CopyId::new(request.copy_id.max(1) as u64).ok()
            else {
                return HandlerResult::Error(GameError::InvalidRequest("copy id is invalid"));
            };
            let Some((fleet_id, fleet)) = account.fleet.fleets.iter().next() else {
                return HandlerResult::Error(GameError::InvalidState("fleet is not configured"));
            };
            let fleet_id = *fleet_id;
            let fleet_members = fleet.members.clone();
            let requested_hero_ids: Vec<i32> =
                request.hero_groups.iter().flatten().copied().collect();
            let mut hero_ids = requested_hero_ids
                .into_iter()
                .filter_map(|id| u64::try_from(id).ok())
                .filter_map(|id| {
                    account
                        .dock
                        .heroes
                        .keys()
                        .find(|hero| hero.get() == id)
                        .copied()
                })
                .take(6)
                .collect::<Vec<_>>();
            if hero_ids.is_empty() {
                hero_ids = fleet_members.into_iter().take(6).collect();
            }
            if hero_ids.is_empty() {
                return HandlerResult::Error(GameError::InvalidState("fleet has no heroes"));
            }
            if let Some(catalog) = battle_catalog {
                let hero_ids = hero_ids.iter().map(|id| id.get()).collect::<Vec<_>>();
                let Some(supply_cost) =
                    battle_supply_cost_typed(account, Some(catalog), request.copy_id, &hero_ids, 1)
                else {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "battle copy or supply is invalid",
                    ));
                };
                if !catalog.copies.contains_key(&request.copy_id)
                    || account
                        .resources
                        .amount(blueoath_domain::CurrencyKind::Supply)
                        .get()
                        < supply_cost
                {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "battle copy or supply is invalid",
                    ));
                }
            }
            let remaining_fleet_ids = battle_session_fleet_ids(request.copy_id, battle_catalog)
                .into_iter()
                .filter_map(|id| u32::try_from(id).ok())
                .collect::<Vec<_>>();
            if BattleService::start_with_context(
                account,
                BattleStartContext {
                    chapter_id: blueoath_domain::ChapterId::new(request.copy_id.max(1) as u64)
                        .unwrap(),
                    copy_id,
                    fleet_id,
                    hero_ids: hero_ids.clone(),
                    remaining_fleet_ids,
                    started_at: u64::from(current_unix_seconds()),
                    expires_at: u64::from(current_unix_seconds()).saturating_add(1_800),
                },
            )
            .is_err()
            {
                return HandlerResult::Error(GameError::InvalidState("battle cannot start"));
            }
            HandlerResult::Reply(Response::raw(
                method,
                battle_start_payload_from_typed_account(
                    account,
                    request.copy_id,
                    &request.hero_groups,
                    battle_catalog,
                    ship_stat_multiplier,
                    BattleStartOptions {
                        is_running_fight: request.is_running_fight,
                        battle_mode: request.battle_mode,
                        anim_mode: request.anim_mode,
                        match_type: request.match_type,
                    },
                ),
            ))
        }

        "dailycopy.CopyEnter" => {
            let Ok(request) = DailyCopyEnterRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "daily copy enter request is invalid",
                ));
            };
            let known_copy = battle_catalog
                .map(|catalog| catalog.daily_group_by_copy.contains_key(&request.copy_id))
                .unwrap_or(request.chapter_id == 1);
            if request.chapter_id <= 0
                || request.copy_id <= 0
                || request.tactic_id <= 0
                || !known_copy
            {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "daily copy enter request is invalid",
                ));
            }
            let fleet_id = blueoath_domain::FleetId::new(request.tactic_id as u64).ok();
            let mut hero_ids = fleet_id
                .and_then(|fleet_id| account.fleet.fleets.get(&fleet_id))
                .map(|fleet| fleet.members.clone())
                .unwrap_or_default();
            if hero_ids.is_empty() {
                hero_ids = account.dock.heroes.keys().copied().take(6).collect();
            }
            let hero_ids_raw = hero_ids.iter().map(|id| id.get()).collect::<Vec<_>>();
            let supply_cost = battle_catalog.and_then(|catalog| {
                battle_supply_cost_typed(account, Some(catalog), request.copy_id, &hero_ids_raw, 1)
            });
            if hero_ids.is_empty()
                || (battle_catalog.is_some() && supply_cost.is_none())
                || supply_cost.is_some_and(|cost| {
                    account
                        .resources
                        .amount(blueoath_domain::CurrencyKind::Supply)
                        .get()
                        < cost
                })
            {
                return HandlerResult::Error(GameError::InvalidState(
                    "insufficient supply or missing daily tactic",
                ));
            }
            let Ok(chapter_id) = blueoath_domain::ChapterId::new(request.chapter_id as u64) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "daily copy chapter is invalid",
                ));
            };
            let Ok(copy_id) = blueoath_domain::CopyId::new(request.copy_id as u64) else {
                return HandlerResult::Error(GameError::InvalidRequest("daily copy id is invalid"));
            };
            let fleet_id = fleet_id.unwrap_or_else(|| {
                account
                    .fleet
                    .fleets
                    .keys()
                    .next()
                    .copied()
                    .unwrap_or_else(|| blueoath_domain::FleetId::new(1).unwrap())
            });
            let now = u64::from(current_unix_seconds());
            let remaining_fleet_ids = battle_session_fleet_ids(request.copy_id, battle_catalog)
                .into_iter()
                .filter_map(|id| u32::try_from(id).ok())
                .collect::<Vec<_>>();
            if BattleService::start_with_context(
                account,
                BattleStartContext {
                    chapter_id,
                    copy_id,
                    fleet_id,
                    hero_ids: hero_ids.clone(),
                    remaining_fleet_ids,
                    started_at: now,
                    expires_at: now.saturating_add(1_800),
                },
            )
            .is_err()
            {
                return HandlerResult::Error(GameError::InvalidState(
                    "daily copy battle cannot start",
                ));
            }
            let chapter_id = blueoath_domain::ChapterId::new(request.chapter_id as u64)
                .expect("validated daily copy chapter");
            account.daily_copy.reset_day = ((now + 8 * 60 * 60) / 86_400) as u32;
            account
                .daily_copy
                .challenge_times
                .entry(chapter_id)
                .and_modify(|times| *times = times.saturating_add(1))
                .or_insert(1);
            HandlerResult::Reply(Response::raw(
                method,
                daily_copy_enter_payload(&battle_start_payload_from_typed_account(
                    account,
                    request.copy_id,
                    &[hero_ids
                        .iter()
                        .filter_map(|id| i32::try_from(id.get()).ok())
                        .collect()],
                    battle_catalog,
                    ship_stat_multiplier,
                    BattleStartOptions::default(),
                )),
            ))
        }

        "copy.AttackBase" => {
            if method != "copy.AttackBase" {
                return HandlerResult::Empty;
            }
            let Ok(request) = CopyAttackRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "copy attack request is invalid",
                ));
            };
            let Ok(copy_id) = blueoath_domain::CopyId::new(request.copy_id) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "battle copy id is invalid",
                ));
            };
            let hero_ids = request
                .hero_ids
                .iter()
                .copied()
                .filter_map(|hero_id| blueoath_domain::HeroId::new(hero_id).ok())
                .collect::<Vec<_>>();
            if BattleService::record_attack(account, copy_id, &hero_ids).is_err() {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "battle attack does not match active session",
                ));
            }
            HandlerResult::Reply(Response::raw(
                method,
                battle_attack_payload_from_request(&request, 0),
            ))
        }

        "copy.QuitBase" => {
            account.battle.active = None;
            HandlerResult::Reply(Response::raw(method, request_args.to_vec()))
        }

        _ => HandlerResult::Empty,
    }
}
