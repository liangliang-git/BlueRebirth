use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(super) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    state: &ServerState,
    method: &str,
    request_args: &[u8],
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

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn invalid(message: &'static str) -> HandlerResult {
    HandlerResult::Error(GameError::InvalidRequest(message))
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
            handle_typed(&mut account, &state, "friend.Apply", &args),
            HandlerResult::PushOnly
        ));
        assert!(account.social.pending.contains(&42));

        assert!(matches!(
            handle_typed(&mut account, &state, "friend.Accept", &args,),
            HandlerResult::PushOnly
        ));
        assert!(!account.social.pending.contains(&42));
        assert!(account.social.friends.contains(&42));

        assert!(matches!(
            handle_typed(&mut account, &state, "friend.SetBlack", &args,),
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
        let result = handle_typed(&mut account, &state, "friend.GetFriendMainData", &[]);
        let HandlerResult::Reply(response) = result else {
            panic!("expected friend projection reply");
        };
        assert!(!response.payload.is_empty());
    }
}
