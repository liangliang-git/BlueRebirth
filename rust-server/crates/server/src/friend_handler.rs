use serde_json::{json, Value};

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(super) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    state: &ServerState,
    method: &str,
    request_args: &[u8],
    _pre_pushes: &mut Vec<Vec<u8>>,
) -> HandlerResult {
    match method {
        "friend.GetFriendMainData" => reply(method, typed_friend_main_payload(state, account)),
        "friend.GetFriendList" => reply(
            method,
            typed_friend_list_payload(state, account, &account.social.friends, 1),
        ),
        "friend.GetRecommendList" => reply(method, Vec::new()),
        "friend.SearchUser" => {
            let Ok(request) = FriendSearchRequest::decode(request_args) else {
                return invalid("friend search request is invalid");
            };
            let matches = (request.uid != 0 && request.uid == account.character.uid)
                || (!request.name.is_empty() && request.name == account.character.name);
            let ids = matches
                .then_some(account.character.uid)
                .into_iter()
                .collect::<Vec<_>>();
            reply(
                method,
                typed_friend_list_payload_from_ids(state, account, &ids, 0),
            )
        }
        "friend.Apply" => {
            let Ok(request) = FriendTargetRequest::decode(request_args) else {
                return invalid("friend application request is invalid");
            };
            if request.uid == account.character.uid
                || account.social.blacklist.contains(&request.uid)
            {
                return invalid("friend application target is invalid");
            }
            account.social.pending.insert(request.uid);
            HandlerResult::PushOnly
        }
        "friend.Accept" => {
            let Ok(request) = FriendTargetRequest::decode(request_args) else {
                return invalid("friend accept request is invalid");
            };
            if !account.social.pending.remove(&request.uid) {
                return invalid("friend application was not found");
            }
            account.social.friends.insert(request.uid);
            HandlerResult::PushOnly
        }
        "friend.Refuse" => {
            let Ok(request) = FriendTargetRequest::decode(request_args) else {
                return invalid("friend refuse request is invalid");
            };
            if !account.social.pending.remove(&request.uid) {
                return invalid("friend application was not found");
            }
            HandlerResult::PushOnly
        }
        "friend.DeleteFriend" => {
            let Ok(request) = FriendTargetRequest::decode(request_args) else {
                return invalid("friend delete request is invalid");
            };
            if !account.social.friends.remove(&request.uid) {
                return invalid("friend was not found");
            }
            HandlerResult::PushOnly
        }
        "friend.SetBlack" => {
            let Ok(request) = FriendTargetRequest::decode(request_args) else {
                return invalid("blacklist request is invalid");
            };
            account.social.friends.remove(&request.uid);
            account.social.blacklist.insert(request.uid);
            HandlerResult::PushOnly
        }
        "friend.DeleteBlack" => {
            let Ok(request) = FriendTargetRequest::decode(request_args) else {
                return invalid("blacklist delete request is invalid");
            };
            if !account.social.blacklist.remove(&request.uid) {
                return invalid("blacklist entry was not found");
            }
            HandlerResult::PushOnly
        }
        "friend.UpdateUserState" => {
            let Ok(request) = FriendUpdateUserStateRequest::decode(request_args) else {
                return invalid("friend user state request is invalid");
            };
            let mut payload = Vec::new();
            append_varint_field(&mut payload, 1, request.state.max(0) as u64);
            append_varint_field(&mut payload, 2, request.uid);
            reply(method, payload)
        }
        _ => HandlerResult::Empty,
    }
}

fn typed_friend_main_payload(
    state: &ServerState,
    account: &blueoath_domain::AccountState,
) -> Vec<u8> {
    let mut output = Vec::new();
    append_typed_friend_ids(
        &mut output,
        1,
        state,
        account,
        account.social.friends.iter().copied(),
        1,
    );
    append_typed_friend_ids(
        &mut output,
        2,
        state,
        account,
        account.social.blacklist.iter().copied(),
        0,
    );
    append_typed_friend_ids(
        &mut output,
        3,
        state,
        account,
        account.social.pending.iter().copied(),
        0,
    );
    append_typed_friend_ids(
        &mut output,
        4,
        state,
        account,
        account.social.applied.iter().copied(),
        0,
    );
    output
}

fn typed_friend_list_payload(
    state: &ServerState,
    account: &blueoath_domain::AccountState,
    ids: &std::collections::BTreeSet<u64>,
    status: i32,
) -> Vec<u8> {
    let ids = ids.iter().copied().collect::<Vec<_>>();
    typed_friend_list_payload_from_ids(state, account, &ids, status)
}

fn typed_friend_list_payload_from_ids(
    state: &ServerState,
    account: &blueoath_domain::AccountState,
    ids: &[u64],
    status: i32,
) -> Vec<u8> {
    let mut output = Vec::new();
    append_typed_friend_ids(&mut output, 1, state, account, ids.iter().copied(), status);
    output
}

fn append_typed_friend_ids<I>(
    output: &mut Vec<u8>,
    field: u8,
    state: &ServerState,
    account: &blueoath_domain::AccountState,
    ids: I,
    status: i32,
) where
    I: IntoIterator<Item = u64>,
{
    for uid in ids {
        if uid == 0 {
            continue;
        }
        let mut friend = Vec::new();
        append_message_field(
            &mut friend,
            1,
            &UserInfoCodec::encode(&typed_friend_user_info(state, account, uid)),
        );
        append_varint_field(&mut friend, 2, 0);
        append_varint_field(&mut friend, 3, status.max(0) as u64);
        append_varint_field(&mut friend, 4, current_unix_seconds() as u64);
        append_message_field(output, field, &friend);
    }
}

fn typed_friend_user_info(
    state: &ServerState,
    account: &blueoath_domain::AccountState,
    uid: u64,
) -> UserInfo {
    if uid == account.character.uid {
        return user_info_from_typed_account(state, account);
    }
    UserInfo {
        uid,
        uname: "Commander".to_owned(),
        level: 1,
        ..UserInfo::default()
    }
}

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let state = context.state;
    let account = &mut *context.account;

    match method {
        "friend.GetFriendMainData" => reply(
            method,
            friend_main_payload(state, account.as_deref().unwrap_or(&Value::Null)),
        ),
        "friend.GetFriendList" => reply(
            method,
            friend_list_payload(
                state,
                account.as_deref().unwrap_or(&Value::Null),
                "friends",
                1,
            ),
        ),
        "friend.GetRecommendList" => reply(
            method,
            friend_list_payload(
                state,
                account.as_deref().unwrap_or(&Value::Null),
                "recommend",
                0,
            ),
        ),
        "friend.SearchUser" => {
            let requested_uid = decode_varint_u64_field(request_args, 1);
            let requested_name = decode_string_field(request_args, 2).unwrap_or_default();
            let current = account.as_deref().unwrap_or(&Value::Null);
            let character = current.get("character").unwrap_or(&Value::Null);
            let own_uid = json_u64(character, "uid").unwrap_or(1);
            let own_name = json_string(character, "name").unwrap_or_default();
            let matches = (requested_uid != 0 && requested_uid == own_uid)
                || (!requested_name.is_empty() && requested_name == own_name);
            reply(
                method,
                friend_common_list_payload(state, current, matches.then_some(own_uid), 0),
            )
        }
        "friend.Apply" => {
            let uid = decode_varint_u64_field(request_args, 1);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            let own_uid = own_uid(account);
            if uid == 0 || uid == own_uid {
                return invalid("friend application target is invalid");
            }
            add_friend_id(account, "applyList", uid);
            HandlerResult::PushOnly
        }
        "friend.Accept" => {
            let uid = decode_varint_u64_field(request_args, 1);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            if uid == 0 || !remove_friend_id(account, "applyList", uid) {
                return invalid("friend application was not found");
            }
            add_friend_id(account, "friends", uid);
            HandlerResult::PushOnly
        }
        "friend.Refuse" => {
            let uid = decode_varint_u64_field(request_args, 1);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            if uid == 0 || !remove_friend_id(account, "applyList", uid) {
                return invalid("friend application was not found");
            }
            HandlerResult::PushOnly
        }
        "friend.DeleteFriend" => {
            let uid = decode_varint_u64_field(request_args, 1);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            if uid == 0 || !remove_friend_id(account, "friends", uid) {
                return invalid("friend was not found");
            }
            HandlerResult::PushOnly
        }
        "friend.SetBlack" => {
            let uid = decode_varint_u64_field(request_args, 1);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            if uid == 0 {
                return invalid("blacklist target is invalid");
            }
            remove_friend_id(account, "friends", uid);
            add_friend_id(account, "blackList", uid);
            HandlerResult::PushOnly
        }
        "friend.DeleteBlack" => {
            let uid = decode_varint_u64_field(request_args, 1);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            if uid == 0 || !remove_friend_id(account, "blackList", uid) {
                return invalid("blacklist entry was not found");
            }
            HandlerResult::PushOnly
        }
        "friend.UpdateUserState" => {
            let mut output = Vec::new();
            append_varint_field(
                &mut output,
                1,
                decode_varint_field(request_args, 1).max(0) as u64,
            );
            append_varint_field(&mut output, 2, decode_varint_u64_field(request_args, 2));
            reply(method, output)
        }
        _ => HandlerResult::Empty,
    }
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn invalid(message: &'static str) -> HandlerResult {
    HandlerResult::Error(GameError::InvalidRequest(message))
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

#[cfg(test)]
mod tests {
    use super::*;
    use blueoath_domain::{NewAccountFactory, ProfileId};

    #[test]
    fn typed_friend_mutations_use_domain_relations() {
        let state = ServerState::new("friend-typed", "Captain", "1.4.0");
        let mut account =
            NewAccountFactory::create(ProfileId::new("friend-typed").unwrap(), "Captain");
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 42);

        assert!(matches!(
            handle_typed(&mut account, &state, "friend.Apply", &args, &mut Vec::new(),),
            HandlerResult::PushOnly
        ));
        assert!(account.social.pending.contains(&42));

        assert!(matches!(
            handle_typed(
                &mut account,
                &state,
                "friend.Accept",
                &args,
                &mut Vec::new(),
            ),
            HandlerResult::PushOnly
        ));
        assert!(!account.social.pending.contains(&42));
        assert!(account.social.friends.contains(&42));

        assert!(matches!(
            handle_typed(
                &mut account,
                &state,
                "friend.SetBlack",
                &args,
                &mut Vec::new(),
            ),
            HandlerResult::PushOnly
        ));
        assert!(!account.social.friends.contains(&42));
        assert!(account.social.blacklist.contains(&42));
    }

    #[test]
    fn typed_friend_projection_contains_domain_friend() {
        let state = ServerState::new("friend-projection", "Captain", "1.4.0");
        let mut account =
            NewAccountFactory::create(ProfileId::new("friend-projection").unwrap(), "Captain");
        account.social.friends.insert(42);
        let result = handle_typed(
            &mut account,
            &state,
            "friend.GetFriendMainData",
            &[],
            &mut Vec::new(),
        );
        let HandlerResult::Reply(response) = result else {
            panic!("expected friend projection reply");
        };
        assert!(!response.payload.is_empty());
    }
}
