use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;
use blueoath_domain::{AccountState, GuildMemberState, GuildState};

pub(crate) fn handle_typed(
    account: &mut AccountState,
    method: &str,
    request_args: &[u8],
    now: u32,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    match method {
        "guild.Create" => {
            let Ok(request) = GuildCreateRequest::decode(request_args) else {
                return invalid("guild create request is invalid");
            };
            if account.guild.is_some() {
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
                request.name.trim(),
                request.emblem.max(0) as u32,
                request.frame.max(0) as u32,
                GUILD_LEADER as u32,
                now,
            ));
            push_guild_state_typed(effects, account);
            HandlerResult::PushOnly
        }
        "guild.GetList" => reply(method, {
            let Ok(request) = GuildListRequest::decode(request_args) else {
                return invalid("guild list request is invalid");
            };
            guild_list_payload_from_typed(account, request.start, request.end)
        }),
        "guild.Search" => reply(method, {
            let Ok(request) = GuildSearchRequest::decode(request_args) else {
                return invalid("guild search request is invalid");
            };
            guild_search_payload_from_typed(account, request.guild_id, &request.name)
        }),
        "guild.Apply" => {
            let Ok(request) = GuildIdRequest::decode(request_args) else {
                return invalid("guild application is invalid");
            };
            if request.guild_id != DEFAULT_GUILD_ID || account.guild.is_some() {
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
            push_guild_state_typed(effects, account);
            effects.push_pre(Response::raw(
                "guild.GetMemberList",
                guild_member_list_payload_from_typed(account),
            ));
            HandlerResult::PushOnly
        }
        "guild.CancelApply" => HandlerResult::PushOnly,
        "guild.GetMemberList" => reply(method, guild_member_list_payload_from_typed(account)),
        "guild.GetApplyList" => reply(method, guild_apply_list_payload_from_typed(account)),
        "guild.Quit" | "guild.Dismiss" => {
            if account.guild.take().is_none() {
                return invalid("captain is not in a guild");
            }
            push_guild_state_typed(effects, account);
            HandlerResult::PushOnly
        }
        "guild.Modify" => {
            let Ok(request) = GuildModifyRequest::decode(request_args) else {
                return invalid("guild modify request is invalid");
            };
            let Some(guild) = account.guild.as_mut() else {
                return invalid("captain is not in a guild");
            };
            if let Some(value) = request.name {
                if value.trim().is_empty() || value.chars().count() > 24 {
                    return invalid("guild name is invalid");
                }
                guild.name = value;
            }
            guild.emblem = request.emblem.max(0) as u32;
            if let Some(value) = request.enounce {
                guild.enounce = value;
            }
            if let Some(value) = request.notice {
                guild.notice = value;
            }
            guild.frame = request.frame.max(0) as u32;
            if let Some(value) = request.chat_room {
                guild.chat_room = value;
            }
            push_guild_state_typed(effects, account);
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
            push_guild_state_typed(effects, account);
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

pub(crate) fn push_guild_state_typed(effects: &mut ResponseEffects, account: &AccountState) {
    effects.push_pre(Response::raw(
        "guild.UpdateOurGuildData",
        guild_info_payload_from_typed(account),
    ));
    effects.push_pre(Response::raw(
        "guild.UpdateMyGuildData",
        guild_user_info_payload_from_typed(account),
    ));
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
        let mut effects = ResponseEffects::default();
        let result = handle_typed(&mut account, "guild.Create", &args, 100, &mut effects);
        assert!(matches!(result, HandlerResult::PushOnly));
        assert_eq!(account.guild.as_ref().unwrap().name, "Typed Guild");
        assert_eq!(effects.into_parts().0.len(), 2);

        let mut modify = Vec::new();
        append_message_field(&mut modify, 1, b"Renamed Guild");
        let mut effects = ResponseEffects::default();
        let result = handle_typed(&mut account, "guild.Modify", &modify, 101, &mut effects);
        assert!(matches!(result, HandlerResult::PushOnly));
        assert_eq!(account.guild.as_ref().unwrap().name, "Renamed Guild");

        let result = handle_typed(&mut account, "guild.Quit", &[], 102, &mut effects);
        assert!(matches!(result, HandlerResult::PushOnly));
        assert!(account.guild.is_none());
    }
}
