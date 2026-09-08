use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;
use crate::features::copy::mopup_state::{
    draw_copy_drop_with_seed, draw_draw_count, next_battle_drop_seed,
};

#[cfg(test)]
pub(crate) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let mut effects = ResponseEffects::default();
    handle_typed_with_catalog(
        account,
        method,
        request_args,
        TypedBattleContext {
            battle_catalog: None,
            fashion_catalog: None,
            drop_multiplier: 1.0,
            ship_stat_multiplier: 1.0,
            effects: &mut effects,
        },
    )
}

pub(crate) fn handle_typed_copy_star_reward(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    chapter_catalog: Option<&ChapterCatalog>,
    task_catalog: Option<&TaskCatalog>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let Ok(request) = CopyStarRewardRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "copy star reward request is invalid",
        ));
    };
    let chapter_id = request.chapter_id;
    let indexes: Vec<i64> = request
        .indexes
        .iter()
        .map(|index| i64::from(*index))
        .collect();
    let Some(chapter_rewards) =
        chapter_catalog.and_then(|catalog| catalog.star_rewards_by_chapter.get(&chapter_id))
    else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "copy star reward chapter was not found",
        ));
    };
    let Some(task_catalog) = task_catalog else {
        return HandlerResult::Error(GameError::InvalidState(
            "copy reward catalog is unavailable",
        ));
    };
    if chapter_id <= 0 || indexes.is_empty() {
        return HandlerResult::Error(GameError::InvalidRequest(
            "copy star reward request is invalid",
        ));
    }
    let Ok(chapter_id_u32) = u32::try_from(chapter_id) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "copy star reward chapter is invalid",
        ));
    };
    let requested_indexes = indexes.clone();
    let star_num = chapter_rewards
        .level_ids
        .iter()
        .filter(|copy_id| {
            u64::try_from(**copy_id)
                .ok()
                .and_then(|id| blueoath_domain::CopyId::new(id).ok())
                .is_some_and(|copy_id| account.battle.passed_copies.contains(&copy_id))
        })
        .map(|_| 7)
        .sum::<i32>();
    let mut pending = Vec::new();
    let mut pending_indexes = std::collections::BTreeSet::new();
    for index in indexes {
        let Some(position) = usize::try_from(index.saturating_sub(1)).ok() else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "copy star reward index is invalid",
            ));
        };
        let Some(required_stars) = chapter_rewards.star_conditions.get(position).copied() else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "copy star reward index is invalid",
            ));
        };
        let Some(reward_id) = chapter_rewards.reward_ids.get(position).copied() else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "copy star reward is not configured",
            ));
        };
        let reward_index = u32::try_from(index).unwrap_or_default();
        if !pending_indexes.insert(index)
            || star_num < required_stars
            || account
                .battle
                .claimed_star_rewards
                .contains(&(chapter_id_u32, reward_index))
        {
            return HandlerResult::Error(GameError::InvalidState(
                "copy star reward is unavailable",
            ));
        }
        let Some(rewards) = task_catalog.rewards_by_id.get(&reward_id) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "copy star reward is not configured",
            ));
        };
        if rewards.is_empty() {
            return HandlerResult::Error(GameError::InvalidRequest(
                "copy star reward is not configured",
            ));
        }
        pending.extend(rewards.iter().map(|(goods_type, item_id, num)| ShopReward {
            goods_type: *goods_type,
            item_id: *item_id,
            num: *num,
            instance_id: 0,
        }));
    }
    if !can_grant_typed_task_rewards(account, &pending) {
        return HandlerResult::Error(GameError::InvalidState(
            "copy star reward type is unsupported",
        ));
    }
    for reward in &pending {
        let _ = grant_typed_task_reward(account, reward);
    }
    for index in requested_indexes {
        account
            .battle
            .claimed_star_rewards
            .insert((chapter_id_u32, u32::try_from(index).unwrap_or_default()));
    }
    effects.push_pre(Response::raw(
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_typed_account(account)),
    ));
    HandlerResult::Reply(Response::raw(method, encode_task_reward_list(&pending)))
}

fn draw_typed_battle_drop_rewards(
    catalog: &BattleCatalog,
    copy_id: i32,
    multiplier: f64,
    grade: i32,
) -> Vec<ShopReward> {
    let mut rewards = Vec::new();
    let mut draw_index = 0u64;
    let seed = next_battle_drop_seed();
    let settle_multiplier = multiplier * battle_evaluation_multipliers(Some(catalog), grade).1;
    let other_multiplier = multiplier * battle_other_drop_multiplier(Some(catalog), grade);
    for drop_id in catalog.copy_drop_ids.get(&copy_id).into_iter().flatten() {
        let draws = draw_draw_count(
            settle_multiplier,
            mix_build_draw_roll(seed.wrapping_add(draw_index)),
        );
        draw_index = draw_index.wrapping_add(1);
        for _ in 0..draws {
            let roll_seed = seed.wrapping_add(draw_index);
            draw_index = draw_index.wrapping_add(1);
            if let Some(mut reward) = draw_copy_drop_with_seed(catalog, *drop_id, 0, roll_seed) {
                catalog.drop_quantities.apply(
                    copy_id,
                    &mut reward,
                    roll_seed.wrapping_add(0xD1B5_4A32_D192_ED03),
                );
                rewards.push(reward);
            }
        }
    }
    for fleet_id in battle_session_fleet_ids(copy_id, Some(catalog)) {
        for drop_id in catalog.fleet_drop_ids.get(&fleet_id).into_iter().flatten() {
            if let Some(mut reward) =
                draw_copy_drop_with_seed(catalog, *drop_id, 0, seed.wrapping_add(draw_index))
            {
                draw_index = draw_index.wrapping_add(1);
                catalog
                    .drop_quantities
                    .apply(copy_id, &mut reward, seed.wrapping_add(draw_index));
                rewards.push(reward);
            }
        }
        for drop_id in catalog
            .fleet_other_drop_ids
            .get(&fleet_id)
            .into_iter()
            .flatten()
        {
            if let Some(mut reward) =
                draw_copy_drop_with_seed(catalog, *drop_id, 0, seed.wrapping_add(draw_index))
            {
                draw_index = draw_index.wrapping_add(1);
                catalog
                    .drop_quantities
                    .apply(copy_id, &mut reward, seed.wrapping_add(draw_index));
                if draw_draw_count(
                    other_multiplier,
                    mix_build_draw_roll(seed.wrapping_add(draw_index)),
                ) > 0
                {
                    rewards.push(reward);
                }
                draw_index = draw_index.wrapping_add(1);
            }
        }
        for drop_id in catalog
            .fleet_settle_drop_ids
            .get(&fleet_id)
            .into_iter()
            .flatten()
        {
            if let Some(mut reward) =
                draw_copy_drop_with_seed(catalog, *drop_id, 0, seed.wrapping_add(draw_index))
            {
                draw_index = draw_index.wrapping_add(1);
                catalog
                    .drop_quantities
                    .apply(copy_id, &mut reward, seed.wrapping_add(draw_index));
                if draw_draw_count(
                    settle_multiplier,
                    mix_build_draw_roll(seed.wrapping_add(draw_index)),
                ) > 0
                {
                    rewards.push(reward);
                }
                draw_index = draw_index.wrapping_add(1);
            }
        }
    }
    if let Some((count, guaranteed)) = catalog.copy_must_drop_rewards.get(&copy_id) {
        for _ in 0..*count {
            rewards.extend(
                guaranteed
                    .iter()
                    .map(|(goods_type, item_id, num)| ShopReward {
                        goods_type: *goods_type,
                        item_id: *item_id,
                        num: *num,
                        instance_id: 0,
                    }),
            );
        }
    }
    rewards
}

pub(crate) struct TypedBattleContext<'a> {
    battle_catalog: Option<&'a BattleCatalog>,
    fashion_catalog: Option<&'a FashionList>,
    drop_multiplier: f64,
    ship_stat_multiplier: f64,
    effects: &'a mut ResponseEffects,
}

impl<'a> TypedBattleContext<'a> {
    pub(crate) fn new(
        battle_catalog: Option<&'a BattleCatalog>,
        fashion_catalog: Option<&'a FashionList>,
        drop_multiplier: f64,
        ship_stat_multiplier: f64,
        effects: &'a mut ResponseEffects,
    ) -> Self {
        Self {
            battle_catalog,
            fashion_catalog,
            drop_multiplier,
            ship_stat_multiplier,
            effects,
        }
    }
}

pub(crate) fn handle_typed_with_catalog(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    context: TypedBattleContext<'_>,
) -> HandlerResult {
    let TypedBattleContext {
        battle_catalog,
        fashion_catalog,
        drop_multiplier,
        ship_stat_multiplier,
        effects,
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
                if !catalog.copies.contains_key(&request.copy_id)
                    || !consume_battle_supply_typed(
                        account,
                        Some(catalog),
                        request.copy_id,
                        &hero_ids.iter().map(|id| id.get()).collect::<Vec<_>>(),
                        1,
                    )
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
                    SHIP_STAT_CATALOG.get(),
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
        "copy.PassBase" => {
            let Some(active) = account.battle.active.as_ref() else {
                return HandlerResult::Error(GameError::InvalidState(
                    "battle session is not active",
                ));
            };
            let copy_id = active.copy_id;
            let hero_ids = active.hero_ids.clone();
            let started_at = active.started_at;
            let Ok(request) = CopyPassRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "battle pass request is invalid",
                ));
            };
            let result = battle_pass_result_from_request(&request);
            save_typed_battle_hero_hp(account, &result.heroes, &hero_ids);
            let grade = if result.grade > 0 { result.grade } else { 3 };
            let first_pass_expected = !account.battle.passed_copies.contains(&copy_id);
            let mut rewards = Vec::new();
            if grade < 9 && first_pass_expected {
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
            if grade < 9 {
                if let Some(catalog) = battle_catalog {
                    rewards.extend(draw_typed_battle_drop_rewards(
                        catalog,
                        copy_id.get() as i32,
                        drop_multiplier,
                        grade,
                    ));
                }
            }
            if !can_grant_typed_task_rewards(account, &rewards) {
                return HandlerResult::Error(GameError::InvalidState(
                    "battle reward cannot be granted",
                ));
            }
            let remaining_fleet_ids = account
                .battle
                .active
                .as_ref()
                .map(|session| session.remaining_fleet_ids.clone())
                .unwrap_or_default();
            let passed_fleet_ids = if result.passed_fleet_ids.is_empty() {
                remaining_fleet_ids
                    .first()
                    .map(|fleet_id| vec![u64::from(*fleet_id)])
                    .unwrap_or_default()
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
            if !passed_fleet_ids.is_empty() && !remaining_fleet_ids.is_empty() {
                if passed_fleet_ids.iter().any(|fleet_id| {
                    !remaining_fleet_ids.contains(&u32::try_from(*fleet_id).unwrap_or(0))
                }) {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "battle pass fleet is not active",
                    ));
                }
                let remaining_fleet_ids = remaining_fleet_ids
                    .into_iter()
                    .filter(|fleet_id| !passed_fleet_ids.contains(&u64::from(*fleet_id)))
                    .collect::<Vec<_>>();
                if !remaining_fleet_ids.is_empty() {
                    if let Some(active) = account.battle.active.as_mut() {
                        active.remaining_fleet_ids = remaining_fleet_ids.clone();
                        active.current_fleet = remaining_fleet_ids[0];
                    }
                    return HandlerResult::Reply(Response::raw(
                        method,
                        battle_pass_payload_with_rewards(
                            i32::try_from(copy_id.get()).unwrap_or_default(),
                            false,
                            grade,
                            result.battle_time,
                            &[],
                        ),
                    ));
                }
            }
            let first_pass = BattleService::settle_at(
                account,
                copy_id,
                grade < 9,
                u64::from(current_unix_seconds()),
            )
            .map_err(|_| GameError::InvalidState("battle settlement is invalid"));
            match first_pass {
                Ok(first_pass) => {
                    for reward in &mut rewards {
                        let _ =
                            grant_typed_task_reward_with_fashion(account, reward, fashion_catalog);
                    }
                    if !rewards.is_empty() {
                        effects.push_post(Response::raw(
                            "bag.UpdateBagData",
                            BagInfoCodec::encode(&bag_info_from_typed_account(account)),
                        ));
                        effects.push_post(Response::raw(
                            "hero.UpdateHeroBagData",
                            HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
                        ));
                        effects.push_post(Response::raw(
                            "equip.UpdateEquipBagData",
                            EquipListCodec::encode(&equip_list_from_typed_account(account)),
                        ));
                        effects.push_post(Response::raw(
                            "fashion.updateData",
                            FashionListCodec::encode(&fashion_list_from_typed_account(
                                account,
                                fashion_catalog,
                            )),
                        ));
                    }
                    if grade < 9 {
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
                        battle_pass_payload_with_rewards(
                            i32::try_from(copy_id.get()).unwrap_or_default(),
                            first_pass,
                            grade,
                            result.battle_time,
                            &rewards,
                        ),
                    ))
                }
                Err(error) => HandlerResult::Error(error),
            }
        }
        "copy.PassMiniGame" => {
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
                return HandlerResult::Error(GameError::InvalidRequest(
                    "mini-game copy id is invalid",
                ));
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
            if first_pass
                && (!rewards.is_empty() && !can_grant_typed_task_rewards(account, &rewards))
            {
                account.battle.passed_copies.remove(&copy_id_typed);
                return HandlerResult::Error(GameError::InvalidState(
                    "mini-game reward is unsupported",
                ));
            }
            if first_pass {
                let mut rewards = rewards;
                for reward in &mut rewards {
                    let _ = grant_typed_task_reward_with_fashion(account, reward, fashion_catalog);
                }
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
                    battle_pass_payload_with_rewards(
                        copy_id,
                        first_pass,
                        3,
                        request.battle_time,
                        &rewards,
                    ),
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
            if hero_ids.is_empty()
                || (battle_catalog.is_some()
                    && !consume_battle_supply_typed(
                        account,
                        battle_catalog,
                        request.copy_id,
                        &hero_ids.iter().map(|id| id.get()).collect::<Vec<_>>(),
                        1,
                    ))
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
                    SHIP_STAT_CATALOG.get(),
                    ship_stat_multiplier,
                    BattleStartOptions::default(),
                )),
            ))
        }
        "copy.QuitBase" => {
            account.battle.active = None;
            HandlerResult::Reply(Response::raw(method, request_args.to_vec()))
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
        _ => {
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
    }
}

pub(crate) fn handle_typed_mop_up(
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
            if hero_ids.is_empty()
                || !consume_battle_supply_typed(
                    account,
                    Some(catalog),
                    copy_id as i32,
                    &hero_ids,
                    sweep_count as i32,
                )
            {
                return HandlerResult::Error(GameError::InvalidState(
                    "insufficient supply or missing supply configuration",
                ));
            }
            let rewards = typed_sweep_rewards(account, catalog, copy_id as i32, sweep_count);
            if !rewards.is_empty() && !can_grant_typed_task_rewards(account, &rewards) {
                return HandlerResult::Error(GameError::InvalidState(
                    "sweep reward is unsupported",
                ));
            }
            for reward in &rewards {
                let _ = grant_typed_task_reward(account, reward);
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
) -> Vec<ShopReward> {
    let mut rewards = Vec::new();
    if !account
        .battle
        .passed_copies
        .iter()
        .any(|id| id.get() == u64::try_from(copy_id).unwrap_or_default())
    {
        if let Some(first) = catalog.copy_first_rewards.get(&copy_id) {
            rewards.extend(first.iter().map(|(goods_type, item_id, num)| ShopReward {
                goods_type: *goods_type,
                item_id: *item_id,
                num: *num,
                instance_id: 0,
            }));
        }
    }
    let drop_ids = catalog
        .copy_drop_ids
        .get(&copy_id)
        .cloned()
        .unwrap_or_default();
    for draw in 0..sweep_count {
        for drop_id in &drop_ids {
            if let Some(reward) = draw_copy_drop_with_seed(
                catalog,
                *drop_id,
                0,
                next_battle_drop_seed().wrapping_add(draw),
            ) {
                rewards.push(reward);
            }
        }
    }
    if let Some((count, guaranteed)) = catalog.copy_must_drop_rewards.get(&copy_id) {
        for _ in 0..*count {
            rewards.extend(
                guaranteed
                    .iter()
                    .map(|(goods_type, item_id, num)| ShopReward {
                        goods_type: *goods_type,
                        item_id: *item_id,
                        num: *num,
                        instance_id: 0,
                    }),
            );
        }
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

fn save_typed_battle_hero_hp(
    account: &mut blueoath_domain::AccountState,
    heroes: &[BattleHeroResult],
    allowed_hero_ids: &[blueoath_domain::HeroId],
) {
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
        if let Some(hero) = account.dock.heroes.get_mut(&hero_id) {
            hero.hp = u64::try_from(result.hp.max(0)).unwrap_or_default();
        }
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

fn daily_copy_enter_payload(start_base_ret: &[u8]) -> Vec<u8> {
    let mut payload = Vec::new();
    append_message_field(&mut payload, 1, start_base_ret);
    payload
}

#[cfg(test)]
mod tests {
    use crate::common::response::HandlerResult;
    use blueoath_domain::{ChapterId, CopyId, FleetId, NewAccountFactory, ProfileId};

    use super::*;

    #[test]
    fn typed_attack_updates_active_battle_session() {
        let mut account = NewAccountFactory::create(ProfileId::new("battle").unwrap(), "Battle");
        let chapter_id = ChapterId::new(1).unwrap();
        let copy_id = CopyId::new(9).unwrap();
        let fleet_id = FleetId::new(1).unwrap();
        let hero_id = account.dock.heroes.keys().next().copied().unwrap();
        account
            .fleet
            .fleets
            .entry(fleet_id)
            .or_default()
            .members
            .push(hero_id);
        blueoath_game::BattleService::start(&mut account, chapter_id, copy_id, fleet_id, 10)
            .unwrap();
        account.battle.active.as_mut().unwrap().hero_ids = vec![hero_id];

        let mut request = Vec::new();
        append_varint_field(&mut request, 1, 1);
        append_varint_field(&mut request, 2, copy_id.get());
        append_varint_field(&mut request, 3, hero_id.get());
        append_varint_field(&mut request, 4, 7);

        assert!(matches!(
            handle_typed(&mut account, "copy.AttackBase", &request),
            HandlerResult::Reply(_)
        ));
        let active = account.battle.active.as_ref().unwrap();
        assert_eq!(active.attack_count, 1);
        assert_eq!(active.revision, 1);
    }

    #[test]
    fn typed_start_and_pass_complete_battle_lifecycle() {
        let mut account =
            NewAccountFactory::create(ProfileId::new("battle-flow").unwrap(), "Battle");
        let fleet_id = FleetId::new(1).unwrap();
        let hero_id = account.dock.heroes.keys().next().copied().unwrap();
        account
            .fleet
            .fleets
            .entry(fleet_id)
            .or_default()
            .members
            .push(hero_id);

        let mut start = Vec::new();
        append_varint_field(&mut start, 2, 9);
        assert!(matches!(
            handle_typed(&mut account, "copy.StartBase", &start),
            HandlerResult::Reply(_)
        ));
        assert!(account.battle.active.is_some());

        assert!(matches!(
            handle_typed(&mut account, "copy.PassBase", &[]),
            HandlerResult::Reply(_)
        ));
        assert!(account.battle.active.is_none());
        assert!(account
            .battle
            .passed_copies
            .contains(&CopyId::new(9).unwrap()));
        assert_eq!(account.battle.records.len(), 1);
    }

    #[test]
    fn typed_mini_game_pass_updates_battle_state() {
        let mut account = NewAccountFactory::create(ProfileId::new("mini-game").unwrap(), "Battle");
        let hero_id = account.dock.heroes.keys().next().copied().unwrap();
        account
            .fleet
            .fleets
            .entry(FleetId::new(1).unwrap())
            .or_default()
            .members
            .push(hero_id);
        let mut request = Vec::new();
        append_varint_field(&mut request, 1, 9);
        append_varint_field(&mut request, 12, 15);
        append_varint_field(&mut request, 19, 1);

        assert!(matches!(
            handle_typed(&mut account, "copy.PassMiniGame", &request),
            HandlerResult::Reply(_)
        ));
        assert!(account
            .battle
            .passed_copies
            .contains(&CopyId::new(9).unwrap()));
        assert_eq!(account.battle.records.len(), 1);
    }

    #[test]
    fn typed_copy_star_reward_is_idempotent_and_persistent_in_state() {
        let mut account =
            NewAccountFactory::create(ProfileId::new("star-reward").unwrap(), "Battle");
        account.battle.passed_copies.insert(CopyId::new(9).unwrap());
        let mut chapter_catalog = ChapterCatalog::default();
        chapter_catalog.star_rewards_by_chapter.insert(
            1,
            ChapterStarRewards {
                level_ids: vec![9],
                star_conditions: vec![7],
                reward_ids: vec![9001],
            },
        );
        let mut task_catalog = TaskCatalog::default();
        task_catalog.rewards_by_id.insert(9001, vec![(1, 5000, 2)]);
        let mut request = Vec::new();
        append_varint_field(&mut request, 1, 1);
        append_varint_field(&mut request, 2, 1);
        let mut effects = ResponseEffects::default();

        assert!(matches!(
            handle_typed_copy_star_reward(
                &mut account,
                "copy.StarReward",
                &request,
                Some(&chapter_catalog),
                Some(&task_catalog),
                &mut effects,
            ),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account
                .inventory
                .items
                .get(&blueoath_domain::TemplateId::new(5000).unwrap()),
            Some(&2)
        );
        assert!(matches!(
            handle_typed_copy_star_reward(
                &mut account,
                "copy.FetchRewardBox",
                &request,
                Some(&chapter_catalog),
                Some(&task_catalog),
                &mut effects,
            ),
            HandlerResult::Error(GameError::InvalidState(_))
        ));
    }

    #[test]
    fn typed_copy_records_project_delete_and_apply_to_fleet() {
        let mut account =
            NewAccountFactory::create(ProfileId::new("copy-record").unwrap(), "Battle");
        let hero_id = account.dock.heroes.keys().next().copied().unwrap();
        account
            .battle
            .records
            .push(blueoath_domain::CopyRecordState {
                copy_id: CopyId::new(9).unwrap(),
                hero_ids: vec![hero_id],
                pass_time: 12,
                secret_id: 2,
                strategy_id: 3,
                power: 99,
                record_time: 100,
                ex_buffs: vec![7],
            });

        let mut request = Vec::new();
        append_varint_field(&mut request, 1, 9);
        append_varint_field(&mut request, 2, 0);
        assert!(matches!(
            handle_typed(&mut account, "copy.GetRecord", &request),
            HandlerResult::Reply(_)
        ));
        assert!(matches!(
            handle_typed(&mut account, "copy.TacticOn", &request),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account.fleet.fleets.values().next().unwrap().members,
            vec![hero_id]
        );
        assert!(matches!(
            handle_typed(&mut account, "copy.DeleteRecord", &request),
            HandlerResult::Reply(_)
        ));
        assert!(account.battle.records.is_empty());
    }

    #[test]
    fn typed_daily_copy_enter_starts_battle_and_updates_daily_state() {
        let mut account =
            NewAccountFactory::create(ProfileId::new("daily-enter").unwrap(), "Battle");
        let hero_id = account.dock.heroes.keys().next().copied().unwrap();
        account.fleet.fleets.insert(
            FleetId::new(1).unwrap(),
            blueoath_domain::FleetRecord {
                formation_id: 2,
                tactic_id: 3,
                members: vec![hero_id],
            },
        );
        let mut request = Vec::new();
        append_varint_field(&mut request, 1, 1);
        append_varint_field(&mut request, 2, 1);
        append_varint_field(&mut request, 3, 1);

        assert!(matches!(
            handle_typed(&mut account, "dailycopy.CopyEnter", &request),
            HandlerResult::Reply(_)
        ));
        assert!(account.battle.active.is_some());
        assert_eq!(
            account
                .daily_copy
                .challenge_times
                .get(&ChapterId::new(1).unwrap()),
            Some(&1)
        );
    }

    #[test]
    fn typed_quit_clears_active_battle_session() {
        let mut account =
            NewAccountFactory::create(ProfileId::new("battle-quit").unwrap(), "Battle");
        account.battle.active = Some(blueoath_domain::BattleSession {
            chapter_id: ChapterId::new(1).unwrap(),
            copy_id: CopyId::new(9).unwrap(),
            current_fleet: 1,
            started_at: 1,
            expires_at: 2,
            revision: 0,
            remaining_fleet_ids: Vec::new(),
            hero_ids: Vec::new(),
            attack_count: 0,
        });
        assert!(matches!(
            handle_typed(&mut account, "copy.QuitBase", &[]),
            HandlerResult::Reply(_)
        ));
        assert!(account.battle.active.is_none());
    }

    #[test]
    fn typed_mop_up_get_returns_domain_queue() {
        let mut account = NewAccountFactory::create(ProfileId::new("mop-up").unwrap(), "Battle");
        account
            .sweep
            .entries
            .push(blueoath_domain::SweepEntryState {
                fleet_id: 1,
                copy_id: 9,
                start_time: 10,
                end_time: 20,
                sweep_counts: 2,
                chapter_id: 0,
            });
        let mut effects = ResponseEffects::default();
        let result = handle_typed_mop_up(
            &ServerState::new("mop-up", "Battle", "test"),
            &mut account,
            "mopUp.GetMopUpData",
            &[],
            None,
            &mut effects,
        );
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(account.sweep.entries[0].copy_id, 9);
    }
}
