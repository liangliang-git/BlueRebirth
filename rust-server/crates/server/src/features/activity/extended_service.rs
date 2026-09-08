use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;

pub(crate) fn handle_typed_battlepass(
    state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let catalog = gameplay_catalog();
    let activity = GameMethod::parse(method).is_family(MethodFamily::ActivityBattlePass);
    match method {
        "battlepass.UpdateBattlePassInfo" | "activitybattlepass.UpdateBattlePassInfo" => reply(
            method,
            typed_battlepass_info_payload(account, catalog, activity),
        ),
        "battlepass.GetReward"
        | "battlepass.GetAllReward"
        | "activitybattlepass.GetReward"
        | "activitybattlepass.GetAllReward" => {
            let pass = typed_battlepass_ref(account, activity);
            let pass_level = pass.pass_level.max(1) as i32;
            let pass_type = pass.pass_type.clamp(1, 2) as i32;
            let levels = if activity {
                &catalog.battlepass_activity_levels
            } else {
                &catalog.battlepass_levels
            };
            let targets = if method.ends_with("GetAllReward") {
                levels
                    .keys()
                    .copied()
                    .filter(|level| *level > 0 && *level <= pass_level)
                    .collect::<Vec<_>>()
            } else {
                let Ok(request) = BattlePassRewardRequest::decode(request_args) else {
                    return invalid("battle pass reward request is invalid");
                };
                vec![request.level]
            };
            let mut claims = Vec::<(i32, i32, Vec<ShopReward>)>::new();
            for level in targets {
                let Some(config) = levels.get(&level) else {
                    continue;
                };
                if level > pass_level
                    || typed_battlepass_ref(account, activity)
                        .claimed_rewards
                        .contains(&(pass_type as u32, level as u32))
                {
                    continue;
                }
                let reward_id = if pass_type >= 2 {
                    config.pay_level_reward
                } else {
                    config.free_level_reward
                };
                let rewards = catalog
                    .rewards_by_id
                    .get(&reward_id)
                    .cloned()
                    .unwrap_or_default();
                claims.push((level, pass_type, rewards));
            }
            let all_rewards = claims
                .iter()
                .flat_map(|(_, _, rewards)| rewards.iter().copied())
                .collect::<Vec<_>>();
            if !all_rewards.is_empty() && !can_grant_typed_task_rewards(account, &all_rewards) {
                return invalid("battle pass reward is unsupported");
            }
            for (level, pass_type, rewards) in &claims {
                for reward in rewards {
                    let _ = grant_typed_task_reward(account, reward);
                }
                typed_battlepass_mut(account, activity)
                    .claimed_rewards
                    .insert((*pass_type as u32, *level as u32));
            }
            let claimed = claims
                .iter()
                .map(|(level, pass_type, _)| (*level, *pass_type))
                .collect::<Vec<_>>();
            if !claimed.is_empty() {
                append_typed_account_refresh_pushes(state, account, effects);
            }
            reply(method, encode_battlepass_reward_response(&claimed))
        }
        "battlepass.RefreshRandomTask" | "activitybattlepass.RefreshRandomTask" => {
            let Ok(request) = BattlePassRefreshRequest::decode(request_args) else {
                return invalid("battle pass refresh request is invalid");
            };
            let pass = typed_battlepass_mut(account, activity);
            pass.last_refresh_task_id = request.task_id.max(0) as u64;
            pass.refresh_count = pass.refresh_count.saturating_add(1);
            let payload = typed_battlepass_info_payload(account, catalog, activity);
            effects.push_pre(Response::raw(
                if activity {
                    "activitybattlepass.UpdateBattlePassInfo"
                } else {
                    "battlepass.UpdateBattlePassInfo"
                },
                payload,
            ));
            HandlerResult::PushOnly
        }
        "battlepass.BuyPassType" | "activitybattlepass.BuyPassType" => {
            let Ok(request) = BattlePassTypeRequest::decode(request_args) else {
                return invalid("battle pass type request is invalid");
            };
            typed_battlepass_mut(account, activity).pass_type = request.pass_type as u32;
            append_typed_battlepass_info_push(account, catalog, activity, effects);
            HandlerResult::PushOnly
        }
        "battlepass.BuyPassLevel" | "activitybattlepass.BuyPassLevel" => {
            let Ok(request) = BattlePassLevelRequest::decode(request_args) else {
                return invalid("battle pass level request is invalid");
            };
            let levels = request.levels;
            let price = if activity {
                catalog.battlepass_activity_param.as_ref()
            } else {
                catalog.battlepass_param.as_ref()
            }
            .and_then(|config| config.buy_level_price);
            if let Some((currency_id, price_per_level)) = price {
                let cost = price_per_level.saturating_mul(levels);
                if !can_consume_typed(account, 5, currency_id, cost) {
                    return invalid("battle pass level cost is insufficient");
                }
                consume_typed(account, 5, currency_id, cost);
            }
            let pass = typed_battlepass_mut(account, activity);
            pass.pass_level = pass.pass_level.max(1).saturating_add(levels as u32).max(1);
            append_typed_battlepass_info_push(account, catalog, activity, effects);
            HandlerResult::PushOnly
        }
        "battlepass.RecieveTaskReward" | "activitybattlepass.RecieveTaskReward" => {
            let Ok(request) = BattlePassTaskRewardRequest::decode(request_args) else {
                return invalid("battle pass task request is invalid");
            };
            let task_id = request.task_id.max(0) as u64;
            let tasks = if activity {
                &catalog.battlepass_activity_tasks
            } else {
                &catalog.battlepass_tasks
            };
            let pass = typed_battlepass_mut(account, activity);
            if pass.claimed_tasks.insert(task_id) {
                if let Some(config) = tasks.get(&(task_id as i32)) {
                    pass.pass_exp = pass
                        .pass_exp
                        .saturating_add(config.experience.max(0) as u64);
                }
            }
            pass.last_task_id = task_id;
            append_typed_battlepass_info_push(account, catalog, activity, effects);
            append_typed_account_refresh_pushes(state, account, effects);
            HandlerResult::PushOnly
        }
        _ => HandlerResult::Empty,
    }
}

fn typed_battlepass_ref(
    account: &blueoath_domain::AccountState,
    activity: bool,
) -> &blueoath_domain::BattlePassState {
    if activity {
        &account.activity_battle_pass
    } else {
        &account.battle_pass
    }
}

fn typed_battlepass_mut(
    account: &mut blueoath_domain::AccountState,
    activity: bool,
) -> &mut blueoath_domain::BattlePassState {
    if activity {
        &mut account.activity_battle_pass
    } else {
        &mut account.battle_pass
    }
}

fn append_typed_battlepass_info_push(
    account: &blueoath_domain::AccountState,
    catalog: &GameplayCatalog,
    activity: bool,
    effects: &mut ResponseEffects,
) {
    effects.push_pre(Response::raw(
        if activity {
            "activitybattlepass.UpdateBattlePassInfo"
        } else {
            "battlepass.UpdateBattlePassInfo"
        },
        typed_battlepass_info_payload(account, catalog, activity),
    ));
}

fn append_typed_account_refresh_pushes(
    state: &ServerState,
    account: &blueoath_domain::AccountState,
    effects: &mut ResponseEffects,
) {
    effects.push_pre(Response::raw(
        "user.UpdateUserInfo",
        UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
    ));
    effects.push_pre(Response::raw(
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_typed_account(account)),
    ));
}

fn typed_battlepass_info_payload(
    account: &blueoath_domain::AccountState,
    catalog: &GameplayCatalog,
    activity: bool,
) -> Vec<u8> {
    let pass = typed_battlepass_ref(account, activity);
    let levels = if activity {
        &catalog.battlepass_activity_levels
    } else {
        &catalog.battlepass_levels
    };
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, pass.pass_type.clamp(1, 2) as u64);
    append_varint_field(&mut output, 2, pass.pass_level.max(1) as u64);
    append_varint_field(&mut output, 3, pass.pass_exp);
    for level in levels.keys().copied().filter(|level| *level > 0) {
        let mut reward = Vec::new();
        append_varint_field(&mut reward, 1, level as u64);
        append_varint_field(
            &mut reward,
            2,
            u64::from(pass.claimed_rewards.contains(&(1, level as u32))),
        );
        append_message_field(&mut output, 4, &reward);
        let mut advanced = Vec::new();
        append_varint_field(&mut advanced, 1, level as u64);
        append_varint_field(
            &mut advanced,
            2,
            u64::from(pass.claimed_rewards.contains(&(2, level as u32))),
        );
        append_message_field(&mut output, 5, &advanced);
    }
    append_varint_field(&mut output, 6, pass.cur_week_index.max(1) as u64);
    output
}

pub(crate) fn handle_typed_exchange(
    state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let catalog = gameplay_catalog();
    match method {
        "exchange.GetExchangeInfo" | "exchange.GetExchange" => {
            reply(method, typed_exchange_info_payload(account, catalog))
        }
        "exchange.Exchange" => {
            let Ok(request) = ExchangeRequest::decode(request_args) else {
                return invalid("exchange request is invalid");
            };
            let id = request.exchange_id;
            let Some(config) = catalog.exchanges.get(&id) else {
                return invalid("exchange item was not found");
            };
            let max_count = config.change_count;
            let consume = config.item_consume.clone();
            let rewards = config.item_reward.clone();
            let current_count = account
                .exchange_times
                .get(&(id.max(0) as u64))
                .copied()
                .unwrap_or_default();
            if max_count > 0 && current_count >= max_count as u32 {
                return invalid("exchange limit reached");
            }
            if consume.is_empty() || rewards.is_empty() {
                return invalid("exchange reward is not configured");
            }
            if consume
                .iter()
                .any(|(kind, item, amount)| !can_consume_typed(account, *kind, *item, *amount))
            {
                return invalid("exchange cost is insufficient");
            }
            let typed_rewards = rewards
                .iter()
                .map(|(kind, item, amount)| ShopReward {
                    goods_type: *kind,
                    item_id: *item,
                    num: *amount,
                    instance_id: 0,
                })
                .collect::<Vec<_>>();
            if !can_grant_typed_task_rewards(account, &typed_rewards) {
                return invalid("exchange reward is unsupported");
            }
            for (kind, item, amount) in consume {
                consume_typed(account, kind, item, amount);
            }
            for reward in &typed_rewards {
                let _ = grant_typed_task_reward(account, reward);
            }
            account
                .exchange_times
                .entry(id.max(0) as u64)
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
            effects.push_pre(Response::raw(
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
            ));
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            reply("exchange.Exchange", encode_rewards_list(&typed_rewards))
        }
        _ => HandlerResult::Empty,
    }
}

pub(crate) fn handle_typed_food_compose(
    state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let catalog = gameplay_catalog();
    match method {
        "foodCompose.GetFoodComposeData" | "foodCompose.GetFoodCompose" => {
            reply(method, typed_food_info_payload(account, catalog))
        }
        "foodCompose.FoodCompose" => {
            let Ok(request) = FoodComposeRequest::decode(request_args) else {
                return invalid("food compose request is invalid");
            };
            let material_ids = request.material_ids;
            let recipe_id = catalog
                .food_recipes
                .iter()
                .find(|(_, recipe)| {
                    let mut configured = recipe
                        .material
                        .iter()
                        .flat_map(|(_, item_id, amount)| {
                            std::iter::repeat_n(*item_id, (*amount).max(1) as usize)
                        })
                        .collect::<Vec<_>>();
                    if configured.len() != material_ids.len() {
                        return false;
                    }
                    configured.sort_unstable();
                    let mut requested = material_ids.clone();
                    requested.sort_unstable();
                    !requested.is_empty() && configured == requested
                })
                .map(|(id, _)| *id)
                .unwrap_or(request.recipe_id);
            let Some(recipe) = catalog.food_recipes.get(&recipe_id) else {
                return invalid("food recipe was not found");
            };
            let materials = recipe.material.clone();
            if materials.is_empty()
                || materials
                    .iter()
                    .any(|(kind, item, amount)| !can_consume_typed(account, *kind, *item, *amount))
            {
                return invalid("food materials are insufficient");
            }
            let reward_id = recipe.reward_id;
            let rewards = catalog
                .rewards_by_id
                .get(&reward_id)
                .cloned()
                .unwrap_or_default();
            if rewards.is_empty() || !can_grant_typed_task_rewards(account, &rewards) {
                return invalid("food reward is unsupported");
            }
            for (kind, item, amount) in materials {
                consume_typed(account, kind, item, amount);
            }
            for reward in &rewards {
                let _ = grant_typed_task_reward(account, reward);
            }
            let recipe_key = recipe_id.max(0) as u64;
            account.food_compose.last_recipe_id = recipe_key;
            account
                .food_compose
                .recipes
                .entry(recipe_key)
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
            effects.push_pre(Response::raw(
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
            ));
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            reply(
                "foodCompose.FoodCompose",
                food_reward_payload(recipe_id, &rewards),
            )
        }
        _ => HandlerResult::Empty,
    }
}

pub(crate) fn handle_typed_world_event(
    state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let catalog = gameplay_catalog();
    match method {
        "worldevent.Progress" => reply(method, typed_world_event_server_progress_payload(account)),
        "worldevent.UserProgress" => {
            reply(method, typed_world_event_user_progress_payload(account))
        }
        "worldevent.UserStage" => reply(method, typed_world_event_stage_payload(account)),
        "worldeventrank.Rank" => reply(method, typed_world_event_rank_payload(account)),
        "worldevent.StageReward" => {
            let Ok(request) = WorldEventStageRequest::decode(request_args) else {
                return invalid("world event reward request is invalid");
            };
            let stage_id = request.stage_id as u64;
            let Some((event_id, event)) = active_world_event(catalog) else {
                return reply(method, encode_rewards_list(&[]));
            };
            let reward_id = event
                .server_stage_rewards
                .iter()
                .find(|(stage, _)| *stage as u64 == stage_id)
                .map(|(_, reward_id)| *reward_id)
                .unwrap_or_default();
            if event_id <= 0
                || stage_id == 0
                || account
                    .world_event
                    .user_progress
                    .max(account.world_event.progress)
                    < stage_id
                || reward_id <= 0
                || account
                    .world_event
                    .claimed_stages_by_event
                    .get(&(event_id as u64))
                    .is_some_and(|stages| stages.contains(&stage_id))
            {
                return reply(method, encode_rewards_list(&[]));
            }
            let rewards = catalog
                .rewards_by_id
                .get(&reward_id)
                .cloned()
                .unwrap_or_default();
            if rewards.is_empty() || !can_grant_typed_task_rewards(account, &rewards) {
                return invalid("world event reward is unsupported");
            }
            for reward in &rewards {
                let _ = grant_typed_task_reward(account, reward);
            }
            account
                .world_event
                .claimed_stages_by_event
                .entry(event_id as u64)
                .or_default()
                .insert(stage_id);
            effects.push_pre(Response::raw(
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
            ));
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            reply(method, encode_rewards_list(&rewards))
        }
        _ => HandlerResult::Empty,
    }
}

fn typed_world_event_server_progress_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, account.world_event.progress);
    append_varint_field(&mut output, 2, 1);
    output
}

fn typed_world_event_rank_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut node = Vec::new();
    append_varint_field(&mut node, 1, 1);
    append_varint_field(&mut node, 2, account.world_event.progress);
    append_varint_field(&mut node, 3, 1);
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &node);
    output
}

fn typed_world_event_stage_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    for stage in &account.world_event.stages {
        append_varint_field(&mut output, 1, *stage);
    }
    output
}

fn typed_world_event_user_progress_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        account
            .world_event
            .user_progress
            .max(account.world_event.progress),
    );
    output
}

fn typed_food_info_payload(
    account: &blueoath_domain::AccountState,
    catalog: &GameplayCatalog,
) -> Vec<u8> {
    let mut output = Vec::new();
    for id in catalog.food_recipes.keys().copied() {
        let mut row = Vec::new();
        append_varint_field(&mut row, 1, id.max(0) as u64);
        append_varint_field(
            &mut row,
            2,
            account
                .food_compose
                .recipes
                .get(&(id.max(0) as u64))
                .copied()
                .unwrap_or_default() as u64,
        );
        append_varint_field(&mut row, 3, 0);
        append_message_field(&mut output, 1, &row);
    }
    append_varint_field(&mut output, 3, account.food_compose.last_recipe_id);
    output
}

fn typed_exchange_info_payload(
    account: &blueoath_domain::AccountState,
    catalog: &GameplayCatalog,
) -> Vec<u8> {
    let mut output = Vec::new();
    for id in catalog.exchanges.keys().copied() {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, id.max(0) as u64);
        append_varint_field(
            &mut item,
            2,
            account
                .exchange_times
                .get(&(id.max(0) as u64))
                .copied()
                .unwrap_or_default() as u64,
        );
        append_message_field(&mut output, 1, &item);
    }
    output
}

fn can_consume_typed(
    account: &blueoath_domain::AccountState,
    kind: i32,
    item: i32,
    amount: i32,
) -> bool {
    let Ok(amount) = u64::try_from(amount) else {
        return false;
    };
    if amount == 0 {
        return false;
    }
    if kind == 5 {
        return typed_currency(item)
            .is_some_and(|currency| account.resources.amount(currency).get() >= amount);
    }
    let Ok(template_id) = blueoath_domain::TemplateId::new(item.max(0) as u64) else {
        return false;
    };
    account
        .inventory
        .items
        .get(&template_id)
        .copied()
        .unwrap_or_default()
        >= amount
}

fn consume_typed(account: &mut blueoath_domain::AccountState, kind: i32, item: i32, amount: i32) {
    let amount = u64::try_from(amount).unwrap_or_default();
    if kind == 5 {
        if let Some(currency) = typed_currency(item) {
            let _ = account.resources.debit(currency, amount);
        }
    } else if let Ok(template_id) = blueoath_domain::TemplateId::new(item.max(0) as u64) {
        let current = account
            .inventory
            .items
            .get(&template_id)
            .copied()
            .unwrap_or_default();
        account
            .inventory
            .items
            .insert(template_id, current.saturating_sub(amount));
    }
}

fn typed_currency(item: i32) -> Option<blueoath_domain::CurrencyKind> {
    Some(match item {
        1 => blueoath_domain::CurrencyKind::Gold,
        2 => blueoath_domain::CurrencyKind::Diamond,
        5 => blueoath_domain::CurrencyKind::Supply,
        30 => blueoath_domain::CurrencyKind::PvePoint,
        _ => return None,
    })
}

pub(crate) fn gameplay_catalog() -> &'static GameplayCatalog {
    GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default)
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

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn invalid(message: &'static str) -> HandlerResult {
    HandlerResult::Error(GameError::InvalidRequest(message))
}

fn active_world_event(catalog: &GameplayCatalog) -> Option<(i32, &WorldEventConfig)> {
    let activity = catalog.activity.get(&5001).or_else(|| {
        catalog
            .activity
            .values()
            .find(|value| value.activity_type == 5001)
    })?;
    let event_id = activity.p1.first().copied()?;
    catalog
        .world_events
        .get(&event_id)
        .map(|event| (event_id, event))
}

#[cfg(test)]
mod tests {
    use crate::common::response::HandlerResult;
    use blueoath_domain::AccountState;

    use super::*;

    #[test]
    fn typed_battlepass_task_claim_updates_state_and_pushes() {
        let mut account = AccountState::default();
        let state = ServerState::new("battle-pass", "Captain", "test");
        let mut effects = ResponseEffects::default();
        let result = handle_typed_battlepass(
            &state,
            &mut account,
            "battlepass.RecieveTaskReward",
            &[0x08, 101],
            &mut effects,
        );
        assert!(matches!(result, HandlerResult::PushOnly));
        assert!(account.battle_pass.claimed_tasks.contains(&101));
        assert_eq!(account.battle_pass.last_task_id, 101);
        assert_eq!(effects.into_parts().0.len(), 3);
    }
}
