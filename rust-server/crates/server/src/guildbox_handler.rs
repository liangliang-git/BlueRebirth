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
            let request = match GuildBoxAnonymousRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => return invalid("guild box anonymous is invalid"),
            };
            account.guild_box.anonymous = request.anonymous;
            reply(method, typed_user_data_payload(account))
        }
        "guildbox.PickShareBox" => {
            let request = match GuildBoxIdRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => return invalid("guild box id is invalid"),
            };
            pick_typed_box(method, &mut account.guild_box.share_boxes, request.box_id)
        }
        "guildbox.PickTaskBox" => {
            let request = match GuildBoxIdRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => return invalid("guild box id is invalid"),
            };
            pick_typed_box(method, &mut account.guild_box.task_boxes, request.box_id)
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

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn invalid(message: &'static str) -> HandlerResult {
    HandlerResult::Error(GameError::InvalidRequest(message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_data_payload_uses_jp_anonymous_and_points_fields() {
        let mut account = AccountState::default();
        account.guild_box.anonymous = true;
        account.guild_box.points_box_count = 4;
        let payload = typed_user_data_payload(&account);
        assert_eq!(decode_varint_field(&payload, 1), 1);
        assert_eq!(decode_varint_field(&payload, 2), 4);
    }

    #[test]
    fn reward_list_payload_contains_box_id() {
        let payload = typed_reward_list_payload(77, &[]);
        assert_eq!(decode_varint_u64_field(&payload, 2), 77);
    }
}
