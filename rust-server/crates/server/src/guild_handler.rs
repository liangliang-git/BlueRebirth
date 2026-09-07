use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;
use blueoath_domain::{AccountState, GuildMemberState, GuildState};

pub(super) fn handle_typed(
    account: &mut AccountState,
    method: &str,
    request_args: &[u8],
    now: u32,
    pre_pushes: &mut Vec<Vec<u8>>,
) -> HandlerResult {
    match method {
        "guild.Create" => {
            let name = decode_string_field(request_args, 1).unwrap_or_default();
            let emblem = decode_varint_field(request_args, 2).max(0) as u32;
            let frame = decode_varint_field(request_args, 3).max(0) as u32;
            if account.guild.is_some() || name.trim().is_empty() || name.chars().count() > 24 {
                return invalid("guild name is invalid or captain already has a guild");
            }
            let mut id = 2_000_000_u64;
            if let Some(profile) = account.profile.as_ref() {
                for byte in profile.id.as_str().bytes() {
                    id = id.wrapping_mul(33).wrapping_add(u64::from(byte));
                }
            }
            account.guild = Some(new_typed_guild(
                account,
                id.max(2_000_000),
                name.trim(),
                emblem,
                frame,
                GUILD_LEADER as u32,
                now,
            ));
            push_guild_state_typed(pre_pushes, account);
            HandlerResult::PushOnly
        }
        "guild.GetList" => reply(
            method,
            guild_list_payload_from_typed(
                account,
                decode_varint_field(request_args, 1),
                decode_varint_field(request_args, 2),
            ),
        ),
        "guild.Search" => reply(
            method,
            guild_search_payload_from_typed(
                account,
                decode_varint_u64_field(request_args, 1),
                &decode_string_field(request_args, 2).unwrap_or_default(),
            ),
        ),
        "guild.Apply" => {
            if decode_varint_u64_field(request_args, 1) != DEFAULT_GUILD_ID
                || account.guild.is_some()
            {
                return invalid("guild application is invalid");
            }
            account.guild = Some(new_typed_guild(
                account,
                DEFAULT_GUILD_ID,
                "蓝色誓约大舰队",
                1,
                0,
                GUILD_MEMBER as u32,
                now,
            ));
            push_guild_state_typed(pre_pushes, account);
            append_method_push(
                pre_pushes,
                "guild.GetMemberList",
                guild_member_list_payload_from_typed(account),
            );
            HandlerResult::PushOnly
        }
        "guild.CancelApply" => HandlerResult::PushOnly,
        "guild.GetMemberList" => reply(method, guild_member_list_payload_from_typed(account)),
        "guild.GetApplyList" => reply(method, guild_apply_list_payload_from_typed(account)),
        "guild.Quit" | "guild.Dismiss" => {
            if account.guild.take().is_none() {
                return invalid("captain is not in a guild");
            }
            push_guild_state_typed(pre_pushes, account);
            HandlerResult::PushOnly
        }
        "guild.Modify" => {
            let Some(guild) = account.guild.as_mut() else {
                return invalid("captain is not in a guild");
            };
            if let Some(value) = decode_string_field(request_args, 1) {
                if value.trim().is_empty() || value.chars().count() > 24 {
                    return invalid("guild name is invalid");
                }
                guild.name = value;
            }
            guild.emblem = decode_varint_field(request_args, 2).max(0) as u32;
            if let Some(value) = decode_string_field(request_args, 3) {
                guild.enounce = value;
            }
            if let Some(value) = decode_string_field(request_args, 4) {
                guild.notice = value;
            }
            guild.frame = decode_varint_field(request_args, 6).max(0) as u32;
            if let Some(value) = decode_string_field(request_args, 7) {
                guild.chat_room = value;
            }
            push_guild_state_typed(pre_pushes, account);
            HandlerResult::PushOnly
        }
        "guild.GetInfo" => reply(method, guild_info_payload_from_typed(account)),
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
            let Some(guild) = account.guild.as_mut() else {
                return invalid("captain is not in a guild");
            };
            if method == "guild.Upgrade" {
                guild.level = guild.level.saturating_add(1).min(10);
            }
            push_guild_state_typed(pre_pushes, account);
            HandlerResult::PushOnly
        }
        _ => HandlerResult::Empty,
    }
}

fn new_typed_guild(
    account: &AccountState,
    id: u64,
    name: &str,
    emblem: u32,
    frame: u32,
    post: u32,
    now: u32,
) -> GuildState {
    let uid = account.character.uid;
    let uname = if account.character.name.is_empty() {
        "指挥官"
    } else {
        account.character.name.as_str()
    };
    GuildState {
        id,
        name: name.to_owned(),
        emblem,
        frame,
        enounce: "欢迎加入大舰队".to_owned(),
        notice: "每日完成大舰队任务".to_owned(),
        member_num: 1,
        leader_id: if post == GUILD_LEADER as u32 {
            uid
        } else {
            9_000_001
        },
        leader_name: if post == GUILD_LEADER as u32 {
            uname.to_owned()
        } else {
            "大舰队指挥官".to_owned()
        },
        create_time: u64::from(now),
        my_post: post,
        join_time: u64::from(now),
        members: vec![GuildMemberState {
            uid,
            name: uname.to_owned(),
            post,
            ..Default::default()
        }],
        ..Default::default()
    }
}

pub(super) fn push_guild_state_typed(pre_pushes: &mut Vec<Vec<u8>>, account: &AccountState) {
    append_method_push(
        pre_pushes,
        "guild.UpdateOurGuildData",
        guild_info_payload_from_typed(account),
    );
    append_method_push(
        pre_pushes,
        "guild.UpdateMyGuildData",
        guild_user_info_payload_from_typed(account),
    );
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
    fn typed_guild_create_modify_and_quit_use_domain_state() {
        let mut account = AccountState::default();
        account.character.uid = 42;
        account.character.name = "Typed Captain".to_owned();
        let mut args = Vec::new();
        append_message_field(&mut args, 1, b"Typed Guild");
        append_varint_field(&mut args, 2, 3);
        let mut pushes = Vec::new();
        let result = handle_typed(&mut account, "guild.Create", &args, 100, &mut pushes);
        assert!(matches!(result, HandlerResult::PushOnly));
        assert_eq!(account.guild.as_ref().unwrap().name, "Typed Guild");
        assert_eq!(pushes.len(), 2);

        let mut modify = Vec::new();
        append_message_field(&mut modify, 1, b"Renamed Guild");
        let result = handle_typed(&mut account, "guild.Modify", &modify, 101, &mut pushes);
        assert!(matches!(result, HandlerResult::PushOnly));
        assert_eq!(account.guild.as_ref().unwrap().name, "Renamed Guild");

        let result = handle_typed(&mut account, "guild.Quit", &[], 102, &mut pushes);
        assert!(matches!(result, HandlerResult::PushOnly));
        assert!(account.guild.is_none());
    }
}
