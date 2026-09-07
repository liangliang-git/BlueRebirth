use serde_json::json;

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(super) fn handle_typed(
    state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    match method {
        "teachingsvr.TeacherList"
        | "teachingsvr.MyStudent"
        | "teachingsvr.StudentList"
        | "teachingsvr.ApplyList"
        | "teachingsvr.Search" => reply(method, teaching_list_payload()),
        "teachingsvr.MyTeacher" => reply(method, teaching_user_payload_typed(state, account)),
        "teachingsvr.GetOtherInfo" => reply(
            method,
            teaching_other_user_payload_typed(
                state,
                account,
                decode_varint_u64_field(request_args, 1),
            ),
        ),
        "teachingsvr.TaskReward" => {
            let mut output = Vec::new();
            append_varint_field(
                &mut output,
                1,
                decode_varint_field(request_args, 1).max(0) as u64,
            );
            reply(method, output)
        }
        "teachingsvr.Apply"
        | "teachingsvr.Agree"
        | "teachingsvr.Refuse"
        | "teachingsvr.Delete"
        | "teachingsvr.Appraise"
        | "teachingsvr.PersonalInfo" => HandlerResult::PushOnly,
        _ => HandlerResult::Empty,
    }
}

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let state = context.state;
    let account = &mut *context.account;
    let current = account.as_deref().unwrap_or(&serde_json::Value::Null);

    match method {
        "teachingsvr.TeacherList"
        | "teachingsvr.MyStudent"
        | "teachingsvr.StudentList"
        | "teachingsvr.ApplyList" => reply(method, teaching_list_payload()),
        "teachingsvr.MyTeacher" => reply(method, teaching_user_payload(state, current)),
        "teachingsvr.Search" => reply(method, teaching_list_payload()),
        "teachingsvr.GetOtherInfo" => reply(
            method,
            teaching_other_user_payload(state, current, decode_varint_u64_field(request_args, 1)),
        ),
        "teachingsvr.TaskReward" => {
            let mut output = Vec::new();
            append_varint_field(
                &mut output,
                1,
                decode_varint_field(request_args, 1).max(0) as u64,
            );
            reply(method, output)
        }
        "teachingsvr.Apply"
        | "teachingsvr.Agree"
        | "teachingsvr.Refuse"
        | "teachingsvr.Delete"
        | "teachingsvr.Appraise"
        | "teachingsvr.PersonalInfo" => {
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            account["lastTeachingAction"] = json!({
                "method": method,
                "args": request_args,
                "time": current_unix_seconds(),
            });
            HandlerResult::PushOnly
        }
        _ => HandlerResult::Empty,
    }
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn teaching_user_payload_typed(
    state: &ServerState,
    account: &blueoath_domain::AccountState,
) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, account.character.uid);
    append_varint_field(&mut output, 7, 0);
    append_varint_field(&mut output, 8, account.character.create_time);
    append_message_field(
        &mut output,
        10,
        &typed_other_user_payload(state, account, account.character.uid),
    );
    append_varint_field(&mut output, 13, 1);
    append_varint_field(&mut output, 14, 1);
    output
}

fn teaching_other_user_payload_typed(
    state: &ServerState,
    account: &blueoath_domain::AccountState,
    requested_uid: u64,
) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, requested_uid.max(1));
    append_message_field(
        &mut output,
        10,
        &typed_other_user_payload(state, account, requested_uid),
    );
    append_varint_field(&mut output, 13, 1);
    append_varint_field(&mut output, 14, 1);
    output
}

fn typed_other_user_payload(
    _state: &ServerState,
    account: &blueoath_domain::AccountState,
    uid: u64,
) -> Vec<u8> {
    let character = &account.character;
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, uid.max(1));
    append_bytes_field(&mut output, 2, character.name.as_bytes());
    append_varint_field(&mut output, 3, u64::from(character.head));
    append_varint_field(&mut output, 5, u64::from(character.level));
    append_varint_field(
        &mut output,
        10,
        character.secretary_id.map(|id| id.get()).unwrap_or(1),
    );
    output
}

fn teaching_list_payload() -> Vec<u8> {
    Vec::new()
}

fn teaching_user_payload(state: &ServerState, account: &serde_json::Value) -> Vec<u8> {
    let character = account.get("character").unwrap_or(&serde_json::Value::Null);
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, json_u64(character, "uid").unwrap_or(1));
    append_varint_field(&mut output, 7, 0);
    append_varint_field(
        &mut output,
        8,
        json_i32(character, "createTime").unwrap_or_default().max(0) as u64,
    );
    append_message_field(
        &mut output,
        10,
        &super::base_handler::other_user_payload(
            state,
            account,
            json_u64(character, "uid").unwrap_or(1),
        ),
    );
    append_varint_field(&mut output, 13, 1);
    append_varint_field(&mut output, 14, 1);
    output
}

fn teaching_other_user_payload(
    state: &ServerState,
    account: &serde_json::Value,
    requested_uid: u64,
) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, requested_uid.max(1));
    append_message_field(
        &mut output,
        10,
        &super::base_handler::other_user_payload(state, account, requested_uid),
    );
    append_varint_field(&mut output, 13, 1);
    append_varint_field(&mut output, 14, 1);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_teaching_payload_uses_domain_character() {
        let state = ServerState::new("teaching", "Fallback", "1.4.0");
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("teaching").unwrap(),
            "Typed Captain",
        );
        account.character.uid = 42;
        account.character.level = 8;
        let payload = teaching_user_payload_typed(&state, &account);
        assert_eq!(decode_varint_field(&payload, 1), 42);
        let nested = decode_repeated_message_field(&payload, 10);
        assert_eq!(
            decode_string_field(&nested[0], 2).as_deref(),
            Some("Typed Captain")
        );
        assert_eq!(decode_varint_field(&nested[0], 5), 8);
    }
}
