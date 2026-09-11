use super::super::*;

use super::context::TypedBattleContext;

use crate::common::error::GameError;
use crate::common::response::{HandlerResult, Response};

pub(crate) fn handle_pass_mini_game(
    method: &str,
    account: &mut blueoath_domain::AccountState,
    request_args: &[u8],
    context: TypedBattleContext<'_>,
) -> HandlerResult {
    let TypedBattleContext {
        battle_catalog,
        fashion_catalog,
        hero_level_catalog: _,
        drop_multiplier: _,
        ship_stat_multiplier: _,
        commander_exp_multiplier: _,
        ship_exp_multiplier: _,
        affection_multiplier: _,
        server_state: _,
        effects,
    } = context;

    let Ok(request) = CopyMiniGamePassRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "mini-game pass request is invalid",
        ));
    };
    let copy_id = request.copy_id;
    if battle_catalog.is_some_and(|catalog| !catalog.copies.contains_key(&copy_id)) {
        return HandlerResult::Error(GameError::InvalidRequest(
            "mini-game copy is not configured",
        ));
    }
    let Ok(copy_id_typed) = blueoath_domain::CopyId::new(copy_id as u64) else {
        return HandlerResult::Error(GameError::InvalidRequest("mini-game copy id is invalid"));
    };
    let first_pass = account.battle.passed_copies.insert(copy_id_typed);
    let rewards = battle_catalog
        .and_then(|catalog| catalog.copy_first_rewards.get(&copy_id))
        .into_iter()
        .flatten()
        .map(|(goods_type, item_id, num)| ShopReward {
            goods_type: *goods_type,
            item_id: *item_id,
            num: *num,
            instance_id: 0,
        })
        .collect::<Vec<_>>();
    if first_pass && (!rewards.is_empty() && !can_grant_typed_task_rewards(account, &rewards)) {
        account.battle.passed_copies.remove(&copy_id_typed);
        return HandlerResult::Error(GameError::InvalidState("mini-game reward is unsupported"));
    }
    if first_pass {
        let mut rewards = rewards;
        if !grant_typed_task_rewards_with_fashion(account, &mut rewards, fashion_catalog) {
            account.battle.passed_copies.remove(&copy_id_typed);
            return HandlerResult::Error(GameError::InvalidState(
                "mini-game reward could not be granted",
            ));
        }
        account.battle.copy_stars.insert(copy_id_typed, 7);
        effects.push_post(Response::raw(
            "bag.UpdateBagData",
            BagInfoCodec::encode(&bag_info_from_typed_account(account)),
        ));
        let hero_ids = account
            .fleet
            .fleets
            .values()
            .next()
            .map(|fleet| fleet.members.clone())
            .unwrap_or_default();
        account
            .battle
            .records
            .push(blueoath_domain::CopyRecordState {
                copy_id: copy_id_typed,
                hero_ids,
                pass_time: u64::try_from(request.battle_time.max(1)).unwrap_or(1),
                secret_id: 0,
                strategy_id: 0,
                power: 0,
                record_time: u64::from(current_unix_seconds()),
                ex_buffs: Vec::new(),
            });
        return HandlerResult::Reply(Response::raw(
            method,
            battle_pass_payload_with_rewards(copy_id, first_pass, 3, request.battle_time, &rewards),
        ));
    }
    let hero_ids = account
        .fleet
        .fleets
        .values()
        .next()
        .map(|fleet| fleet.members.clone())
        .unwrap_or_default();
    account
        .battle
        .records
        .push(blueoath_domain::CopyRecordState {
            copy_id: copy_id_typed,
            hero_ids,
            pass_time: u64::try_from(request.battle_time.max(1)).unwrap_or(1),
            secret_id: 0,
            strategy_id: 0,
            power: 0,
            record_time: u64::from(current_unix_seconds()),
            ex_buffs: Vec::new(),
        });
    HandlerResult::Reply(Response::raw(
        method,
        battle_pass_payload_with_rewards(copy_id, first_pass, 3, request.battle_time, &[]),
    ))
}

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
        ship_stat_multiplier: _,
        commander_exp_multiplier: _,
        ship_exp_multiplier: _,
        affection_multiplier: _,
        server_state: _,
        effects: _,
    } = context;

    match method {
        "copy.GetRecord" => {
            let Ok(request) = CopyRecordRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "copy record request is invalid",
                ));
            };
            HandlerResult::Reply(Response::raw(
                method,
                CopyRecordListCodec::encode(&copy_record_list_from_typed_account(
                    account,
                    request.copy_id,
                )),
            ))
        }

        "copy.DeleteRecord" => {
            let Ok(request) = CopyRecordRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "copy record request is invalid",
                ));
            };
            let Some(position) = account
                .battle
                .records
                .iter()
                .enumerate()
                .filter(|(_, record)| record.copy_id.get() == request.copy_id as u64)
                .map(|(position, _)| position)
                .nth(request.index as usize)
            else {
                return HandlerResult::Error(GameError::InvalidState("copy record is not found"));
            };
            account.battle.records.remove(position);
            HandlerResult::Reply(Response::raw(
                method,
                CopyRecordListCodec::encode(&copy_record_list_from_typed_account(
                    account,
                    request.copy_id,
                )),
            ))
        }

        "copy.TacticOn" => {
            let Ok(request) = CopyRecordRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "copy record request is invalid",
                ));
            };
            let Some(hero_ids) = account
                .battle
                .records
                .iter()
                .filter(|record| record.copy_id.get() == request.copy_id as u64)
                .nth(request.index as usize)
                .map(|record| record.hero_ids.clone())
                .filter(|hero_ids| !hero_ids.is_empty())
            else {
                return HandlerResult::Error(GameError::InvalidState("copy record is not found"));
            };
            {
                let Some(fleet) = account.fleet.fleets.values_mut().next() else {
                    return HandlerResult::Error(GameError::InvalidState(
                        "fleet is not configured",
                    ));
                };
                fleet.members = hero_ids;
            }
            HandlerResult::Reply(Response::raw(
                method,
                FleetInfoCodec::encode(&fleet_info_from_typed_account(account)),
            ))
        }

        "copyinfo.GetCopyInfo" => {
            let Ok(request) = CopyIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "copy info request is invalid",
                ));
            };
            HandlerResult::Reply(Response::raw(
                method,
                CopyInfoCodec::encode_record_response(&copy_info_response_from_typed_account(
                    account,
                    request.copy_id,
                )),
            ))
        }

        "copy.GetRandomFactors" => {
            let Ok(request) = CopyIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "copy random factor request is invalid",
                ));
            };

            HandlerResult::Reply(Response::raw(
                method,
                encode_random_factor_payload(request.copy_id, battle_catalog),
            ))
        }

        _ => HandlerResult::Empty,
    }
}
