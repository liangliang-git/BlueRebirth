use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;
use crate::features::copy::mopup_state::{
    draw_copy_category_rewards_with_seed_multiplier, draw_copy_drop_rewards_with_seed_multiplier,
    next_battle_drop_seed,
};

// 处理星级奖励
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
        .filter_map(|copy_id| {
            let copy_id = u64::try_from(*copy_id)
                .ok()
                .and_then(|id| blueoath_domain::CopyId::new(id).ok())?;
            if !account.battle.passed_copies.contains(&copy_id) {
                return None;
            }
            Some(
                account
                    .battle
                    .copy_stars
                    .get(&copy_id)
                    .copied()
                    .unwrap_or(7),
            )
        })
        .map(|stars| i32::try_from(stars.min(7).count_ones()).unwrap_or(i32::MAX))
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
    let mut pending = pending;
    if !grant_typed_task_rewards_with_fashion(account, &mut pending, None) {
        return HandlerResult::Error(GameError::InvalidState(
            "copy star reward could not be granted",
        ));
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

// 抽取掉落奖励
fn draw_typed_battle_drop_rewards(
    catalog: &BattleCatalog,
    copy_id: i32,
    multiplier: f64,
    grade: i32,
    include_first_clear: bool,
) -> Vec<ShopReward> {
    let fleet_ids = battle_session_fleet_ids(copy_id, Some(catalog));
    draw_typed_battle_drop_rewards_for_fleets(
        catalog,
        copy_id,
        &fleet_ids,
        multiplier,
        grade,
        true,
        include_first_clear,
    )
}

fn draw_typed_battle_wave_drop_rewards(
    catalog: &BattleCatalog,
    copy_id: i32,
    fleet_ids: &[i32],
    multiplier: f64,
    grade: i32,
    include_stage_rewards: bool,
) -> Vec<ShopReward> {
    draw_typed_battle_drop_rewards_for_fleets(
        catalog,
        copy_id,
        fleet_ids,
        multiplier,
        grade,
        include_stage_rewards,
        false,
    )
}

fn draw_typed_battle_drop_rewards_for_fleets(
    catalog: &BattleCatalog,
    copy_id: i32,
    fleet_ids: &[i32],
    multiplier: f64,
    grade: i32,
    include_stage_rewards: bool,
    include_first_clear: bool,
) -> Vec<ShopReward> {
    let mut rewards = Vec::new();
    let mut draw_index = 0u64;
    let seed = next_battle_drop_seed();
    let settle_multiplier = multiplier * battle_evaluation_multipliers(Some(catalog), grade).1;
    let other_multiplier = multiplier * battle_other_drop_multiplier(Some(catalog), grade);
    let is_daily_copy = catalog.daily_group_by_copy.contains_key(&copy_id);
    if include_stage_rewards {
        let basic_drop_ids = if is_daily_copy {
            catalog.daily_basic_drop_ids_by_copy.get(&copy_id)
        } else {
            catalog.copy_drop_ids.get(&copy_id)
        };
        for drop_id in basic_drop_ids.into_iter().flatten() {
            let category_rewards = if is_daily_copy {
                draw_copy_drop_rewards_with_seed_multiplier(
                    catalog,
                    *drop_id,
                    0,
                    seed.wrapping_add(draw_index),
                    multiplier,
                )
            } else {
                let include_display_first_clear =
                    include_first_clear && !catalog.copy_first_rewards.contains_key(&copy_id);
                draw_copy_category_rewards_with_seed_multiplier(
                    catalog,
                    *drop_id,
                    seed.wrapping_add(draw_index),
                    include_display_first_clear,
                    multiplier,
                )
            };
            draw_index = draw_index.wrapping_add(1);
            for mut reward in category_rewards {
                catalog.drop_quantities.apply(
                    copy_id,
                    &mut reward,
                    seed.wrapping_add(draw_index)
                        .wrapping_add(0xD1B5_4A32_D192_ED03),
                );
                rewards.push(reward);
            }
        }

        // config_daily_group.extra_drop is the actual daily extra-reward pool.
        // Extra attempts are intentionally unlimited up to the wire-visible
        // 99,999 sentinel, so every successful daily run can draw this pool.
        if is_daily_copy {
            for drop_id in catalog
                .daily_extra_drop_ids_by_copy
                .get(&copy_id)
                .into_iter()
                .flatten()
            {
                let category_rewards = draw_copy_drop_rewards_with_seed_multiplier(
                    catalog,
                    *drop_id,
                    0,
                    seed.wrapping_add(draw_index),
                    multiplier,
                );
                draw_index = draw_index.wrapping_add(1);
                for mut reward in category_rewards {
                    catalog.drop_quantities.apply(
                        copy_id,
                        &mut reward,
                        seed.wrapping_add(draw_index)
                            .wrapping_add(0xD1B5_4A32_D192_ED03),
                    );
                    rewards.push(reward);
                }
            }
        }
    }

    for fleet_id in fleet_ids {
        for drop_id in catalog.fleet_drop_ids.get(&fleet_id).into_iter().flatten() {
            let fleet_rewards = draw_copy_drop_rewards_with_seed_multiplier(
                catalog,
                *drop_id,
                0,
                seed.wrapping_add(draw_index),
                multiplier,
            );
            draw_index = draw_index.wrapping_add(1);
            for mut reward in fleet_rewards {
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
            let fleet_rewards = draw_copy_drop_rewards_with_seed_multiplier(
                catalog,
                *drop_id,
                0,
                seed.wrapping_add(draw_index),
                other_multiplier,
            );
            draw_index = draw_index.wrapping_add(1);
            if !fleet_rewards.is_empty() {
                for mut reward in fleet_rewards {
                    draw_index = draw_index.wrapping_add(1);
                    catalog.drop_quantities.apply(
                        copy_id,
                        &mut reward,
                        seed.wrapping_add(draw_index),
                    );
                    rewards.push(reward);
                }
            }
        }
        for drop_id in catalog
            .fleet_settle_drop_ids
            .get(&fleet_id)
            .into_iter()
            .flatten()
        {
            let fleet_rewards = draw_copy_drop_rewards_with_seed_multiplier(
                catalog,
                *drop_id,
                0,
                seed.wrapping_add(draw_index),
                settle_multiplier,
            );
            draw_index = draw_index.wrapping_add(1);
            if !fleet_rewards.is_empty() {
                for mut reward in fleet_rewards {
                    draw_index = draw_index.wrapping_add(1);
                    catalog.drop_quantities.apply(
                        copy_id,
                        &mut reward,
                        seed.wrapping_add(draw_index),
                    );
                    rewards.push(reward);
                }
            }
        }
    }

    if include_stage_rewards {
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
    }
    rewards
}

fn queue_battle_inventory_refresh(
    effects: &mut ResponseEffects,
    account: &blueoath_domain::AccountState,
    fashion_catalog: Option<&FashionList>,
) {
    // Battle rewards mutate several independent client caches. Sending only bag data
    // leaves equipment/ship/fashion drops invisible until a later full refresh.
    // Queue snapshots before the battle callback so the result screen cannot overwrite
    // them with its pre-settlement cache.
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
        FashionListCodec::encode(&fashion_list_from_typed_account(account, fashion_catalog)),
    ));
}

// 战斗上下文类型
pub(crate) struct TypedBattleContext<'a> {
    battle_catalog: Option<&'a BattleCatalog>,
    fashion_catalog: Option<&'a FashionList>,
    hero_level_catalog: Option<&'a HeroLevelCatalog>,
    drop_multiplier: f64,
    ship_stat_multiplier: f64,
    commander_exp_multiplier: f64,
    ship_exp_multiplier: f64,
    affection_multiplier: f64,
    server_state: Option<&'a ServerState>,
    effects: &'a mut ResponseEffects,
}

impl<'a> TypedBattleContext<'a> {
    pub(crate) fn new(
        battle_catalog: Option<&'a BattleCatalog>,
        fashion_catalog: Option<&'a FashionList>,
        hero_level_catalog: Option<&'a HeroLevelCatalog>,
        drop_multiplier: f64,
        ship_stat_multiplier: f64,
        commander_exp_multiplier: f64,
        ship_exp_multiplier: f64,
        effects: &'a mut ResponseEffects,
    ) -> Self {
        Self {
            battle_catalog,
            fashion_catalog,
            hero_level_catalog,
            drop_multiplier,
            ship_stat_multiplier,
            commander_exp_multiplier,
            ship_exp_multiplier,
            affection_multiplier: 1.0,
            server_state: None,
            effects,
        }
    }

    pub(crate) fn with_affection_multiplier(mut self, multiplier: f64) -> Self {
        self.affection_multiplier = multiplier;
        self
    }

    pub(crate) fn with_server_state(mut self, state: &'a ServerState) -> Self {
        self.server_state = Some(state);
        self
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
        hero_level_catalog,
        drop_multiplier,
        ship_stat_multiplier,
        commander_exp_multiplier,
        ship_exp_multiplier,
        affection_multiplier,
        server_state,
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
            if !reported_fleet_ids.is_empty()
                && !reported_fleet_ids.contains(&u64::from(current_fleet_id))
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
                            .and_then(|catalog| {
                                catalog.copy_first_rewards.get(&(copy_id.get() as i32))
                            })
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
                return HandlerResult::Error(GameError::InvalidState(
                    "battle reward cannot be granted",
                ));
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
                    if !grant_typed_task_rewards_with_fashion(
                        account,
                        &mut rewards,
                        fashion_catalog,
                    ) {
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
                        let ship_exp =
                            scale_reward(i64::from(ship_base), ship_exp_multiplier * exp_ratio)
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
                            UserInfoCodec::encode(&user_info_from_typed_account(
                                server_state,
                                account,
                            )),
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

fn save_typed_battle_hero_hp(
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
    use super::*;

    #[test]
    fn battle_inventory_refresh_covers_all_reward_caches() {
        let account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("battle-refresh").unwrap(),
            "Captain",
        );
        let mut effects = ResponseEffects::default();

        queue_battle_inventory_refresh(&mut effects, &account, None);

        let (pre, post, error) = effects.into_parts();
        assert_eq!(
            pre.iter()
                .map(|response| response.method.as_str())
                .collect::<Vec<_>>(),
            vec![
                "bag.UpdateBagData",
                "hero.UpdateHeroBagData",
                "equip.UpdateEquipBagData",
                "fashion.updateData",
            ]
        );
        assert!(post.is_empty());
        assert!(error.is_none());
    }

    #[test]
    fn daily_battle_draws_basic_and_unlimited_extra_drop_pools() {
        let mut catalog = BattleCatalog::default();
        catalog.daily_group_by_copy.insert(20_101, 2);
        catalog
            .daily_basic_drop_ids_by_copy
            .insert(20_101, vec![33_011]);
        catalog
            .daily_extra_drop_ids_by_copy
            .insert(20_101, vec![33_012]);
        catalog.drop_pools.insert(
            33_011,
            BattleDropPool {
                separate_entries: vec![(1, 13_001, 180, 180, 10_000)],
                separate_count: 1,
                ..BattleDropPool::default()
            },
        );
        catalog.drop_pools.insert(
            33_012,
            BattleDropPool {
                separate_entries: vec![(5, 13, 10, 10, 10_000)],
                separate_count: 1,
                ..BattleDropPool::default()
            },
        );

        let rewards = draw_typed_battle_drop_rewards(&catalog, 20_101, 1.0, 3, false);

        assert!(
            rewards.iter().any(|reward| reward.goods_type == 1
                && reward.item_id == 13_001
                && reward.num == 180),
            "rewards: {rewards:?}"
        );
        assert!(rewards
            .iter()
            .any(|reward| reward.goods_type == 5 && reward.item_id == 13 && reward.num == 10));
    }

    #[test]
    fn multi_wave_drop_rewards_are_scoped_to_current_fleet() {
        let mut catalog = BattleCatalog::default();
        catalog.copies.insert(
            20_101,
            BattleCopy {
                config_id: 20_101,
                copy_type: 2,
                fleet_ids: vec![11, 12],
            },
        );
        catalog.copy_drop_ids.insert(20_101, vec![20_1001]);
        catalog.copy_drop_pools.insert(
            20_1001,
            BattleCopyDropPool {
                guaranteed_entries: vec![(1, 900, 5, 1, 10_000)],
                ..BattleCopyDropPool::default()
            },
        );
        catalog.fleet_drop_ids.insert(11, vec![11_001]);
        catalog.fleet_drop_ids.insert(12, vec![12_001]);
        for (drop_id, item_id) in [(11_001, 11), (12_001, 12)] {
            catalog.drop_pools.insert(
                drop_id,
                BattleDropPool {
                    separate_entries: vec![(1, item_id, 1, 1, 10_000)],
                    separate_count: 1,
                    ..BattleDropPool::default()
                },
            );
        }

        let first_wave =
            draw_typed_battle_wave_drop_rewards(&catalog, 20_101, &[11], 1.0, 3, false);
        assert_eq!(
            first_wave
                .iter()
                .map(|reward| (reward.goods_type, reward.item_id, reward.num))
                .collect::<Vec<_>>(),
            vec![(1, 11, 1)]
        );

        let final_wave = draw_typed_battle_wave_drop_rewards(&catalog, 20_101, &[12], 1.0, 3, true);
        assert!(final_wave
            .iter()
            .any(|reward| (reward.goods_type, reward.item_id, reward.num) == (1, 12, 1)));
        assert!(final_wave
            .iter()
            .any(|reward| (reward.goods_type, reward.item_id, reward.num) == (1, 900, 5)));
        assert!(!final_wave.iter().any(|reward| reward.item_id == 11));
    }

    #[test]
    fn intermediate_battle_pass_grants_wave_reward_and_keeps_session_active() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("battle-wave-settlement").unwrap(),
            "Captain",
        );
        let hero_id = *account.dock.heroes.keys().next().expect("starter hero");
        account.battle.active = Some(blueoath_domain::BattleSession {
            chapter_id: blueoath_domain::ChapterId::new(20_101).unwrap(),
            copy_id: blueoath_domain::CopyId::new(20_101).unwrap(),
            current_fleet: 11,
            started_at: u64::from(current_unix_seconds()),
            expires_at: u64::from(current_unix_seconds()).saturating_add(1_800),
            revision: 0,
            remaining_fleet_ids: vec![11, 12],
            hero_ids: vec![hero_id],
            attack_count: 0,
        });

        let mut catalog = BattleCatalog::default();
        catalog.copies.insert(
            20_101,
            BattleCopy {
                config_id: 20_101,
                copy_type: 2,
                fleet_ids: vec![11, 12],
            },
        );
        catalog.fleet_drop_ids.insert(11, vec![11_001]);
        catalog.fleet_rewards.insert(
            11,
            BattleFleetReward {
                commander_exp: 10,
                ship_exp: 20,
            },
        );
        catalog.drop_pools.insert(
            11_001,
            BattleDropPool {
                separate_entries: vec![(1, 11, 1, 1, 10_000)],
                separate_count: 1,
                ..BattleDropPool::default()
            },
        );
        catalog.supply_cost_by_copy.insert(20_101, (100, 0));

        let mut pass = Vec::new();
        append_varint_field(&mut pass, 8, 3);
        append_varint_field(&mut pass, 12, 12);
        let mut fleet = Vec::new();
        append_varint_field(&mut fleet, 1, 11);
        append_message_field(&mut pass, 17, &fleet);
        let mut extra_reported_fleet = Vec::new();
        append_varint_field(&mut extra_reported_fleet, 1, 12);
        append_message_field(&mut pass, 20, &extra_reported_fleet);

        let mut effects = ResponseEffects::default();
        let result = handle_typed_with_catalog(
            &mut account,
            "copy.PassBase",
            &pass,
            TypedBattleContext::new(Some(&catalog), None, None, 1.0, 1.0, 1.0, 1.0, &mut effects),
        );

        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(
            account
                .inventory
                .items
                .get(&blueoath_domain::TemplateId::new(11).unwrap()),
            Some(&1)
        );
        let active = account.battle.active.as_ref().expect("next wave remains");
        assert_eq!(active.remaining_fleet_ids, vec![12]);
        assert_eq!(active.current_fleet, 12);
        assert!(account.battle.passed_copies.is_empty());
        assert_eq!(account.character.exp, 10);
        assert_eq!(
            account
                .resources
                .amount(blueoath_domain::CurrencyKind::Supply)
                .get(),
            10_000
        );

        catalog.fleet_rewards.insert(
            12,
            BattleFleetReward {
                commander_exp: 30,
                ship_exp: 40,
            },
        );
        let mut final_pass = Vec::new();
        append_varint_field(&mut final_pass, 8, 3);
        append_varint_field(&mut final_pass, 12, 12);
        let mut final_fleet = Vec::new();
        append_varint_field(&mut final_fleet, 1, 12);
        append_message_field(&mut final_pass, 17, &final_fleet);

        let mut final_effects = ResponseEffects::default();
        let final_result = handle_typed_with_catalog(
            &mut account,
            "copy.PassBase",
            &final_pass,
            TypedBattleContext::new(
                Some(&catalog),
                None,
                None,
                1.0,
                1.0,
                1.0,
                1.0,
                &mut final_effects,
            ),
        );

        assert!(matches!(final_result, HandlerResult::Reply(_)));
        assert_eq!(account.character.exp, 40);
        assert_eq!(
            account
                .resources
                .amount(blueoath_domain::CurrencyKind::Supply)
                .get(),
            9_900
        );
        assert!(account.battle.active.is_none());
    }
}
