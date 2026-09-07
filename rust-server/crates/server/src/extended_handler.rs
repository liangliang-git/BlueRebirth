use serde_json::{json, Value};

use super::*;

pub(super) fn handles(method: &str) -> bool {
    [
        "battlepass.",
        "activitybattlepass.",
        "exchange.",
        "foodCompose.",
        "worldevent.",
        "worldeventrank.",
    ]
    .iter()
    .any(|prefix| method.starts_with(prefix))
}

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    match method {
        "battlepass.GetReward" | "battlepass.GetAllReward" => {
            handle_battlepass_reward(context, method, request_args, false)
        }
        "activitybattlepass.GetReward" | "activitybattlepass.GetAllReward" => {
            handle_battlepass_reward(context, method, request_args, true)
        }
        "battlepass.RefreshRandomTask" | "activitybattlepass.RefreshRandomTask" => {
            handle_battlepass_action(context, method, request_args, false)
        }
        "battlepass.RecieveTaskReward" | "activitybattlepass.RecieveTaskReward" => {
            handle_battlepass_task_reward(context, request_args, method.starts_with("activity"))
        }
        "battlepass.BuyPassType"
        | "activitybattlepass.BuyPassType"
        | "battlepass.BuyPassLevel"
        | "activitybattlepass.BuyPassLevel" => handle_battlepass_action(
            context,
            method,
            request_args,
            method.starts_with("activity"),
        ),
        "battlepass.UpdateBattlePassInfo" | "activitybattlepass.UpdateBattlePassInfo" => {
            let activity = method.starts_with("activity");
            Some(battlepass_info_payload(
                context.account.as_deref()?,
                gameplay_catalog(),
                activity,
            ))
        }
        "exchange.GetExchangeInfo" | "exchange.GetExchange" => {
            Some(exchange_info_payload(context.account.as_deref()?))
        }
        "exchange.Exchange" => handle_exchange(context, request_args),
        "foodCompose.GetFoodComposeData" | "foodCompose.GetFoodCompose" => Some(food_info_payload(
            context.account.as_deref()?,
            gameplay_catalog(),
        )),
        "foodCompose.FoodCompose" => handle_food_compose(context, request_args),
        "worldevent.Progress" => Some(world_event_server_progress_payload(
            context.account.as_deref()?,
        )),
        "worldevent.UserStage" => Some(world_event_user_stage_payload(context.account.as_deref()?)),
        "worldevent.StageReward" => handle_world_event_reward(context, request_args),
        "worldeventrank.Rank" => Some(world_event_rank_payload(context.account.as_deref()?)),
        "worldevent.UserProgress" => Some(world_event_user_progress_payload(
            context.account.as_deref()?,
        )),
        _ => None,
    }
}

pub(super) fn gameplay_catalog() -> &'static GameplayCatalog {
    GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default)
}

fn handle_battlepass_reward<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
    activity: bool,
) -> Option<Vec<u8>> {
    let catalog = gameplay_catalog();
    let levels = if activity {
        &catalog.battlepass_activity_levels
    } else {
        &catalog.battlepass_levels
    };
    let state_key = if activity {
        "activityBattlePass"
    } else {
        "battlePass"
    };
    let pass_level = context
        .account
        .as_deref()
        .and_then(|account| account.get(state_key))
        .and_then(|state| state.get("passLevel"))
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .clamp(1, i64::from(i32::MAX)) as i32;
    let requested_level = decode_varint_field(request_args, 1);
    let targets = if method.ends_with("GetAllReward") {
        levels
            .keys()
            .copied()
            .filter(|level| *level > 0 && *level <= pass_level)
            .collect::<Vec<_>>()
    } else {
        vec![requested_level]
    };
    let mut claimed = Vec::new();
    let mut rewards = Vec::new();
    let (response, refreshes) = {
        let Some(account) = context.account.as_deref_mut() else {
            *context.response_err = 1;
            *context.response_err_msg = "account is unavailable".to_owned();
            return Some(Vec::new());
        };
        let pass_type = state_i32(account, state_key, "passType").clamp(1, 2);
        for level in targets {
            let Some(config) = levels.get(&level) else {
                continue;
            };
            if level > pass_level {
                continue;
            }
            let reward_id = if pass_type >= 2 {
                json_i32(config, "pay_level_reward").unwrap_or_default()
            } else {
                json_i32(config, "free_level_reward").unwrap_or_default()
            };
            let key = format!("{pass_type}:{level}");
            let was_claimed = account
                .get(state_key)
                .and_then(|state| state.get("claimedRewards"))
                .and_then(Value::as_array)
                .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(&key)));
            if was_claimed {
                continue;
            }
            if reward_id > 0 {
                rewards.extend(grant_rewards_by_id(
                    account,
                    catalog,
                    reward_id,
                    context.catalogs.fashion,
                ));
            }
            state_array_mut(account, state_key, "claimedRewards").push(json!(key));
            claimed.push((level, pass_type));
        }
        let refreshes =
            (!claimed.is_empty()).then(|| account_refresh_payloads(context.state, account));
        (encode_battlepass_reward_response(&claimed), refreshes)
    };
    if let Some((user_payload, bag_payload)) = refreshes {
        append_method_push(context.pre_pushes, "user.UpdateUserInfo", user_payload);
        append_method_push(context.pre_pushes, "bag.UpdateBagData", bag_payload);
    }
    Some(response)
}

fn handle_battlepass_action<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
    activity: bool,
) -> Option<Vec<u8>> {
    let buy_level_price = if activity {
        gameplay_catalog()
            .battlepass_activity_param
            .as_ref()
            .and_then(|value| value.get("buy_level_price"))
            .and_then(Value::as_array)
    } else {
        gameplay_catalog()
            .battlepass_param
            .as_ref()
            .and_then(|value| value.get("buy_level_price"))
            .and_then(Value::as_array)
    }
    .and_then(|values| {
        Some((
            i32::try_from(values.first()?.as_i64()?).ok()?,
            i32::try_from(values.get(1)?.as_i64()?).ok()?,
        ))
    });
    let state_key = if activity {
        "activityBattlePass"
    } else {
        "battlePass"
    };
    let payload = {
        let Some(account) = context.account.as_deref_mut() else {
            *context.response_err = 1;
            *context.response_err_msg = "account is unavailable".to_owned();
            return Some(Vec::new());
        };
        if method.ends_with("BuyPassLevel") {
            let levels = decode_varint_field(request_args, 1).max(1);
            if let Some((currency_id, price_per_level)) = buy_level_price {
                let cost = price_per_level.saturating_mul(levels);
                if !can_consume(account, 5, currency_id, cost) {
                    *context.response_err = 1;
                    *context.response_err_msg = "battle pass level cost is insufficient".to_owned();
                    return Some(Vec::new());
                }
                consume_reward(account, 5, currency_id, cost);
            }
        }
        let state = battlepass_state_mut(account, state_key);
        if method.ends_with("RefreshRandomTask") {
            state["lastRefreshTaskId"] = json!(decode_varint_field(request_args, 1));
            state["refreshCount"] = json!(state_i64(state, "refreshCount").saturating_add(1));
        } else if method.ends_with("BuyPassType") {
            state["passType"] = json!(decode_varint_field(request_args, 1).clamp(1, 2));
        } else if method.ends_with("BuyPassLevel") {
            let levels = decode_varint_field(request_args, 1).max(1);
            state["passLevel"] = json!(state_i64(state, "passLevel")
                .saturating_add(i64::from(levels))
                .clamp(1, i64::from(i32::MAX)));
        }
        battlepass_info_payload(account, gameplay_catalog(), activity)
    };
    append_method_push(
        context.pre_pushes,
        if activity {
            "activitybattlepass.UpdateBattlePassInfo"
        } else {
            "battlepass.UpdateBattlePassInfo"
        },
        payload,
    );
    Some(Vec::new())
}

fn handle_battlepass_task_reward<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    request_args: &[u8],
    activity: bool,
) -> Option<Vec<u8>> {
    let catalog = gameplay_catalog();
    let tasks = if activity {
        &catalog.battlepass_activity_tasks
    } else {
        &catalog.battlepass_tasks
    };
    let task_id = decode_varint_field(request_args, 1);
    let state_key = if activity {
        "activityBattlePass"
    } else {
        "battlePass"
    };
    let Some(account) = context.account.as_deref_mut() else {
        *context.response_err = 1;
        *context.response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    let state = battlepass_state_mut(account, state_key);
    let already_claimed = state
        .get("claimedTasks")
        .and_then(Value::as_array)
        .is_some_and(|values| {
            values
                .iter()
                .any(|value| value.as_i64() == Some(i64::from(task_id)))
        });
    if !already_claimed {
        if let Some(config) = tasks.get(&task_id) {
            let exp = json_i32(config, "battlepass_exp")
                .unwrap_or_default()
                .max(0);
            state["passExp"] = json!(state_i64(state, "passExp").saturating_add(i64::from(exp)));
        }
        state["claimedTasks"]
            .as_array_mut()
            .expect("claimed tasks must be an array")
            .push(json!(task_id));
    }
    state["lastTaskId"] = json!(task_id);
    let (payload, user_payload, bag_payload) = (
        battlepass_info_payload(account, catalog, activity),
        UserInfoCodec::encode(&user_info_from_account(context.state, Some(account))),
        BagInfoCodec::encode(&bag_info_from_account(account)),
    );
    append_method_push(
        context.pre_pushes,
        if activity {
            "activitybattlepass.UpdateBattlePassInfo"
        } else {
            "battlepass.UpdateBattlePassInfo"
        },
        payload,
    );
    append_method_push(context.pre_pushes, "user.UpdateUserInfo", user_payload);
    append_method_push(context.pre_pushes, "bag.UpdateBagData", bag_payload);
    Some(Vec::new())
}

fn battlepass_state_mut<'a>(account: &'a mut Value, key: &str) -> &'a mut Value {
    account
        .as_object_mut()
        .expect("account must be an object")
        .entry(key.to_owned())
        .or_insert_with(|| {
            json!({
                "passType": 1,
                "passLevel": 1,
                "passExp": 0,
                "curWeekIndex": 1,
                "claimedRewards": [],
                "claimedTasks": [],
                "tasks": [],
                "refreshCount": 0,
            })
        })
}

fn state_i64(state: &Value, key: &str) -> i64 {
    state.get(key).and_then(Value::as_i64).unwrap_or_default()
}

fn state_i32(account: &Value, state_key: &str, key: &str) -> i32 {
    account
        .get(state_key)
        .and_then(|state| state.get(key))
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or_default()
}

fn state_array_mut<'a>(account: &'a mut Value, state_key: &str, key: &str) -> &'a mut Vec<Value> {
    battlepass_state_mut(account, state_key)[key]
        .as_array_mut()
        .expect("state array must be an array")
}

fn battlepass_info_payload(account: &Value, catalog: &GameplayCatalog, activity: bool) -> Vec<u8> {
    let state_key = if activity {
        "activityBattlePass"
    } else {
        "battlePass"
    };
    let state = account.get(state_key).unwrap_or(&Value::Null);
    let levels = if activity {
        &catalog.battlepass_activity_levels
    } else {
        &catalog.battlepass_levels
    };
    let pass_type = state_i64(state, "passType").clamp(1, 2) as u64;
    let pass_level = state_i64(state, "passLevel").max(1) as u64;
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, pass_type);
    append_varint_field(&mut output, 2, pass_level);
    append_varint_field(&mut output, 3, state_i64(state, "passExp").max(0) as u64);
    let claimed = |level: i32, kind: i64| {
        let key = format!("{kind}:{level}");
        state
            .get("claimedRewards")
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(&key)))
    };
    for level in levels.keys().copied().filter(|level| *level > 0) {
        let mut reward = Vec::new();
        append_varint_field(&mut reward, 1, level as u64);
        append_varint_field(&mut reward, 2, u64::from(claimed(level, 1)));
        append_message_field(&mut output, 4, &reward);
        let mut advanced = Vec::new();
        append_varint_field(&mut advanced, 1, level as u64);
        append_varint_field(&mut advanced, 2, u64::from(claimed(level, 2)));
        append_message_field(&mut output, 5, &advanced);
    }
    append_varint_field(
        &mut output,
        6,
        state_i64(state, "curWeekIndex").max(1) as u64,
    );
    output
}

fn encode_battlepass_reward_response(claimed: &[(i32, i32)]) -> Vec<u8> {
    let mut output = Vec::new();
    for (level, pass_type) in claimed {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, (*level).max(0) as u64);
        append_varint_field(&mut item, 2, (*pass_type).max(0) as u64);
        append_message_field(&mut output, 1, &item);
    }
    output
}

fn grant_rewards_by_id(
    account: &mut Value,
    catalog: &GameplayCatalog,
    reward_id: i32,
    fashion_catalog: Option<&blueoath_protocol::FashionList>,
) -> Vec<ShopReward> {
    catalog
        .rewards_by_id
        .get(&reward_id)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|reward| grant_reward(account, reward, current_unix_seconds(), fashion_catalog))
        .collect()
}

fn account_refresh_payloads(state: &ServerState, account: &Value) -> (Vec<u8>, Vec<u8>) {
    (
        UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
        BagInfoCodec::encode(&bag_info_from_account(account)),
    )
}

fn exchange_info_payload(account: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    let times = account.get("exchangeTimes");
    for id in gameplay_catalog().exchanges.keys().copied() {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, id.max(0) as u64);
        append_varint_field(
            &mut item,
            2,
            times
                .and_then(|value| value.get(id.to_string()))
                .and_then(Value::as_i64)
                .unwrap_or_default()
                .max(0) as u64,
        );
        append_message_field(&mut output, 1, &item);
    }
    output
}

fn handle_exchange<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let id = decode_varint_field(request_args, 1);
    let Some(config) = gameplay_catalog().exchanges.get(&id) else {
        *context.response_err = 1;
        *context.response_err_msg = "exchange item was not found".to_owned();
        return Some(Vec::new());
    };
    let max_count = json_i32(config, "change_count").unwrap_or_default();
    let consume = reward_triplets(config, "item_consume");
    let rewards = reward_triplets(config, "item_reward");
    let Some(account) = context.account.as_deref_mut() else {
        *context.response_err = 1;
        *context.response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    let current_count = account
        .get("exchangeTimes")
        .and_then(|value| value.get(id.to_string()))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    if max_count > 0 && current_count >= i64::from(max_count) {
        *context.response_err = 1;
        *context.response_err_msg = "exchange limit reached".to_owned();
        return Some(Vec::new());
    }
    if consume.is_empty() || rewards.is_empty() {
        *context.response_err = 1;
        *context.response_err_msg = "exchange reward is not configured".to_owned();
        return Some(Vec::new());
    }
    if consume
        .iter()
        .any(|(kind, item, amount)| !can_consume(account, *kind, *item, *amount))
    {
        *context.response_err = 1;
        *context.response_err_msg = "exchange cost is insufficient".to_owned();
        return Some(Vec::new());
    }
    for (kind, item, amount) in &consume {
        consume_reward(account, *kind, *item, *amount);
    }
    let granted = rewards
        .into_iter()
        .flat_map(|(kind, item, amount)| {
            let reward = ShopReward {
                goods_type: kind,
                item_id: item,
                num: amount,
                instance_id: 0,
            };
            Some(grant_reward(
                account,
                reward,
                current_unix_seconds(),
                context.catalogs.fashion,
            ))
        })
        .collect::<Vec<_>>();
    let entries = account
        .as_object_mut()
        .expect("account must be an object")
        .entry("exchangeTimes".to_owned())
        .or_insert_with(|| json!({}));
    let key = id.to_string();
    entries[&key] = json!(entries
        .get(&key)
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .saturating_add(1));
    let refreshes = account_refresh_payloads(context.state, account);
    append_method_push(context.pre_pushes, "user.UpdateUserInfo", refreshes.0);
    append_method_push(context.pre_pushes, "bag.UpdateBagData", refreshes.1);
    Some(encode_rewards_list(&granted))
}

fn reward_triplets(value: &Value, key: &str) -> Vec<(i32, i32, i32)> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            Some((
                i32::try_from(row.first()?.as_i64()?).ok()?,
                i32::try_from(row.get(1)?.as_i64()?).ok()?,
                i32::try_from(row.get(2)?.as_i64()?).ok()?,
            ))
        })
        .filter(|(kind, item, amount)| *kind > 0 && *item > 0 && *amount > 0)
        .collect()
}

fn can_consume(account: &Value, kind: i32, item: i32, amount: i32) -> bool {
    if kind == 5 {
        currency_character_key(item)
            .is_some_and(|key| character_i64(account, key) >= i64::from(amount))
    } else {
        bag_item_count(account, item) >= i64::from(amount)
    }
}

fn consume_reward(account: &mut Value, kind: i32, item: i32, amount: i32) {
    if kind == 5 {
        if let Some(key) = currency_character_key(item) {
            add_character_i64(account, key, amount.saturating_neg());
        }
    } else {
        consume_bag_item(account, item, amount);
    }
}

fn handle_food_compose<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let material_ids = decode_repeated_varint_field(request_args, 1);
    let catalog = gameplay_catalog();
    let recipe_id = catalog
        .food_recipes
        .iter()
        .find(|(_, recipe)| {
            let mut configured = recipe_material_ids(recipe);
            if configured.len() != material_ids.len() {
                return false;
            }
            configured.sort_unstable();
            let mut requested = material_ids.clone();
            requested.sort_unstable();
            !requested.is_empty() && configured == requested
        })
        .map(|(id, _)| *id)
        .unwrap_or_else(|| decode_varint_field(request_args, 2));
    let Some(recipe) = catalog.food_recipes.get(&recipe_id) else {
        *context.response_err = 1;
        *context.response_err_msg = "food recipe was not found".to_owned();
        return Some(Vec::new());
    };
    let materials = recipe
        .get("material")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            Some((
                i32::try_from(row.first()?.as_i64()?).ok()?,
                i32::try_from(row.get(1)?.as_i64()?).ok()?,
                i32::try_from(row.get(2)?.as_i64()?).ok()?,
            ))
        })
        .filter(|(_, item, amount)| *item > 0 && *amount > 0)
        .collect::<Vec<_>>();
    let Some(account) = context.account.as_deref_mut() else {
        *context.response_err = 1;
        *context.response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    if materials
        .iter()
        .any(|(kind, item, amount)| !can_consume(account, *kind, *item, *amount))
    {
        *context.response_err = 1;
        *context.response_err_msg = "food materials are insufficient".to_owned();
        return Some(Vec::new());
    }
    for (kind, item, amount) in &materials {
        consume_reward(account, *kind, *item, *amount);
    }
    let reward_id = recipe
        .get("reward")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or_default();
    let configured_rewards = catalog
        .rewards_by_id
        .get(&reward_id)
        .cloned()
        .unwrap_or_default();
    if configured_rewards.is_empty() {
        *context.response_err = 1;
        *context.response_err_msg = "food reward was not configured".to_owned();
        return Some(Vec::new());
    }
    let granted = configured_rewards
        .into_iter()
        .map(|reward| {
            grant_reward(
                account,
                reward,
                current_unix_seconds(),
                context.catalogs.fashion,
            )
        })
        .collect::<Vec<_>>();
    let state = account
        .as_object_mut()
        .expect("account must be an object")
        .entry("foodCompose".to_owned())
        .or_insert_with(|| json!({"recipes": {}, "lastRecipeId": 0}));
    state["lastRecipeId"] = json!(recipe_id);
    let recipes = state
        .get_mut("recipes")
        .and_then(Value::as_object_mut)
        .expect("food recipes must be an object");
    let key = recipe_id.to_string();
    recipes[&key] = json!(recipes
        .get(&key)
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .saturating_add(1));
    let refreshes = account_refresh_payloads(context.state, account);
    append_method_push(context.pre_pushes, "user.UpdateUserInfo", refreshes.0);
    append_method_push(context.pre_pushes, "bag.UpdateBagData", refreshes.1);
    Some(food_reward_payload(recipe_id, &granted))
}

fn recipe_material_ids(recipe: &Value) -> Vec<i32> {
    recipe
        .get("material")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            let item_id = i32::try_from(row.get(1)?.as_i64()?).ok()?;
            let amount = i32::try_from(row.get(2)?.as_i64()?).ok()?.max(1);
            Some(std::iter::repeat_n(item_id, usize::try_from(amount).ok()?))
        })
        .flatten()
        .collect()
}

fn food_info_payload(account: &Value, catalog: &GameplayCatalog) -> Vec<u8> {
    let state = account.get("foodCompose").unwrap_or(&Value::Null);
    let recipes = state.get("recipes");
    let mut output = Vec::new();
    for id in catalog.food_recipes.keys().copied() {
        let mut row = Vec::new();
        append_varint_field(&mut row, 1, id.max(0) as u64);
        append_varint_field(
            &mut row,
            2,
            recipes
                .and_then(|value| value.get(id.to_string()))
                .and_then(Value::as_i64)
                .unwrap_or_default()
                .max(0) as u64,
        );
        append_varint_field(&mut row, 3, 0);
        append_message_field(&mut output, 1, &row);
    }
    append_varint_field(
        &mut output,
        3,
        state
            .get("lastRecipeId")
            .and_then(Value::as_i64)
            .unwrap_or_default()
            .max(0) as u64,
    );
    output
}

fn food_reward_payload(recipe_id: i32, rewards: &[ShopReward]) -> Vec<u8> {
    let mut output = Vec::new();
    for reward in rewards {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, reward.goods_type.max(0) as u64);
        append_varint_field(&mut item, 2, reward.item_id.max(0) as u64);
        append_varint_field(&mut item, 3, reward.num.max(0) as u64);
        append_message_field(&mut output, 1, &item);
    }
    append_varint_field(&mut output, 2, recipe_id.max(0) as u64);
    output
}

fn handle_world_event_reward<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let stage_id = decode_varint_field(request_args, 1);
    let catalog = gameplay_catalog();
    let Some((event_id, event)) = active_world_event(catalog) else {
        return Some(encode_rewards_list(&[]));
    };
    let reward_id = event
        .get("server_stage_rewards")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            let stage = row.first()?.as_i64()?;
            let reward = row.get(1)?.as_i64()?;
            (stage == i64::from(stage_id))
                .then(|| i32::try_from(reward).ok())
                .flatten()
        })
        .collect::<Vec<_>>()
        .into_iter()
        .next()
        .unwrap_or_default();
    let Some(account) = context.account.as_deref_mut() else {
        *context.response_err = 1;
        *context.response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    let progress = account
        .get("worldEventUserProgress")
        .or_else(|| account.get("worldEventProgress"))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    if event_id <= 0 || stage_id <= 0 || progress < i64::from(stage_id) || reward_id <= 0 {
        return Some(encode_rewards_list(&[]));
    }
    let already_claimed = account
        .get("worldEventClaimedStagesByEvent")
        .and_then(Value::as_object)
        .and_then(|events| events.get(&event_id.to_string()))
        .and_then(Value::as_array)
        .is_some_and(|values| {
            values
                .iter()
                .any(|value| value.as_i64() == Some(i64::from(stage_id)))
        });
    if already_claimed {
        return Some(encode_rewards_list(&[]));
    }
    let rewards = if reward_id > 0 {
        grant_rewards_by_id(account, catalog, reward_id, context.catalogs.fashion)
    } else {
        Vec::new()
    };
    let claims = account
        .as_object_mut()
        .expect("account must be an object")
        .entry("worldEventClaimedStagesByEvent".to_owned())
        .or_insert_with(|| json!({}));
    claims
        .as_object_mut()
        .expect("world event claims must be an object")
        .entry(event_id.to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("world event stage claims must be an array")
        .push(json!(stage_id));
    let refreshes = account_refresh_payloads(context.state, account);
    append_method_push(context.pre_pushes, "user.UpdateUserInfo", refreshes.0);
    append_method_push(context.pre_pushes, "bag.UpdateBagData", refreshes.1);
    Some(encode_rewards_list(&rewards))
}

fn active_world_event(catalog: &GameplayCatalog) -> Option<(i32, &Value)> {
    let activity = catalog.activity.get(&5001).or_else(|| {
        catalog
            .activity
            .values()
            .find(|value| json_i32(value, "type") == Some(5001))
    })?;
    let event_id = activity
        .get("p1")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())?;
    catalog
        .world_events
        .get(&event_id)
        .map(|event| (event_id, event))
}

fn world_event_rank_payload(account: &Value) -> Vec<u8> {
    let progress = account
        .get("worldEventProgress")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0) as u64;
    let mut node = Vec::new();
    append_varint_field(&mut node, 1, 1);
    append_varint_field(&mut node, 2, progress);
    append_varint_field(&mut node, 3, 1);
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &node);
    output
}

fn world_event_server_progress_payload(account: &Value) -> Vec<u8> {
    let progress = account
        .get("worldEventProgress")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0) as u64;
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, progress);
    append_varint_field(&mut output, 2, 1);
    output
}

fn world_event_user_progress_payload(account: &Value) -> Vec<u8> {
    let progress = account
        .get("worldEventUserProgress")
        .or_else(|| account.get("worldEventProgress"))
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0) as u64;
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, progress);
    output
}

fn world_event_user_stage_payload(account: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    if let Some(stages) = account.get("worldEventStages").and_then(Value::as_array) {
        for stage in stages.iter().filter_map(Value::as_i64) {
            append_varint_field(&mut output, 1, stage.max(0) as u64);
        }
    }
    output
}
