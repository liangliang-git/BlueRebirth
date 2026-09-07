use serde_json::{json, Value};

use super::*;

pub(super) const GUILD_MEMBER: i32 = 100;
pub(super) const GUILD_LEADER: i32 = 1;
pub(super) const DEFAULT_GUILD_ID: u64 = 9_000_001;

fn guild(account: &Value) -> Option<&Value> {
    account.get("guild").filter(|value| value.is_object())
}

fn guild_mut(account: &mut Value) -> Option<&mut serde_json::Map<String, Value>> {
    account.get_mut("guild").and_then(Value::as_object_mut)
}

fn guild_base_value(account: &Value) -> Value {
    let character = account.get("character").unwrap_or(&Value::Null);
    json!({
        "id": DEFAULT_GUILD_ID,
        "name": "蓝色誓约大舰队",
        "emblem": 1,
        "frame": 0,
        "enounce": "欢迎加入大舰队",
        "notice": "每日完成大舰队任务",
        "level": 1,
        "exp": 0,
        "memberNum": 1,
        "leaderId": 9000001u64,
        "leaderName": "大舰队指挥官",
        "limitLevel": 0,
        "power": 0,
        "honor": 0,
        "createTime": json_i32(character, "createTime").unwrap_or_default(),
        "chatRoom": ""
    })
}

fn guild_id(guild: &Value) -> u64 {
    guild.get("id").and_then(Value::as_u64).unwrap_or_default()
}

fn append_string_field(output: &mut Vec<u8>, field: u8, value: &str) {
    append_message_field(output, field, value.as_bytes());
}

fn append_limit(output: &mut Vec<u8>, level: i32) {
    let mut limit = Vec::new();
    append_varint_field(&mut limit, 1, 1);
    append_varint_field(&mut limit, 2, level.max(0) as u64);
    append_varint_field(&mut limit, 3, 0);
    append_message_field(output, 7, &limit);
}

fn encode_base_guild_info(guild: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, guild_id(guild));
    append_string_field(
        &mut output,
        2,
        json_string(guild, "name").as_deref().unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        3,
        json_i32(guild, "emblem").unwrap_or(1).max(0) as u64,
    );
    append_string_field(
        &mut output,
        4,
        json_string(guild, "enounce").as_deref().unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        5,
        json_i32(guild, "level").unwrap_or(1).max(0) as u64,
    );
    append_varint_field(
        &mut output,
        6,
        json_i32(guild, "memberNum").unwrap_or(1).max(0) as u64,
    );
    append_limit(
        &mut output,
        json_i32(guild, "limitLevel").unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        8,
        json_u64(guild, "leaderId").unwrap_or_default(),
    );
    append_string_field(
        &mut output,
        9,
        json_string(guild, "leaderName")
            .as_deref()
            .unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        10,
        json_i32(guild, "frame").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(
        &mut output,
        11,
        json_i32(guild, "power").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(
        &mut output,
        12,
        json_i32(guild, "honor").unwrap_or_default().max(0) as u64,
    );
    output
}

pub(super) fn guild_list_payload(account: &Value, from_rank: i32, num: i32) -> Vec<u8> {
    let candidate = guild(account)
        .cloned()
        .unwrap_or_else(|| guild_base_value(account));
    let mut output = Vec::new();
    if from_rank <= 0 && num != 0 {
        append_varint_field(&mut output, 1, 1);
        append_message_field(&mut output, 2, &encode_base_guild_info(&candidate));
    } else {
        append_varint_field(&mut output, 1, 0);
    }
    output
}

pub(super) fn guild_search_payload(account: &Value, guild_id_arg: u64, name: &str) -> Vec<u8> {
    let candidate = guild(account)
        .cloned()
        .unwrap_or_else(|| guild_base_value(account));
    let matches_id = guild_id_arg != 0 && guild_id(&candidate) == guild_id_arg;
    let matches_name = !name.is_empty()
        && json_string(&candidate, "name").is_some_and(|guild_name| guild_name.contains(name));
    let mut output = Vec::new();
    if matches_id || matches_name {
        append_message_field(&mut output, 1, &encode_base_guild_info(&candidate));
    }
    output
}

pub(super) fn guild_info_payload(account: &Value) -> Vec<u8> {
    let Some(guild) = guild(account) else {
        return Vec::new();
    };
    let mut output = Vec::new();
    append_string_field(
        &mut output,
        1,
        json_string(guild, "name").as_deref().unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        2,
        json_i32(guild, "emblem").unwrap_or(1).max(0) as u64,
    );
    append_string_field(
        &mut output,
        3,
        json_string(guild, "enounce").as_deref().unwrap_or_default(),
    );
    append_string_field(
        &mut output,
        4,
        json_string(guild, "notice").as_deref().unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        5,
        json_i32(guild, "level").unwrap_or(1).max(0) as u64,
    );
    append_varint_field(
        &mut output,
        6,
        json_i32(guild, "exp").unwrap_or_default().max(0) as u64,
    );
    append_limit(
        &mut output,
        json_i32(guild, "limitLevel").unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        8,
        json_i32(guild, "memberNum").unwrap_or(1).max(0) as u64,
    );
    append_string_field(
        &mut output,
        9,
        json_string(guild, "leaderName")
            .as_deref()
            .unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        10,
        json_u64(guild, "leaderId").unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        13,
        json_i32(guild, "frame").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(&mut output, 15, 0);
    append_varint_field(
        &mut output,
        17,
        json_i32(guild, "applyNum").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(
        &mut output,
        18,
        json_i32(guild, "createTime").unwrap_or_default().max(0) as u64,
    );
    append_string_field(
        &mut output,
        20,
        json_string(guild, "chatRoom")
            .as_deref()
            .unwrap_or_default(),
    );
    output
}

pub(super) fn guild_user_info_payload(account: &Value) -> Vec<u8> {
    let Some(guild) = guild(account) else {
        let mut output = Vec::new();
        append_varint_field(&mut output, 1, 0);
        append_varint_field(&mut output, 3, 0);
        append_varint_field(&mut output, 10, 0);
        return output;
    };
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, guild_id(guild));
    append_varint_field(&mut output, 2, 0);
    append_varint_field(&mut output, 3, 0);
    append_varint_field(
        &mut output,
        10,
        json_i32(guild, "myPost").unwrap_or(GUILD_MEMBER).max(0) as u64,
    );
    append_varint_field(&mut output, 15, 0);
    append_varint_field(&mut output, 16, 0);
    append_varint_field(&mut output, 17, 100);
    append_varint_field(&mut output, 18, 0);
    append_varint_field(
        &mut output,
        20,
        json_i32(guild, "joinTime").unwrap_or_default().max(0) as u64,
    );
    output
}

fn encode_other_user_info(account: &Value, member: &Value, guild: &Value) -> Vec<u8> {
    let character = account.get("character").unwrap_or(&Value::Null);
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, json_u64(member, "uid").unwrap_or(1));
    append_string_field(
        &mut output,
        2,
        json_string(member, "name")
            .or_else(|| json_string(character, "name"))
            .as_deref()
            .unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        3,
        json_i32(character, "head").unwrap_or(1021051).max(0) as u64,
    );
    append_varint_field(
        &mut output,
        4,
        json_i32(character, "headFrame").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(
        &mut output,
        6,
        json_i32(character, "class").unwrap_or(1).max(0) as u64,
    );
    append_varint_field(
        &mut output,
        7,
        json_i32(character, "level").unwrap_or(1).max(0) as u64,
    );
    append_varint_field(
        &mut output,
        9,
        json_i32(member, "power").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(&mut output, 12, guild_id(guild));
    append_string_field(
        &mut output,
        13,
        json_string(guild, "name").as_deref().unwrap_or_default(),
    );
    output
}

pub(super) fn guild_member_list_payload(account: &Value) -> Vec<u8> {
    let Some(guild) = guild(account) else {
        return Vec::new();
    };
    let mut output = Vec::new();
    if let Some(members) = guild.get("members").and_then(Value::as_array) {
        for member in members {
            let mut item = Vec::new();
            append_message_field(
                &mut item,
                1,
                &encode_other_user_info(account, member, guild),
            );
            append_varint_field(&mut item, 2, 0);
            append_varint_field(
                &mut item,
                3,
                json_i32(member, "contribute").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut item,
                7,
                json_i32(member, "post").unwrap_or(GUILD_MEMBER).max(0) as u64,
            );
            append_varint_field(
                &mut item,
                8,
                json_i32(member, "todayContribute")
                    .unwrap_or_default()
                    .max(0) as u64,
            );
            append_varint_field(&mut item, 9, 0);
            append_varint_field(&mut item, 10, 0);
            append_message_field(&mut output, 1, &item);
        }
    }
    output
}

pub(super) fn guild_apply_list_payload(account: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    let Some(guild) = guild(account) else {
        return output;
    };
    if let Some(apply_list) = guild.get("applyList").and_then(Value::as_array) {
        for apply in apply_list {
            let mut user = Vec::new();
            append_varint_field(&mut user, 1, json_u64(apply, "uid").unwrap_or_default());
            append_string_field(
                &mut user,
                2,
                json_string(apply, "name").as_deref().unwrap_or_default(),
            );
            append_varint_field(&mut user, 7, 1);
            let mut item = Vec::new();
            append_message_field(&mut item, 1, &user);
            append_varint_field(
                &mut item,
                2,
                json_i32(apply, "time").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut item,
                3,
                json_i32(apply, "quality").unwrap_or_default().max(0) as u64,
            );
            append_message_field(&mut output, 1, &item);
        }
    }
    output
}

fn create_guild_value(
    account: &Value,
    id: u64,
    name: &str,
    emblem: i32,
    frame: i32,
    post: i32,
    now: u32,
) -> Value {
    let character = account.get("character").unwrap_or(&Value::Null);
    let uid = json_u64(character, "uid").unwrap_or(1);
    let uname = json_string(character, "name").unwrap_or_else(|| "指挥官".to_owned());
    json!({
        "id": id,
        "name": name,
        "emblem": emblem.max(0),
        "frame": frame.max(0),
        "enounce": "欢迎加入大舰队",
        "notice": "每日完成大舰队任务",
        "level": 1,
        "exp": 0,
        "memberNum": 1,
        "leaderId": if post == GUILD_LEADER { uid } else { 9000001 },
        "leaderName": if post == GUILD_LEADER { uname.clone() } else { "大舰队指挥官".to_owned() },
        "limitLevel": 0,
        "power": 0,
        "honor": 0,
        "createTime": now,
        "chatRoom": "",
        "myPost": post,
        "joinTime": now,
        "members": [{
            "uid": uid,
            "name": uname,
            "post": post,
            "contribute": 0,
            "todayContribute": 0,
            "power": 0
        }],
        "applyList": []
    })
}

pub(super) fn create_guild(
    account: &mut Value,
    name: &str,
    emblem: i32,
    frame: i32,
    now: u32,
) -> bool {
    if guild(account).is_some() || name.trim().is_empty() || name.chars().count() > 24 {
        return false;
    }
    let profile = json_string(account, "profileId").unwrap_or_else(|| "local".to_owned());
    let mut id = 2_000_000u64;
    for byte in profile.bytes() {
        id = id.wrapping_mul(33).wrapping_add(u64::from(byte));
    }
    let value = create_guild_value(
        account,
        id.max(2_000_000),
        name.trim(),
        emblem,
        frame,
        GUILD_LEADER,
        now,
    );
    account["guild"] = value;
    true
}

pub(super) fn join_default_guild(account: &mut Value, now: u32) -> bool {
    if guild(account).is_some() {
        return false;
    }
    let candidate = guild_base_value(account);
    let id = guild_id(&candidate);
    let name = json_string(&candidate, "name").unwrap_or_default();
    let value = create_guild_value(account, id, &name, 1, 0, GUILD_MEMBER, now);
    account["guild"] = value;
    true
}

pub(super) fn leave_guild(account: &mut Value) -> bool {
    account
        .as_object_mut()
        .and_then(|root| root.remove("guild"))
        .is_some()
}

pub(super) fn modify_guild(account: &mut Value, args: &Value) -> bool {
    let Some(guild) = guild_mut(account) else {
        return false;
    };
    for (key, source) in [
        ("name", "name"),
        ("enounce", "enounce"),
        ("notice", "notice"),
        ("chatRoom", "chatRoom"),
    ] {
        if let Some(value) = args.get(source).and_then(Value::as_str) {
            guild.insert(key.to_owned(), Value::String(value.to_owned()));
        }
    }
    for (key, source) in [
        ("emblem", "emblem"),
        ("frame", "frame"),
        ("limitLevel", "limitLevel"),
    ] {
        if let Some(value) = args.get(source).and_then(Value::as_i64) {
            guild.insert(key.to_owned(), json!(value.max(0)));
        }
    }
    true
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
