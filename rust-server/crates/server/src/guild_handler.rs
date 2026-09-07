use serde_json::json;

use super::*;

pub(super) fn push_guild_state(pre_pushes: &mut Vec<Vec<u8>>, account: &Value) {
    append_method_push(
        pre_pushes,
        "guild.UpdateOurGuildData",
        guild_info_payload(account),
    );
    append_method_push(
        pre_pushes,
        "guild.UpdateMyGuildData",
        guild_user_info_payload(account),
    );
}

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let account = &mut *context.account;
    let pre_pushes = &mut *context.pre_pushes;
    let response_err = &mut *context.response_err;
    let response_err_msg = &mut *context.response_err_msg;

    match method {
        "guild.Create" => {
            let name = decode_string_field(request_args, 1).unwrap_or_default();
            let emblem = decode_varint_field(request_args, 2);
            let frame = decode_varint_field(request_args, 3);
            if let Some(account) = account.as_deref_mut() {
                if create_guild(account, &name, emblem, frame, current_unix_seconds()) {
                    push_guild_state(pre_pushes, account);
                } else {
                    *response_err = 1;
                    *response_err_msg =
                        "guild name is invalid or captain already has a guild".to_owned();
                }
            }
            Some(Vec::new())
        }
        "guild.GetList" => Some(guild_list_payload(
            account.as_deref().unwrap_or(&Value::Null),
            decode_varint_field(request_args, 1),
            decode_varint_field(request_args, 2),
        )),
        "guild.Search" => Some(guild_search_payload(
            account.as_deref().unwrap_or(&Value::Null),
            decode_varint_u64_field(request_args, 1),
            &decode_string_field(request_args, 2).unwrap_or_default(),
        )),
        "guild.Apply" => {
            let requested_id = decode_varint_u64_field(request_args, 1);
            if let Some(account) = account.as_deref_mut() {
                if requested_id != DEFAULT_GUILD_ID
                    || !join_default_guild(account, current_unix_seconds())
                {
                    *response_err = 1;
                    *response_err_msg = "guild application is invalid".to_owned();
                } else {
                    push_guild_state(pre_pushes, account);
                    append_method_push(
                        pre_pushes,
                        "guild.GetMemberList",
                        guild_member_list_payload(account),
                    );
                }
            }
            Some(Vec::new())
        }
        "guild.CancelApply" => {
            if let Some(account) = account.as_deref_mut() {
                account["guildApplyId"] = json!(0);
                account["guildApplyTime"] = json!(0);
            }
            Some(Vec::new())
        }
        "guild.GetMemberList" => Some(guild_member_list_payload(
            account.as_deref().unwrap_or(&Value::Null),
        )),
        "guild.GetApplyList" => Some(guild_apply_list_payload(
            account.as_deref().unwrap_or(&Value::Null),
        )),
        "guild.Quit" | "guild.Dismiss" => {
            if let Some(account) = account.as_deref_mut() {
                if leave_guild(account) {
                    push_guild_state(pre_pushes, account);
                } else {
                    *response_err = 1;
                    *response_err_msg = "captain is not in a guild".to_owned();
                }
            }
            Some(Vec::new())
        }
        "guild.Modify" => {
            if let Some(account) = account.as_deref_mut() {
                let args = json!({
                    "name": decode_string_field(request_args, 1),
                    "emblem": decode_varint_field(request_args, 2),
                    "enounce": decode_string_field(request_args, 3),
                    "notice": decode_string_field(request_args, 4),
                    "frame": decode_varint_field(request_args, 6),
                    "chatRoom": decode_string_field(request_args, 7)
                });
                if modify_guild(account, &args) {
                    push_guild_state(pre_pushes, account);
                } else {
                    *response_err = 1;
                    *response_err_msg = "captain is not in a guild".to_owned();
                }
            }
            Some(Vec::new())
        }
        "guild.GetInfo" => Some(guild_info_payload(
            account.as_deref().unwrap_or(&Value::Null),
        )),
        "guild.Verify"
        | "guild.Appoint"
        | "guild.Remove"
        | "guild.Transfer"
        | "guild.Upgrade"
        | "guild.RejectAll"
        | "guild.AcceptAll"
        | "guild.Publicity"
        | "guild.SetGuildLevelOfShow"
        | "guild.Impeach" => {
            if let Some(account) = account.as_deref_mut() {
                let now = current_unix_seconds();
                account["lastGuildAction"] = json!({
                    "method": method,
                    "args": request_args,
                    "time": now,
                });
                if method == "guild.Upgrade" {
                    let level = account
                        .get("guild")
                        .and_then(|guild| guild.get("level"))
                        .and_then(Value::as_i64)
                        .unwrap_or(1);
                    if let Some(guild) = account.get_mut("guild") {
                        guild["level"] = json!(level.saturating_add(1).min(10));
                    }
                }
                push_guild_state(pre_pushes, account);
            }
            Some(Vec::new())
        }
        _ => None,
    }
}
