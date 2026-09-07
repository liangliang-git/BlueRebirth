use serde_json::{json, Value};

use super::*;

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let response_err = &mut *context.response_err;
    let response_err_msg = &mut *context.response_err_msg;
    let account = context.account.as_deref_mut()?;
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    match method {
        "guildbox.SetAnonymous" => {
            let anonymous = decode_varint_field(request_args, 1).max(0);
            let state = guild_box_state_mut(account);
            state["anonymous"] = json!(anonymous);
            Some(user_data_payload(state))
        }
        "guildbox.PickShareBox" => {
            let box_id = decode_varint_u64_field(request_args, 1);
            Some(pick_box_payload(
                account,
                "shareBoxes",
                box_id,
                response_err,
                response_err_msg,
            ))
        }
        "guildbox.PickTaskBox" => {
            let box_id = decode_varint_u64_field(request_args, 1);
            Some(pick_box_payload(
                account,
                "taskBoxes",
                box_id,
                response_err,
                response_err_msg,
            ))
        }
        "guildbox.PickPointsBox" => {
            let state = guild_box_state_mut(account);
            let count = json_i64(state, "pointsBoxCount").unwrap_or_default();
            if count <= 0 {
                *context.response_err = 1;
                *context.response_err_msg = "guild points box is empty".to_owned();
                return Some(Vec::new());
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
            Some(reward_list_payload(0, &rewards))
        }
        "guildbox.PickAllTaskBox" => {
            let state = guild_box_state_mut(account);
            if let Some(boxes) = state.get_mut("taskBoxes").and_then(Value::as_array_mut) {
                for item in boxes {
                    item["isPick"] = json!(true);
                }
            }
            Some(reward_list_payload(0, &[]))
        }
        "guildbox.GuildData" => Some(guild_data_payload(account)),
        "guildbox.UserData" => {
            let state = guild_box_state_mut(account);
            Some(user_data_payload(state))
        }
        "guildbox.UserAllList" => Some(all_list_payload(account)),
        _ => None,
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

fn pick_box_payload(
    account: &mut Value,
    key: &str,
    box_id: u64,
    response_err: &mut i32,
    response_err_msg: &mut String,
) -> Vec<u8> {
    if box_id == 0 {
        *response_err = 1;
        *response_err_msg = "guild box id is invalid".to_owned();
        return Vec::new();
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
            *response_err = 1;
            *response_err_msg = "guild box was already picked".to_owned();
            return Vec::new();
        }
        item["isPick"] = json!(true);
    } else {
        *response_err = 1;
        *response_err_msg = "guild box was not found".to_owned();
        return Vec::new();
    }
    reward_list_payload(box_id, &[])
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
    use super::*;

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
