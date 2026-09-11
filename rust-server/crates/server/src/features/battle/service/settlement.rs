use super::super::*;

use super::context::TypedBattleContext;
use super::rewards::{
    draw_typed_battle_drop_rewards_for_fleets, draw_typed_battle_wave_drop_rewards,
    queue_battle_inventory_refresh,
};

use crate::common::error::GameError;
use crate::common::response::{HandlerResult, Response};

pub(crate) fn handle_pass_base(
    method: &str,
    account: &mut blueoath_domain::AccountState,
    request_args: &[u8],
    context: TypedBattleContext<'_>,
) -> HandlerResult {
    let TypedBattleContext {
        battle_catalog,
        fashion_catalog,
        hero_level_catalog,
        drop_multiplier,
        ship_stat_multiplier,
        commander_exp_multiplier,
        ship_exp_multiplier,
        affection_multiplier,
        server_state,
        effects,
    } = context;

    let Some(active) = account.battle.active.as_ref() else {
        return HandlerResult::Error(GameError::InvalidState("battle session is not active"));
    };
    let copy_id = active.copy_id;
    let hero_ids = active.hero_ids.clone();
    let started_at = active.started_at;
    let Ok(request) = CopyPassRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest("battle pass request is invalid"));
    };
    let result = battle_pass_result_from_request(&request);
    save_typed_battle_hero_hp(
        account,
        &result.heroes,
        &hero_ids,
        SHIP_STAT_CATALOG.get(),
        EQUIP_CATALOG.get(),
        SHIP_REMOULD_CATALOG.get(),
        ship_stat_multiplier,
    );
    let grade = if result.grade > 0 { result.grade } else { 3 };
    let first_pass_expected = !account.battle.passed_copies.contains(&copy_id);
    let remaining_fleet_ids = account
        .battle
        .active
        .as_ref()
        .map(|session| session.remaining_fleet_ids.clone())
        .unwrap_or_default();
    let reported_fleet_ids = if result.passed_fleet_ids.is_empty() {
        Vec::new()
    } else {
        result
            .passed_fleet_ids
            .iter()
            .map(|fleet_id| {
                battle_catalog
                    .and_then(|catalog| {
                        battle_fleet_aliases(
                            i32::try_from(copy_id.get()).unwrap_or_default(),
                            Some(catalog),
                        )
                        .into_iter()
                        .find(|(wire_id, _)| i64::from(*wire_id) == *fleet_id as i64)
                        .map(|(_, real_id)| real_id as u64)
                    })
                    .unwrap_or(*fleet_id)
            })
            .collect()
    };
    let Some(current_fleet_id) = remaining_fleet_ids.first().copied() else {
        return HandlerResult::Error(GameError::InvalidState(
            "battle session has no remaining wave",
        ));
    };
    // Client can report more than one fleet in FleetInfo/EnemyFleets. One
    // PassBase is one wave settlement; session order is authoritative.
    // Otherwise a 5-wave copy can be completed after 4 requests.
    if !reported_fleet_ids.is_empty() && !reported_fleet_ids.contains(&u64::from(current_fleet_id))
    {
        tracing::warn!(
            copy_id = copy_id.get(),
            current_fleet_id,
            reported_fleet_ids = ?reported_fleet_ids,
            "battle pass reported fleet is not current; using session wave"
        );
    }
    let passed_fleet_ids = vec![u64::from(current_fleet_id)];
    let remaining_after_wave = remaining_fleet_ids
        .into_iter()
        .filter(|fleet_id| !passed_fleet_ids.contains(&u64::from(*fleet_id)))
        .collect::<Vec<_>>();
    let is_final_wave = remaining_after_wave.is_empty();
    let mut rewards = Vec::new();
    if grade < 9 {
        if is_final_wave && first_pass_expected {
            rewards.extend(
                battle_catalog
                    .and_then(|catalog| catalog.copy_first_rewards.get(&(copy_id.get() as i32)))
                    .into_iter()
                    .flatten()
                    .map(|(goods_type, item_id, num)| ShopReward {
                        goods_type: *goods_type,
                        item_id: *item_id,
                        num: *num,
                        instance_id: 0,
                    }),
            );
        }
        if let Some(catalog) = battle_catalog {
            let current_fleet_ids = passed_fleet_ids
                .iter()
                .filter_map(|fleet_id| i32::try_from(*fleet_id).ok())
                .collect::<Vec<_>>();
            let wave_rewards = if is_final_wave {
                draw_typed_battle_drop_rewards_for_fleets(
                    catalog,
                    copy_id.get() as i32,
                    &current_fleet_ids,
                    drop_multiplier,
                    grade,
                    true,
                    first_pass_expected,
                )
            } else {
                draw_typed_battle_wave_drop_rewards(
                    catalog,
                    copy_id.get() as i32,
                    &current_fleet_ids,
                    drop_multiplier,
                    grade,
                    false,
                )
            };
            rewards.extend(wave_rewards);
        }
    }
    if !can_grant_typed_task_rewards(account, &rewards) {
        return HandlerResult::Error(GameError::InvalidState("battle reward cannot be granted"));
    }
    let supply_cost = if is_final_wave {
        match battle_catalog {
            Some(catalog) => {
                let Some(cost) = battle_supply_cost_typed(
                    account,
                    Some(catalog),
                    i32::try_from(copy_id.get()).unwrap_or_default(),
                    &hero_ids.iter().map(|id| id.get()).collect::<Vec<_>>(),
                    1,
                ) else {
                    return HandlerResult::Error(GameError::InvalidState(
                        "battle supply configuration is invalid",
                    ));
                };
                Some(cost)
            }
            None => None,
        }
    } else {
        None
    };
    if let Some(supply_cost) = supply_cost {
        if account
            .resources
            .amount(blueoath_domain::CurrencyKind::Supply)
            .get()
            < supply_cost
        {
            return HandlerResult::Error(GameError::InvalidState(
                "insufficient supply for battle settlement",
            ));
        }
    }
    let account_before_settlement = account.clone();
    let first_pass = if is_final_wave {
        BattleService::settle_at(
            account,
            copy_id,
            grade < 9,
            u64::from(current_unix_seconds()),
        )
        .map_err(|_| GameError::InvalidState("battle settlement is invalid"))
    } else {
        Ok(false)
    };
    match first_pass {
        Ok(first_pass) => {
            if is_final_wave && grade < 9 {
                if let Some(group_id) = battle_catalog.and_then(|catalog| {
                    catalog
                        .daily_group_by_copy
                        .get(&i32::try_from(copy_id.get()).ok()?)
                }) {
                    let group_id = u64::try_from(*group_id).unwrap_or_default();
                    let success_times = account
                        .daily_copy
                        .group_success_times
                        .entry(group_id)
                        .or_default();
                    *success_times = success_times.saturating_add(1);
                }
            }
            if !grant_typed_task_rewards_with_fashion(account, &mut rewards, fashion_catalog) {
                *account = account_before_settlement;
                return HandlerResult::Error(GameError::InvalidState(
                    "battle reward could not be granted",
                ));
            }
            let exp_rewards = if grade < 9 {
                let current_fleet_ids = passed_fleet_ids
                    .iter()
                    .filter_map(|fleet_id| i32::try_from(*fleet_id).ok())
                    .collect::<Vec<_>>();
                let (commander_base, ship_base) =
                    battle_fleet_experience(battle_catalog, &current_fleet_ids);
                let (exp_ratio, _) = battle_evaluation_multipliers(battle_catalog, grade);
                let commander_exp = scale_reward(
                    i64::from(commander_base),
                    commander_exp_multiplier * exp_ratio,
                )
                .max(0) as u64;
                let ship_exp = scale_reward(i64::from(ship_base), ship_exp_multiplier * exp_ratio)
                    .max(0) as u64;
                add_commander_battle_exp_typed(
                    account,
                    commander_exp,
                    COMMANDER_LEVEL_CATALOG.get(),
                );
                add_ship_battle_exp_typed(account, &hero_ids, ship_exp, hero_level_catalog)
            } else {
                Vec::new()
            };
            let settlement_changed = if grade < 9 {
                let shipwrecked_ids = result
                    .heroes
                    .iter()
                    .filter(|hero| hero.hp == 0)
                    .map(|hero| hero.hero_id)
                    .collect::<std::collections::HashSet<_>>();
                battle_catalog
                    .and_then(|catalog| {
                        catalog
                            .settlement_by_copy
                            .get(&i32::try_from(copy_id.get()).unwrap_or_default())
                            .copied()
                    })
                    .is_some_and(|rule| {
                        apply_battle_settlement_typed(
                            account,
                            &hero_ids,
                            result.mvp_hero_id,
                            &shipwrecked_ids,
                            rule,
                            affection_multiplier,
                        )
                    })
            } else {
                false
            };
            if supply_cost.is_some_and(|_| {
                !consume_battle_supply_typed(
                    account,
                    battle_catalog,
                    i32::try_from(copy_id.get()).unwrap_or_default(),
                    &hero_ids.iter().map(|id| id.get()).collect::<Vec<_>>(),
                    1,
                )
            }) {
                *account = account_before_settlement;
                return HandlerResult::Error(GameError::InvalidState(
                    "battle supply settlement failed",
                ));
            }
            if !is_final_wave {
                if let Some(active) = account.battle.active.as_mut() {
                    active.remaining_fleet_ids = remaining_after_wave.clone();
                    active.current_fleet = remaining_after_wave[0];
                }
            }
            if is_final_wave && grade < 9 {
                let star_level = account.battle.copy_stars.entry(copy_id).or_default();
                *star_level = (*star_level).max(7);
            }
            if !rewards.is_empty() || !exp_rewards.is_empty() || settlement_changed {
                queue_battle_inventory_refresh(effects, account, fashion_catalog);
            }
            if let Some(server_state) = server_state {
                effects.push_post(Response::raw(
                    "user.UpdateUserInfo",
                    UserInfoCodec::encode(&user_info_from_typed_account(server_state, account)),
                ));
            }
            if is_final_wave && grade < 9 {
                account
                    .battle
                    .records
                    .push(blueoath_domain::CopyRecordState {
                        copy_id,
                        hero_ids,
                        pass_time: u64::try_from(result.battle_time).unwrap_or_else(|_| {
                            u64::from(current_unix_seconds()).saturating_sub(started_at)
                        }),
                        secret_id: 0,
                        strategy_id: 0,
                        power: 0,
                        record_time: u64::from(current_unix_seconds()),
                        ex_buffs: Vec::new(),
                    });
            }
            HandlerResult::Reply(Response::raw(
                method,
                battle_pass_payload_with_experience(
                    i32::try_from(copy_id.get()).unwrap_or_default(),
                    first_pass,
                    grade,
                    result.battle_time,
                    &rewards,
                    &exp_rewards,
                ),
            ))
        }
        Err(error) => HandlerResult::Error(error),
    }
}

pub(crate) fn save_typed_battle_hero_hp(
    account: &mut blueoath_domain::AccountState,
    heroes: &[BattleHeroResult],
    allowed_hero_ids: &[blueoath_domain::HeroId],
    ship_stat_catalog: Option<&ShipStatCatalog>,
    equip_catalog: Option<&EquipCatalog>,
    remould_catalog: Option<&ShipRemouldCatalog>,
    ship_stat_multiplier: f64,
) {
    let hero_snapshot = account.dock.heroes.clone();
    for result in heroes {
        if result.hero_id == 0
            || (!allowed_hero_ids.is_empty()
                && !allowed_hero_ids
                    .iter()
                    .any(|hero_id| hero_id.get() == result.hero_id))
        {
            continue;
        }
        let Ok(hero_id) = blueoath_domain::HeroId::new(result.hero_id) else {
            continue;
        };
        let progress = &account.activities.progress;
        if let Some(hero) = account.dock.heroes.get_mut(&hero_id) {
            let max_hp = ship_max_hp_for_typed_hero_with_heroes(
                hero,
                progress,
                &account.dock.equipments,
                ship_stat_catalog,
                equip_catalog,
                remould_catalog,
                ship_stat_multiplier,
                Some(&hero_snapshot),
            );
            hero.hp = typed_hero_hp_from_client_ratio(result.hp, max_hp).min(max_hp);
        }
    }
}
