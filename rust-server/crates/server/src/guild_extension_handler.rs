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
        MethodFamily::GuildOffer | MethodFamily::GuildOfferRank | MethodFamily::GuildWar
    )
}

pub(super) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let progress = &mut account.activities.progress;
    match method {
        "guildwar.GetGuildwarInfo" => reply(method, guildwar_info_payload_typed(progress)),
        "guildwar.GetBaseInfo" => reply(method, guildwar_base_payload_typed(progress)),
        "guildwar.GetHeroLockInfo" => reply(method, guildwar_hero_lock_payload_typed(progress)),
        "guildwar.GetRankList" => reply(method, guildwar_rank_payload_typed(progress)),
        "guildwar.GetRankUserList" => reply(
            method,
            guildwar_user_rank_payload_typed(account.character.uid, progress),
        ),
        "guildwar.GetBattleReport" => reply(method, guildwar_battle_report_payload_typed(progress)),
        "guildwar.BattleReport" => {
            reply(method, guildwar_battle_report_info_payload_typed(progress))
        }
        "guildwar.GetGuildReward" => {
            let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
            reply(method, guildwar_reward_list_payload(catalog))
        }
        "guildwar.GetHaveScores" => {
            let mut output = Vec::new();
            append_varint_field(
                &mut output,
                1,
                decode_varint_field(request_args, 1).max(0) as u64,
            );
            append_varint_field(
                &mut output,
                2,
                u64::from(typed_guildwar_value(progress, "guildWar:score") > 0),
            );
            reply(method, output)
        }
        "guildwar.GetHaveGuildReward" => {
            let mut output = Vec::new();
            append_varint_field(
                &mut output,
                1,
                u64::from(typed_guildwar_value(progress, "guildWar:rewardClaimed") == 0),
            );
            reply(method, output)
        }
        "guildwar.GetGuildGradeId" => {
            let mut output = Vec::new();
            append_varint_field(&mut output, 1, 1);
            append_varint_field(&mut output, 2, 0);
            reply(method, output)
        }
        "guildwar.UpdateBaseInfo" => {
            progress.insert(
                "guildWar:baseId".to_owned(),
                decode_varint_field(request_args, 1).max(1) as u64,
            );
            progress.insert(
                "guildWar:stageId".to_owned(),
                decode_varint_field(request_args, 2).max(1) as u64,
            );
            reply(method, guildwar_base_payload_typed(progress))
        }
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
            "guild extension method is unsupported",
        )),
    }
}

fn typed_guildwar_value(progress: &std::collections::BTreeMap<String, u64>, key: &str) -> u64 {
    progress.get(key).copied().unwrap_or_default()
}

fn guildwar_base_payload_typed(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        typed_guildwar_value(progress, "guildWar:baseId").max(1),
    );
    append_varint_field(
        &mut output,
        2,
        typed_guildwar_value(progress, "guildWar:stageId").max(1),
    );
    append_varint_field(
        &mut output,
        3,
        typed_guildwar_value(progress, "guildWar:sectionId").max(1),
    );
    output
}

fn guildwar_info_payload_typed(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &guildwar_base_payload_typed(progress));
    append_varint_field(&mut output, 2, 0);
    append_varint_field(&mut output, 3, 0);
    output
}

fn guildwar_rank_payload_typed(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut row = Vec::new();
    append_varint_field(&mut row, 1, 1);
    append_varint_field(&mut row, 2, 1);
    append_bytes_field(&mut row, 3, b"BlueOath");
    append_varint_field(&mut row, 4, 1);
    append_varint_field(
        &mut row,
        5,
        typed_guildwar_value(progress, "guildWar:score"),
    );
    append_varint_field(&mut row, 6, 1);
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &row);
    append_message_field(&mut output, 2, &row);
    output
}

fn guildwar_user_rank_payload_typed(
    uid: u64,
    progress: &std::collections::BTreeMap<String, u64>,
) -> Vec<u8> {
    let score = typed_guildwar_value(progress, "guildWar:score");
    let mut user = Vec::new();
    append_varint_field(&mut user, 1, 1);
    append_varint_field(&mut user, 2, uid);
    append_varint_field(&mut user, 4, score);
    append_varint_field(&mut user, 5, score);
    append_varint_field(&mut user, 6, 0);
    let mut base = Vec::new();
    append_varint_field(
        &mut base,
        1,
        typed_guildwar_value(progress, "guildWar:baseId").max(1),
    );
    append_message_field(&mut base, 2, &user);
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &base);
    output
}

fn guildwar_hero_lock_payload_typed(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut output = Vec::new();
    for key in progress.keys() {
        if let Some(hero_id) = key
            .strip_prefix("guildWar:heroLock:")
            .and_then(|value| value.parse::<u64>().ok())
        {
            append_varint_field(&mut output, 1, hero_id);
        }
    }
    output
}

fn guildwar_battle_report_payload_typed(
    progress: &std::collections::BTreeMap<String, u64>,
) -> Vec<u8> {
    let mut output = Vec::new();
    let mut report_ids = std::collections::BTreeSet::new();
    for key in progress.keys() {
        if let Some(report_id) = key
            .strip_prefix("guildWar:report:")
            .and_then(|value| value.split_once(':'))
            .and_then(|(id, _)| id.parse::<u64>().ok())
        {
            report_ids.insert(report_id);
        }
    }
    for report_id in report_ids {
        append_message_field(
            &mut output,
            1,
            &guildwar_battle_report_info_typed(progress, report_id),
        );
    }
    output
}

fn guildwar_battle_report_info_payload_typed(
    progress: &std::collections::BTreeMap<String, u64>,
) -> Vec<u8> {
    let report_id = progress
        .keys()
        .filter_map(|key| {
            key.strip_prefix("guildWar:report:")
                .and_then(|value| value.split_once(':'))
                .and_then(|(id, _)| id.parse::<u64>().ok())
        })
        .max();
    report_id
        .map(|id| guildwar_battle_report_info_typed(progress, id))
        .unwrap_or_default()
}

fn guildwar_battle_report_info_typed(
    progress: &std::collections::BTreeMap<String, u64>,
    report_id: u64,
) -> Vec<u8> {
    let prefix = format!("guildWar:report:{report_id}:");
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        typed_guildwar_value(progress, &format!("{prefix}uid")).max(1),
    );
    append_bytes_field(&mut output, 2, b"BlueOath");
    for (field, name) in [
        (3, "baseId"),
        (4, "sectionId"),
        (5, "damage"),
        (6, "reportTime"),
        (7, "reportType"),
        (8, "score"),
    ] {
        append_varint_field(
            &mut output,
            field,
            typed_guildwar_value(progress, &format!("{prefix}{name}")),
        );
    }
    output
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

    #[test]
    fn typed_guildwar_uses_activity_progress() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("guild-war-typed").unwrap(),
            "Captain",
        );
        let mut update = Vec::new();
        append_varint_field(&mut update, 1, 7);
        append_varint_field(&mut update, 2, 3);
        assert!(matches!(
            handle_typed(&mut account, "guildwar.UpdateBaseInfo", &update),
            HandlerResult::Reply(_)
        ));
        assert_eq!(account.activities.progress.get("guildWar:baseId"), Some(&7));
        let HandlerResult::Reply(response) =
            handle_typed(&mut account, "guildwar.GetGuildwarInfo", &[])
        else {
            panic!("expected guild war response");
        };
        assert_eq!(decode_repeated_message_field(&response.payload, 1).len(), 1);
    }
}
