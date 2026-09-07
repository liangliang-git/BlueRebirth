use serde_json::json;

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
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
) -> HandlerResult {
    let account = &mut *context.account;
    let pre_pushes = &mut *context.pre_pushes;

    match method {
        "guild.Create" => {
            let name = decode_string_field(request_args, 1).unwrap_or_default();
            let emblem = decode_varint_field(request_args, 2);
            let frame = decode_varint_field(request_args, 3);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            if create_guild(account, &name, emblem, frame, current_unix_seconds()) {
                push_guild_state(pre_pushes, account);
            } else {
                return invalid("guild name is invalid or captain already has a guild");
            }
            HandlerResult::PushOnly
        }
        "guild.GetList" => reply(
            method,
            guild_list_payload(
                account.as_deref().unwrap_or(&Value::Null),
                decode_varint_field(request_args, 1),
                decode_varint_field(request_args, 2),
            ),
        ),
        "guild.Search" => reply(
            method,
            guild_search_payload(
                account.as_deref().unwrap_or(&Value::Null),
                decode_varint_u64_field(request_args, 1),
                &decode_string_field(request_args, 2).unwrap_or_default(),
            ),
        ),
        "guild.Apply" => {
            let requested_id = decode_varint_u64_field(request_args, 1);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            if requested_id != DEFAULT_GUILD_ID
                || !join_default_guild(account, current_unix_seconds())
            {
                return invalid("guild application is invalid");
            }
            push_guild_state(pre_pushes, account);
            append_method_push(
                pre_pushes,
                "guild.GetMemberList",
                guild_member_list_payload(account),
            );
            HandlerResult::PushOnly
        }
        "guild.CancelApply" => {
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            account["guildApplyId"] = json!(0);
            account["guildApplyTime"] = json!(0);
            HandlerResult::PushOnly
        }
        "guild.GetMemberList" => reply(
            method,
            guild_member_list_payload(account.as_deref().unwrap_or(&Value::Null)),
        ),
        "guild.GetApplyList" => reply(
            method,
            guild_apply_list_payload(account.as_deref().unwrap_or(&Value::Null)),
        ),
        "guild.Quit" | "guild.Dismiss" => {
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            if leave_guild(account) {
                push_guild_state(pre_pushes, account);
            } else {
                return invalid("captain is not in a guild");
            }
            HandlerResult::PushOnly
        }
        "guild.Modify" => {
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
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
                return invalid("captain is not in a guild");
            }
            HandlerResult::PushOnly
        }
        "guild.GetInfo" => reply(
            method,
            guild_info_payload(account.as_deref().unwrap_or(&Value::Null)),
        ),
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
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
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
            HandlerResult::PushOnly
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

#[cfg(test)]
mod tests {
    use crate::common::response::HandlerResult;

    use super::*;

    #[test]
    fn handler_exposes_typed_result() {
        let _: for<'state, 'account, 'scratch> fn(
            &mut GameLoginRequestContext<'state, 'account, 'scratch>,
            &str,
            &[u8],
        ) -> HandlerResult = handle;
    }
}
