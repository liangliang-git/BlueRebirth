use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;
fn big_activity_rank_row(uid: u64, name: &str, merits: u64, rank: usize) -> Vec<u8> {
    let mut row = Vec::new();
    append_varint_field(&mut row, 1, uid);
    append_varint_field(&mut row, 2, rank.max(1) as u64);
    append_bytes_field(&mut row, 3, name.as_bytes());
    append_varint_field(&mut row, 4, merits);
    append_varint_field(&mut row, 6, 0);
    append_varint_field(&mut row, 8, u64::from(current_unix_seconds()));
    row
}

pub(crate) fn handles(method: &str) -> bool {
    matches!(
        GameMethod::parse(method).family(),
        MethodFamily::BigActivity | MethodFamily::GuildBigActivity | MethodFamily::HeroAwaken
    )
}

pub(crate) fn handle_typed(
    server_state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    match method {
        "bigactivity.GetBigActivityInfo" => {
            HandlerResult::Reply(Response::raw(method, typed_big_activity_payload(account)))
        }
        "bigactivity.GetBigActivityRank" | "bigactivity.GetBigActivityRankEx" => {
            let Ok(request) = BigActivityRankRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "big activity rank request is invalid",
                ));
            };
            let start = request.start.max(1);
            HandlerResult::Reply(Response::raw(
                method,
                typed_big_activity_rank_payload(server_state, account, start),
            ))
        }
        "guildbigactivity.UserData" => {
            HandlerResult::Reply(Response::raw(method, typed_guild_activity_payload(account)))
        }
        "guildbigactivity.GuildRateData" => {
            HandlerResult::Reply(Response::raw(method, typed_guild_rate_payload(account)))
        }
        "guildbigactivity.PresentItem" => {
            let Ok(request) = GuildActivityPresentRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "guild activity request is invalid",
                ));
            };
            let item_id = request.item_id;
            let count = request.count.clamp(1, 99);
            let progress = &mut account.activities.progress;
            progress.insert(
                "guildBigActivity\u{1f}lastItemId".to_owned(),
                item_id as u64,
            );
            let count_key = "guildBigActivity\u{1f}presentCount".to_owned();
            let current = progress.get(&count_key).copied().unwrap_or_default();
            progress.insert(count_key, current.saturating_add(count as u64));
            HandlerResult::Reply(Response::raw(method, typed_guild_activity_payload(account)))
        }
        "guildbigactivityrank.GetGuildRankList" => {
            HandlerResult::Reply(Response::raw(method, typed_guild_rank_payload(account)))
        }
        "heroawaken.FinishAwaken" => {
            let Ok(request) = HeroAwakenFinishRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "hero awaken request is invalid",
                ));
            };
            account.activities.progress.insert(
                "heroAwaken\u{1f}isFinished".to_owned(),
                u64::from(request.finished),
            );
            HandlerResult::Reply(Response::raw(
                method,
                typed_hero_awaken_finish_payload(account),
            ))
        }
        "heroawaken.MilestoneInfo" => HandlerResult::Reply(Response::raw(
            method,
            typed_hero_awaken_milestone_payload(account),
        )),
        "heroawaken.RewardMilestone" => handle_typed_hero_awaken_reward(account, request_args),
        _ => HandlerResult::Empty,
    }
}

fn typed_activity_value(account: &blueoath_domain::AccountState, key: &str) -> u64 {
    account
        .activities
        .progress
        .get(key)
        .copied()
        .unwrap_or_default()
}

fn typed_big_activity_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let merits = typed_activity_value(account, "bigActivity\u{1f}merits");
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, merits);
    append_varint_field(&mut output, 4, merits);
    output
}

fn typed_big_activity_rank_payload(
    server_state: &ServerState,
    current: &blueoath_domain::AccountState,
    start: i32,
) -> Vec<u8> {
    let mut entries = server_state
        .social_store
        .as_ref()
        .and_then(|store| store.list_typed_accounts().ok())
        .unwrap_or_default()
        .into_iter()
        .map(|account| {
            (
                account.character.uid,
                typed_activity_value(&account, "bigActivity\u{1f}merits"),
                account.character.name,
            )
        })
        .collect::<Vec<_>>();
    let current_merits = typed_activity_value(current, "bigActivity\u{1f}merits");
    if let Some(existing) = entries
        .iter_mut()
        .find(|(uid, _, _)| *uid == current.character.uid)
    {
        existing.1 = current_merits;
        existing.2 = current.character.name.clone();
    } else {
        entries.push((
            current.character.uid,
            current_merits,
            current.character.name.clone(),
        ));
    }
    entries.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let offset = usize::try_from(start.saturating_sub(1)).unwrap_or_default();
    let mut output = Vec::new();
    for (index, (uid, merits, name)) in entries.iter().skip(offset).take(50).enumerate() {
        append_message_field(
            &mut output,
            1,
            &big_activity_rank_row(
                *uid,
                name,
                *merits,
                offset.saturating_add(index).saturating_add(1),
            ),
        );
    }
    let current_rank = entries
        .iter()
        .position(|(uid, _, _)| *uid == current.character.uid)
        .map(|rank| rank.saturating_add(1))
        .unwrap_or(1);
    append_message_field(
        &mut output,
        2,
        &big_activity_rank_row(
            current.character.uid,
            &current.character.name,
            current_merits,
            current_rank,
        ),
    );
    output
}

fn typed_guild_activity_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let points = account
        .activities
        .progress
        .iter()
        .filter(|(key, _)| key.starts_with("guildBigActivity\u{1f}points:"))
        .map(|(_, value)| *value)
        .sum::<u64>();
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, points);
    append_varint_field(
        &mut output,
        2,
        typed_activity_value(account, "guildBigActivity\u{1f}presentCount"),
    );
    append_varint_field(&mut output, 3, 0);
    output
}

fn typed_guild_rate_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let points = account
        .activities
        .progress
        .iter()
        .filter(|(key, _)| key.starts_with("guildBigActivity\u{1f}points:"))
        .map(|(_, value)| *value)
        .sum::<u64>();
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, points);
    append_varint_field(&mut output, 2, points.saturating_add(1));
    append_varint_field(
        &mut output,
        3,
        100_u64.saturating_sub(typed_activity_value(
            account,
            "guildBigActivity\u{1f}presentCount",
        )),
    );
    append_varint_field(&mut output, 4, 100);
    output
}

fn typed_hero_awaken_finish_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        typed_activity_value(account, "heroAwaken\u{1f}isFinished"),
    );
    output
}

fn typed_hero_awaken_milestone_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        typed_activity_value(account, "heroAwaken\u{1f}totalPt"),
    );
    for key in account.activities.progress.keys() {
        if let Some(milestone) = key
            .strip_prefix("heroAwaken\u{1f}claimed:")
            .and_then(|value| value.parse::<u64>().ok())
        {
            append_varint_field(&mut output, 2, milestone);
        }
    }
    output
}

fn handle_typed_hero_awaken_reward(
    account: &mut blueoath_domain::AccountState,
    request_args: &[u8],
) -> HandlerResult {
    let Ok(request) = HeroAwakenRewardRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "hero awaken reward request is invalid",
        ));
    };
    let milestone = request.milestone;
    if typed_activity_value(account, "heroAwaken\u{1f}totalPt") < milestone as u64 {
        return HandlerResult::Error(GameError::InvalidState(
            "hero awaken milestone is not reached",
        ));
    }
    let claim_key = format!("heroAwaken\u{1f}claimed:{milestone}");
    if account.activities.progress.contains_key(&claim_key) {
        return HandlerResult::Reply(Response::raw(
            "heroawaken.RewardMilestone",
            encode_rewards_list(&[]),
        ));
    }
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    let reward_id = catalog
        .activity
        .get(&5002)
        .and_then(|activity| {
            activity
                .p4
                .iter()
                .find(|row| row.first().copied() == Some(milestone))
                .and_then(|row| row.get(1).copied())
        })
        .unwrap_or_default();
    let rewards = catalog
        .rewards_by_id
        .get(&reward_id)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|reward| ShopReward {
            goods_type: reward.goods_type,
            item_id: reward.item_id,
            num: reward.num,
            instance_id: reward.instance_id,
        })
        .collect::<Vec<_>>();
    if rewards.is_empty() || !rewards.iter().all(can_grant_typed_task_reward_for_activity) {
        return HandlerResult::Error(GameError::InvalidState("hero awaken reward is unsupported"));
    }
    if !can_grant_typed_task_rewards(account, &rewards) {
        return HandlerResult::Error(GameError::InvalidState(
            "hero awaken reward cannot be granted",
        ));
    }
    for reward in &rewards {
        let _ = grant_typed_task_reward(account, reward);
    }
    account.activities.progress.insert(claim_key, 1);
    HandlerResult::Reply(Response::raw(
        "heroawaken.RewardMilestone",
        encode_rewards_list(&rewards),
    ))
}

fn can_grant_typed_task_reward_for_activity(reward: &ShopReward) -> bool {
    reward.num > 0 && (reward.goods_type == 5 || matches!(reward.goods_type, 1 | 6))
}

fn typed_guild_rank_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let points = account
        .activities
        .progress
        .iter()
        .filter(|(key, _)| key.starts_with("guildBigActivity\u{1f}points:"))
        .map(|(_, value)| *value)
        .sum::<u64>();
    let mut rank = Vec::new();
    append_varint_field(&mut rank, 1, 0);
    append_bytes_field(&mut rank, 2, b"BlueOath");
    append_varint_field(&mut rank, 4, 1);
    append_varint_field(&mut rank, 5, points);
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &rank);
    append_message_field(&mut output, 2, &rank);
    output
}

#[cfg(test)]
mod tests {
    use crate::common::response::HandlerResult;

    use super::*;

    #[test]
    fn big_activity_rank_aggregates_accounts_and_keeps_current_rank() {
        let root = std::env::temp_dir().join(format!(
            "blueoath-big-rank-{}-{}",
            std::process::id(),
            current_unix_millis()
        ));
        let store = blueoath_storage::ProfileStore::open(&root).unwrap();
        let mut lower = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("lower").unwrap(),
            "Lower",
        );
        lower.character.uid = 1;
        lower
            .activities
            .progress
            .insert("bigActivity\u{1f}merits".to_owned(), 10);
        store.save_typed_account(&mut lower).unwrap();
        let mut higher = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("higher").unwrap(),
            "Higher",
        );
        higher.character.uid = 2;
        higher
            .activities
            .progress
            .insert("bigActivity\u{1f}merits".to_owned(), 30);
        store.save_typed_account(&mut higher).unwrap();
        let mut server_state = ServerState::new("local", "Local", "1.4.0");
        server_state.social_store = Some(store.clone());
        let current = lower;

        let payload = typed_big_activity_rank_payload(&server_state, &current, 1);
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

    #[test]
    fn typed_activity_extra_mutations_use_progress_state() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("activity-extra-typed").unwrap(),
            "Captain",
        );
        let state = ServerState::new("activity-extra-typed", "Captain", "test");
        let mut present = Vec::new();
        append_varint_field(&mut present, 1, 42);
        append_varint_field(&mut present, 2, 3);
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "guildbigactivity.PresentItem",
                &present,
            ),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account
                .activities
                .progress
                .get("guildBigActivity\u{1f}presentCount"),
            Some(&3)
        );

        let mut finish = Vec::new();
        append_varint_field(&mut finish, 1, 1);
        assert!(matches!(
            handle_typed(&state, &mut account, "heroawaken.FinishAwaken", &finish),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account
                .activities
                .progress
                .get("heroAwaken\u{1f}isFinished"),
            Some(&1)
        );
    }
}
