use super::super::*;
use super::rewards::draw_typed_battle_drop_rewards;

use crate::common::error::GameError;
use crate::common::response::{HandlerResult, Response, ResponseEffects};

pub(crate) fn handle(
    state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    battle_catalog: Option<&BattleCatalog>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let now = current_unix_seconds() as u64;
    match method {
        "mopUp.GetMopUpData" => {
            HandlerResult::Reply(Response::raw(method, typed_mop_up_payload(account, &[])))
        }
        "mopUp.CheckSweep" => {
            let active = account
                .sweep
                .entries
                .iter()
                .filter(|entry| entry.end_time > now)
                .count();
            HandlerResult::Reply(Response::raw(method, vec![0x08, (active == 0) as u8]))
        }
        "mopUp.StopSweep" => {
            let Ok(request) = MopUpRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest("sweep request is invalid"));
            };
            let fleet_id = request.fleet_id;
            let copy_id = request.copy_id;
            account.sweep.entries.retain(|entry| {
                (fleet_id != 0 && entry.fleet_id != fleet_id)
                    || (copy_id != 0 && entry.copy_id != copy_id)
            });
            let payload = typed_mop_up_payload(account, &[]);
            effects.push_post(Response::raw("mopUp.GetMopUpData", payload.clone()));
            HandlerResult::Reply(Response::raw(method, payload))
        }
        "mopUp.StartSweep" => {
            let Ok(request) = MopUpRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest("sweep request is invalid"));
            };
            let fleet_id = request.fleet_id;
            let copy_id = request.copy_id;
            let sweep_count = request.sweep_count;
            if fleet_id == 0
                || copy_id == 0
                || !(1..=99).contains(&sweep_count)
                || !account.fleet.fleets.keys().any(|id| id.get() == fleet_id)
                || !account
                    .battle
                    .passed_copies
                    .iter()
                    .any(|id| id.get() == copy_id)
            {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "sweep fleet or copy is not available",
                ));
            }
            let Some(catalog) = battle_catalog else {
                return HandlerResult::Error(GameError::CatalogUnavailable);
            };
            let Some(fleet_id_typed) = blueoath_domain::FleetId::new(fleet_id).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest("sweep fleet is invalid"));
            };
            let hero_ids = account
                .fleet
                .fleets
                .get(&fleet_id_typed)
                .map(|fleet| fleet.members.iter().map(|id| id.get()).collect::<Vec<_>>())
                .unwrap_or_default();
            let supply_cost = battle_supply_cost_typed(
                account,
                Some(catalog),
                copy_id as i32,
                &hero_ids,
                sweep_count as i32,
            );
            if hero_ids.is_empty()
                || supply_cost.is_none()
                || supply_cost.is_some_and(|cost| {
                    account
                        .resources
                        .amount(blueoath_domain::CurrencyKind::Supply)
                        .get()
                        < cost
                })
            {
                return HandlerResult::Error(GameError::InvalidState(
                    "insufficient supply or missing supply configuration",
                ));
            }
            let rewards = typed_sweep_rewards(
                account,
                catalog,
                copy_id as i32,
                sweep_count,
                state.drop_multiplier,
            );
            if !rewards.is_empty() && !can_grant_typed_task_rewards(account, &rewards) {
                return HandlerResult::Error(GameError::InvalidState(
                    "sweep reward is unsupported",
                ));
            }
            let account_before_settlement = account.clone();
            let mut rewards = rewards;
            if !grant_typed_task_rewards_with_fashion(account, &mut rewards, None) {
                *account = account_before_settlement;
                return HandlerResult::Error(GameError::InvalidState(
                    "sweep reward could not be granted",
                ));
            }
            apply_typed_sweep_experience(
                account,
                catalog,
                copy_id,
                fleet_id,
                sweep_count,
                state.commander_exp_multiplier,
                state.ship_exp_multiplier,
            );
            account
                .sweep
                .entries
                .retain(|entry| entry.fleet_id != fleet_id);
            account
                .sweep
                .entries
                .push(blueoath_domain::SweepEntryState {
                    fleet_id,
                    copy_id,
                    start_time: now,
                    end_time: now.saturating_add(1),
                    sweep_counts: sweep_count,
                    chapter_id: 0,
                });
            account.sweep.entries.retain(|entry| entry.end_time > now);
            if account
                .resources
                .debit(
                    blueoath_domain::CurrencyKind::Supply,
                    supply_cost.expect("validated sweep supply cost"),
                )
                .is_err()
            {
                *account = account_before_settlement;
                return HandlerResult::Error(GameError::InvalidState(
                    "sweep supply settlement failed",
                ));
            }
            let pass_rets = mop_up_pass_rets(copy_id as i32, &rewards);
            let payload = typed_mop_up_payload(account, &pass_rets);
            effects.push_pre(Response::raw(
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
            ));
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "equip.UpdateEquipBagData",
                EquipListCodec::encode(&equip_list_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "fashion.updateData",
                FashionListCodec::encode(&fashion_list_from_typed_account(account, None)),
            ));
            effects.push_post(Response::raw("mopUp.GetMopUpData", payload.clone()));
            HandlerResult::Reply(Response::raw(method, payload))
        }
        _ => HandlerResult::Empty,
    }
}

fn typed_sweep_rewards(
    account: &blueoath_domain::AccountState,
    catalog: &BattleCatalog,
    copy_id: i32,
    sweep_count: u64,
    drop_multiplier: f64,
) -> Vec<ShopReward> {
    let mut rewards = Vec::new();
    let first_pass = !account
        .battle
        .passed_copies
        .iter()
        .any(|id| id.get() == u64::try_from(copy_id).unwrap_or_default());
    if first_pass {
        if let Some(first) = catalog.copy_first_rewards.get(&copy_id) {
            rewards.extend(first.iter().map(|(goods_type, item_id, num)| ShopReward {
                goods_type: *goods_type,
                item_id: *item_id,
                num: *num,
                instance_id: 0,
            }));
        }
    }
    let has_copy_first_reward = catalog.copy_first_rewards.contains_key(&copy_id);
    for index in 0..sweep_count {
        rewards.extend(draw_typed_battle_drop_rewards(
            catalog,
            copy_id,
            drop_multiplier,
            3,
            index == 0 && first_pass && !has_copy_first_reward,
        ));
    }
    rewards
}

fn apply_typed_sweep_experience(
    account: &mut blueoath_domain::AccountState,
    catalog: &BattleCatalog,
    copy_id: u64,
    fleet_id: u64,
    sweep_count: u64,
    commander_multiplier: f64,
    ship_multiplier: f64,
) {
    let Ok(fleet_id) = blueoath_domain::FleetId::new(fleet_id) else {
        return;
    };
    let hero_ids = account
        .fleet
        .fleets
        .get(&fleet_id)
        .map(|fleet| fleet.members.clone())
        .unwrap_or_default();
    if hero_ids.is_empty() {
        return;
    }
    let Ok(copy_id) = i32::try_from(copy_id) else {
        return;
    };
    let (commander_base, ship_base) = battle_copy_experience(Some(catalog), copy_id);
    let count = sweep_count as f64;
    let commander_exp =
        scale_reward(i64::from(commander_base), commander_multiplier * count).max(0) as u64;
    account.character.exp = account.character.exp.saturating_add(commander_exp);

    let ship_exp = scale_reward(i64::from(ship_base), ship_multiplier * count).max(0);
    for hero_id in hero_ids {
        let Some(hero) = account.dock.heroes.get_mut(&hero_id) else {
            continue;
        };
        let mood_multiplier = if i64::from(hero.mood) >= MOOD_AFFECTION_BONUS_THRESHOLD {
            1.2
        } else {
            1.0
        };
        let gained = scale_reward(ship_exp, mood_multiplier).max(0) as u64;
        hero.exp = hero.exp.saturating_add(gained);
    }
}

fn typed_mop_up_payload(account: &blueoath_domain::AccountState, pass_rets: &[Vec<u8>]) -> Vec<u8> {
    let now = current_unix_seconds() as u64;
    let mut output = Vec::new();
    let active = account
        .sweep
        .entries
        .iter()
        .filter(|entry| entry.end_time > now)
        .count();
    append_varint_field(&mut output, 1, active as u64);
    for entry in &account.sweep.entries {
        let mut encoded = Vec::new();
        append_varint_field(&mut encoded, 1, entry.fleet_id);
        append_varint_field(&mut encoded, 2, entry.copy_id);
        append_varint_field(&mut encoded, 3, entry.start_time);
        append_varint_field(&mut encoded, 4, entry.end_time);
        append_varint_field(&mut encoded, 5, entry.sweep_counts);
        append_varint_field(&mut encoded, 6, entry.chapter_id);
        append_message_field(&mut output, 2, &encoded);
    }
    for pass_ret in pass_rets {
        append_message_field(&mut output, 3, pass_ret);
    }
    output
}
