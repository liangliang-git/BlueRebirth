use serde_json::{json, Value};

use super::super::config::{SharedPush, SharedSocialState};
use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

const ROOM_ID_FIELD: u8 = 1;
const COPY_ID_FIELD: u8 = 2;
const KICKED_UID_FIELD: u8 = 3;
const HERO_LIST_FIELD: u8 = 4;

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
