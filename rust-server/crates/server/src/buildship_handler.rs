use serde_json::{json, Value};

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

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
