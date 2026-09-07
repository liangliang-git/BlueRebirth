use super::super::config::{
    SharedPush, SharedSocialState, TypedBattleRoom, TypedBattleSession, TypedCoopRoom,
    TypedCoopUser, TypedMatchQueueEntry,
};
use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;
fn battle_room_ret(room_id: u64) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, room_id);
    output
}

fn pvp_match_ready_payload(room_id: u64) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, room_id);
    output
}

fn battle_auto_msg_payload(uid: u64, msg_id: i32) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, uid);
    append_varint_field(&mut output, 2, msg_id.max(0) as u64);
    output
}

fn battle_match_ret_payload(uids: &[u64]) -> Vec<u8> {
    let mut match_ret = Vec::new();
    for uid in uids.iter().copied().filter(|uid| *uid > 0) {
        let mut user = Vec::new();
        append_varint_field(&mut user, 1, uid);
        append_bytes_field(&mut user, 2, b"local");
        append_message_field(&mut match_ret, 1, &user);
    }
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &match_ret);
    output
}

fn battle_create_multi_ret(battle_port: u16, battle_id: u64, request_args: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, battle_id);
    append_bytes_field(&mut output, 2, b"127.0.0.1");
    append_varint_field(&mut output, 3, u64::from(battle_port));
    append_message_field(&mut output, 4, request_args);
    output
}

#[cfg(test)]
const ROOM_ID_FIELD: u8 = 1;
#[cfg(test)]
const COPY_ID_FIELD: u8 = 2;
#[cfg(test)]
const HERO_LIST_FIELD: u8 = 4;

pub(super) fn handles_typed(method: &str) -> bool {
    matches!(
        GameMethod::parse(method).family(),
        MethodFamily::MatchServer | MethodFamily::Room | MethodFamily::Battle
    ) || canonical_typed_method(method).is_some()
}

pub(super) fn handle_typed(
    state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let Some(method) = canonical_typed_method(method) else {
        return HandlerResult::Empty;
    };
    let now = current_unix_seconds();
    let uid = account.character.uid.max(1);
    drain_shared_pushes(state, uid, effects);
    match method {
        "matchsvr.CreateRoom" => {
            let Ok(request) = CoopCreateRoomRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "co-op create request is invalid",
                ));
            };
            let Some(hero_ids) = typed_coop_hero_ids(account, &request.hero_ids, false) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "co-op hero list is invalid",
                ));
            };
            let mut shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let mut room_id = u64::from(now % 2_000_000_000).saturating_add(uid.min(999));
            while shared.typed_rooms.contains_key(&room_id) {
                room_id = room_id.saturating_add(1);
            }
            let room = TypedCoopRoom {
                room_id,
                copy_id: request.copy_id,
                owner_id: uid,
                is_public: true,
                capacity: 2,
                create_time: now,
                state: 0,
                users: vec![typed_coop_user(account, uid, hero_ids, now)],
                ..TypedCoopRoom::default()
            };
            let payload = typed_coop_room_payload(&room);
            shared.typed_rooms.insert(room_id, room);
            HandlerResult::Reply(Response::raw(method, payload))
        }
        "matchsvr.EnterRoom" => {
            let Ok(request) = CoopRoomHeroesRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "co-op room request is invalid",
                ));
            };
            let room_id = request.room_id;
            let Some(hero_ids) = typed_coop_hero_ids(account, &request.hero_ids, false) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "co-op hero list is invalid",
                ));
            };
            let mut shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let Some(room) = shared.typed_rooms.get_mut(&room_id) else {
                return HandlerResult::Error(GameError::NotFound("co-op room"));
            };
            if !room.users.iter().any(|user| user.uid == uid)
                && room.users.len() >= room.capacity as usize
            {
                return HandlerResult::Error(GameError::InvalidState("co-op room is full"));
            }
            if let Some(user) = room.users.iter_mut().find(|user| user.uid == uid) {
                user.hero_ids = hero_ids;
            } else {
                room.users
                    .push(typed_coop_user(account, uid, hero_ids, now));
            }
            let payload = typed_coop_room_payload(room);
            let users = room.users.clone();
            enqueue_typed_room_push(&mut shared, &users, uid, payload.clone());
            HandlerResult::Reply(Response::raw(method, payload))
        }
        "matchsvr.ExitRoom" | "matchsvr.DismissRoom" | "matchsvr.Leave" => {
            let Ok(request) = CoopRoomIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest("co-op room id is invalid"));
            };
            let room_id = request.room_id;
            let mut shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let Some(room) = shared.typed_rooms.get_mut(&room_id) else {
                return HandlerResult::Reply(Response::raw(method, Vec::new()));
            };
            if room.owner_id == uid {
                shared.typed_rooms.remove(&room_id);
            } else {
                room.users.retain(|user| user.uid != uid);
                let payload = typed_coop_room_payload(room);
                let users = room.users.clone();
                enqueue_typed_room_push(&mut shared, &users, uid, payload);
            }
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "matchsvr.Ready" | "matchsvr.Cancel" => {
            let Ok(request) = CoopRoomIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest("co-op room id is invalid"));
            };
            let room_id = request.room_id;
            let mut shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let Some(room) = shared.typed_rooms.get_mut(&room_id) else {
                return HandlerResult::Error(GameError::NotFound("co-op room"));
            };
            let Some(user) = room.users.iter_mut().find(|user| user.uid == uid) else {
                return HandlerResult::Error(GameError::InvalidState("user is not in co-op room"));
            };
            user.is_ready = method == "matchsvr.Ready";
            let payload = typed_coop_room_payload(room);
            let users = room.users.clone();
            enqueue_typed_room_push(&mut shared, &users, uid, payload);
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "matchsvr.Kick" => {
            let Ok(request) = CoopKickRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "co-op kick request is invalid",
                ));
            };
            let room_id = request.room_id;
            let kicked_uid = request.kicked_uid;
            let mut shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let Some(room) = shared.typed_rooms.get_mut(&room_id) else {
                return HandlerResult::Error(GameError::NotFound("co-op room"));
            };
            if room.owner_id != uid || kicked_uid == 0 || kicked_uid == uid {
                return HandlerResult::Error(GameError::InvalidRequest("co-op kick is invalid"));
            }
            room.users.retain(|user| user.uid != kicked_uid);
            let payload = typed_coop_room_payload(room);
            let users = room.users.clone();
            enqueue_typed_room_push(&mut shared, &users, uid, payload);
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "matchsvr.UploadTactic" | "matchsvr.Deliver" => {
            let Ok(request) = CoopRoomHeroesRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "co-op room request is invalid",
                ));
            };
            let room_id = request.room_id;
            let Some(hero_ids) = typed_coop_hero_ids(account, &request.hero_ids, false) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "co-op hero list is invalid",
                ));
            };
            let mut shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let Some(room) = shared.typed_rooms.get_mut(&room_id) else {
                return HandlerResult::Error(GameError::NotFound("co-op room"));
            };
            let Some(user) = room.users.iter_mut().find(|user| user.uid == uid) else {
                return HandlerResult::Error(GameError::InvalidState("user is not in co-op room"));
            };
            user.hero_ids = hero_ids;
            let payload = typed_coop_room_payload(room);
            let users = room.users.clone();
            enqueue_typed_room_push(&mut shared, &users, uid, payload);
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "matchsvr.SendSetPassWord" => {
            let Ok(request) = CoopPasswordRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "co-op password request is invalid",
                ));
            };
            let room_id = request.room_id;
            let mut shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let Some(room) = shared.typed_rooms.get_mut(&room_id) else {
                return HandlerResult::Error(GameError::NotFound("co-op room"));
            };
            if !room.users.iter().any(|user| user.uid == uid) {
                return HandlerResult::Error(GameError::InvalidState("user is not in co-op room"));
            }
            room.password = request.password;
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "matchsvr.GetChapterInfo" => {
            let Ok(request) = CoopRoomIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest("co-op room id is invalid"));
            };
            let room_id = request.room_id;
            let shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let Some(room) = shared.typed_rooms.get(&room_id) else {
                return HandlerResult::Error(GameError::NotFound("co-op room"));
            };
            HandlerResult::Reply(Response::raw(method, typed_coop_room_payload(room)))
        }
        "matchsvr.Remind"
        | "matchsvr.Invite"
        | "matchsvr.RefuseInvite"
        | "matchsvr.AcceptInvite" => HandlerResult::Reply(Response::raw(method, Vec::new())),
        "matchsvr.GetRoomList" => {
            let shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let mut output = Vec::new();
            for room in shared.typed_rooms.values().filter(|room| room.is_public) {
                append_message_field(&mut output, 1, &typed_coop_room_payload(room));
            }
            HandlerResult::Reply(Response::raw(method, output))
        }
        "matchsvr.SwitchRoomPublicState" => {
            let Ok(request) = CoopRoomIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest("co-op room id is invalid"));
            };
            let room_id = request.room_id;
            let mut shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let Some(room) = shared.typed_rooms.get_mut(&room_id) else {
                return HandlerResult::Error(GameError::NotFound("co-op room"));
            };
            if room.owner_id != uid {
                return HandlerResult::Error(GameError::InvalidState(
                    "only room owner can change visibility",
                ));
            }
            room.is_public = !room.is_public;
            let payload = typed_coop_room_payload(room);
            let users = room.users.clone();
            enqueue_typed_room_push(&mut shared, &users, uid, payload);
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "matchsvr.Start" | "room.StartMatch" => {
            let Ok(request) = CoopRoomIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest("co-op room id is invalid"));
            };
            typed_coop_set_state(state, uid, request.room_id, 1, method)
        }
        "room.StopMatch" => {
            let Ok(request) = CoopRoomIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest("co-op room id is invalid"));
            };
            typed_coop_set_state(state, uid, request.room_id, 0, method)
        }
        "matchsvr.ChangeChapter" => {
            let Ok(request) = CoopChangeChapterRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "co-op chapter request is invalid",
                ));
            };
            let room_id = request.room_id;
            let copy_id = request.copy_id;
            let mut shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let Some(room) = shared.typed_rooms.get_mut(&room_id) else {
                return HandlerResult::Error(GameError::NotFound("co-op room"));
            };
            if room.owner_id != uid || copy_id <= 0 {
                return HandlerResult::Error(GameError::InvalidRequest("co-op chapter is invalid"));
            }
            room.copy_id = copy_id;
            let payload = typed_coop_room_payload(room);
            let users = room.users.clone();
            enqueue_typed_room_push(&mut shared, &users, uid, payload);
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "matchsvr.CancelFocus" => {
            let Ok(request) = CoopRoomIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest("co-op room id is invalid"));
            };
            typed_coop_set_focus(state, uid, request.room_id, false, method)
        }
        "matchsvr.SetAutoReady" => {
            let Ok(request) = CoopRoomIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest("co-op room id is invalid"));
            };
            typed_coop_set_auto_ready(state, uid, request.room_id, true, method)
        }
        "matchsvr.CancelAutoReady" => {
            let Ok(request) = CoopRoomIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest("co-op room id is invalid"));
            };
            typed_coop_set_auto_ready(state, uid, request.room_id, false, method)
        }
        "battle.CreateRoom" => {
            let now = current_unix_seconds();
            let mut shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let room_id = next_typed_battle_id(&shared, now, uid);
            shared.typed_battle_rooms.insert(
                room_id,
                TypedBattleRoom {
                    room_id,
                    owner_id: uid,
                    users: vec![uid],
                },
            );
            HandlerResult::Reply(Response::raw(method, battle_room_ret(room_id)))
        }
        "battle.JoinRoom" => {
            let Ok(request) = CoopRoomIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "battle room id is invalid",
                ));
            };
            let room_id = request.room_id;
            let mut shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let Some(room) = shared.typed_battle_rooms.get_mut(&room_id) else {
                return HandlerResult::Error(GameError::NotFound("battle room"));
            };
            if !room.users.contains(&uid) {
                if room.users.len() >= 2 {
                    return HandlerResult::Error(GameError::InvalidState("battle room is full"));
                }
                room.users.push(uid);
            }
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "battle.LeaveRoom" => {
            let mut shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let room_id = shared
                .typed_battle_rooms
                .iter()
                .find(|(_, room)| room.users.contains(&uid))
                .map(|(room_id, _)| *room_id);
            if let Some(room_id) = room_id {
                let remove = shared
                    .typed_battle_rooms
                    .get(&room_id)
                    .is_some_and(|room| room.owner_id == uid);
                if remove {
                    shared.typed_battle_rooms.remove(&room_id);
                } else if let Some(room) = shared.typed_battle_rooms.get_mut(&room_id) {
                    room.users.retain(|user_uid| *user_uid != uid);
                }
            }
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "battle.MatchJoin" => {
            let Ok(request) = CoopMatchTypeRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "battle match request is invalid",
                ));
            };
            let match_type = request.match_type.max(0) as u32;
            let mut shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            shared
                .typed_match_queue
                .retain(|entry| entry.uid != uid || entry.match_type != match_type);
            if let Some(index) = shared
                .typed_match_queue
                .iter()
                .position(|entry| entry.match_type == match_type && entry.uid != uid)
            {
                let opponent = shared.typed_match_queue.remove(index);
                let payload = battle_match_ret_payload(&[opponent.uid, uid]);
                enqueue_shared_push(
                    &mut shared,
                    opponent.uid,
                    "battle.MatchJoin",
                    payload.clone(),
                );
                HandlerResult::Reply(Response::raw(method, payload))
            } else {
                shared
                    .typed_match_queue
                    .push(TypedMatchQueueEntry { uid, match_type });
                HandlerResult::Reply(Response::raw(method, battle_match_ret_payload(&[uid])))
            }
        }
        "battle.MatchLeave" => {
            let Ok(request) = CoopMatchTypeRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "battle match request is invalid",
                ));
            };
            let match_type = request.match_type.max(0) as u32;
            if let Ok(mut shared) = state.shared_social.lock() {
                shared
                    .typed_match_queue
                    .retain(|entry| entry.uid != uid || entry.match_type != match_type);
            }
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "battle.SendAutoMsg" => {
            let Ok(request) = BattleAutoMessageRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "battle auto message request is invalid",
                ));
            };
            let msg_id = request.message_id.max(0);
            let payload = battle_auto_msg_payload(uid, msg_id);
            if let Ok(mut shared) = state.shared_social.lock() {
                let recipients = shared
                    .typed_battle_rooms
                    .values()
                    .find(|room| room.users.contains(&uid))
                    .map(|room| room.users.clone())
                    .unwrap_or_default();
                for recipient_uid in recipients.into_iter().filter(|value| *value != uid) {
                    enqueue_shared_push(
                        &mut shared,
                        recipient_uid,
                        "battle.receiveAutoMsg",
                        payload.clone(),
                    );
                }
            }
            effects.push_post(Response::raw("battle.receiveAutoMsg", payload));
            let mut response = Vec::new();
            append_varint_field(&mut response, 1, 0);
            HandlerResult::Reply(Response::raw(method, response))
        }
        "battle.pvpMatchReady" | "battle.pvpMatchReadyTimeout" => {
            let now = current_unix_seconds();
            let room_id = state
                .shared_social
                .lock()
                .ok()
                .and_then(|shared| {
                    shared
                        .typed_battle_rooms
                        .iter()
                        .find(|(_, room)| room.users.contains(&uid))
                        .map(|(room_id, _)| *room_id)
                })
                .unwrap_or_else(|| u64::from(now % 2_000_000_000).saturating_add(uid.min(999)));
            HandlerResult::Reply(Response::raw(method, pvp_match_ready_payload(room_id)))
        }
        "battle.CreateMutiBattle" => {
            let now = current_unix_seconds();
            let mut shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let battle_id = next_typed_battle_id(&shared, now, uid);
            shared.typed_battles.insert(
                battle_id,
                TypedBattleSession {
                    battle_id,
                    users: vec![uid],
                },
            );
            HandlerResult::Reply(Response::raw(
                method,
                battle_create_multi_ret(state.battle_port, battle_id, request_args),
            ))
        }
        "battle.createBattleInfo" => {
            let shared = match state.shared_social.lock() {
                Ok(shared) => shared,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidState(
                        "social state is unavailable",
                    ))
                }
            };
            let battle_id = shared
                .typed_battles
                .values()
                .find(|battle| battle.users.contains(&uid))
                .map(|battle| battle.battle_id)
                .or_else(|| {
                    shared
                        .typed_battle_rooms
                        .values()
                        .find(|room| room.users.contains(&uid))
                        .map(|room| room.room_id)
                })
                .unwrap_or_default();
            HandlerResult::Reply(Response::raw(
                method,
                typed_battle_push_payload(state.battle_port, battle_id, uid),
            ))
        }
        _ => HandlerResult::Error(GameError::InvalidRequest("co-op method is unsupported")),
    }
}

fn canonical_typed_method(method: &str) -> Option<&str> {
    if let Some(suffix) = method
        .strip_prefix("matchsvr_")
        .and_then(|value| value.split_once('.').map(|(_, suffix)| suffix))
    {
        return match suffix {
            "CreateRoom" => Some("matchsvr.CreateRoom"),
            "EnterRoom" => Some("matchsvr.EnterRoom"),
            "ExitRoom" | "Leave" => Some("matchsvr.ExitRoom"),
            "DismissRoom" => Some("matchsvr.DismissRoom"),
            "Ready" => Some("matchsvr.Ready"),
            "Cancel" => Some("matchsvr.Cancel"),
            "Kick" => Some("matchsvr.Kick"),
            "Tactic" => Some("matchsvr.UploadTactic"),
            "Deliver" => Some("matchsvr.Deliver"),
            "SendSetPassWord" => Some("matchsvr.SendSetPassWord"),
            "GetChapterInfo" => Some("matchsvr.GetChapterInfo"),
            "Remind" => Some("matchsvr.Remind"),
            "Invite" => Some("matchsvr.Invite"),
            "RefuseInvite" => Some("matchsvr.RefuseInvite"),
            "AcceptInvite" => Some("matchsvr.AcceptInvite"),
            "GetRoomList" => Some("matchsvr.GetRoomList"),
            "SwitchRoomPublicState" => Some("matchsvr.SwitchRoomPublicState"),
            "Start" => Some("matchsvr.Start"),
            "ChangeChapter" => Some("matchsvr.ChangeChapter"),
            "CancelFocus" => Some("matchsvr.CancelFocus"),
            "SetAutoReady" => Some("matchsvr.SetAutoReady"),
            "CancelAutoReady" => Some("matchsvr.CancelAutoReady"),
            _ => Some(method),
        };
    }
    match method {
        "matchsvr.CreateRoom"
        | "matchsvr.EnterRoom"
        | "matchsvr.ExitRoom"
        | "matchsvr.DismissRoom"
        | "matchsvr.Ready"
        | "matchsvr.Cancel"
        | "matchsvr.Kick"
        | "matchsvr.UploadTactic"
        | "matchsvr.Deliver"
        | "matchsvr.SendSetPassWord"
        | "matchsvr.GetChapterInfo"
        | "matchsvr.Remind"
        | "matchsvr.Invite"
        | "matchsvr.RefuseInvite"
        | "matchsvr.AcceptInvite"
        | "matchsvr.GetRoomList"
        | "matchsvr.SwitchRoomPublicState"
        | "matchsvr.Start"
        | "matchsvr.ChangeChapter"
        | "matchsvr.CancelFocus"
        | "matchsvr.SetAutoReady"
        | "matchsvr.CancelAutoReady"
        | "battle.CreateRoom"
        | "battle.JoinRoom"
        | "battle.LeaveRoom"
        | "battle.MatchJoin"
        | "battle.MatchLeave"
        | "battle.SendAutoMsg"
        | "battle.pvpMatchReady"
        | "battle.pvpMatchReadyTimeout"
        | "battle.CreateMutiBattle"
        | "battle.createBattleInfo"
        | "room.StartMatch"
        | "room.StopMatch" => Some(method),
        _ if matches!(
            GameMethod::parse(method).family(),
            MethodFamily::MatchServer | MethodFamily::Room | MethodFamily::Battle
        ) =>
        {
            Some(method)
        }
        _ => None,
    }
}

fn typed_coop_user(
    account: &blueoath_domain::AccountState,
    uid: u64,
    hero_ids: Vec<i32>,
    now: u32,
) -> TypedCoopUser {
    TypedCoopUser {
        uid,
        name: account.character.name.clone(),
        head: account.character.head,
        fashioning: 0,
        enter_time: now,
        hero_ids,
        ..TypedCoopUser::default()
    }
}

fn typed_coop_hero_ids(
    account: &blueoath_domain::AccountState,
    ids: &[u64],
    require_non_empty: bool,
) -> Option<Vec<i32>> {
    if ids.is_empty() && !require_non_empty {
        return Some(
            account
                .fleet
                .fleets
                .values()
                .next()
                .map(|fleet| {
                    fleet
                        .members
                        .iter()
                        .filter_map(|id| i32::try_from(id.get()).ok())
                        .take(6)
                        .collect()
                })
                .unwrap_or_default(),
        );
    }
    if ids.len() > 6 || ids.contains(&0) {
        return None;
    }
    let mut seen = std::collections::BTreeSet::new();
    if ids.iter().any(|id| !seen.insert(*id)) {
        return None;
    }
    ids.iter()
        .all(|id| account.dock.heroes.keys().any(|hero| hero.get() == *id))
        .then(|| {
            ids.iter()
                .filter_map(|id| i32::try_from(*id).ok())
                .collect()
        })
}

pub(crate) fn typed_coop_room_payload(room: &TypedCoopRoom) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, room.room_id);
    append_varint_field(&mut output, 2, room.copy_id.max(0) as u64);
    append_varint_field(&mut output, 3, u64::from(room.is_public));
    append_varint_field(&mut output, 4, room.owner_id);
    for user in &room.users {
        let mut encoded = Vec::new();
        append_varint_field(&mut encoded, 1, user.uid);
        append_bytes_field(&mut encoded, 2, user.name.as_bytes());
        append_varint_field(&mut encoded, 3, u64::from(user.head));
        append_varint_field(&mut encoded, 4, u64::from(user.fashioning));
        append_varint_field(&mut encoded, 5, u64::from(user.is_ready));
        append_varint_field(&mut encoded, 6, u64::from(user.enter_time));
        let mut heroes = Vec::new();
        for hero_id in &user.hero_ids {
            append_varint_field(&mut heroes, 1, (*hero_id).max(0) as u64);
        }
        append_message_field(&mut encoded, 7, &heroes);
        append_message_field(&mut output, 5, &encoded);
    }
    append_varint_field(&mut output, 6, u64::from(room.capacity.max(1)));
    append_varint_field(&mut output, 7, u64::from(room.create_time));
    output
}

fn enqueue_typed_room_push(
    shared: &mut SharedSocialState,
    users: &[TypedCoopUser],
    sender_uid: u64,
    payload: Vec<u8>,
) {
    for recipient_uid in users
        .iter()
        .map(|user| user.uid)
        .filter(|uid| *uid != sender_uid)
    {
        enqueue_shared_push(
            shared,
            recipient_uid,
            "match.UpdateRoomInfo",
            payload.clone(),
        );
    }
}

fn typed_coop_set_state(
    state: &ServerState,
    uid: u64,
    room_id: u64,
    next_state: u32,
    method: &str,
) -> HandlerResult {
    let mut shared = match state.shared_social.lock() {
        Ok(shared) => shared,
        Err(_) => {
            return HandlerResult::Error(GameError::InvalidState("social state is unavailable"))
        }
    };
    let Some(room) = shared.typed_rooms.get_mut(&room_id) else {
        return HandlerResult::Error(GameError::NotFound("co-op room"));
    };
    if room.owner_id != uid {
        return HandlerResult::Error(GameError::InvalidState("only room owner can change state"));
    }
    room.state = next_state;
    let encoded = typed_coop_room_payload(room);
    let users = room.users.clone();
    enqueue_typed_room_push(&mut shared, &users, uid, encoded);
    HandlerResult::Reply(Response::raw(method, Vec::new()))
}

fn typed_coop_set_focus(
    state: &ServerState,
    uid: u64,
    room_id: u64,
    focus: bool,
    method: &str,
) -> HandlerResult {
    let mut shared = match state.shared_social.lock() {
        Ok(shared) => shared,
        Err(_) => {
            return HandlerResult::Error(GameError::InvalidState("social state is unavailable"))
        }
    };
    let Some(room) = shared.typed_rooms.get_mut(&room_id) else {
        return HandlerResult::Error(GameError::NotFound("co-op room"));
    };
    if !room.users.iter().any(|user| user.uid == uid) {
        return HandlerResult::Error(GameError::InvalidState("user is not in co-op room"));
    }
    room.focus = focus;
    HandlerResult::Reply(Response::raw(method, Vec::new()))
}

fn typed_coop_set_auto_ready(
    state: &ServerState,
    uid: u64,
    room_id: u64,
    auto_ready: bool,
    method: &str,
) -> HandlerResult {
    let mut shared = match state.shared_social.lock() {
        Ok(shared) => shared,
        Err(_) => {
            return HandlerResult::Error(GameError::InvalidState("social state is unavailable"))
        }
    };
    let Some(room) = shared.typed_rooms.get_mut(&room_id) else {
        return HandlerResult::Error(GameError::NotFound("co-op room"));
    };
    let Some(user) = room.users.iter_mut().find(|user| user.uid == uid) else {
        return HandlerResult::Error(GameError::InvalidState("user is not in co-op room"));
    };
    user.auto_ready = auto_ready;
    HandlerResult::Reply(Response::raw(method, Vec::new()))
}

fn next_typed_battle_id(shared: &SharedSocialState, now: u32, uid: u64) -> u64 {
    let mut id = u64::from(now % 2_000_000_000).saturating_add(uid.min(999));
    while shared.typed_battle_rooms.contains_key(&id) || shared.typed_battles.contains_key(&id) {
        id = id.saturating_add(1);
    }
    id
}

fn typed_battle_push_payload(port: u16, battle_id: u64, uid: u64) -> Vec<u8> {
    let mut output = Vec::new();
    append_bytes_field(&mut output, 1, b"127.0.0.1");
    append_varint_field(&mut output, 2, u64::from(port));
    append_varint_field(&mut output, 3, battle_id);
    append_bytes_field(&mut output, 4, b"local-battle");
    append_varint_field(&mut output, 6, uid);
    output
}

fn enqueue_shared_push(
    shared: &mut SharedSocialState,
    recipient_uid: u64,
    method: &str,
    payload: Vec<u8>,
) {
    shared
        .pending_pushes
        .entry(recipient_uid)
        .or_default()
        .push((method.to_owned(), payload.clone()));
    let _ = shared.push_tx.send(SharedPush {
        recipient_uid,
        method: method.to_owned(),
        payload,
    });
}

fn drain_shared_pushes(state: &ServerState, uid: u64, effects: &mut ResponseEffects) {
    let pending = state
        .shared_social
        .lock()
        .ok()
        .and_then(|mut shared| shared.pending_pushes.remove(&uid))
        .unwrap_or_default();
    for (method, payload) in pending {
        effects.push_post(Response::raw(method, payload));
    }
}

#[cfg(test)]
mod typed_tests {
    use super::*;
    use blueoath_domain::{FleetId, FleetRecord, NewAccountFactory, ProfileId};

    #[test]
    fn typed_room_lifecycle_uses_transient_typed_state() {
        let state = ServerState::new("coop", "Captain", "test");
        let mut account = NewAccountFactory::create(ProfileId::new("coop").unwrap(), "Captain");
        let hero_id = account.dock.heroes.keys().next().copied().unwrap();
        account.fleet.fleets.insert(
            FleetId::new(1).unwrap(),
            FleetRecord {
                formation_id: 2,
                tactic_id: 1,
                members: vec![hero_id],
            },
        );
        let mut hero_list = Vec::new();
        append_varint_field(&mut hero_list, 1, hero_id.get());
        let mut create = Vec::new();
        append_varint_field(&mut create, COPY_ID_FIELD, 9);
        append_message_field(&mut create, HERO_LIST_FIELD, &hero_list);
        let mut effects = ResponseEffects::default();

        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "matchsvr.CreateRoom",
                &create,
                &mut effects
            ),
            HandlerResult::Reply(_)
        ));
        let room_id = state
            .shared_social
            .lock()
            .unwrap()
            .typed_rooms
            .keys()
            .next()
            .copied()
            .unwrap();
        let mut room_request = Vec::new();
        append_varint_field(&mut room_request, ROOM_ID_FIELD, room_id);
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "matchsvr.Ready",
                &room_request,
                &mut effects
            ),
            HandlerResult::Reply(_)
        ));
        assert!(
            state
                .shared_social
                .lock()
                .unwrap()
                .typed_rooms
                .get(&room_id)
                .unwrap()
                .users[0]
                .is_ready
        );
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "matchsvr.GetRoomList",
                &[],
                &mut effects
            ),
            HandlerResult::Reply(_)
        ));
        assert!(matches!(
            handle_typed(&state, &mut account, "matchsvr_7.Unknown", &[], &mut effects),
            HandlerResult::Error(GameError::InvalidRequest(_))
        ));
    }

    #[test]
    fn typed_battle_control_plane_avoids_json_state() {
        let state = ServerState::new("battle", "Captain", "test");
        let mut first = NewAccountFactory::create(ProfileId::new("battle-1").unwrap(), "First");
        let mut second = NewAccountFactory::create(ProfileId::new("battle-2").unwrap(), "Second");
        second.character.uid = 2;
        let mut effects = ResponseEffects::default();

        assert!(matches!(
            handle_typed(&state, &mut first, "battle.CreateRoom", &[], &mut effects),
            HandlerResult::Reply(_)
        ));
        let room_id = state
            .shared_social
            .lock()
            .unwrap()
            .typed_battle_rooms
            .keys()
            .next()
            .copied()
            .unwrap();
        let mut join = Vec::new();
        append_varint_field(&mut join, ROOM_ID_FIELD, room_id);
        assert!(matches!(
            handle_typed(&state, &mut second, "battle.JoinRoom", &join, &mut effects),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            state
                .shared_social
                .lock()
                .unwrap()
                .typed_battle_rooms
                .get(&room_id)
                .unwrap()
                .users,
            vec![1, 2]
        );
    }
}
