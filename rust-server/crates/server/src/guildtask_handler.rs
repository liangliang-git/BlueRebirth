use serde_json::{json, Value};

use super::*;

pub(super) fn handles(method: &str) -> bool {
    method.starts_with("guildtask.")
}

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let task_id = match method {
        "guildtask.GuildTaskAccept" | "guildtask.GuildTaskFinish" | "guildtask.Donate" => {
            decode_varint_field(request_args, 2)
        }
        _ => decode_varint_field(request_args, 1),
    };
    match method {
        "guildtask.UpdateGuildTaskData" => Some(guild_task_data_payload(
            context.account.as_deref().unwrap_or(&Value::Null),
        )),
        "guildtask.UpdateGuildTaskUserData" => Some(guild_task_user_payload(
            context.account.as_deref().unwrap_or(&Value::Null),
        )),
        "guildtask.AcceptTask" | "guildtask.GuildTaskAccept" => {
            let Some(account) = context.account.as_deref_mut() else {
                *context.response_err = 1;
                *context.response_err_msg = "account is unavailable".to_owned();
                return Some(Vec::new());
            };
            let state = guild_task_state_mut(account);
            push_unique_i32(&mut state["acceptedTasks"], task_id);
            state["lastTaskId"] = json!(task_id);
            Some(Vec::new())
        }
        "guildtask.GuildTaskFinish" => {
            let Some(account) = context.account.as_deref_mut() else {
                *context.response_err = 1;
                *context.response_err_msg = "account is unavailable".to_owned();
                return Some(Vec::new());
            };
            let state = guild_task_state_mut(account);
            push_unique_i32(&mut state["finishedTasks"], task_id);
            state["lastTaskId"] = json!(task_id);
            Some(Vec::new())
        }
        "guildtask.Donate" => handle_donate(context, request_args, task_id),
        "guildtask.DrawTaskReward" => handle_draw_reward(context, task_id),
        "guildtask.ConstantRewardPoolGetReward" => handle_reward(context, task_id),
        _ => None,
    }
}

fn guild_task_state_mut(account: &mut Value) -> &mut Value {
    account
        .as_object_mut()
        .expect("account must be an object")
        .entry("guildTask".to_owned())
        .or_insert_with(|| {
            json!({
                "acceptedTasks": [],
                "finishedTasks": [],
                "claimedRewards": [],
                "contribute": 0,
                "todayAcceptCount": 0,
                "todayFinishStepCount": 0,
                "lastTaskId": 0
            })
        })
}

fn push_unique_i32(target: &mut Value, value: i32) {
    if value <= 0 {
        return;
    }
    let values = target
        .as_array_mut()
        .expect("guild task list must be an array");
    if !values
        .iter()
        .any(|entry| entry.as_i64() == Some(i64::from(value)))
    {
        values.push(json!(value));
    }
}

fn handle_donate<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    request_args: &[u8],
    task_id: i32,
) -> Option<Vec<u8>> {
    let contribute = decode_varint_field(request_args, 4);
    let items = decode_repeated_message_field(request_args, 3)
        .into_iter()
        .filter_map(|item| {
            let goods_type = decode_varint_field(&item, 1);
            let item_id = decode_varint_field(&item, 2);
            let amount = decode_varint_field(&item, 3);
            (goods_type > 0 && item_id > 0 && amount > 0).then_some((goods_type, item_id, amount))
        })
        .collect::<Vec<_>>();
    if task_id <= 0 || contribute <= 0 || items.is_empty() {
        *context.response_err = 1;
        *context.response_err_msg = "guild task donation is invalid".to_owned();
        return Some(Vec::new());
    }
    let Some(account) = context.account.as_deref_mut() else {
        *context.response_err = 1;
        *context.response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    if items.iter().any(|(goods_type, item_id, amount)| {
        !guild_task_can_consume(account, *goods_type, *item_id, *amount)
    }) {
        *context.response_err = 1;
        *context.response_err_msg = "guild task donation items are insufficient".to_owned();
        return Some(Vec::new());
    }
    for (goods_type, item_id, amount) in items {
        guild_task_consume(account, goods_type, item_id, amount);
    }
    let state = guild_task_state_mut(account);
    state["contribute"] =
        json!(state_i64(state, "contribute").saturating_add(i64::from(contribute)));
    state["lastTaskId"] = json!(task_id);
    append_method_push(
        context.pre_pushes,
        "user.UpdateUserInfo",
        UserInfoCodec::encode(&user_info_from_account(context.state, Some(account))),
    );
    append_method_push(
        context.pre_pushes,
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_account(account)),
    );
    Some(Vec::new())
}

fn guild_task_can_consume(account: &Value, goods_type: i32, item_id: i32, amount: i32) -> bool {
    if goods_type == 5 {
        currency_character_key(item_id)
            .is_some_and(|key| character_i64(account, key) >= i64::from(amount))
    } else {
        bag_item_count(account, item_id) >= i64::from(amount)
    }
}

fn guild_task_consume(account: &mut Value, goods_type: i32, item_id: i32, amount: i32) {
    if goods_type == 5 {
        if let Some(key) = currency_character_key(item_id) {
            add_character_i64(account, key, amount.saturating_neg());
        }
    } else {
        consume_bag_item(account, item_id, amount);
    }
}

fn handle_reward<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    requested_reward_id: i32,
) -> Option<Vec<u8>> {
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    let Some(account) = context.account.as_deref_mut() else {
        *context.response_err = 1;
        *context.response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    let reward_id =
        if requested_reward_id > 0 && catalog.rewards_by_id.contains_key(&requested_reward_id) {
            requested_reward_id
        } else {
            let task_id = account
                .get("guildTask")
                .and_then(|state| state.get("lastTaskId"))
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok())
                .unwrap_or_default();
            catalog
                .guild_tasks
                .get(&task_id)
                .and_then(|task| json_i32(task, "player_rewards"))
                .unwrap_or_default()
        };
    let already_claimed = reward_id > 0
        && account
            .get("guildTask")
            .and_then(|state| state.get("claimedRewards"))
            .and_then(Value::as_array)
            .is_some_and(|values| {
                values
                    .iter()
                    .any(|value| value.as_i64() == Some(i64::from(reward_id)))
            });
    if already_claimed {
        return Some(encode_rewards_list(&[]));
    }
    let rewards = if reward_id > 0 {
        catalog
            .rewards_by_id
            .get(&reward_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|reward| {
                grant_reward(
                    account,
                    reward,
                    current_unix_seconds(),
                    context.catalogs.fashion,
                )
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if reward_id > 0 {
        let state = guild_task_state_mut(account);
        push_unique_i32(&mut state["claimedRewards"], reward_id);
    }
    Some(encode_rewards_list(&rewards))
}

fn handle_draw_reward<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    requested_reward_id: i32,
) -> Option<Vec<u8>> {
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    let reward_id =
        if requested_reward_id > 0 && catalog.rewards_by_id.contains_key(&requested_reward_id) {
            requested_reward_id
        } else {
            catalog
                .guild_tasks
                .values()
                .filter_map(|task| json_i32(task, "player_rewards"))
                .find(|id| *id > 0)
                .unwrap_or_default()
        };
    let rewards = catalog
        .rewards_by_id
        .get(&reward_id)
        .cloned()
        .unwrap_or_default();
    let first = rewards.first().copied().unwrap_or(ShopReward {
        goods_type: 0,
        item_id: 0,
        num: 0,
        instance_id: 0,
    });
    let Some(account) = context.account.as_deref_mut() else {
        *context.response_err = 1;
        *context.response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    let reward = if reward_id > 0 {
        rewards
            .into_iter()
            .map(|item| {
                grant_reward(
                    account,
                    item,
                    current_unix_seconds(),
                    context.catalogs.fashion,
                )
            })
            .next()
            .unwrap_or(first)
    } else {
        first
    };
    let mut output = Vec::new();
    // TRANDOMREWARDDATA uses sparse protobuf field numbers: ItemType=3,
    // ItemId=5, ItemNum=7, GiveUid=9. Fields 1/2/4/6 are not part of
    // this message and make the JP client decode an empty reward.
    append_varint_field(&mut output, 3, reward.goods_type.max(0) as u64);
    append_varint_field(&mut output, 5, reward.item_id.max(0) as u64);
    append_varint_field(&mut output, 7, reward.num.max(0) as u64);
    append_varint_field(&mut output, 9, 0);
    Some(output)
}

fn state_i64(state: &Value, key: &str) -> i64 {
    state.get(key).and_then(Value::as_i64).unwrap_or_default()
}

fn guild_task_user_payload(account: &Value) -> Vec<u8> {
    let state = account.get("guildTask").unwrap_or(&Value::Null);
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, state_i64(state, "contribute").max(0) as u64);
    append_varint_field(
        &mut output,
        2,
        state_i64(state, "todayAcceptCount").max(0) as u64,
    );
    append_varint_field(
        &mut output,
        3,
        state_i64(state, "todayFinishStepCount").max(0) as u64,
    );
    if let Some(task_id) = state.get("lastTaskId").and_then(Value::as_i64) {
        let mut task = Vec::new();
        append_varint_field(&mut task, 1, 1);
        append_varint_field(&mut task, 2, task_id.max(0) as u64);
        append_varint_field(&mut task, 3, 1);
        append_message_field(&mut output, 4, &task);
    }
    output
}

fn guild_task_data_payload(account: &Value) -> Vec<u8> {
    let state = account.get("guildTask").unwrap_or(&Value::Null);
    let mut output = Vec::new();
    if let Some(values) = state.get("acceptedTasks").and_then(Value::as_array) {
        for (index, task_id) in values.iter().filter_map(Value::as_i64).enumerate() {
            let mut task = Vec::new();
            append_varint_field(&mut task, 1, (index as u64).saturating_add(1));
            append_varint_field(&mut task, 2, task_id.max(0) as u64);
            append_varint_field(&mut task, 3, 0);
            append_varint_field(&mut task, 4, 1);
            append_varint_field(&mut task, 5, 0);
            append_message_field(&mut output, 1, &task);
        }
    }
    append_varint_field(
        &mut output,
        2,
        state_i64(state, "todayAcceptCount").max(0) as u64,
    );
    append_varint_field(
        &mut output,
        3,
        state_i64(state, "todayFinishStepCount").max(0) as u64,
    );
    append_varint_field(
        &mut output,
        4,
        u64::from(
            state
                .get("acceptedTasks")
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty),
        ),
    );
    append_varint_field(&mut output, 6, 1);
    output
}
