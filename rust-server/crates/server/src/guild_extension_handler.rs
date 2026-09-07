use serde_json::{json, Value};

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(super) fn handles(method: &str) -> bool {
    matches!(
        GameMethod::parse(method).family(),
        MethodFamily::GuildOffer | MethodFamily::GuildOfferRank | MethodFamily::GuildWar
    )
}

pub(super) fn handles_typed(method: &str) -> bool {
    matches!(
        GameMethod::parse(method).family(),
        MethodFamily::GuildOffer | MethodFamily::GuildOfferRank
    )
}

pub(super) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let progress = &mut account.activities.progress;
    match method {
        "guildOffer.GetGuildOffer" => reply(method, guild_offer_payload_typed(progress)),
        "guildOfferUser.GetGuildOfferUser" => {
            reply(method, guild_offer_user_payload_typed(progress))
        }
        "guildOffer.GuildOffer" | "guildOffer.GuildOfferUser" => {
            let task_id = decode_varint_field(request_args, 1);
            if task_id <= 0 {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "guild offer task id is invalid",
                ));
            }
            progress.insert(format!("guildOffer:offer:{task_id}:completed"), 1);
            HandlerResult::PushOnly
        }
        "guildOffer.AddOffer" => {
            let task_id = decode_varint_field(request_args, 1);
            let task_index = decode_varint_field(request_args, 2);
            if task_id <= 0 {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "guild offer task id is invalid",
                ));
            }
            let prefix = format!("guildOffer:offer:{task_id}:");
            progress
                .entry(format!("{prefix}index"))
                .or_insert_with(|| u64::try_from(task_index.max(0)).unwrap_or_default());
            progress.entry(format!("{prefix}quality")).or_insert(1);
            progress.entry(format!("{prefix}progress")).or_insert(0);
            progress.entry(format!("{prefix}completed")).or_insert(0);
            HandlerResult::PushOnly
        }
        "guildOffer.AbandonOffer" => {
            let task_id = decode_varint_field(request_args, 1);
            let prefix = format!("guildOffer:offer:{task_id}:");
            progress.retain(|key, _| !key.starts_with(&prefix));
            HandlerResult::PushOnly
        }
        "guildOffer.BuyOfferCount" => {
            let count = progress
                .entry("guildOffer:dailyBuyCount".to_owned())
                .or_default();
            *count = count.saturating_add(1);
            HandlerResult::PushOnly
        }
        "guildOffer.GetRankList" | "guildofferrank.GetGuildRankList" => {
            reply(method, guild_offer_rank_payload_typed(progress))
        }
        "guildOffer.ReceiveOfferRewardPerson"
        | "guildOffer.ReceiveOfferRewardGuild"
        | "guildOffer.ReceiveOfferRewardAll" => HandlerResult::Error(GameError::InvalidRequest(
            "guild offer reward requires typed reward catalog",
        )),
        _ => HandlerResult::Error(GameError::InvalidRequest(
            "guild offer method is unsupported",
        )),
    }
}

fn typed_offer_value(progress: &std::collections::BTreeMap<String, u64>, key: &str) -> u64 {
    progress.get(key).copied().unwrap_or_default()
}

fn guild_offer_payload_typed(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        typed_offer_value(progress, "guildOffer:guildPoints"),
    );
    let mut offers = std::collections::BTreeSet::new();
    for key in progress.keys() {
        if let Some(task_id) = key
            .strip_prefix("guildOffer:offer:")
            .and_then(|key| key.split_once(':'))
            .and_then(|(id, _)| id.parse::<u64>().ok())
        {
            offers.insert(task_id);
        }
    }
    for task_id in offers {
        let prefix = format!("guildOffer:offer:{task_id}:");
        let mut offer = Vec::new();
        append_varint_field(&mut offer, 1, task_id);
        append_varint_field(
            &mut offer,
            2,
            typed_offer_value(progress, &format!("{prefix}index")),
        );
        append_varint_field(
            &mut offer,
            3,
            typed_offer_value(progress, &format!("{prefix}quality")).max(1),
        );
        append_varint_field(
            &mut offer,
            4,
            typed_offer_value(progress, &format!("{prefix}progress")),
        );
        append_varint_field(
            &mut offer,
            5,
            typed_offer_value(progress, &format!("{prefix}completed")),
        );
        append_message_field(&mut output, 2, &offer);
    }
    output
}

fn guild_offer_user_payload_typed(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, 35);
    append_varint_field(
        &mut output,
        2,
        typed_offer_value(progress, "guildOffer:dailyBuyCount"),
    );
    append_varint_field(
        &mut output,
        7,
        typed_offer_value(progress, "guildOffer:guildPoints"),
    );
    append_varint_field(
        &mut output,
        8,
        typed_offer_value(progress, "guildOffer:personalPoints"),
    );
    output
}

fn guild_offer_rank_payload_typed(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut row = Vec::new();
    append_varint_field(&mut row, 1, 1);
    append_varint_field(
        &mut row,
        2,
        typed_offer_value(progress, "guildOffer:guildPoints"),
    );
    append_varint_field(&mut row, 3, 1);
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &row);
    append_varint_field(&mut output, 2, 1);
    output
}

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    if GameMethod::parse(method).is_family(MethodFamily::GuildWar) {
        return handle_guildwar(context, method, request_args);
    }
    handle_guild_offer(context, method, request_args)
}

fn guild_offer_state_mut(account: &mut Value) -> &mut Value {
    account
        .as_object_mut()
        .expect("account must be an object")
        .entry("guildOffer".to_owned())
        .or_insert_with(|| {
            json!({
                "offers": [],
                "personalPoints": 0,
                "guildPoints": 0,
                "claimedPersonal": [],
                "claimedGuild": [],
                "dailyBuyCount": 0
            })
        })
}

fn handle_guild_offer<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    match method {
        "guildOffer.GetGuildOffer" => reply(
            method,
            guild_offer_payload(context.account.as_deref().unwrap_or(&Value::Null)),
        ),
        "guildOfferUser.GetGuildOfferUser" => reply(
            method,
            guild_offer_user_payload(context.account.as_deref().unwrap_or(&Value::Null)),
        ),
        "guildOffer.GuildOffer" | "guildOffer.GuildOfferUser" => {
            let task_id = decode_varint_field(request_args, 1);
            let Some(account) = context.account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            let state = guild_offer_state_mut(account);
            if task_id > 0 {
                let offers = state["offers"]
                    .as_array_mut()
                    .expect("offers must be an array");
                if let Some(offer) = offers.iter_mut().find(|offer| {
                    offer.get("taskId").and_then(Value::as_i64) == Some(i64::from(task_id))
                }) {
                    offer["completed"] = json!(true);
                }
            }
            HandlerResult::PushOnly
        }
        "guildOffer.AddOffer" => {
            let task_id = decode_varint_field(request_args, 1);
            let task_index = decode_varint_field(request_args, 2);
            let Some(account) = context.account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            let state = guild_offer_state_mut(account);
            let offers = state["offers"]
                .as_array_mut()
                .expect("offers must be an array");
            if task_id > 0
                && !offers.iter().any(|offer| {
                    offer.get("taskId").and_then(Value::as_i64) == Some(i64::from(task_id))
                })
            {
                offers.push(json!({
                    "taskId": task_id,
                    "taskIndex": task_index,
                    "quality": 1,
                    "progress": 0,
                    "completed": false
                }));
            }
            HandlerResult::PushOnly
        }
        "guildOffer.AbandonOffer" => {
            let task_id = decode_varint_field(request_args, 1);
            let Some(account) = context.account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            let state = guild_offer_state_mut(account);
            state["offers"]
                .as_array_mut()
                .expect("offers must be an array")
                .retain(|offer| {
                    offer.get("taskId").and_then(Value::as_i64) != Some(i64::from(task_id))
                });
            HandlerResult::PushOnly
        }
        "guildOffer.ReceiveOfferRewardPerson" => handle_guild_offer_reward(context, true),
        "guildOffer.ReceiveOfferRewardGuild" | "guildOffer.ReceiveOfferRewardAll" => {
            handle_guild_offer_reward(context, false)
        }
        "guildOffer.BuyOfferCount" => {
            let Some(account) = context.account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            let state = guild_offer_state_mut(account);
            state["dailyBuyCount"] = json!(state_i64(state, "dailyBuyCount").saturating_add(1));
            HandlerResult::PushOnly
        }
        "guildOffer.GetRankList" | "guildofferrank.GetGuildRankList" => reply(
            method,
            guild_offer_rank_payload(context.account.as_deref().unwrap_or(&Value::Null)),
        ),
        _ => HandlerResult::Empty,
    }
}

fn handle_guild_offer_reward<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    personal: bool,
) -> HandlerResult {
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    let Some(account) = context.account.as_deref_mut() else {
        return HandlerResult::Error(GameError::AccountUnavailable);
    };
    let points_key = if personal {
        "personalPoints"
    } else {
        "guildPoints"
    };
    let claimed_key = if personal {
        "claimedPersonal"
    } else {
        "claimedGuild"
    };
    let state = account.get("guildOffer").unwrap_or(&Value::Null);
    let points = state_i64(state, points_key);
    let rows = if personal {
        &catalog.guild_offer_personal_rewards
    } else {
        &catalog.guild_offer_rewards
    };
    let mut claim_ids = Vec::new();
    let mut reward_ids = Vec::new();
    for (id, row) in rows {
        let threshold = json_i32(row, "score").unwrap_or_default();
        let claimed = state[claimed_key].as_array().is_some_and(|values| {
            values
                .iter()
                .any(|value| value.as_i64() == Some(i64::from(*id)))
        });
        if i64::from(threshold) <= points && !claimed {
            if let Some(reward_id) = json_i32(row, "reward") {
                reward_ids.push(reward_id);
            }
            claim_ids.push(*id);
        }
    }
    let mut rewards = Vec::new();
    for reward_id in reward_ids {
        rewards.extend(grant_rewards_by_id(
            account,
            catalog,
            reward_id,
            context.catalogs.fashion,
        ));
    }
    let state = guild_offer_state_mut(account);
    for id in claim_ids {
        state[claimed_key]
            .as_array_mut()
            .expect("claimed rewards must be an array")
            .push(json!(id));
    }
    reply(
        if personal {
            "guildOffer.ReceiveOfferRewardPerson"
        } else {
            "guildOffer.ReceiveOfferRewardGuild"
        },
        encode_rewards_list(&rewards),
    )
}

fn guild_offer_payload(account: &Value) -> Vec<u8> {
    let state = account.get("guildOffer").unwrap_or(&Value::Null);
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        state_i64(state, "guildPoints").max(0) as u64,
    );
    if let Some(offers) = state.get("offers").and_then(Value::as_array) {
        for offer in offers {
            let mut item = Vec::new();
            append_varint_field(
                &mut item,
                1,
                json_i32(offer, "taskId").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut item,
                2,
                json_i32(offer, "taskIndex").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut item,
                3,
                json_i32(offer, "quality").unwrap_or(1).max(0) as u64,
            );
            append_varint_field(
                &mut item,
                4,
                json_i32(offer, "progress").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut item,
                5,
                u64::from(
                    offer
                        .get("completed")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                ),
            );
            append_message_field(&mut output, 2, &item);
        }
    }
    output
}

fn guild_offer_user_payload(account: &Value) -> Vec<u8> {
    let state = account.get("guildOffer").unwrap_or(&Value::Null);
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, 35);
    append_varint_field(
        &mut output,
        2,
        state_i64(state, "dailyBuyCount").max(0) as u64,
    );
    append_varint_field(&mut output, 3, 0);
    append_varint_field(&mut output, 4, 0);
    append_varint_field(&mut output, 5, 0);
    append_varint_field(&mut output, 6, 0);
    append_varint_field(
        &mut output,
        7,
        state_i64(state, "guildPoints").max(0) as u64,
    );
    append_varint_field(
        &mut output,
        8,
        state_i64(state, "personalPoints").max(0) as u64,
    );
    output
}

fn guild_offer_rank_payload(account: &Value) -> Vec<u8> {
    let state = account.get("guildOffer").unwrap_or(&Value::Null);
    let mut row = Vec::new();
    append_varint_field(&mut row, 1, 1);
    append_varint_field(&mut row, 2, state_i64(state, "guildPoints").max(0) as u64);
    append_varint_field(&mut row, 3, 1);
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &row);
    append_varint_field(&mut output, 2, 1);
    output
}

fn handle_guildwar<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    match method {
        "guildwar.GetGuildwarInfo" => reply(
            method,
            guildwar_info_payload(context.account.as_deref().unwrap_or(&Value::Null)),
        ),
        "guildwar.GetBaseInfo" => reply(
            method,
            guildwar_base_payload(context.account.as_deref().unwrap_or(&Value::Null)),
        ),
        "guildwar.GetHeroLockInfo" => reply(
            method,
            guildwar_hero_lock_payload(context.account.as_deref().unwrap_or(&Value::Null)),
        ),
        "guildwar.GetRankList" => reply(
            method,
            guildwar_rank_payload(context.account.as_deref().unwrap_or(&Value::Null)),
        ),
        "guildwar.GetRankUserList" => reply(
            method,
            guildwar_user_rank_payload(context.account.as_deref().unwrap_or(&Value::Null)),
        ),
        "guildwar.GetBattleReport" => reply(
            method,
            guildwar_battle_report_payload(context.account.as_deref().unwrap_or(&Value::Null)),
        ),
        "guildwar.BattleReport" => reply(
            method,
            guildwar_battle_report_info_payload(context.account.as_deref().unwrap_or(&Value::Null)),
        ),
        "guildwar.GetGuildReward" => handle_guildwar_reward(context),
        "guildwar.GetHaveScores" => {
            let score = context
                .account
                .as_deref()
                .and_then(|account| account.get("guildWar"))
                .map(|state| state_i64(state, "score"))
                .unwrap_or_default();
            let mut output = Vec::new();
            append_varint_field(
                &mut output,
                1,
                decode_varint_field(request_args, 1).max(0) as u64,
            );
            append_varint_field(&mut output, 2, u64::from(score > 0));
            reply(method, output)
        }
        "guildwar.GetHaveGuildReward" => {
            let claimed = context
                .account
                .as_deref()
                .and_then(|account| account.get("guildWar"))
                .and_then(|state| state.get("rewardClaimed"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut output = Vec::new();
            append_varint_field(&mut output, 1, u64::from(!claimed));
            reply(method, output)
        }
        "guildwar.GetGuildGradeId" => {
            let mut output = Vec::new();
            append_varint_field(&mut output, 1, 1);
            append_varint_field(&mut output, 2, 0);
            reply(method, output)
        }
        "guildwar.UpdateBaseInfo" => {
            let Some(account) = context.account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            let state = guild_war_state_mut(account);
            state["baseId"] = json!(decode_varint_field(request_args, 1));
            state["stageId"] = json!(decode_varint_field(request_args, 2));
            reply(method, guildwar_base_payload(account))
        }
        _ => HandlerResult::Empty,
    }
}

fn guild_war_state_mut(account: &mut Value) -> &mut Value {
    account
        .as_object_mut()
        .expect("account must be an object")
        .entry("guildWar".to_owned())
        .or_insert_with(|| json!({"baseId": 1, "stageId": 1, "sectionId": 1, "score": 0, "rewardClaimed": false, "heroLockList": [], "reports": []}))
}

fn handle_guildwar_reward<'state, 'account, 'scratch>(
    _context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
) -> HandlerResult {
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    reply(
        "guildwar.GetGuildReward",
        guildwar_reward_list_payload(catalog),
    )
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn guildwar_reward_list_payload(catalog: &GameplayCatalog) -> Vec<u8> {
    let mut output = Vec::new();
    for row in catalog.guild_war_rewards.values() {
        let base_id = json_i32(row, "base_id").unwrap_or_default();
        let stage = json_i32(row, "stage").unwrap_or_default();
        let reward_id = json_i32(row, "guild_reward").unwrap_or_default();
        if base_id <= 0 || stage <= 0 || reward_id <= 0 {
            continue;
        }
        let mut base = Vec::new();
        append_varint_field(&mut base, 1, base_id as u64);
        append_varint_field(&mut base, 2, 1);
        append_varint_field(&mut base, 3, 1);
        if let Some(rewards) = catalog.rewards_by_id.get(&reward_id) {
            for reward in rewards {
                let mut item = Vec::new();
                append_varint_field(&mut item, 1, reward.goods_type.max(0) as u64);
                append_varint_field(&mut item, 2, reward.item_id.max(0) as u64);
                append_varint_field(&mut item, 3, reward.num.max(0) as u64);
                append_message_field(&mut base, 4, &item);
            }
        }
        append_varint_field(&mut base, 5, stage as u64);
        append_message_field(&mut output, 1, &base);
    }
    output
}

fn guildwar_base_payload(account: &Value) -> Vec<u8> {
    let state = account.get("guildWar").unwrap_or(&Value::Null);
    let base_id = state_i64(state, "baseId").max(1) as i32;
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, base_id as u64);
    append_varint_field(&mut output, 2, state_i64(state, "stageId").max(1) as u64);
    append_varint_field(&mut output, 3, state_i64(state, "sectionId").max(1) as u64);
    output
}

fn guildwar_info_payload(account: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &guildwar_base_payload(account));
    append_varint_field(&mut output, 2, 0);
    append_varint_field(&mut output, 3, 0);
    output
}

fn guildwar_rank_payload(account: &Value) -> Vec<u8> {
    let state = account.get("guildWar").unwrap_or(&Value::Null);
    let row = guildwar_rank_data_payload(state, 1);
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &row);
    append_message_field(&mut output, 2, &row);
    output
}

fn guildwar_rank_data_payload(state: &Value, rank: i32) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, 1);
    append_varint_field(&mut output, 2, 1);
    append_bytes_field(&mut output, 3, b"BlueOath");
    append_varint_field(&mut output, 4, rank.max(1) as u64);
    append_varint_field(&mut output, 5, state_i64(state, "score").max(0) as u64);
    append_varint_field(&mut output, 6, 1);
    output
}

fn guildwar_user_rank_payload(account: &Value) -> Vec<u8> {
    let state = account.get("guildWar").unwrap_or(&Value::Null);
    let uid = account
        .get("character")
        .and_then(|value| json_u64(value, "uid"))
        .unwrap_or(1);
    let mut user = Vec::new();
    append_varint_field(&mut user, 1, 1);
    append_varint_field(&mut user, 2, uid);
    append_varint_field(&mut user, 4, state_i64(state, "score").max(0) as u64);
    append_varint_field(&mut user, 5, state_i64(state, "score").max(0) as u64);
    append_varint_field(&mut user, 6, 0);
    let mut base = Vec::new();
    append_varint_field(&mut base, 1, state_i64(state, "baseId").max(1) as u64);
    append_message_field(&mut base, 2, &user);
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &base);
    output
}

fn guildwar_hero_lock_payload(account: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    if let Some(ids) = account
        .get("guildWar")
        .and_then(|state| state.get("heroLockList"))
        .and_then(Value::as_array)
    {
        for id in ids.iter().filter_map(Value::as_i64) {
            append_varint_field(&mut output, 1, id.max(0) as u64);
        }
    }
    output
}

fn guildwar_battle_report_payload(account: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    if let Some(reports) = account
        .get("guildWar")
        .and_then(|state| state.get("reports"))
        .and_then(Value::as_array)
    {
        for report in reports {
            append_message_field(&mut output, 1, &guildwar_battle_report_info(report));
        }
    }
    output
}

fn guildwar_battle_report_info_payload(account: &Value) -> Vec<u8> {
    account
        .get("guildWar")
        .and_then(|state| state.get("reports"))
        .and_then(Value::as_array)
        .and_then(|reports| reports.last())
        .map(guildwar_battle_report_info)
        .unwrap_or_default()
}

fn guildwar_battle_report_info(report: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, json_u64(report, "uid").unwrap_or(1));
    append_bytes_field(&mut output, 2, b"BlueOath");
    append_varint_field(
        &mut output,
        3,
        json_i32(report, "baseId").unwrap_or(1).max(0) as u64,
    );
    append_varint_field(
        &mut output,
        4,
        json_i32(report, "sectionId").unwrap_or(1).max(0) as u64,
    );
    append_varint_field(
        &mut output,
        5,
        json_i32(report, "damage").unwrap_or(0).max(0) as u64,
    );
    append_varint_field(
        &mut output,
        6,
        json_i32(report, "reportTime").unwrap_or(0).max(0) as u64,
    );
    append_varint_field(
        &mut output,
        7,
        json_i32(report, "reportType").unwrap_or(0).max(0) as u64,
    );
    append_varint_field(
        &mut output,
        8,
        json_i32(report, "score").unwrap_or(0).max(0) as u64,
    );
    output
}

fn state_i64(state: &Value, key: &str) -> i64 {
    state.get(key).and_then(Value::as_i64).unwrap_or_default()
}

fn grant_rewards_by_id(
    account: &mut Value,
    catalog: &GameplayCatalog,
    reward_id: i32,
    fashion_catalog: Option<&blueoath_protocol::FashionList>,
) -> Vec<ShopReward> {
    catalog
        .rewards_by_id
        .get(&reward_id)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|reward| grant_reward(account, reward, current_unix_seconds(), fashion_catalog))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_guild_offer_uses_activity_progress() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("guild-offer-typed").unwrap(),
            "Captain",
        );
        let mut add = Vec::new();
        append_varint_field(&mut add, 1, 12);
        append_varint_field(&mut add, 2, 4);
        assert!(matches!(
            handle_typed(&mut account, "guildOffer.AddOffer", &add),
            HandlerResult::PushOnly
        ));
        assert!(account
            .activities
            .progress
            .contains_key("guildOffer:offer:12:index"));
        let HandlerResult::Reply(response) =
            handle_typed(&mut account, "guildOffer.GetGuildOffer", &[])
        else {
            panic!("expected guild offer response");
        };
        assert_eq!(decode_repeated_message_field(&response.payload, 2).len(), 1);
        assert!(matches!(
            handle_typed(&mut account, "guildOffer.BuyOfferCount", &[]),
            HandlerResult::PushOnly
        ));
        assert_eq!(
            account.activities.progress.get("guildOffer:dailyBuyCount"),
            Some(&1)
        );
    }
}
