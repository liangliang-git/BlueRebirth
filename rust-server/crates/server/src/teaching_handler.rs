use serde_json::json;

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
    let current = account.as_deref().unwrap_or(&serde_json::Value::Null);

    match method {
        "teachingsvr.TeacherList"
        | "teachingsvr.MyStudent"
        | "teachingsvr.StudentList"
        | "teachingsvr.ApplyList" => Some(teaching_list_payload()),
        "teachingsvr.MyTeacher" => Some(teaching_user_payload(state, current)),
        "teachingsvr.Search" => Some(teaching_list_payload()),
        "teachingsvr.GetOtherInfo" => Some(teaching_other_user_payload(
            state,
            current,
            decode_varint_u64_field(request_args, 1),
        )),
        "teachingsvr.TaskReward" => {
            let mut output = Vec::new();
            append_varint_field(
                &mut output,
                1,
                decode_varint_field(request_args, 1).max(0) as u64,
            );
            Some(output)
        }
        "teachingsvr.Apply"
        | "teachingsvr.Agree"
        | "teachingsvr.Refuse"
        | "teachingsvr.Delete"
        | "teachingsvr.Appraise"
        | "teachingsvr.PersonalInfo" => {
            if let Some(account) = account.as_deref_mut() {
                account["lastTeachingAction"] = json!({
                    "method": method,
                    "args": request_args,
                    "time": current_unix_seconds(),
                });
            } else {
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
            }
            Some(Vec::new())
        }
        _ => None,
    }
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
