use serde_json::{json, Value};

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(super) fn handles(method: &str) -> bool {
    matches!(
        GameMethod::parse(method).family(),
        MethodFamily::BigActivity | MethodFamily::GuildBigActivity | MethodFamily::HeroAwaken
    )
}

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let Some(account) = context.account.as_deref_mut() else {
        return HandlerResult::Error(GameError::AccountUnavailable);
    };
    if GameMethod::parse(method).is_family(MethodFamily::HeroAwaken) {
        let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
        return handle_hero_awaken(
            account,
            catalog,
            method,
            request_args,
            context.catalogs.fashion,
        );
    }
    if GameMethod::parse(method).is_family(MethodFamily::BigActivity) {
        return handle_big_activity(context.state, account, method, request_args);
    }
    handle_guild_big_activity(context.state, account, method, request_args)
}

fn handle_big_activity(
    server_state: &ServerState,
    account: &mut Value,
    method: &str,
    args: &[u8],
) -> HandlerResult {
    let state = activity_state_mut(account, "bigActivity");
    match method {
        "bigactivity.GetBigActivityInfo" => reply(method, big_activity_payload(state)),
        "bigactivity.GetBigActivityRank" | "bigactivity.GetBigActivityRankEx" => {
            let start = decode_varint_field(args, 1).max(1);
            reply(
                method,
                big_activity_rank_payload(server_state, account, start),
            )
        }
        _ => HandlerResult::Empty,
    }
}

fn handle_guild_big_activity(
    server_state: &ServerState,
    account: &mut Value,
    method: &str,
    args: &[u8],
) -> HandlerResult {
    let state = activity_state_mut(account, "guildBigActivity");
    match method {
        "guildbigactivity.UserData" => reply(method, guild_big_activity_user_payload(state)),
        "guildbigactivity.GuildRateData" => reply(method, guild_big_activity_rate_payload(state)),
        "guildbigactivity.PresentItem" => {
            let item_id = decode_varint_field(args, 1).max(0);
            let count = decode_varint_field(args, 2).clamp(1, 99);
            state["lastItemId"] = json!(item_id);
            state["presentCount"] =
                json!(state_i64(state, "presentCount").saturating_add(i64::from(count)));
            reply(method, guild_big_activity_user_payload(state))
        }
        "guildbigactivityrank.GetGuildRankList" => reply(
            method,
            guild_big_activity_rank_payload(server_state, account),
        ),
        _ => HandlerResult::Empty,
    }
}

fn handle_hero_awaken(
    account: &mut Value,
    catalog: &GameplayCatalog,
    method: &str,
    args: &[u8],
    fashion_catalog: Option<&blueoath_protocol::FashionList>,
) -> HandlerResult {
    match method {
        "heroawaken.FinishAwaken" => {
            let state = activity_state_mut(account, "heroAwaken");
            state["isFinished"] = json!(decode_varint_field(args, 1) != 0);
            reply(method, hero_awaken_finish_payload(state))
        }
        "heroawaken.MilestoneInfo" => reply(
            method,
            hero_awaken_milestone_payload(activity_state_mut(account, "heroAwaken")),
        ),
        "heroawaken.RewardMilestone" => {
            let milestone = decode_varint_field(args, 1).max(0);
            let total_pt = account
                .get("heroAwaken")
                .and_then(|state| state.get("totalPt"))
                .and_then(Value::as_i64)
                .unwrap_or_default();
            if milestone <= 0 || total_pt < i64::from(milestone) {
                return reply(method, encode_rewards_list(&[]));
            }
            let already_claimed = account
                .get("heroAwaken")
                .and_then(|state| state.get("claimedMilestones"))
                .and_then(Value::as_array)
                .is_some_and(|values| {
                    values
                        .iter()
                        .any(|value| value.as_i64() == Some(i64::from(milestone)))
                });
            if already_claimed {
                return reply(method, encode_rewards_list(&[]));
            }
            let reward_id = catalog
                .activity
                .get(&5002)
                .and_then(|activity| activity.get("p4"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_array)
                .find(|row| row.first().and_then(Value::as_i64) == Some(i64::from(milestone)))
                .and_then(|row| row.get(1).and_then(Value::as_i64))
                .and_then(|id| i32::try_from(id).ok())
                .unwrap_or_default();
            let rewards = if reward_id > 0 {
                catalog
                    .rewards_by_id
                    .get(&reward_id)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|reward| {
                        grant_reward(account, reward, current_unix_seconds(), fashion_catalog)
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            if !rewards.is_empty() {
                let state = activity_state_mut(account, "heroAwaken");
                push_unique_i32(&mut state["claimedMilestones"], milestone);
            }
            reply(method, encode_rewards_list(&rewards))
        }
        _ => HandlerResult::Empty,
    }
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn activity_state_mut<'a>(account: &'a mut Value, key: &str) -> &'a mut Value {
    let default = match key {
        "bigActivity" => json!({
            "merits": 0, "dayMerits": 0, "historyMerits": 0,
            "meritsMax": 0, "percent": 0, "rank": 0, "numberList": []
        }),
        "guildBigActivity" => json!({"points": 0, "rate": 0, "presentCount": 0, "rank": 0}),
        "heroAwaken" => json!({"isFinished": false, "lastHeroId": 0, "claimedMilestones": []}),
        _ => json!({}),
    };
    account
        .as_object_mut()
        .expect("account must be an object")
        .entry(key.to_owned())
        .or_insert(default)
}

fn big_activity_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    for (field, key) in [
        (1, "merits"),
        (2, "dayMerits"),
        (3, "rank"),
        (4, "historyMerits"),
        (5, "meritsMax"),
    ] {
        append_varint_field(&mut output, field, state_i64(state, key).max(0) as u64);
    }
    for value in state
        .get("meritsList")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
    {
        let mut item = Vec::new();
        append_varint_field(
            &mut item,
            1,
            value.first().and_then(Value::as_i64).unwrap_or(0).max(0) as u64,
        );
        append_varint_field(
            &mut item,
            2,
            value.get(1).and_then(Value::as_i64).unwrap_or(0).max(0) as u64,
        );
        append_message_field(&mut output, 6, &item);
    }
    for value in state
        .get("numberList")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
    {
        let mut item = Vec::new();
        append_varint_field(
            &mut item,
            1,
            value.first().and_then(Value::as_i64).unwrap_or(0).max(0) as u64,
        );
        append_varint_field(
            &mut item,
            2,
            value.get(1).and_then(Value::as_i64).unwrap_or(0).max(0) as u64,
        );
        append_message_field(&mut output, 7, &item);
    }
    append_varint_field(&mut output, 8, state_i64(state, "percent").max(0) as u64);
    output
}

fn big_activity_rank_payload(server_state: &ServerState, account: &Value, start: i32) -> Vec<u8> {
    let mut entries = account_directory(server_state, account);
    entries.sort_by(|left, right| {
        state_i64(right.1.get("bigActivity").unwrap_or(&Value::Null), "merits")
            .cmp(&state_i64(
                left.1.get("bigActivity").unwrap_or(&Value::Null),
                "merits",
            ))
            .then_with(|| left.0.cmp(&right.0))
    });
    let mut output = Vec::new();
    let offset = usize::try_from(start.saturating_sub(1)).unwrap_or_default();
    for (index, (_, entry)) in entries.iter().skip(offset).take(50).enumerate() {
        append_message_field(
            &mut output,
            1,
            &big_activity_rank_row(entry, offset.saturating_add(index).saturating_add(1)),
        );
    }
    let current_uid = account_uid(account).unwrap_or(1);
    let current_rank = entries
        .iter()
        .position(|(uid, _)| *uid == current_uid)
        .map(|rank| rank.saturating_add(1))
        .unwrap_or(1);
    append_message_field(
        &mut output,
        2,
        &big_activity_rank_row(account, current_rank),
    );
    output
}

fn account_directory(server_state: &ServerState, current: &Value) -> Vec<(u64, Value)> {
    let mut entries = server_state
        .social_store
        .as_ref()
        .and_then(|store| store.list().ok())
        .into_iter()
        .flatten()
        .filter_map(|(_, account)| account_uid(&account).map(|uid| (uid, account)))
        .collect::<Vec<_>>();
    let current_uid = account_uid(current).unwrap_or(1);
    if let Some(existing) = entries.iter_mut().find(|(uid, _)| *uid == current_uid) {
        existing.1 = current.clone();
    } else {
        entries.push((current_uid, current.clone()));
    }
    entries
}

fn account_uid(account: &Value) -> Option<u64> {
    account
        .get("character")
        .and_then(|character| json_u64(character, "uid"))
        .filter(|uid| *uid > 0)
}

fn big_activity_rank_row(account: &Value, rank: usize) -> Vec<u8> {
    let activity = account.get("bigActivity").unwrap_or(&Value::Null);
    let mut row = Vec::new();
    append_varint_field(&mut row, 1, account_uid(account).unwrap_or(1));
    append_varint_field(&mut row, 2, rank.max(1) as u64);
    append_bytes_field(
        &mut row,
        3,
        json_string(account.get("character").unwrap_or(&Value::Null), "name")
            .unwrap_or_else(|| "local".to_owned())
            .as_bytes(),
    );
    append_varint_field(&mut row, 4, state_i64(activity, "merits").max(0) as u64);
    append_varint_field(&mut row, 6, 0);
    append_varint_field(&mut row, 8, u64::from(current_unix_seconds()));
    row
}

fn guild_big_activity_user_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, state_i64(state, "points").max(0) as u64);
    append_varint_field(
        &mut output,
        2,
        state_i64(state, "presentCount").max(0) as u64,
    );
    append_varint_field(&mut output, 3, state_i64(state, "rank").max(0) as u64);
    output
}

fn guild_big_activity_rate_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, state_i64(state, "rate").max(0) as u64);
    append_varint_field(
        &mut output,
        2,
        state_i64(state, "rate").saturating_add(1).max(0) as u64,
    );
    append_varint_field(
        &mut output,
        3,
        100i64
            .saturating_sub(state_i64(state, "presentCount"))
            .max(0) as u64,
    );
    append_varint_field(&mut output, 4, 100);
    output
}

fn guild_big_activity_rank_payload(server_state: &ServerState, current: &Value) -> Vec<u8> {
    let mut guilds = std::collections::BTreeMap::<u64, (String, i64, i64)>::new();
    for (_, account) in account_directory(server_state, current) {
        let guild = account.get("guild").unwrap_or(&Value::Null);
        let guild_id = json_u64(guild, "guildId").unwrap_or(0);
        let guild_name = json_string(guild, "name").unwrap_or_else(|| "BlueOath".to_owned());
        let points = state_i64(
            account.get("guildBigActivity").unwrap_or(&Value::Null),
            "points",
        )
        .max(0);
        let member_count = guild
            .get("members")
            .and_then(Value::as_array)
            .map(|members| members.len() as i64)
            .unwrap_or(1)
            .max(1);
        let entry = guilds.entry(guild_id).or_insert((guild_name, 0, 0));
        entry.1 = entry.1.saturating_add(points);
        entry.2 = entry.2.saturating_add(member_count);
    }
    let mut ranked = guilds.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .1
             .1
            .cmp(&left.1 .1)
            .then_with(|| left.0.cmp(&right.0))
    });
    let mut output = Vec::new();
    for (index, (guild_id, (name, points, member_count))) in ranked.iter().enumerate() {
        append_message_field(
            &mut output,
            1,
            &guild_big_activity_rank_row(
                *guild_id,
                index.saturating_add(1),
                name,
                *points,
                *member_count,
            ),
        );
    }
    let current_guild_id = current
        .get("guild")
        .and_then(|guild| json_u64(guild, "guildId"))
        .unwrap_or(0);
    let current_rank = ranked
        .iter()
        .position(|(guild_id, _)| *guild_id == current_guild_id)
        .map(|rank| rank.saturating_add(1))
        .unwrap_or(1);
    let current_state = current.get("guildBigActivity").unwrap_or(&Value::Null);
    let current_name = json_string(current.get("guild").unwrap_or(&Value::Null), "name")
        .unwrap_or_else(|| "BlueOath".to_owned());
    append_message_field(
        &mut output,
        2,
        &guild_big_activity_rank_row(
            current_guild_id,
            current_rank,
            &current_name,
            state_i64(current_state, "points").max(0),
            1,
        ),
    );
    output
}

fn guild_big_activity_rank_row(
    guild_id: u64,
    rank: usize,
    name: &str,
    points: i64,
    member_count: i64,
) -> Vec<u8> {
    let mut row = Vec::new();
    append_varint_field(&mut row, 1, guild_id);
    append_varint_field(&mut row, 2, rank.max(1) as u64);
    append_varint_field(&mut row, 3, points.max(0) as u64);
    append_varint_field(&mut row, 4, member_count.max(1) as u64);
    append_bytes_field(&mut row, 5, name.as_bytes());
    append_varint_field(&mut row, 6, 1);
    append_varint_field(&mut row, 7, points.max(0) as u64);
    row
}

fn hero_awaken_finish_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        u64::from(
            state
                .get("isFinished")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
    );
    output
}

fn hero_awaken_milestone_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, state_i64(state, "totalPt").max(0) as u64);
    for value in state
        .get("claimedMilestones")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
    {
        append_varint_field(&mut output, 2, value.max(0) as u64);
    }
    output
}

fn state_i64(state: &Value, key: &str) -> i64 {
    state.get(key).and_then(Value::as_i64).unwrap_or_default()
}

fn push_unique_i32(target: &mut Value, value: i32) {
    if value <= 0 {
        return;
    }
    let values = target.as_array_mut().expect("state list must be an array");
    if !values
        .iter()
        .any(|entry| entry.as_i64() == Some(i64::from(value)))
    {
        values.push(json!(value));
    }
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

    #[test]
    fn big_activity_rank_aggregates_accounts_and_keeps_current_rank() {
        let root = std::env::temp_dir().join(format!(
            "blueoath-big-rank-{}-{}",
            std::process::id(),
            current_unix_millis()
        ));
        let store = blueoath_storage::ProfileStore::open(&root).unwrap();
        store
            .legacy_json_accounts()
            .save(
                "lower",
                &json!({
                    "character": {"uid": 1, "name": "Lower"},
                    "bigActivity": {"merits": 10}
                }),
            )
            .unwrap();
        store
            .legacy_json_accounts()
            .save(
                "higher",
                &json!({
                    "character": {"uid": 2, "name": "Higher"},
                    "bigActivity": {"merits": 30}
                }),
            )
            .unwrap();
        let mut server_state = ServerState::new("local", "Local", "1.4.0");
        server_state.social_store = Some(store);
        let current = json!({
            "character": {"uid": 1, "name": "Lower"},
            "bigActivity": {"merits": 10}
        });

        let payload = big_activity_rank_payload(&server_state, &current, 1);
        let rows = decode_repeated_message_field(&payload, 1);
        assert_eq!(rows.len(), 2);
        assert_eq!(decode_varint_u64_field(&rows[0], 1), 2);
        assert_eq!(decode_varint_field(&rows[0], 2), 1);
        assert_eq!(decode_varint_u64_field(&rows[1], 1), 1);
        assert_eq!(decode_varint_field(&rows[1], 2), 2);
        assert_eq!(
            decode_varint_field(&decode_repeated_message_field(&payload, 2)[0], 2),
            2
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
