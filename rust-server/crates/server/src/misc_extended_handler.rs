use serde_json::{json, Value};

use super::*;

pub(super) fn handles(method: &str) -> bool {
    method.starts_with("magazine.") || method.starts_with("interactionitem.")
}

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    if method.starts_with("magazine.") {
        return handle_magazine(context, catalog, method, request_args);
    }
    handle_interaction_item(context, catalog, method, request_args)
}

fn handle_magazine<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    catalog: &GameplayCatalog,
    method: &str,
    args: &[u8],
) -> Option<Vec<u8>> {
    let Some(account) = context.account.as_deref_mut() else {
        *context.response_err = 1;
        *context.response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    if method == "magazine.FetchMagazineReward" {
        let reward_key = decode_varint_field(args, 1);
        let already_claimed = account
            .get("magazine")
            .and_then(|value| value.get("claimedRewards"))
            .and_then(Value::as_array)
            .is_some_and(|values| {
                values
                    .iter()
                    .any(|value| value.as_i64() == Some(i64::from(reward_key)))
            });
        let reward_id = magazine_reward_id(catalog, reward_key);
        let rewards = if !already_claimed && reward_id > 0 {
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
        let state = magazine_state_mut(account);
        push_unique_i32(&mut state["claimedRewards"], reward_key);
        return if rewards.is_empty() {
            Some(magazine_payload(state))
        } else {
            Some(encode_rewards_list(&rewards))
        };
    }
    let state = magazine_state_mut(account);
    match method {
        "magazine.GetMagazine" | "magazine.Magazine" => Some(magazine_payload(state)),
        "magazine.UpdateMagazineInfo" => Some(magazine_update_payload(state)),
        "magazine.AddHero" => {
            let hero_id = decode_varint_field(args, 1);
            push_unique_i32(&mut state["heroes"], hero_id);
            Some(magazine_payload(state))
        }
        "magazine.Vote" => {
            let page_id = decode_varint_field(args, 1);
            push_unique_i32(&mut state["votes"], page_id);
            Some(magazine_payload(state))
        }
        "magazine.UnLock" => {
            let page_id = decode_varint_field(args, 1);
            push_unique_i32(&mut state["unlocked"], page_id);
            Some(magazine_payload(state))
        }
        _ => None,
    }
}

fn handle_interaction_item<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    catalog: &GameplayCatalog,
    method: &str,
    args: &[u8],
) -> Option<Vec<u8>> {
    let Some(account) = context.account.as_deref_mut() else {
        *context.response_err = 1;
        *context.response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    if matches!(
        method,
        "interactionitem.GetItemReward"
            | "interactionitem.BuyChristmasFurniture"
            | "interactionitem.GetSpringPaperFlowerReward"
    ) {
        let item_id = decode_varint_field(args, 1);
        let reward_defs = interaction_reward_defs(catalog, item_id);
        let already_claimed = account
            .get("interactionItems")
            .and_then(|value| value.get("rewards"))
            .and_then(Value::as_array)
            .is_some_and(|values| {
                values
                    .iter()
                    .any(|value| value.as_i64() == Some(i64::from(item_id)))
            });
        if !already_claimed && !reward_defs.is_empty() {
            let rewards = reward_defs
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
            let state = interaction_state_mut(account);
            push_unique_i32(&mut state["rewards"], item_id);
            return Some(encode_rewards_list(&rewards));
        }
        let state = interaction_state_mut(account);
        push_unique_i32(&mut state["rewards"], item_id);
        return if method == "interactionitem.GetItemReward" {
            Some(encode_rewards_list(&[]))
        } else {
            Some(interaction_payload(state))
        };
    }
    let state = interaction_state_mut(account);
    match method {
        "interactionitem.RefreshInteractionItems" => Some(interaction_payload(state)),
        "interactionitem.SetCrystalBallToy" => {
            state["crystalBallToy"] = json!(decode_varint_field(args, 1));
            Some(interaction_payload(state))
        }
        "interactionitem.SetBagItemVisible" => {
            let item_id = decode_varint_field(args, 1);
            let visible = decode_varint_field(args, 2) != 0;
            state["visible"][item_id.to_string()] = json!(visible);
            Some(interaction_payload(state))
        }
        "interactionitem.SetMutexBagGroupState" => {
            let group_id = decode_varint_field(args, 1);
            state["groups"][group_id.to_string()] = json!(decode_varint_field(args, 2));
            Some(interaction_payload(state))
        }
        "interactionitem.SetPosterState" => {
            let poster_id = decode_varint_field(args, 1);
            state["posters"][poster_id.to_string()] = json!(decode_varint_field(args, 2));
            Some(interaction_payload(state))
        }
        _ => None,
    }
}

fn magazine_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    for id in state
        .get("unlocked")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
    {
        let mut magazine = Vec::new();
        append_varint_field(&mut magazine, 1, id.max(0) as u64);
        if let Some(rewards) = state.get("claimedRewards").and_then(Value::as_array) {
            for (index, reward) in rewards.iter().filter_map(Value::as_i64).enumerate() {
                let mut item = Vec::new();
                append_varint_field(&mut item, 1, reward.max(0) as u64);
                append_varint_field(
                    &mut item,
                    2,
                    u64::from(current_unix_seconds().saturating_add(index as u32)),
                );
                append_message_field(&mut magazine, 2, &item);
            }
        }
        append_message_field(&mut output, 1, &magazine);
    }
    for (index, hero_id) in state
        .get("heroes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .enumerate()
    {
        let mut hero = Vec::new();
        append_varint_field(&mut hero, 1, (index as u64).saturating_add(1));
        append_varint_field(&mut hero, 2, hero_id.max(0) as u64);
        append_varint_field(&mut hero, 3, 0);
        append_message_field(&mut output, 2, &hero);
    }
    for magazine_id in state
        .get("unlocked")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
    {
        let mut unlock = Vec::new();
        append_varint_field(&mut unlock, 1, magazine_id.max(0) as u64);
        append_varint_field(&mut unlock, 2, u64::from(current_unix_seconds()));
        append_message_field(&mut output, 3, &unlock);
    }
    output
}

fn magazine_update_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_ids(&mut output, 1, state.get("unlocked"));
    append_varint_field(&mut output, 2, u64::from(current_unix_seconds()));
    append_varint_field(&mut output, 3, 1);
    output
}

fn magazine_state_mut(account: &mut Value) -> &mut Value {
    account
        .as_object_mut()
        .expect("account must be an object")
        .entry("magazine".to_owned())
        .or_insert_with(|| json!({"heroes": [], "votes": [], "unlocked": [], "claimedRewards": []}))
}

fn interaction_state_mut(account: &mut Value) -> &mut Value {
    account
        .as_object_mut()
        .expect("account must be an object")
        .entry("interactionItems".to_owned())
        .or_insert_with(|| json!({"visible": {}, "groups": {}, "posters": {}, "rewards": []}))
}

fn magazine_reward_id(catalog: &GameplayCatalog, reward_key: i32) -> i32 {
    if catalog.rewards_by_id.contains_key(&reward_key) {
        return reward_key;
    }
    catalog
        .magazine_info
        .values()
        .filter_map(|info| info.get("rewards").and_then(Value::as_array))
        .flat_map(|values| values.iter().filter_map(Value::as_i64))
        .nth(usize::try_from(reward_key.saturating_sub(1)).unwrap_or(usize::MAX))
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or_default()
}

fn interaction_reward_defs(catalog: &GameplayCatalog, item_id: i32) -> Vec<ShopReward> {
    let Some(item) = catalog.interaction_items.get(&item_id) else {
        return Vec::new();
    };
    if let Some(reward_id) = json_i32(item, "reward").filter(|id| *id > 0) {
        return catalog
            .rewards_by_id
            .get(&reward_id)
            .cloned()
            .unwrap_or_default();
    }
    let Some(drop_id) = json_i32(item, "drop_id").filter(|id| *id > 0) else {
        return Vec::new();
    };
    catalog
        .drop_items
        .get(&drop_id)
        .and_then(|drop| drop.get("drop"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            Some(ShopReward {
                goods_type: i32::try_from(row.first()?.as_i64()?).ok()?,
                item_id: i32::try_from(row.get(1)?.as_i64()?).ok()?,
                num: i32::try_from(row.get(2)?.as_i64()?).ok()?,
                instance_id: 0,
            })
        })
        .filter(|reward| reward.goods_type > 0 && reward.item_id > 0 && reward.num > 0)
        .collect()
}

fn interaction_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        state
            .get("crystalBallToy")
            .and_then(Value::as_i64)
            .unwrap_or_default()
            .max(0) as u64,
    );
    append_ids(&mut output, 2, state.get("rewards"));
    for (key, value) in state
        .get("visible")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, key.parse::<u64>().unwrap_or_default());
        append_varint_field(&mut item, 2, u64::from(value.as_bool().unwrap_or(false)));
        append_message_field(&mut output, 3, &item);
    }
    output
}

fn append_ids(output: &mut Vec<u8>, field: u8, values: Option<&Value>) {
    for value in values
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
    {
        append_varint_field(output, field, value.max(0) as u64);
    }
}

fn push_unique_i32(target: &mut Value, value: i32) {
    if value <= 0 {
        return;
    }
    let values = target.as_array_mut().expect("state list must be an array");
    if !values
        .iter()
        .any(|entry| entry.as_i64() == Some(i64::from(value)))
    {
        values.push(json!(value));
    }
}
