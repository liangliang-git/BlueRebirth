use super::*;

pub(super) const GUILD_MEMBER: i32 = 100;
pub(super) const GUILD_LEADER: i32 = 1;
pub(super) const DEFAULT_GUILD_ID: u64 = 9_000_001;

fn append_limit(output: &mut Vec<u8>, level: i32) {
    let mut limit = Vec::new();
    append_varint_field(&mut limit, 1, 1);
    append_varint_field(&mut limit, 2, level.max(0) as u64);
    append_varint_field(&mut limit, 3, 0);
    append_message_field(output, 7, &limit);
}

fn append_typed_string(output: &mut Vec<u8>, field: u8, value: &str) {
    append_message_field(output, field, value.as_bytes());
}

fn typed_default_guild(account: &blueoath_domain::AccountState) -> blueoath_domain::GuildState {
    blueoath_domain::GuildState {
        id: DEFAULT_GUILD_ID,
        name: "蓝色誓约大舰队".to_owned(),
        emblem: 1,
        enounce: "欢迎加入大舰队".to_owned(),
        notice: "每日完成大舰队任务".to_owned(),
        member_num: 1,
        leader_id: 9_000_001,
        leader_name: "大舰队指挥官".to_owned(),
        create_time: account.character.create_time,
        ..Default::default()
    }
}

fn encode_typed_base_guild_info(guild: &blueoath_domain::GuildState) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, guild.id);
    append_typed_string(&mut output, 2, &guild.name);
    append_varint_field(&mut output, 3, u64::from(guild.emblem));
    append_typed_string(&mut output, 4, &guild.enounce);
    append_varint_field(&mut output, 5, u64::from(guild.level));
    append_varint_field(&mut output, 6, u64::from(guild.member_num));
    append_limit(&mut output, guild.limit_level as i32);
    append_varint_field(&mut output, 8, guild.leader_id);
    append_typed_string(&mut output, 9, &guild.leader_name);
    append_varint_field(&mut output, 10, u64::from(guild.frame));
    append_varint_field(&mut output, 11, guild.power);
    append_varint_field(&mut output, 12, guild.honor);
    output
}

pub(super) fn guild_list_payload_from_typed(
    account: &blueoath_domain::AccountState,
    from_rank: i32,
    num: i32,
) -> Vec<u8> {
    let candidate = account
        .guild
        .clone()
        .unwrap_or_else(|| typed_default_guild(account));
    let mut output = Vec::new();
    if from_rank <= 0 && num != 0 {
        append_varint_field(&mut output, 1, 1);
        append_message_field(&mut output, 2, &encode_typed_base_guild_info(&candidate));
    } else {
        append_varint_field(&mut output, 1, 0);
    }
    output
}

pub(super) fn guild_search_payload_from_typed(
    account: &blueoath_domain::AccountState,
    guild_id_arg: u64,
    name: &str,
) -> Vec<u8> {
    let candidate = account
        .guild
        .clone()
        .unwrap_or_else(|| typed_default_guild(account));
    let matches_id = guild_id_arg != 0 && candidate.id == guild_id_arg;
    let matches_name = !name.is_empty() && candidate.name.contains(name);
    let mut output = Vec::new();
    if matches_id || matches_name {
        append_message_field(&mut output, 1, &encode_typed_base_guild_info(&candidate));
    }
    output
}

pub(super) fn guild_info_payload_from_typed(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let Some(guild) = account.guild.as_ref() else {
        return Vec::new();
    };
    let mut output = Vec::new();
    append_typed_string(&mut output, 1, &guild.name);
    append_varint_field(&mut output, 2, u64::from(guild.emblem));
    append_typed_string(&mut output, 3, &guild.enounce);
    append_typed_string(&mut output, 4, &guild.notice);
    append_varint_field(&mut output, 5, u64::from(guild.level));
    append_varint_field(&mut output, 6, guild.exp);
    append_limit(&mut output, guild.limit_level as i32);
    append_varint_field(&mut output, 8, u64::from(guild.member_num));
    append_typed_string(&mut output, 9, &guild.leader_name);
    append_varint_field(&mut output, 10, guild.leader_id);
    append_varint_field(&mut output, 13, u64::from(guild.frame));
    append_varint_field(&mut output, 15, 0);
    append_varint_field(&mut output, 17, u64::from(guild.apply_num));
    append_varint_field(&mut output, 18, guild.create_time);
    append_typed_string(&mut output, 20, &guild.chat_room);
    output
}

pub(super) fn guild_user_info_payload_from_typed(
    account: &blueoath_domain::AccountState,
) -> Vec<u8> {
    let mut output = Vec::new();
    let Some(guild) = account.guild.as_ref() else {
        append_varint_field(&mut output, 1, 0);
        append_varint_field(&mut output, 3, 0);
        append_varint_field(&mut output, 10, 0);
        return output;
    };
    append_varint_field(&mut output, 1, guild.id);
    append_varint_field(&mut output, 2, 0);
    append_varint_field(&mut output, 3, 0);
    append_varint_field(&mut output, 10, u64::from(guild.my_post));
    append_varint_field(&mut output, 15, 0);
    append_varint_field(&mut output, 16, 0);
    append_varint_field(&mut output, 17, 100);
    append_varint_field(&mut output, 18, 0);
    append_varint_field(&mut output, 20, guild.join_time);
    output
}

pub(super) fn guild_member_list_payload_from_typed(
    account: &blueoath_domain::AccountState,
) -> Vec<u8> {
    let Some(guild) = account.guild.as_ref() else {
        return Vec::new();
    };
    let mut output = Vec::new();
    for member in &guild.members {
        let mut user = Vec::new();
        append_varint_field(&mut user, 1, member.uid);
        append_typed_string(&mut user, 2, &member.name);
        append_varint_field(&mut user, 3, u64::from(account.character.head));
        append_varint_field(&mut user, 4, u64::from(account.character.head_frame));
        append_varint_field(&mut user, 6, u64::from(account.character.class_id));
        append_varint_field(&mut user, 7, u64::from(account.character.level));
        append_varint_field(&mut user, 9, member.power);
        append_varint_field(&mut user, 12, guild.id);
        append_typed_string(&mut user, 13, &guild.name);
        let mut item = Vec::new();
        append_message_field(&mut item, 1, &user);
        append_varint_field(&mut item, 2, 0);
        append_varint_field(&mut item, 3, member.contribute);
        append_varint_field(&mut item, 7, u64::from(member.post));
        append_varint_field(&mut item, 8, member.today_contribute);
        append_varint_field(&mut item, 9, 0);
        append_varint_field(&mut item, 10, 0);
        append_message_field(&mut output, 1, &item);
    }
    output
}

pub(super) fn guild_apply_list_payload_from_typed(
    account: &blueoath_domain::AccountState,
) -> Vec<u8> {
    let Some(guild) = account.guild.as_ref() else {
        return Vec::new();
    };
    let mut output = Vec::new();
    for apply in &guild.applications {
        let mut user = Vec::new();
        append_varint_field(&mut user, 1, apply.uid);
        append_typed_string(&mut user, 2, &apply.name);
        append_varint_field(&mut user, 7, 1);
        let mut item = Vec::new();
        append_message_field(&mut item, 1, &user);
        append_varint_field(&mut item, 2, apply.time);
        append_varint_field(&mut item, 3, u64::from(apply.quality));
        append_message_field(&mut output, 1, &item);
    }
    output
}
