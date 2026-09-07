use serde_json::{json, Value};

use super::*;

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let state = context.state;
    let account = &mut *context.account;
    let response_err = &mut *context.response_err;
    let response_err_msg = &mut *context.response_err_msg;

    match method {
        "friend.GetFriendMainData" => Some(friend_main_payload(
            state,
            account.as_deref().unwrap_or(&Value::Null),
        )),
        "friend.GetFriendList" => Some(friend_list_payload(
            state,
            account.as_deref().unwrap_or(&Value::Null),
            "friends",
            1,
        )),
        "friend.GetRecommendList" => Some(friend_list_payload(
            state,
            account.as_deref().unwrap_or(&Value::Null),
            "recommend",
            0,
        )),
        "friend.SearchUser" => {
            let requested_uid = decode_varint_u64_field(request_args, 1);
            let requested_name = decode_string_field(request_args, 2).unwrap_or_default();
            let current = account.as_deref().unwrap_or(&Value::Null);
            let character = current.get("character").unwrap_or(&Value::Null);
            let own_uid = json_u64(character, "uid").unwrap_or(1);
            let own_name = json_string(character, "name").unwrap_or_default();
            let matches = (requested_uid != 0 && requested_uid == own_uid)
                || (!requested_name.is_empty() && requested_name == own_name);
            Some(friend_common_list_payload(
                state,
                current,
                matches.then_some(own_uid),
                0,
            ))
        }
        "friend.Apply" => {
            let uid = decode_varint_u64_field(request_args, 1);
            if let Some(account) = account.as_mut().map(|value| &mut **value) {
                let own_uid = own_uid(account);
                if uid == 0 || uid == own_uid {
                    *response_err = 1;
                    *response_err_msg = "friend application target is invalid".to_owned();
                } else {
                    add_friend_id(account, "applyList", uid);
                }
            }
            Some(Vec::new())
        }
        "friend.Accept" => {
            let uid = decode_varint_u64_field(request_args, 1);
            if let Some(account) = account.as_mut().map(|value| &mut **value) {
                if uid == 0 || !remove_friend_id(account, "applyList", uid) {
                    *response_err = 1;
                    *response_err_msg = "friend application was not found".to_owned();
                } else {
                    add_friend_id(account, "friends", uid);
                }
            }
            Some(Vec::new())
        }
        "friend.Refuse" => {
            let uid = decode_varint_u64_field(request_args, 1);
            if let Some(account) = account.as_mut().map(|value| &mut **value) {
                if uid == 0 || !remove_friend_id(account, "applyList", uid) {
                    *response_err = 1;
                    *response_err_msg = "friend application was not found".to_owned();
                }
            }
            Some(Vec::new())
        }
        "friend.DeleteFriend" => {
            let uid = decode_varint_u64_field(request_args, 1);
            if let Some(account) = account.as_mut().map(|value| &mut **value) {
                if uid == 0 || !remove_friend_id(account, "friends", uid) {
                    *response_err = 1;
                    *response_err_msg = "friend was not found".to_owned();
                }
            }
            Some(Vec::new())
        }
        "friend.SetBlack" => {
            let uid = decode_varint_u64_field(request_args, 1);
            if let Some(account) = account.as_mut().map(|value| &mut **value) {
                if uid == 0 {
                    *response_err = 1;
                    *response_err_msg = "blacklist target is invalid".to_owned();
                } else {
                    remove_friend_id(account, "friends", uid);
                    add_friend_id(account, "blackList", uid);
                }
            }
            Some(Vec::new())
        }
        "friend.DeleteBlack" => {
            let uid = decode_varint_u64_field(request_args, 1);
            if let Some(account) = account.as_mut().map(|value| &mut **value) {
                if uid == 0 || !remove_friend_id(account, "blackList", uid) {
                    *response_err = 1;
                    *response_err_msg = "blacklist entry was not found".to_owned();
                }
            }
            Some(Vec::new())
        }
        "friend.UpdateUserState" => {
            let mut output = Vec::new();
            append_varint_field(
                &mut output,
                1,
                decode_varint_field(request_args, 1).max(0) as u64,
            );
            append_varint_field(&mut output, 2, decode_varint_u64_field(request_args, 2));
            Some(output)
        }
        _ => None,
    }
}

fn friend_main_payload(state: &ServerState, account: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_friend_ids(
        &mut output,
        1,
        state,
        account,
        &friend_ids(account, "friends"),
        1,
    );
    append_friend_ids(
        &mut output,
        2,
        state,
        account,
        &friend_ids(account, "blackList"),
        0,
    );
    append_friend_ids(
        &mut output,
        3,
        state,
        account,
        &friend_ids(account, "applyList"),
        0,
    );
    append_friend_ids(
        &mut output,
        4,
        state,
        account,
        &friend_ids(account, "applyRecordList"),
        0,
    );
    output
}

fn friend_list_payload(state: &ServerState, account: &Value, key: &str, status: i32) -> Vec<u8> {
    let ids = if key == "recommend" {
        Vec::new()
    } else {
        friend_ids(account, key)
    };
    friend_common_list_payload_with_ids(state, account, &ids, status)
}

fn friend_common_list_payload(
    state: &ServerState,
    account: &Value,
    uid: Option<u64>,
    status: i32,
) -> Vec<u8> {
    let ids = uid.into_iter().collect::<Vec<_>>();
    friend_common_list_payload_with_ids(state, account, &ids, status)
}

fn friend_common_list_payload_with_ids(
    state: &ServerState,
    account: &Value,
    ids: &[u64],
    status: i32,
) -> Vec<u8> {
    let mut output = Vec::new();
    append_friend_ids(&mut output, 1, state, account, ids, status);
    output
}

fn append_friend_ids(
    output: &mut Vec<u8>,
    field: u8,
    state: &ServerState,
    account: &Value,
    ids: &[u64],
    status: i32,
) {
    for uid in ids {
        let mut friend = Vec::new();
        append_message_field(
            &mut friend,
            1,
            &super::base_handler::other_user_payload(state, account, *uid),
        );
        append_varint_field(&mut friend, 2, 0);
        append_varint_field(&mut friend, 3, status.max(0) as u64);
        append_varint_field(&mut friend, 4, current_unix_seconds() as u64);
        append_message_field(output, field, &friend);
    }
}

fn own_uid(account: &Value) -> u64 {
    account
        .get("character")
        .and_then(|character| json_u64(character, "uid"))
        .unwrap_or(1)
}

fn friend_ids(account: &Value, key: &str) -> Vec<u64> {
    account
        .get("friend")
        .and_then(|friend| friend.get(key))
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default()
}

fn add_friend_id(account: &mut Value, key: &str, uid: u64) {
    let Some(root) = account.as_object_mut() else {
        return;
    };
    let friend = root.entry("friend").or_insert_with(|| {
        json!({
            "friends": [],
            "blackList": [],
            "applyList": [],
            "applyRecordList": []
        })
    });
    let Some(ids) = friend.get_mut(key).and_then(Value::as_array_mut) else {
        return;
    };
    if !ids.iter().any(|value| value.as_u64() == Some(uid)) {
        ids.push(json!(uid));
    }
}

fn remove_friend_id(account: &mut Value, key: &str, uid: u64) -> bool {
    let Some(ids) = account
        .get_mut("friend")
        .and_then(|friend| friend.get_mut(key))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let old_len = ids.len();
    ids.retain(|value| value.as_u64() != Some(uid));
    old_len != ids.len()
}
