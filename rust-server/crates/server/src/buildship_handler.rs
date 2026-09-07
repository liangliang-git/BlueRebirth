use serde_json::{json, Value};

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;
use blueoath_domain::{
    AccountState, CurrencyKind, EquipId, EquipmentState, HeroId, HeroState, TemplateId,
};

pub(super) fn handle_typed(
    account: &mut AccountState,
    method: &str,
    request_args: &[u8],
    now: u32,
    task_catalog: Option<&TaskCatalog>,
    pre_pushes: &mut Vec<Vec<u8>>,
) -> HandlerResult {
    let catalog = BUILD_SHIP_CATALOG.get_or_init(BuildShipCatalog::default);
    match method {
        "buildship.BuildShipInfo" => HandlerResult::Reply(Response::raw(
            method,
            buildship_info_payload_from_typed(account, now),
        )),
        "buildship.BuildShip" => {
            let pool_id = decode_varint_field(request_args, 1);
            let pulls = decode_varint_field(request_args, 2).clamp(1, 10);
            if !typed_build_pool_known(catalog, pool_id) || !build_drop_exists(catalog, pool_id) {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "build pool is unavailable",
                ));
            }
            let snapshot = account.clone();
            if account.dock.heroes.len().saturating_add(pulls as usize) > 200
                || !consume_typed_build_cost(account, catalog, pool_id, pulls)
            {
                return HandlerResult::Error(GameError::InvalidState(
                    "hero dock or build resources are insufficient",
                ));
            }
            let mut rewards = Vec::new();
            for _ in 0..pulls {
                let Some((goods_type, item_id, num)) = draw_build_ship_reward(pool_id) else {
                    *account = snapshot;
                    return HandlerResult::Error(GameError::InvalidState(
                        "build pool has no drop entries",
                    ));
                };
                let Some(reward) = grant_typed_build_reward(
                    account,
                    catalog,
                    ShopReward {
                        goods_type,
                        item_id,
                        num: num.max(1),
                        instance_id: 0,
                    },
                    now,
                ) else {
                    *account = snapshot;
                    return HandlerResult::Error(GameError::InvalidState(
                        "build reward is unsupported",
                    ));
                };
                rewards.push(reward);
            }
            if pulls == 10
                && catalog
                    .extract_type_by_pool
                    .get(&pool_id)
                    .is_some_and(|kind| *kind == 2 || *kind == 4)
                && !rewards.iter().any(|reward| {
                    reward.goods_type == 3
                        && catalog
                            .ship_quality
                            .get(&reward.item_id)
                            .copied()
                            .unwrap_or_default()
                            >= 3
                })
            {
                if let Some(index) = rewards.iter().rposition(|reward| reward.goods_type == 3) {
                    let root_drop = catalog
                        .extract_to_drop
                        .get(&pool_id)
                        .copied()
                        .unwrap_or_default();
                    let roll = mix_build_draw_roll(
                        u64::from(current_unix_millis())
                            ^ BUILD_DRAW_SEQUENCE.fetch_add(1, Ordering::Relaxed),
                    );
                    if let Some((goods_type, item_id, num)) =
                        draw_sr_build_reward_with_roll(catalog, root_drop, roll)
                    {
                        remove_typed_hero(account, rewards[index].instance_id);
                        let Some(replacement) = grant_typed_build_reward(
                            account,
                            catalog,
                            ShopReward {
                                goods_type,
                                item_id,
                                num: num.max(1),
                                instance_id: 0,
                            },
                            now,
                        ) else {
                            *account = snapshot;
                            return HandlerResult::Error(GameError::InvalidState(
                                "build guarantee reward is unsupported",
                            ));
                        };
                        rewards[index] = replacement;
                    }
                }
            }
            let old = account
                .build_ship
                .draw_counts
                .get(&(pool_id as u64))
                .copied()
                .unwrap_or_default();
            let Some(next) = old.checked_add(pulls as u32) else {
                *account = snapshot;
                return HandlerResult::Error(GameError::InvalidState("build draw count overflow"));
            };
            account.build_ship.draw_counts.insert(pool_id as u64, next);
            advance_typed_task_event(account, task_catalog, 2720, 1);
            push_typed_build_updates(account, now, task_catalog, pre_pushes);
            HandlerResult::Reply(Response::raw(method, encode_buildship_ret(&rewards)))
        }
        "buildship.BuildShipBox" | "buildship.BuildShipReward" => {
            let pool_id = decode_varint_field(request_args, 1);
            let milestone = decode_varint_field(request_args, 2);
            if pool_id <= 0 || milestone <= 0 {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "build reward milestone is invalid",
                ));
            }
            let pool_key = pool_id as u64;
            let milestone_key = milestone as u32;
            let draw_count = account
                .build_ship
                .draw_counts
                .get(&pool_key)
                .copied()
                .unwrap_or_default();
            let claims = if method == "buildship.BuildShipBox" {
                &account.build_ship.used_box_info
            } else {
                &account.build_ship.used_reward_info
            };
            if draw_count < milestone_key
                || claims
                    .get(&pool_key)
                    .is_some_and(|items| items.contains(&milestone_key))
            {
                return HandlerResult::Error(GameError::InvalidState(
                    "build reward is unavailable",
                ));
            }
            let reward_tuple = if method == "buildship.BuildShipBox" {
                catalog
                    .box_drop_by_pool_count
                    .get(&(pool_id, milestone))
                    .copied()
                    .and_then(|drop_id| {
                        let roll = mix_build_draw_roll(
                            u64::from(current_unix_millis())
                                ^ BUILD_DRAW_SEQUENCE.fetch_add(1, Ordering::Relaxed),
                        );
                        draw_build_drop_reward_with_roll(catalog, drop_id, roll)
                    })
            } else {
                catalog
                    .reward_by_pool_count
                    .get(&(pool_id, milestone))
                    .copied()
            };
            let Some((goods_type, item_id, num)) = reward_tuple else {
                return HandlerResult::Error(GameError::InvalidState(
                    "build reward is not configured",
                ));
            };
            let snapshot = account.clone();
            let Some(reward) = grant_typed_build_reward(
                account,
                catalog,
                ShopReward {
                    goods_type,
                    item_id,
                    num: num.max(1),
                    instance_id: 0,
                },
                now,
            ) else {
                *account = snapshot;
                return HandlerResult::Error(GameError::InvalidState(
                    "build reward is unsupported",
                ));
            };
            let claims = if method == "buildship.BuildShipBox" {
                &mut account.build_ship.used_box_info
            } else {
                &mut account.build_ship.used_reward_info
            };
            claims.entry(pool_key).or_default().insert(milestone_key);
            push_typed_build_updates(account, now, task_catalog, pre_pushes);
            HandlerResult::Reply(Response::raw(method, encode_buildship_ret(&[reward])))
        }
        _ => HandlerResult::Empty,
    }
}

fn typed_build_pool_known(catalog: &BuildShipCatalog, pool_id: i32) -> bool {
    const ENABLED_POOLS: &[i32] = &[106, 109, 124, 150, 151, 152, 154];
    if catalog.extract_to_drop.is_empty() {
        ENABLED_POOLS.contains(&pool_id)
    } else {
        catalog.extract_to_drop.contains_key(&pool_id)
    }
}

fn typed_currency(item_id: i32) -> Option<CurrencyKind> {
    Some(match item_id {
        1 => CurrencyKind::Gold,
        2 => CurrencyKind::Diamond,
        5 => CurrencyKind::Supply,
        30 => CurrencyKind::PvePoint,
        _ => return None,
    })
}

fn consume_typed_build_cost(
    account: &mut AccountState,
    catalog: &BuildShipCatalog,
    pool_id: i32,
    pulls: i32,
) -> bool {
    let base = catalog
        .expend_by_pool
        .get(&pool_id)
        .cloned()
        .unwrap_or_default();
    let configured = if pulls == 10 {
        catalog
            .ten_expend_by_pool
            .get(&pool_id)
            .cloned()
            .filter(|items| !items.is_empty())
            .unwrap_or_else(|| {
                base.iter()
                    .map(|(kind, id, amount)| (*kind, *id, amount.saturating_mul(pulls)))
                    .collect()
            })
    } else {
        base.iter()
            .map(|(kind, id, amount)| (*kind, *id, amount.saturating_mul(pulls)))
            .collect()
    };
    let mut totals = std::collections::BTreeMap::<(i32, i32), u64>::new();
    for (kind, item_id, amount) in configured {
        if kind <= 0 || item_id <= 0 || amount <= 0 {
            return false;
        }
        let entry = totals.entry((kind, item_id)).or_default();
        *entry = entry.saturating_add(amount as u64);
    }
    for ((kind, item_id), amount) in &totals {
        let available = if *kind == 5 {
            typed_currency(*item_id).map(|currency| account.resources.amount(currency).get())
        } else if *kind == 1 || *kind == 6 {
            TemplateId::new(*item_id as u64).ok().map(|id| {
                account
                    .inventory
                    .items
                    .get(&id)
                    .copied()
                    .unwrap_or_default()
            })
        } else {
            None
        };
        if available.is_none_or(|available| available < *amount) {
            return false;
        }
    }
    for ((kind, item_id), amount) in totals {
        if kind == 5 {
            let Some(currency) = typed_currency(item_id) else {
                return false;
            };
            if account.resources.debit(currency, amount).is_err() {
                return false;
            }
        } else {
            let Ok(template_id) = TemplateId::new(item_id as u64) else {
                return false;
            };
            let Some(current) = account.inventory.items.get_mut(&template_id) else {
                return false;
            };
            *current -= amount;
            if *current == 0 {
                account.inventory.items.remove(&template_id);
            }
        }
    }
    true
}

fn grant_typed_build_reward(
    account: &mut AccountState,
    catalog: &BuildShipCatalog,
    reward: ShopReward,
    now: u32,
) -> Option<ShopReward> {
    let amount = u64::try_from(reward.num.max(1)).ok()?;
    match reward.goods_type {
        5 => {
            let currency = typed_currency(reward.item_id)?;
            account.resources.credit(currency, amount).ok()?;
            Some(reward)
        }
        1 | 6 => {
            let template_id = TemplateId::new(reward.item_id as u64).ok()?;
            let current = account
                .inventory
                .items
                .get(&template_id)
                .copied()
                .unwrap_or_default();
            account
                .inventory
                .items
                .insert(template_id, current.checked_add(amount)?);
            Some(reward)
        }
        2 => {
            let template_id = TemplateId::new(reward.item_id as u64).ok()?;
            let mut last_id = 0;
            for _ in 0..amount {
                let id = next_typed_equip_id(account)?;
                account.dock.equipments.insert(
                    id,
                    EquipmentState {
                        id,
                        template_id,
                        enhance_level: 0,
                        star: 0,
                        enhance_exp: 0,
                        hero_id: None,
                    },
                );
                last_id = id.get();
            }
            Some(ShopReward {
                instance_id: i32::try_from(last_id).ok()?,
                ..reward
            })
        }
        3 => {
            let mut last_id = 0;
            for _ in 0..amount {
                let id = next_typed_hero_id(account)?;
                let template_id = TemplateId::new(reward.item_id as u64).ok()?;
                let mut hero = HeroState {
                    id,
                    template_id,
                    name: String::new(),
                    change_name_time: 0,
                    level: 1,
                    exp: 0,
                    mood: MOOD_INITIAL as u32,
                    affection: 500_000,
                    hp: 10_000_000_000,
                    locked: false,
                    equip_slots: vec![None; 6],
                    pskills: std::collections::BTreeMap::new(),
                };
                if let Some(defaults) = catalog.ship_defaults.get(&reward.item_id) {
                    for (slot, equip_template) in defaults.iter().take(6).enumerate() {
                        let equip_id = next_typed_equip_id(account)?;
                        let equip_template = TemplateId::new(*equip_template as u64).ok()?;
                        account.dock.equipments.insert(
                            equip_id,
                            EquipmentState {
                                id: equip_id,
                                template_id: equip_template,
                                enhance_level: 0,
                                star: 0,
                                enhance_exp: 0,
                                hero_id: Some(id),
                            },
                        );
                        hero.equip_slots[slot] = Some(equip_id);
                    }
                }
                account.dock.heroes.insert(id, hero);
                last_id = id.get();
            }
            let _ = now;
            Some(ShopReward {
                instance_id: i32::try_from(last_id).ok()?,
                ..reward
            })
        }
        _ => None,
    }
}

fn next_typed_hero_id(account: &AccountState) -> Option<HeroId> {
    let next = account
        .dock
        .heroes
        .keys()
        .map(|id| id.get())
        .max()
        .unwrap_or_default()
        .checked_add(1)?;
    HeroId::new(next).ok()
}

fn next_typed_equip_id(account: &AccountState) -> Option<EquipId> {
    let next = account
        .dock
        .equipments
        .keys()
        .map(|id| id.get())
        .max()
        .unwrap_or_default()
        .checked_add(1)?;
    EquipId::new(next).ok()
}

fn remove_typed_hero(account: &mut AccountState, hero_id: i32) {
    let Ok(hero_id) = HeroId::new(u64::try_from(hero_id).unwrap_or_default()) else {
        return;
    };
    if let Some(hero) = account.dock.heroes.remove(&hero_id) {
        for equip_id in hero.equip_slots.into_iter().flatten() {
            account.dock.equipments.remove(&equip_id);
        }
    }
}

fn push_typed_build_updates(
    account: &AccountState,
    now: u32,
    task_catalog: Option<&TaskCatalog>,
    pre_pushes: &mut Vec<Vec<u8>>,
) {
    pre_pushes.push(HeroBagCodec::encode(&hero_bag_from_typed_account(account)));
    append_method_push(
        pre_pushes,
        "equip.UpdateEquipBagData",
        EquipListCodec::encode(&equip_list_from_typed_account(account)),
    );
    append_method_push(
        pre_pushes,
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_typed_account(account)),
    );
    append_method_push(
        pre_pushes,
        "buildship.BuildShipInfo",
        buildship_info_payload_from_typed(account, now),
    );
    append_method_push(
        pre_pushes,
        "task.TaskInfo",
        task_info_payload_from_typed_account(account, task_catalog),
    );
}

fn ensure_build_state(account: &mut Value) -> Option<&mut serde_json::Map<String, Value>> {
    let root = account.as_object_mut()?;
    let state = root
        .entry("buildState".to_owned())
        .or_insert_with(|| json!({"drawCount": {}, "usedBoxInfo": {}, "usedRewardInfo": {}}));
    state.as_object_mut()
}

fn record_build_claim(account: &mut Value, used_key: &str, pool_id: i32, milestone: i32) -> bool {
    let Some(state) = ensure_build_state(account) else {
        return false;
    };
    let used = state
        .entry(used_key.to_owned())
        .or_insert_with(|| json!({}));
    let Some(used) = used.as_object_mut() else {
        return false;
    };
    let list = used.entry(pool_id.to_string()).or_insert_with(|| json!([]));
    let Some(list) = list.as_array_mut() else {
        return false;
    };
    list.push(json!(milestone));
    true
}

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let payload = handle_legacy(context, method, request_args);
    if let Some(error) = context.handler_error.clone() {
        HandlerResult::Error(error)
    } else {
        match payload {
            Some(payload) => HandlerResult::Reply(Response::raw(method, payload)),
            None => HandlerResult::Empty,
        }
    }
}

fn handle_legacy<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let account = &mut *context.account;
    let catalogs = context.catalogs;
    let GameLoginCatalogs {
        fashion: fashion_catalog,
        equip: equip_catalog,
        handbook_behaviours,
        tasks: task_catalog,
        ..
    } = catalogs;
    let account_view = account.as_deref();
    let pre_pushes = &mut *context.pre_pushes;
    let handler_error = &mut *context.handler_error;

    match method {
        "buildship.BuildShipInfo" => {
            Some(buildship_info_payload(account_view, current_unix_seconds()))
        }
        "buildship.BuildShip" => {
            let pool_id = decode_varint_field(request_args, 1);
            let pulls = decode_varint_field(request_args, 2).clamp(1, 10);
            let build_catalog = BUILD_SHIP_CATALOG.get_or_init(BuildShipCatalog::default);
            const ENABLED_POOLS: &[i32] = &[106, 109, 124, 150, 151, 152, 154];
            let pool_known = {
                if build_catalog.extract_to_drop.is_empty() {
                    ENABLED_POOLS.contains(&pool_id)
                } else {
                    build_catalog.extract_to_drop.contains_key(&pool_id)
                }
            };
            if !pool_known {
                *handler_error = Some(GameError::Internal("build pool is unavailable".to_owned()));
                Some(Vec::new())
            } else if let Some(account) = account.as_deref_mut() {
                let capacity = account
                    .get("dock")
                    .and_then(|d| d.get("bagSize"))
                    .and_then(Value::as_i64)
                    .unwrap_or(200);
                let current = account
                    .get("dock")
                    .and_then(|d| d.get("heroes"))
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len) as i64;
                if current + i64::from(pulls) > capacity {
                    *handler_error = Some(GameError::Internal("hero dock is full".to_owned()));
                    Some(Vec::new())
                } else if !build_drop_exists(build_catalog, pool_id) {
                    *handler_error = Some(GameError::Internal(
                        "build pool has no drop entries".to_owned(),
                    ));
                    Some(Vec::new())
                } else if !consume_buildship_cost(account, build_catalog, pool_id, pulls) {
                    *handler_error =
                        Some(GameError::Internal("not enough build resources".to_owned()));
                    Some(Vec::new())
                } else {
                    // Draw and grant as one transaction. If a malformed nested pool fails
                    // halfway through, restore costs and any partial ship/equipment grants.
                    let account_before_draw = account.clone();
                    let now = current_unix_seconds();
                    let mut rewards = Vec::new();
                    for _ in 0..pulls {
                        let Some((goods_type, item_id, num)) = draw_build_ship_reward(pool_id)
                        else {
                            *handler_error = Some(GameError::Internal(
                                "build pool has no drop entries".to_owned(),
                            ));
                            break;
                        };
                        let reward = ShopReward {
                            goods_type,
                            item_id,
                            num: num.max(1),
                            instance_id: 0,
                        };
                        let reward = grant_reward(account, reward, now, fashion_catalog);
                        if reward.goods_type == 3 {
                            apply_ship_defaults(
                                account,
                                build_catalog,
                                reward.item_id,
                                reward.instance_id,
                            );
                        }
                        rewards.push(reward);
                    }
                    if handler_error.is_none()
                        && pulls == 10
                        && build_catalog
                            .extract_type_by_pool
                            .get(&pool_id)
                            .is_some_and(|kind| *kind == 2 || *kind == 4)
                        && !rewards.iter().any(|reward| {
                            reward.goods_type == 3
                                && build_catalog
                                    .ship_quality
                                    .get(&reward.item_id)
                                    .copied()
                                    .unwrap_or_default()
                                    >= 3
                        })
                    {
                        if let Some(index) =
                            rewards.iter().rposition(|reward| reward.goods_type == 3)
                        {
                            let root_drop = build_catalog
                                .extract_to_drop
                                .get(&pool_id)
                                .copied()
                                .unwrap_or_default();
                            let guarantee_roll = mix_build_draw_roll(
                                u64::from(current_unix_millis())
                                    ^ BUILD_DRAW_SEQUENCE.fetch_add(1, Ordering::Relaxed),
                            );
                            if let Some((goods_type, item_id, num)) = draw_sr_build_reward_with_roll(
                                build_catalog,
                                root_drop,
                                guarantee_roll,
                            ) {
                                remove_ship_instance(account, rewards[index].instance_id);
                                let replacement = grant_reward(
                                    account,
                                    ShopReward {
                                        goods_type,
                                        item_id,
                                        num,
                                        instance_id: 0,
                                    },
                                    now,
                                    fashion_catalog,
                                );
                                if replacement.goods_type == 3 {
                                    apply_ship_defaults(
                                        account,
                                        build_catalog,
                                        replacement.item_id,
                                        replacement.instance_id,
                                    );
                                }
                                rewards[index] = replacement;
                            }
                        }
                    }
                    if handler_error.is_some() {
                        *account = account_before_draw;
                        Some(Vec::new())
                    } else {
                        let Some(state) = ensure_build_state(account) else {
                            *account = account_before_draw;
                            *handler_error = Some(GameError::Internal(
                                "account snapshot is invalid".to_owned(),
                            ));
                            return Some(Vec::new());
                        };
                        let counts = state
                            .entry("drawCount".to_owned())
                            .or_insert_with(|| json!({}));
                        let Some(counts) = counts.as_object_mut() else {
                            *account = account_before_draw;
                            *handler_error =
                                Some(GameError::Internal("build state is invalid".to_owned()));
                            return Some(Vec::new());
                        };
                        let old = counts
                            .get(&pool_id.to_string())
                            .and_then(Value::as_i64)
                            .unwrap_or_default();
                        counts.insert(pool_id.to_string(), json!(old + i64::from(pulls)));
                        // Exploration is a successful build request, not a client-reported
                        // reward. Count one trusted action after all pulls commit.
                        advance_task_event(account, task_catalog, 2720, 1, now);
                        pre_pushes.push(encode_hero_bag_push(account));
                        if let Some(payload) =
                            illustrate_info_payload_for_rewards(&rewards, handbook_behaviours)
                        {
                            append_method_push(pre_pushes, "illustrate.IllustrateInfo", payload);
                        }
                        append_method_push(
                            pre_pushes,
                            "equip.UpdateEquipBagData",
                            EquipListCodec::encode(&equip_list_from_account(
                                account,
                                equip_catalog,
                            )),
                        );
                        append_method_push(
                            pre_pushes,
                            "bag.UpdateBagData",
                            BagInfoCodec::encode(&bag_info_from_account(account)),
                        );
                        append_method_push(
                            pre_pushes,
                            "buildship.BuildShipInfo",
                            buildship_info_payload(Some(account), now),
                        );
                        append_method_push(
                            pre_pushes,
                            "task.TaskInfo",
                            task_info_payload(account, task_catalog),
                        );
                        Some(encode_buildship_ret(&rewards))
                    }
                }
            } else {
                Some(Vec::new())
            }
        }
        "buildship.BuildShipBox" | "buildship.BuildShipReward" => {
            let pool_id = decode_varint_field(request_args, 1);
            let milestone = decode_varint_field(request_args, 2);
            if account.is_none() {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            } else {
                let Some(account) = account.as_deref_mut() else {
                    *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                    return Some(Vec::new());
                };
                let catalog = BUILD_SHIP_CATALOG.get_or_init(BuildShipCatalog::default);
                let Some(state) = ensure_build_state(account) else {
                    *handler_error = Some(GameError::Internal(
                        "account snapshot is invalid".to_owned(),
                    ));
                    return Some(Vec::new());
                };
                // Migrate legacy snapshots that stored UsedRewardInfo as a bare array.
                // Keep claimed milestones only when they are represented by the current map shape.
                for key in ["usedBoxInfo", "usedRewardInfo"] {
                    if !state.get(key).is_some_and(Value::is_object) {
                        state[key] = json!({});
                    }
                }
                let draw_count = state
                    .get("drawCount")
                    .and_then(|v| v.get(pool_id.to_string()))
                    .and_then(Value::as_i64)
                    .unwrap_or_default();
                let used_key = if method == "buildship.BuildShipBox" {
                    "usedBoxInfo"
                } else {
                    "usedRewardInfo"
                };
                let already = state
                    .get(used_key)
                    .and_then(|v| v.get(pool_id.to_string()))
                    .and_then(Value::as_array)
                    .is_some_and(|xs| xs.iter().any(|x| json_i64_any(x) == i64::from(milestone)));
                if milestone <= 0 || draw_count < i64::from(milestone) || already {
                    *handler_error = Some(GameError::Internal(
                        "build reward is unavailable".to_owned(),
                    ));
                    Some(Vec::new())
                } else {
                    let mut rewards = Vec::new();
                    if method == "buildship.BuildShipBox" {
                        if let Some(drop_id) =
                            catalog.box_drop_by_pool_count.get(&(pool_id, milestone))
                        {
                            let roll = mix_build_draw_roll(
                                u64::from(current_unix_millis())
                                    ^ BUILD_DRAW_SEQUENCE.fetch_add(1, Ordering::Relaxed),
                            );
                            if let Some((goods_type, item_id, num)) =
                                draw_build_drop_reward_with_roll(catalog, *drop_id, roll)
                            {
                                let reward = grant_reward(
                                    account,
                                    ShopReward {
                                        goods_type,
                                        item_id,
                                        num: num.max(1),
                                        instance_id: 0,
                                    },
                                    current_unix_seconds(),
                                    fashion_catalog,
                                );
                                if reward.goods_type == 3 {
                                    apply_ship_defaults(
                                        account,
                                        catalog,
                                        reward.item_id,
                                        reward.instance_id,
                                    );
                                }
                                rewards.push(reward);
                            }
                        }
                    } else if let Some(&(goods_type, item_id, num)) =
                        catalog.reward_by_pool_count.get(&(pool_id, milestone))
                    {
                        let reward = grant_reward(
                            account,
                            ShopReward {
                                goods_type,
                                item_id,
                                num: num.max(1),
                                instance_id: 0,
                            },
                            current_unix_seconds(),
                            fashion_catalog,
                        );
                        if reward.goods_type == 3 {
                            apply_ship_defaults(
                                account,
                                catalog,
                                reward.item_id,
                                reward.instance_id,
                            );
                        }
                        rewards.push(reward);
                    }
                    if rewards.is_empty() {
                        *handler_error = Some(GameError::Internal(
                            "build reward is not configured".to_owned(),
                        ));
                        Some(Vec::new())
                    } else {
                        if !record_build_claim(account, used_key, pool_id, milestone) {
                            *handler_error =
                                Some(GameError::Internal("build state is invalid".to_owned()));
                            return Some(Vec::new());
                        }
                        let now = current_unix_seconds();
                        pre_pushes.push(encode_hero_bag_push(account));
                        if let Some(payload) =
                            illustrate_info_payload_for_rewards(&rewards, handbook_behaviours)
                        {
                            append_method_push(pre_pushes, "illustrate.IllustrateInfo", payload);
                        }
                        append_method_push(
                            pre_pushes,
                            "equip.UpdateEquipBagData",
                            EquipListCodec::encode(&equip_list_from_account(
                                account,
                                equip_catalog,
                            )),
                        );
                        append_method_push(
                            pre_pushes,
                            "bag.UpdateBagData",
                            BagInfoCodec::encode(&bag_info_from_account(account)),
                        );
                        append_method_push(
                            pre_pushes,
                            "buildship.BuildShipInfo",
                            buildship_info_payload(Some(account), now),
                        );
                        Some(encode_buildship_ret(&rewards))
                    }
                }
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod typed_tests {
    use super::*;

    #[test]
    fn typed_build_reward_creates_ship_and_default_equipment() {
        let mut catalog = BuildShipCatalog::default();
        catalog.ship_defaults.insert(100, vec![200, 201]);
        let mut account = AccountState::default();
        let reward = grant_typed_build_reward(
            &mut account,
            &catalog,
            ShopReward {
                goods_type: 3,
                item_id: 100,
                num: 1,
                instance_id: 0,
            },
            100,
        )
        .unwrap();
        assert_eq!(reward.instance_id, 1);
        assert_eq!(account.dock.heroes.len(), 1);
        assert_eq!(account.dock.equipments.len(), 2);
        let hero = account.dock.heroes.values().next().unwrap();
        assert_eq!(hero.equip_slots.iter().flatten().count(), 2);
        assert!(hero
            .equip_slots
            .iter()
            .flatten()
            .all(|id| account.dock.equipments.contains_key(id)));
    }

    #[test]
    fn typed_build_cost_consumes_currency_and_items_atomically() {
        let mut catalog = BuildShipCatalog::default();
        catalog
            .expend_by_pool
            .insert(1, vec![(5, 1, 10), (1, 900, 2)]);
        let mut account = AccountState::default();
        account.resources.credit(CurrencyKind::Gold, 10).unwrap();
        account
            .inventory
            .items
            .insert(TemplateId::new(900).unwrap(), 2);
        assert!(consume_typed_build_cost(&mut account, &catalog, 1, 1));
        assert_eq!(account.resources.amount(CurrencyKind::Gold).get(), 0);
        assert!(account.inventory.items.is_empty());
        assert!(!consume_typed_build_cost(&mut account, &catalog, 1, 1));
    }
}
