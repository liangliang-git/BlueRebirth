use serde_json::{json, Value};

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;
use blueoath_domain::{AccountState, GuildBoxItemState};

pub(super) fn handle_typed(
    account: &mut AccountState,
    server_state: &ServerState,
    method: &str,
    request_args: &[u8],
    catalog: &GameplayCatalog,
    pre_pushes: &mut Vec<Vec<u8>>,
) -> HandlerResult {
    match method {
        "guildbox.SetAnonymous" => {
            account.guild_box.anonymous = decode_varint_field(request_args, 1) != 0;
            reply(method, typed_user_data_payload(account))
        }
        "guildbox.PickShareBox" => {
            let box_id = decode_varint_u64_field(request_args, 1);
            pick_typed_box(method, &mut account.guild_box.share_boxes, box_id)
        }
        "guildbox.PickTaskBox" => {
            let box_id = decode_varint_u64_field(request_args, 1);
            pick_typed_box(method, &mut account.guild_box.task_boxes, box_id)
        }
        "guildbox.PickPointsBox" => {
            if account.guild_box.points_box_count == 0 {
                return invalid("guild points box is empty");
            }
            let reward_id = catalog
                .guild_box_scores
                .values()
                .find_map(|config| json_i32(config, "reward"));
            let rewards = reward_id
                .and_then(|id| catalog.rewards_by_id.get(&id))
                .cloned()
                .unwrap_or_default();
            if rewards.is_empty() || !can_grant_typed_task_rewards(account, &rewards) {
                return invalid("guild points reward is unavailable");
            }
            for reward in &rewards {
                let _ = grant_typed_task_reward(account, reward);
            }
            account.guild_box.points_box_count -= 1;
            append_method_push(
                pre_pushes,
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_typed_account(server_state, account)),
            );
            append_method_push(
                pre_pushes,
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            );
            reply(method, typed_reward_list_payload(0, &rewards))
        }
        "guildbox.PickAllTaskBox" => {
            for item in &mut account.guild_box.task_boxes {
                item.is_picked = true;
            }
            reply(method, typed_reward_list_payload(0, &[]))
        }
        "guildbox.GuildData" => reply(method, typed_guild_data_payload(account)),
        "guildbox.UserData" => reply(method, typed_user_data_payload(account)),
        "guildbox.UserAllList" => reply(method, typed_all_list_payload(account)),
        _ => HandlerResult::Empty,
    }
}

fn pick_typed_box(method: &str, boxes: &mut [GuildBoxItemState], box_id: u64) -> HandlerResult {
    if box_id == 0 {
        return invalid("guild box id is invalid");
    }
    let Some(item) = boxes.iter_mut().find(|item| item.box_id == box_id) else {
        return invalid("guild box was not found");
    };
    if item.is_picked {
        return invalid("guild box was already picked");
    }
    item.is_picked = true;
    reply(method, typed_reward_list_payload(box_id, &[]))
}

fn typed_guild_data_payload(account: &AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, account.guild_box.progress);
    output
}

fn typed_user_data_payload(account: &AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, u64::from(account.guild_box.anonymous));
    append_varint_field(
        &mut output,
        2,
        u64::from(account.guild_box.points_box_count),
    );
    output
}

fn append_typed_box_list(output: &mut Vec<u8>, field: u8, boxes: &[GuildBoxItemState]) {
    for item in boxes {
        let mut encoded = Vec::new();
        append_varint_field(&mut encoded, 1, item.box_id);
        append_varint_field(&mut encoded, 2, item.end_time);
        append_varint_field(&mut encoded, 3, item.box_uid);
        append_varint_field(&mut encoded, 4, u64::from(item.is_picked));
        append_varint_field(&mut encoded, 5, u64::from(item.recharge_id));
        append_bytes_field(&mut encoded, 6, item.recharge_name.as_bytes());
        append_message_field(output, field, &encoded);
    }
}

fn typed_all_list_payload(account: &AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    append_typed_box_list(&mut output, 1, &account.guild_box.share_boxes);
    append_typed_box_list(&mut output, 2, &account.guild_box.task_boxes);
    output
}

fn typed_reward_list_payload(box_id: u64, rewards: &[ShopReward]) -> Vec<u8> {
    let mut output = encode_rewards_list(rewards);
    append_varint_field(&mut output, 2, box_id);
    output
}

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let Some(account) = context.account.as_deref_mut() else {
        return HandlerResult::Error(GameError::AccountUnavailable);
    };
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    match method {
        "guildbox.SetAnonymous" => {
            let anonymous = decode_varint_field(request_args, 1).max(0);
            let state = guild_box_state_mut(account);
            state["anonymous"] = json!(anonymous);
            reply(method, user_data_payload(state))
        }
        "guildbox.PickShareBox" => {
            let box_id = decode_varint_u64_field(request_args, 1);
            pick_box_result(method, account, "shareBoxes", box_id)
        }
        "guildbox.PickTaskBox" => {
            let box_id = decode_varint_u64_field(request_args, 1);
            pick_box_result(method, account, "taskBoxes", box_id)
        }
        "guildbox.PickPointsBox" => {
            let state = guild_box_state_mut(account);
            let count = json_i64(state, "pointsBoxCount").unwrap_or_default();
            if count <= 0 {
                return invalid("guild points box is empty");
            }
            state["pointsBoxCount"] = json!(count - 1);
            let reward_id = catalog
                .guild_box_scores
                .values()
                .find_map(|config| json_i32(config, "reward"));
            let rewards = reward_id
                .and_then(|id| catalog.rewards_by_id.get(&id))
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
                .collect::<Vec<_>>();
            let refreshes = (
                UserInfoCodec::encode(&user_info_from_account(context.state, Some(account))),
                BagInfoCodec::encode(&bag_info_from_account(account)),
            );
            append_method_push(context.pre_pushes, "user.UpdateUserInfo", refreshes.0);
            append_method_push(context.pre_pushes, "bag.UpdateBagData", refreshes.1);
            reply(method, reward_list_payload(0, &rewards))
        }
        "guildbox.PickAllTaskBox" => {
            let state = guild_box_state_mut(account);
            if let Some(boxes) = state.get_mut("taskBoxes").and_then(Value::as_array_mut) {
                for item in boxes {
                    item["isPick"] = json!(true);
                }
            }
            reply(method, reward_list_payload(0, &[]))
        }
        "guildbox.GuildData" => reply(method, guild_data_payload(account)),
        "guildbox.UserData" => {
            let state = guild_box_state_mut(account);
            reply(method, user_data_payload(state))
        }
        "guildbox.UserAllList" => reply(method, all_list_payload(account)),
        _ => HandlerResult::Empty,
    }
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn invalid(message: &'static str) -> HandlerResult {
    HandlerResult::Error(GameError::InvalidRequest(message))
}

fn pick_box_result(method: &str, account: &mut Value, key: &str, box_id: u64) -> HandlerResult {
    match pick_box_payload(account, key, box_id) {
        Ok(payload) => reply(method, payload),
        Err(message) => invalid(message),
    }
}

fn guild_box_state_mut(account: &mut Value) -> &mut Value {
    account
        .as_object_mut()
        .expect("account must be an object")
        .entry("guildBox".to_owned())
        .or_insert_with(|| {
            json!({
                "progress": 0,
                "anonymous": 0,
                "pointsBoxCount": 0,
                "shareBoxes": [],
                "taskBoxes": []
            })
        })
}

fn pick_box_payload(account: &mut Value, key: &str, box_id: u64) -> Result<Vec<u8>, &'static str> {
    if box_id == 0 {
        return Err("guild box id is invalid");
    }
    let state = guild_box_state_mut(account);
    let found = state
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .find(|item| json_u64(item, "boxId") == Some(box_id));
    if let Some(item) = found {
        if json_bool(item, "isPick") {
            return Err("guild box was already picked");
        }
        item["isPick"] = json!(true);
    } else {
        return Err("guild box was not found");
    }
    Ok(reward_list_payload(box_id, &[]))
}

fn guild_data_payload(account: &mut Value) -> Vec<u8> {
    let state = guild_box_state_mut(account);
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        json_i64(state, "progress").unwrap_or_default().max(0) as u64,
    );
    output
}

fn user_data_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        json_i64(state, "anonymous").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(
        &mut output,
        2,
        json_i64(state, "pointsBoxCount").unwrap_or_default().max(0) as u64,
    );
    output
}

fn all_list_payload(account: &mut Value) -> Vec<u8> {
    let state = guild_box_state_mut(account);
    let mut output = Vec::new();
    append_box_list(&mut output, 1, state.get("shareBoxes"));
    append_box_list(&mut output, 2, state.get("taskBoxes"));
    output
}

fn append_box_list(output: &mut Vec<u8>, field: u8, value: Option<&Value>) {
    for item in value.and_then(Value::as_array).into_iter().flatten() {
        let mut encoded = Vec::new();
        append_varint_field(&mut encoded, 1, json_u64(item, "boxId").unwrap_or_default());
        append_varint_field(
            &mut encoded,
            2,
            json_u64(item, "endTime").unwrap_or_default(),
        );
        append_varint_field(
            &mut encoded,
            3,
            json_u64(item, "boxUid").unwrap_or_default(),
        );
        append_varint_field(&mut encoded, 4, u64::from(json_bool(item, "isPick")));
        append_varint_field(
            &mut encoded,
            5,
            json_i64(item, "rechargeId").unwrap_or_default().max(0) as u64,
        );
        append_bytes_field(
            &mut encoded,
            6,
            json_string(item, "rechargeName")
                .unwrap_or_default()
                .as_bytes(),
        );
        append_message_field(output, field, &encoded);
    }
}

fn reward_list_payload(box_id: u64, rewards: &[ShopReward]) -> Vec<u8> {
    let mut output = encode_rewards_list(rewards);
    append_varint_field(&mut output, 2, box_id);
    output
}

#[cfg(test)]
mod tests {
    use crate::common::response::HandlerResult;

    use super::*;

    #[test]
    fn handler_exposes_typed_result() {
        let _: for<'state, 'account, 'scratch> fn(
            &mut GameLoginRequestContext<'state, 'account, 'scratch>,
            &str,
            &[u8],
        ) -> HandlerResult = handle;
    }

    #[test]
    fn user_data_payload_uses_jp_anonymous_and_points_fields() {
        let payload = user_data_payload(&json!({"anonymous": 1, "pointsBoxCount": 4}));
        assert_eq!(decode_varint_field(&payload, 1), 1);
        assert_eq!(decode_varint_field(&payload, 2), 4);
    }

    #[test]
    fn reward_list_payload_contains_box_id() {
        let payload = reward_list_payload(77, &[]);
        assert_eq!(decode_varint_u64_field(&payload, 2), 77);
    }
}
