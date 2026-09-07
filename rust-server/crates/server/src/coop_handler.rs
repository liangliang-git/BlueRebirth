use serde_json::{json, Value};

use super::super::config::{
    SharedPush, SharedSocialState, TypedBattleRoom, TypedBattleSession, TypedCoopRoom,
    TypedCoopUser, TypedMatchQueueEntry,
};
use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

const ROOM_ID_FIELD: u8 = 1;
const COPY_ID_FIELD: u8 = 2;
const KICKED_UID_FIELD: u8 = 3;
const HERO_LIST_FIELD: u8 = 4;

pub(super) fn handles_typed(method: &str) -> bool {
    method.starts_with("matchsvr_")
        || method.starts_with("matchsvr.")
        || method.starts_with("room.")
        || method.starts_with("battle.")
        || canonical_typed_method(method).is_some()
}

pub(super) fn handle_typed(
    state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    post_pushes: &mut Vec<Vec<u8>>,
) -> HandlerResult {
    let Some(method) = canonical_typed_method(method) else {
        return HandlerResult::Empty;
    };
    let now = current_unix_seconds();
    let uid = account.character.uid.max(1);
    drain_shared_pushes(state, uid, post_pushes);
    match method {
        "matchsvr.CreateRoom" => {
            let copy_id = decode_varint_field(request_args, COPY_ID_FIELD);
            if copy_id <= 0 {
                return HandlerResult::Error(GameError::InvalidRequest("co-op copy is invalid"));
            }
            let Some(hero_ids) = typed_coop_hero_ids(account, request_args, true) else {
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
                copy_id,
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
            let room_id = decode_varint_u64_field(request_args, ROOM_ID_FIELD);
            let Some(hero_ids) = typed_coop_hero_ids(account, request_args, false) else {
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
            let room_id = decode_varint_u64_field(request_args, ROOM_ID_FIELD);
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
            let room_id = decode_varint_u64_field(request_args, ROOM_ID_FIELD);
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
            let room_id = decode_varint_u64_field(request_args, ROOM_ID_FIELD);
            let kicked_uid = decode_varint_u64_field(request_args, KICKED_UID_FIELD);
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
            let room_id = decode_varint_u64_field(request_args, ROOM_ID_FIELD);
            let Some(hero_ids) = typed_coop_hero_ids(account, request_args, false) else {
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
            let room_id = decode_varint_u64_field(request_args, ROOM_ID_FIELD);
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
            room.password = decode_varint_u64_field(request_args, 2);
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "matchsvr.GetChapterInfo" => {
            let room_id = decode_varint_u64_field(request_args, ROOM_ID_FIELD);
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
            let room_id = decode_varint_u64_field(request_args, ROOM_ID_FIELD);
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
            typed_coop_set_state(state, uid, request_args, 1, method)
        }
        "room.StopMatch" => typed_coop_set_state(state, uid, request_args, 0, method),
        "matchsvr.ChangeChapter" => {
            let room_id = decode_varint_u64_field(request_args, ROOM_ID_FIELD);
            let copy_id = decode_varint_field(request_args, COPY_ID_FIELD);
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
        "matchsvr.CancelFocus" => typed_coop_set_focus(state, uid, request_args, false, method),
        "matchsvr.SetAutoReady" => {
            typed_coop_set_auto_ready(state, uid, request_args, true, method)
        }
        "matchsvr.CancelAutoReady" => {
            typed_coop_set_auto_ready(state, uid, request_args, false, method)
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
            let room_id = decode_varint_u64_field(request_args, ROOM_ID_FIELD);
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
            let match_type = decode_varint_field(request_args, 1).max(0) as u32;
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
            let match_type = decode_varint_field(request_args, 1).max(0) as u32;
            if let Ok(mut shared) = state.shared_social.lock() {
                shared
                    .typed_match_queue
                    .retain(|entry| entry.uid != uid || entry.match_type != match_type);
            }
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "battle.SendAutoMsg" => {
            let msg_id = decode_varint_field(request_args, 1).max(0);
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
            append_method_push(post_pushes, "battle.receiveAutoMsg", payload);
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
        _ if method.starts_with("matchsvr.")
            || method.starts_with("room.")
            || method.starts_with("battle.") =>
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
    payload: &[u8],
    require_non_empty: bool,
) -> Option<Vec<i32>> {
    let ids = decode_pve_hero_ids(payload);
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
    if ids.len() > 6 || ids.iter().any(|id| *id <= 0) {
        return None;
    }
    let mut seen = std::collections::BTreeSet::new();
    if ids.iter().any(|id| !seen.insert(*id)) {
        return None;
    }
    ids.iter()
        .all(|id| {
            account
                .dock
                .heroes
                .keys()
                .any(|hero| hero.get() == *id as u64)
        })
        .then_some(ids)
}

fn typed_coop_room_payload(room: &TypedCoopRoom) -> Vec<u8> {
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
    payload: &[u8],
    next_state: u32,
    method: &str,
) -> HandlerResult {
    let room_id = decode_varint_u64_field(payload, ROOM_ID_FIELD);
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
    payload: &[u8],
    focus: bool,
    method: &str,
) -> HandlerResult {
    let room_id = decode_varint_u64_field(payload, ROOM_ID_FIELD);
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
    payload: &[u8],
    auto_ready: bool,
    method: &str,
) -> HandlerResult {
    let room_id = decode_varint_u64_field(payload, ROOM_ID_FIELD);
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

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let payload = handle_legacy(context, method, request_args);
    if let Some(error) = context.handler_error.clone() {
        HandlerResult::Error(error)
    } else {
        match payload {
            Some(payload) => HandlerResult::Reply(Response::raw(method, payload)),
            None => HandlerResult::Empty,
        }
    }
}

fn handle_legacy<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    // RoomService scopes its RPC method by zone: matchsvr_<zone>.<action>. The
    // action protocol is identical to matchsvr.<action>; normalize before
    // entering stateful handling so dynamic client routes do not fall through.
    if let Some(suffix) = method
        .strip_prefix("matchsvr_")
        .and_then(|scoped| scoped.find('.').map(|index| &scoped[index..]))
    {
        let canonical = match suffix {
            ".CreateRoom" => "matchsvr.CreateRoom",
            ".EnterRoom" => "matchsvr.EnterRoom",
            ".ExitRoom" => "matchsvr.ExitRoom",
            ".Tactic" => "matchsvr.UploadTactic",
            ".Deliver" => "matchsvr.Deliver",
            ".SendSetPassWord" => "matchsvr.SendSetPassWord",
            ".Kick" => "matchsvr.Kick",
            ".GetChapterInfo" => "matchsvr.GetChapterInfo",
            ".CancelFocus" => "matchsvr.CancelFocus",
            ".Leave" => "matchsvr.ExitRoom",
            ".Invite" => "matchsvr.Invite",
            ".RefuseInvite" => "matchsvr.RefuseInvite",
            ".AcceptInvite" => "matchsvr.AcceptInvite",
            ".ChangeChapter" => "matchsvr.ChangeChapter",
            ".Remind" => "matchsvr.Remind",
            ".Ready" => "matchsvr.Ready",
            ".Cancel" => "matchsvr.Cancel",
            ".SetAutoReady" => "matchsvr.SetAutoReady",
            ".CancelAutoReady" => "matchsvr.CancelAutoReady",
            _ => return None,
        };
        return handle_legacy(context, canonical, request_args);
    }

    let account = context.account.as_deref_mut()?;
    let now = current_unix_seconds();
    let uid = account
        .get("character")
        .and_then(|value| json_i64(value, "uid"))
        .unwrap_or(1)
        .max(1) as u64;
    let room_id = decode_varint_u64_field(request_args, ROOM_ID_FIELD);
    drain_shared_pushes(context.state, uid, context.post_pushes);
    hydrate_shared_room(context.state, account);

    match method {
        "matchsvr.CreateRoom" => {
            let copy_id = decode_varint_field(request_args, COPY_ID_FIELD);
            if copy_id <= 0 {
                *context.handler_error =
                    Some(GameError::Internal("co-op copy is invalid".to_owned()));
                return Some(Vec::new());
            }
            let hero_ids = decode_pve_hero_ids(request_args);
            let generated_room_id = u64::from(now % 2_000_000_000).saturating_add(uid.min(999));
            account["pveRoom"] = json!({
                "roomId": generated_room_id,
                "copyId": copy_id,
                "ownerId": uid,
                "isPublic": true,
                "capacity": 2,
                "createTime": now,
                "users": [{
                    "uid": uid,
                    "name": account_name(account),
                    "head": character_i64(account, "head"),
                    "fashioning": first_hero_fashioning(account),
                    "isReady": false,
                    "enterTime": now,
                    "heroIds": hero_ids
                }]
            });
            sync_shared_room(context.state, account);
            let payload = pve_room_payload(account);
            append_method_push(context.post_pushes, "match.UpdateRoomInfo", payload.clone());
            Some(payload)
        }
        "matchsvr.EnterRoom" => {
            let Some(mut room) = shared_room(context.state, room_id) else {
                *context.handler_error = Some(GameError::Internal("room is not found".to_owned()));
                return Some(Vec::new());
            };
            let capacity = room
                .get("capacity")
                .and_then(Value::as_i64)
                .unwrap_or(2)
                .max(1) as usize;
            let users = room
                .get_mut("users")
                .and_then(Value::as_array_mut)
                .expect("room users must be an array");
            if users
                .iter()
                .all(|user| user.get("uid").and_then(Value::as_u64) != Some(uid))
            {
                if users.len() >= capacity {
                    *context.handler_error = Some(GameError::Internal("room is full".to_owned()));
                    return Some(Vec::new());
                }
                users.push(room_user_from_account(account, uid, request_args, now));
            } else {
                update_room_user_in_list(users, uid, request_args);
            }
            account["pveRoom"] = room;
            sync_shared_room(context.state, account);
            let payload = pve_room_payload(account);
            append_method_push(context.post_pushes, "match.UpdateRoomInfo", payload);
            Some(Vec::new())
        }
        "matchsvr.ExitRoom" => {
            if room_matches(account, room_id) {
                leave_shared_room(context.state, account, room_id, uid);
            }
            Some(Vec::new())
        }
        "matchsvr.DismissRoom" => {
            if !room_matches(account, room_id) {
                *context.handler_error = Some(GameError::Internal("room is not found".to_owned()));
                return Some(Vec::new());
            }
            let owner_id = account
                .get("pveRoom")
                .and_then(|room| room.get("ownerId"))
                .and_then(Value::as_u64)
                .unwrap_or_default();
            if owner_id != uid {
                *context.handler_error = Some(GameError::Internal(
                    "only room owner can dismiss".to_owned(),
                ));
                return Some(Vec::new());
            }
            leave_shared_room(context.state, account, room_id, uid);
            Some(Vec::new())
        }
        "matchsvr.Ready" | "matchsvr.Cancel" => {
            if !room_matches(account, room_id) {
                *context.handler_error = Some(GameError::Internal("room is not found".to_owned()));
                return Some(Vec::new());
            }
            let ready = method == "matchsvr.Ready";
            set_room_user_ready(account, uid, ready);
            sync_shared_room(context.state, account);
            let payload = pve_room_payload(account);
            append_method_push(context.post_pushes, "match.UpdateRoomInfo", payload);
            Some(Vec::new())
        }
        "matchsvr.Kick" => {
            let kicked_uid = decode_varint_u64_field(request_args, KICKED_UID_FIELD);
            let owner_id = account
                .get("pveRoom")
                .and_then(|room| room.get("ownerId"))
                .and_then(Value::as_u64)
                .unwrap_or_default();
            if owner_id != uid || kicked_uid == 0 || kicked_uid == uid {
                *context.handler_error = Some(GameError::Internal(
                    "only room owner can kick another user".to_owned(),
                ));
                return Some(Vec::new());
            }
            if let Some(users) = account
                .get_mut("pveRoom")
                .and_then(|room| room.get_mut("users"))
                .and_then(Value::as_array_mut)
            {
                users.retain(|user| user.get("uid").and_then(Value::as_u64) != Some(kicked_uid));
            }
            sync_shared_room(context.state, account);
            Some(Vec::new())
        }
        "matchsvr.UploadTactic" => {
            let hero_ids = decode_pve_hero_ids(request_args);
            if account.get("pveRoom").and_then(Value::as_object).is_none()
                || !valid_pve_hero_ids(account, &hero_ids)
            {
                *context.handler_error =
                    Some(GameError::Internal("co-op fleet is invalid".to_owned()));
                return Some(Vec::new());
            }
            if let Some(user) = room_user_mut(account, uid) {
                user["heroIds"] = json!(hero_ids);
            }
            sync_shared_room(context.state, account);
            append_method_push(
                context.post_pushes,
                "match.UpdateRoomInfo",
                pve_room_payload(account),
            );
            Some(Vec::new())
        }
        "matchsvr.GetRoomList" => {
            let mut output = Vec::new();
            let mut rooms = shared_rooms(context.state);
            if rooms.is_empty() {
                if let Some(room) = account.get("pveRoom").filter(|room| room.is_object()) {
                    rooms.push(room.clone());
                }
            }
            for room in rooms.into_iter().filter(|room| {
                room.get("isPublic")
                    .and_then(Value::as_bool)
                    .unwrap_or(true)
            }) {
                append_message_field(&mut output, 1, &pve_room_payload_from_room(&room));
            }
            Some(output)
        }
        "matchsvr.SwitchRoomPublicState" => {
            let owner_id = account
                .get("pveRoom")
                .and_then(|room| room.get("ownerId"))
                .and_then(Value::as_u64)
                .unwrap_or_default();
            if owner_id != uid {
                *context.handler_error = Some(GameError::Internal(
                    "only room owner can change visibility".to_owned(),
                ));
                return Some(Vec::new());
            }
            if let Some(room) = account.get_mut("pveRoom").and_then(Value::as_object_mut) {
                let current = room
                    .get("isPublic")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                room.insert("isPublic".to_owned(), json!(!current));
            }
            sync_shared_room(context.state, account);
            Some(Vec::new())
        }
        "matchsvr.Start" => {
            if !room_matches(account, room_id) {
                *context.handler_error = Some(GameError::Internal("room is not found".to_owned()));
                return Some(Vec::new());
            }
            let owner_id = account
                .get("pveRoom")
                .and_then(|room| room.get("ownerId"))
                .and_then(Value::as_u64)
                .unwrap_or_default();
            if owner_id != uid {
                *context.handler_error =
                    Some(GameError::Internal("only room owner can start".to_owned()));
                return Some(Vec::new());
            }
            if let Some(room) = account.get_mut("pveRoom").and_then(Value::as_object_mut) {
                room.insert("state".to_owned(), json!("fighting"));
            }
            sync_shared_room(context.state, account);
            Some(pve_room_payload(account))
        }
        "battle.CreateRoom" => {
            let battle_room_id = local_room_id(now, uid);
            let battle_room = json!({
                "roomId": battle_room_id,
                "ownerId": uid,
                "createdAt": now,
                "matchType": 0,
                "users": [uid]
            });
            account["battleRoom"] = battle_room.clone();
            sync_shared_battle(context.state, battle_room_id, battle_room);
            Some(battle_room_ret(battle_room_id))
        }
        "battle.JoinRoom" => {
            let requested_room_id = decode_varint_u64_field(request_args, ROOM_ID_FIELD);
            if requested_room_id == 0 {
                *context.handler_error =
                    Some(GameError::Internal("battle room is invalid".to_owned()));
                return Some(Vec::new());
            }
            let Some(mut battle_room) = shared_battle(context.state, requested_room_id) else {
                *context.handler_error =
                    Some(GameError::Internal("battle room is not found".to_owned()));
                return Some(Vec::new());
            };
            let users = battle_room
                .get_mut("users")
                .and_then(Value::as_array_mut)
                .expect("battle room users must be an array");
            if !users.iter().any(|value| value.as_u64() == Some(uid)) {
                if users.len() >= 2 {
                    *context.handler_error =
                        Some(GameError::Internal("battle room is full".to_owned()));
                    return Some(Vec::new());
                }
                users.push(json!(uid));
            }
            account["battleRoom"] = battle_room.clone();
            sync_shared_battle(context.state, requested_room_id, battle_room);
            Some(Vec::new())
        }
        "battle.LeaveRoom" => {
            let battle_room_id = account
                .get("battleRoom")
                .and_then(|room| room.get("roomId"))
                .and_then(Value::as_u64)
                .unwrap_or_default();
            if battle_room_id > 0 {
                leave_shared_battle(context.state, battle_room_id, uid);
            }
            account["battleRoom"] = Value::Null;
            Some(Vec::new())
        }
        "battle.MatchJoin" => {
            let match_type = i64::from(decode_varint_field(request_args, 1).max(0));
            let queued = json!({
                "uid": uid,
                "matchType": match_type,
                "time": now
            });
            let opponent = if let Ok(mut shared) = context.state.shared_social.lock() {
                shared.match_queue.retain(|entry| {
                    entry.get("uid").and_then(Value::as_u64) != Some(uid)
                        || entry.get("matchType").and_then(Value::as_i64) != Some(match_type)
                });
                let index = shared.match_queue.iter().position(|entry| {
                    entry.get("uid").and_then(Value::as_u64) != Some(uid)
                        && entry.get("matchType").and_then(Value::as_i64) == Some(match_type)
                });
                index
                    .and_then(|index| shared.match_queue.get(index).cloned())
                    .inspect(|_| {
                        if let Some(index) = index {
                            shared.match_queue.remove(index);
                        }
                    })
                    .or_else(|| {
                        shared.match_queue.push(queued.clone());
                        None
                    })
            } else {
                None
            };
            if let Some(opponent) = opponent {
                let opponent_uid = opponent
                    .get("uid")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                let payload = battle_match_ret_payload(&[opponent_uid, uid]);
                account["battleMatch"] = json!({
                    "matchType": match_type,
                    "state": "matched",
                    "time": now,
                    "opponentUid": opponent_uid
                });
                if let Ok(mut shared) = context.state.shared_social.lock() {
                    enqueue_shared_push(
                        &mut shared,
                        opponent_uid,
                        "battle.MatchJoin",
                        payload.clone(),
                    );
                }
                Some(payload)
            } else {
                account["battleMatch"] = json!({
                    "matchType": match_type,
                    "state": "queued",
                    "time": now
                });
                Some(battle_match_ret_payload(&[uid]))
            }
        }
        "battle.MatchLeave" => {
            let match_type = account
                .get("battleMatch")
                .and_then(|value| value.get("matchType"))
                .and_then(Value::as_i64);
            if let Ok(mut shared) = context.state.shared_social.lock() {
                shared.match_queue.retain(|entry| {
                    entry.get("uid").and_then(Value::as_u64) != Some(uid)
                        || match_type
                            .map(|value| {
                                entry.get("matchType").and_then(Value::as_i64) != Some(value)
                            })
                            .unwrap_or(true)
                });
            }
            account["battleMatch"] = Value::Null;
            Some(Vec::new())
        }
        "battle.SendAutoMsg" => {
            let msg_id = decode_varint_field(request_args, 1).max(0);
            let mut output = Vec::new();
            append_varint_field(&mut output, 1, 0);
            let battle_room_id = account
                .get("battleRoom")
                .and_then(|room| room.get("roomId"))
                .and_then(Value::as_u64)
                .unwrap_or_default();
            if battle_room_id > 0 {
                queue_battle_push(
                    context.state,
                    battle_room_id,
                    uid,
                    battle_auto_msg_payload(uid, msg_id),
                );
            }
            append_method_push(
                context.post_pushes,
                "battle.receiveAutoMsg",
                battle_auto_msg_payload(uid, msg_id),
            );
            Some(output)
        }
        "battle.pvpMatchReady" => {
            let room_id = pvp_match_room_id(account, now, uid);
            account["pvpMatch"] = json!({
                "roomId": room_id,
                "state": "ready",
                "time": now,
            });
            Some(pvp_match_ready_payload(room_id))
        }
        "battle.pvpMatchReadyTimeout" => {
            let room_id = pvp_match_room_id(account, now, uid);
            account["pvpMatch"] = json!({
                "roomId": room_id,
                "state": "timeout",
                "time": now,
            });
            Some(pvp_match_ready_payload(room_id))
        }
        "battle.CreateMutiBattle" => {
            let battle_id = local_room_id(now, uid);
            let battle_session = json!({
                "battleId": battle_id,
                "time": now,
                "args": request_args,
                "users": [uid]
            });
            account["battleSession"] = battle_session.clone();
            sync_shared_battle(context.state, battle_id, battle_session);
            Some(battle_create_multi_ret(
                context.state.battle_port,
                battle_id,
                request_args,
            ))
        }
        "battle.createBattleInfo" => Some(battle_push_payload(context.state.battle_port, account)),
        "room.StartMatch" => {
            if let Some(room) = account.get_mut("pveRoom").and_then(Value::as_object_mut) {
                room.insert("state".to_owned(), json!("fighting"));
            }
            sync_shared_room(context.state, account);
            Some(pve_room_payload(account))
        }
        "room.StopMatch" => {
            if let Some(room) = account.get_mut("pveRoom").and_then(Value::as_object_mut) {
                room.insert("state".to_owned(), json!("idle"));
            }
            sync_shared_room(context.state, account);
            Some(Vec::new())
        }
        "matchsvr.Deliver" => {
            if room_matches(account, room_id) {
                update_room_user(account, uid, request_args, now);
                sync_shared_room(context.state, account);
                append_method_push(
                    context.post_pushes,
                    "match.UpdateRoomInfo",
                    pve_room_payload(account),
                );
            }
            Some(Vec::new())
        }
        "matchsvr.SendSetPassWord" => {
            if room_matches(account, room_id) {
                if let Some(room) = account.get_mut("pveRoom").and_then(Value::as_object_mut) {
                    room.insert(
                        "password".to_owned(),
                        json!(decode_varint_field(request_args, 2).max(0)),
                    );
                }
                sync_shared_room(context.state, account);
            }
            Some(Vec::new())
        }
        "matchsvr.GetChapterInfo" => {
            if room_matches(account, room_id) {
                Some(pve_room_payload(account))
            } else {
                Some(Vec::new())
            }
        }
        "matchsvr.CancelFocus" => {
            if room_matches(account, room_id) {
                if let Some(room) = account.get_mut("pveRoom").and_then(Value::as_object_mut) {
                    room.insert("focus".to_owned(), json!(false));
                }
                sync_shared_room(context.state, account);
            }
            Some(Vec::new())
        }
        "matchsvr.Leave" => {
            if room_matches(account, room_id) {
                leave_shared_room(context.state, account, room_id, uid);
            }
            Some(Vec::new())
        }
        "matchsvr.Invite" | "matchsvr.RefuseInvite" | "matchsvr.AcceptInvite" => {
            account["lastRoomInvite"] = json!({
                "method": method,
                "args": request_args,
                "time": now
            });
            Some(Vec::new())
        }
        "matchsvr.ChangeChapter" => {
            if room_matches(account, room_id) {
                if let Some(room) = account.get_mut("pveRoom").and_then(Value::as_object_mut) {
                    let copy_id = decode_varint_field(request_args, COPY_ID_FIELD);
                    if copy_id > 0 {
                        room.insert("copyId".to_owned(), json!(copy_id));
                    }
                }
                sync_shared_room(context.state, account);
            }
            Some(Vec::new())
        }
        "matchsvr.Remind" => {
            account["lastRoomRemind"] = json!({"args": request_args, "time": now});
            Some(Vec::new())
        }
        "matchsvr.SetAutoReady" | "matchsvr.CancelAutoReady" => {
            if room_matches(account, room_id) {
                if let Some(user) = room_user_mut(account, uid) {
                    user["autoReady"] = json!(method == "matchsvr.SetAutoReady");
                }
                sync_shared_room(context.state, account);
            }
            Some(Vec::new())
        }
        _ => None,
    }
}

fn local_room_id(now: u32, uid: u64) -> u64 {
    u64::from(now % 2_000_000_000).saturating_add(uid.min(999))
}

fn battle_room_ret(room_id: u64) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, room_id);
    output
}

fn pvp_match_room_id(account: &Value, now: u32, uid: u64) -> u64 {
    account
        .get("pvpMatch")
        .and_then(|value| value.get("roomId"))
        .and_then(Value::as_u64)
        .or_else(|| {
            account
                .get("battleRoom")
                .and_then(|value| value.get("roomId"))
                .and_then(Value::as_u64)
        })
        .unwrap_or_else(|| local_room_id(now, uid))
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

fn battle_push_payload(battle_port: u16, account: &Value) -> Vec<u8> {
    let battle_id = account
        .get("battleSession")
        .and_then(|value| value.get("battleId"))
        .and_then(Value::as_u64)
        .or_else(|| {
            account
                .get("battleRoom")
                .and_then(|value| value.get("roomId"))
                .and_then(Value::as_u64)
        })
        .unwrap_or_default();
    let uid = account
        .get("character")
        .and_then(|value| value.get("uid"))
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let mut output = Vec::new();
    append_bytes_field(&mut output, 1, b"127.0.0.1");
    append_varint_field(&mut output, 2, u64::from(battle_port));
    append_varint_field(&mut output, 3, battle_id);
    append_bytes_field(&mut output, 4, b"local-battle");
    append_varint_field(&mut output, 6, uid);
    output
}

fn shared_room(state: &ServerState, room_id: u64) -> Option<Value> {
    state
        .shared_social
        .lock()
        .ok()?
        .rooms
        .get(&room_id)
        .cloned()
}

fn shared_rooms(state: &ServerState) -> Vec<Value> {
    state
        .shared_social
        .lock()
        .map(|shared| shared.rooms.values().cloned().collect())
        .unwrap_or_default()
}

fn shared_battle(state: &ServerState, battle_id: u64) -> Option<Value> {
    state
        .shared_social
        .lock()
        .ok()?
        .battles
        .get(&battle_id)
        .cloned()
}

fn sync_shared_battle(state: &ServerState, battle_id: u64, battle: Value) {
    if let Ok(mut shared) = state.shared_social.lock() {
        shared.battles.insert(battle_id, battle);
    }
}

fn leave_shared_battle(state: &ServerState, battle_id: u64, uid: u64) {
    if let Ok(mut shared) = state.shared_social.lock() {
        let mut remove = false;
        if let Some(battle) = shared.battles.get_mut(&battle_id) {
            let owner_id = battle
                .get("ownerId")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            if owner_id == uid {
                remove = true;
            } else if let Some(users) = battle.get_mut("users").and_then(Value::as_array_mut) {
                users.retain(|value| value.as_u64() != Some(uid));
            }
        }
        if remove {
            shared.battles.remove(&battle_id);
        }
    }
}

fn queue_battle_push(state: &ServerState, battle_id: u64, sender_uid: u64, payload: Vec<u8>) {
    if let Ok(mut shared) = state.shared_social.lock() {
        let recipients = shared
            .battles
            .get(&battle_id)
            .and_then(|battle| battle.get("users"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for recipient_uid in recipients
            .into_iter()
            .filter_map(|value| value.as_u64())
            .filter(|recipient_uid| *recipient_uid != sender_uid)
        {
            enqueue_shared_push(
                &mut shared,
                recipient_uid,
                "battle.receiveAutoMsg",
                payload.clone(),
            );
        }
    }
}

fn sync_shared_room(state: &ServerState, account: &Value) {
    let Some(room) = account.get("pveRoom").filter(|room| room.is_object()) else {
        return;
    };
    let Some(room_id) = room.get("roomId").and_then(Value::as_u64) else {
        return;
    };
    if let Ok(mut shared) = state.shared_social.lock() {
        shared.rooms.insert(room_id, room.clone());
        let sender_uid = account_uid(account);
        let payload = pve_room_payload_from_room(room);
        let recipients = room
            .get("users")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|user| user.get("uid").and_then(Value::as_u64))
            .filter(|recipient_uid| *recipient_uid != sender_uid);
        for recipient_uid in recipients {
            enqueue_shared_push(
                &mut shared,
                recipient_uid,
                "match.UpdateRoomInfo",
                payload.clone(),
            );
        }
    }
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

fn drain_shared_pushes(state: &ServerState, uid: u64, pushes: &mut Vec<Vec<u8>>) {
    let pending = state
        .shared_social
        .lock()
        .ok()
        .and_then(|mut shared| shared.pending_pushes.remove(&uid))
        .unwrap_or_default();
    for (method, payload) in pending {
        append_method_push(pushes, &method, payload);
    }
}

fn account_uid(account: &Value) -> u64 {
    account
        .get("character")
        .and_then(|value| json_u64(value, "uid"))
        .filter(|uid| *uid > 0)
        .unwrap_or(1)
}

fn hydrate_shared_room(state: &ServerState, account: &mut Value) {
    let Some(room_id) = account
        .get("pveRoom")
        .and_then(|room| room.get("roomId"))
        .and_then(Value::as_u64)
    else {
        return;
    };
    if let Some(room) = shared_room(state, room_id) {
        account["pveRoom"] = room;
    } else {
        sync_shared_room(state, account);
    }
}

fn leave_shared_room(state: &ServerState, account: &mut Value, room_id: u64, uid: u64) {
    if let Ok(mut shared) = state.shared_social.lock() {
        let mut remove_room = false;
        if let Some(room) = shared.rooms.get_mut(&room_id) {
            let owner_id = room
                .get("ownerId")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            if owner_id == uid {
                remove_room = true;
            } else if let Some(users) = room.get_mut("users").and_then(Value::as_array_mut) {
                users.retain(|user| user.get("uid").and_then(Value::as_u64) != Some(uid));
            }
        }
        if remove_room {
            shared.rooms.remove(&room_id);
        }
    }
    account["pveRoom"] = Value::Null;
}

fn room_user_from_account(account: &Value, uid: u64, request_args: &[u8], now: u32) -> Value {
    json!({
        "uid": uid,
        "name": account_name(account),
        "head": character_i64(account, "head"),
        "fashioning": first_hero_fashioning(account),
        "isReady": false,
        "enterTime": now,
        "heroIds": decode_pve_hero_ids(request_args)
    })
}

fn update_room_user_in_list(users: &mut [Value], uid: u64, request_args: &[u8]) {
    if let Some(user) = users
        .iter_mut()
        .find(|user| user.get("uid").and_then(Value::as_u64) == Some(uid))
    {
        user["heroIds"] = json!(decode_pve_hero_ids(request_args));
    }
}

pub(crate) fn pve_room_payload(account: &Value) -> Vec<u8> {
    let Some(room) = account.get("pveRoom").and_then(Value::as_object) else {
        return Vec::new();
    };
    pve_room_payload_from_room(&Value::Object(room.clone()))
}

fn pve_room_payload_from_room(room_value: &Value) -> Vec<u8> {
    let Some(room) = room_value.as_object() else {
        return Vec::new();
    };
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        room.get("roomId")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        2,
        room.get("copyId")
            .and_then(Value::as_i64)
            .unwrap_or_default()
            .max(0) as u64,
    );
    append_varint_field(
        &mut output,
        3,
        u64::from(
            room.get("isPublic")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        ),
    );
    append_varint_field(
        &mut output,
        4,
        room.get("ownerId")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
    );
    for user in room
        .get("users")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut encoded = Vec::new();
        append_varint_field(
            &mut encoded,
            1,
            user.get("uid").and_then(Value::as_u64).unwrap_or_default(),
        );
        append_bytes_field(
            &mut encoded,
            2,
            user.get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .as_bytes(),
        );
        append_varint_field(
            &mut encoded,
            3,
            user.get("head")
                .and_then(Value::as_i64)
                .unwrap_or_default()
                .max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            4,
            user.get("fashioning")
                .and_then(Value::as_i64)
                .unwrap_or_default()
                .max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            5,
            u64::from(
                user.get("isReady")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
        );
        append_varint_field(
            &mut encoded,
            6,
            user.get("enterTime")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
        );
        let mut heroes = Vec::new();
        for hero_id in user
            .get("heroIds")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_i64)
        {
            append_varint_field(&mut heroes, 1, hero_id.max(0) as u64);
        }
        append_message_field(&mut encoded, 7, &heroes);
        append_message_field(&mut output, 5, &encoded);
    }
    append_varint_field(
        &mut output,
        6,
        room.get("capacity")
            .and_then(Value::as_i64)
            .unwrap_or(2)
            .max(1) as u64,
    );
    append_varint_field(
        &mut output,
        7,
        room.get("createTime")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
    );
    output
}

fn room_matches(account: &Value, room_id: u64) -> bool {
    room_id > 0
        && account
            .get("pveRoom")
            .and_then(|room| room.get("roomId"))
            .and_then(Value::as_u64)
            == Some(room_id)
}

fn room_user_mut(account: &mut Value, uid: u64) -> Option<&mut Value> {
    account
        .get_mut("pveRoom")?
        .get_mut("users")?
        .as_array_mut()?
        .iter_mut()
        .find(|user| user.get("uid").and_then(Value::as_u64) == Some(uid))
}

fn set_room_user_ready(account: &mut Value, uid: u64, ready: bool) {
    if let Some(user) = room_user_mut(account, uid) {
        user["isReady"] = json!(ready);
    }
}

fn update_room_user(account: &mut Value, uid: u64, request_args: &[u8], now: u32) {
    let hero_ids = decode_pve_hero_ids(request_args);
    let name = account_name(account);
    let head = character_i64(account, "head");
    let fashioning = first_hero_fashioning(account);
    if let Some(user) = room_user_mut(account, uid) {
        user["heroIds"] = json!(hero_ids);
        return;
    }
    if let Some(users) = account
        .get_mut("pveRoom")
        .and_then(|room| room.get_mut("users"))
        .and_then(Value::as_array_mut)
    {
        users.push(json!({
            "uid": uid,
            "name": name,
            "head": head,
            "fashioning": fashioning,
            "isReady": false,
            "enterTime": now,
            "heroIds": hero_ids
        }));
    }
}

fn decode_pve_hero_ids(payload: &[u8]) -> Vec<i32> {
    decode_repeated_message_field(payload, HERO_LIST_FIELD)
        .into_iter()
        .flat_map(|list| decode_repeated_varint_field(&list, 1))
        .filter(|id| *id > 0)
        .take(6)
        .collect()
}

fn valid_pve_hero_ids(account: &Value, hero_ids: &[i32]) -> bool {
    if hero_ids.len() > 6 || hero_ids.iter().any(|id| *id <= 0) {
        return false;
    }
    if hero_ids
        .iter()
        .enumerate()
        .any(|(index, id)| hero_ids[index + 1..].contains(id))
    {
        return false;
    }
    let owned = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|hero| json_i32(hero, "heroId"))
        .collect::<std::collections::BTreeSet<_>>();
    hero_ids.iter().all(|id| owned.contains(id))
}

fn account_name(account: &Value) -> String {
    account
        .get("character")
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn character_i64(account: &Value, key: &str) -> u64 {
    account
        .get("character")
        .and_then(|value| json_i64(value, key))
        .unwrap_or_default()
        .max(0) as u64
}

fn first_hero_fashioning(account: &Value) -> u64 {
    account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .and_then(|heroes| heroes.first())
        .and_then(|hero| json_i64(hero, "fashioning"))
        .unwrap_or_default()
        .max(0) as u64
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
        let mut pushes = Vec::new();

        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "matchsvr.CreateRoom",
                &create,
                &mut pushes
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
                &mut pushes
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
                &mut pushes
            ),
            HandlerResult::Reply(_)
        ));
        assert!(matches!(
            handle_typed(&state, &mut account, "matchsvr_7.Unknown", &[], &mut pushes),
            HandlerResult::Error(GameError::InvalidRequest(_))
        ));
    }

    #[test]
    fn typed_battle_control_plane_avoids_json_state() {
        let state = ServerState::new("battle", "Captain", "test");
        let mut first = NewAccountFactory::create(ProfileId::new("battle-1").unwrap(), "First");
        let mut second = NewAccountFactory::create(ProfileId::new("battle-2").unwrap(), "Second");
        second.character.uid = 2;
        let mut pushes = Vec::new();

        assert!(matches!(
            handle_typed(&state, &mut first, "battle.CreateRoom", &[], &mut pushes),
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
            handle_typed(&state, &mut second, "battle.JoinRoom", &join, &mut pushes),
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
