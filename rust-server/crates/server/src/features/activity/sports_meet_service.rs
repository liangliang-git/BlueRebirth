use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;
use crate::game_config::GameplayCatalog;

pub(crate) fn handle_typed(
    server_state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    catalog: &GameplayCatalog,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
    match method {
        "sportsmeet.GetSportsTickCount" => reply(method, tick_count_payload_typed(account)),
        "sportsmeet.GetPointsRewardDetail" => reply(method, points_detail_payload_typed(account)),
        "sportsmeet.ReceivePointsReward" => {
            let points = match SportsMeetPointsRequest::decode(request_args) {
                Ok(request) => request.points,
                Err(_) => return invalid("sports meet points are invalid"),
            };
            receive_points_reward_typed(
                server_state,
                account,
                catalog,
                method,
                Some(points),
                effects,
            )
        }
        "sportsmeet.ReceiveAllPointsReward" => {
            receive_points_reward_typed(server_state, account, catalog, method, None, effects)
        }
        "sportsmeetrank.GetOwnerRankData" => reply(method, owner_rank_payload_typed(account)),
        "sportsmeetrank.GetAttackBeeRank" => reply(method, rank_payload_typed(account, 2)),
        "sportsmeetrank.GetTrackRank" => reply(method, rank_payload_typed(account, 2)),
        "sportsmeetrank.GetSteeplechaseRank" => reply(method, rank_payload_typed(account, 2)),
        _ => HandlerResult::Error(GameError::InvalidRequest(
            "sports meet operation is not supported",
        )),
    }
}

fn invalid(message: &'static str) -> HandlerResult {
    HandlerResult::Error(GameError::InvalidRequest(message))
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn tick_count_payload_typed(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    for (copy_id, free_count) in &account.sports_meet.free_counts {
        let mut encoded = Vec::new();
        append_varint_field(&mut encoded, 1, *copy_id);
        append_varint_field(&mut encoded, 2, u64::from(*free_count));
        append_message_field(&mut output, 1, &encoded);
    }
    append_varint_field(&mut output, 2, u64::from(account.sports_meet.tick_count));
    output
}

fn points_detail_payload_typed(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, account.sports_meet.points);
    for points in &account.sports_meet.received_points {
        append_varint_field(&mut output, 2, *points);
    }
    output
}

fn receive_points_reward_typed(
    server_state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    catalog: &GameplayCatalog,
    method: &str,
    requested_points: Option<u64>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let points_to_receive = if let Some(points) = requested_points {
        if points == 0
            || points > account.sports_meet.points
            || account.sports_meet.received_points.contains(&points)
        {
            Vec::new()
        } else {
            vec![points]
        }
    } else {
        let mut points = catalog
            .sportsmeet_awards
            .values()
            .filter_map(|row| u64::try_from(row.score).ok())
            .filter(|score| {
                *score <= account.sports_meet.points
                    && !account.sports_meet.received_points.contains(score)
            })
            .collect::<Vec<_>>();
        points.sort_unstable();
        points.dedup();
        points
    };
    let configured = points_to_receive
        .into_iter()
        .filter_map(|points| {
            let rewards = sportsmeet_rewards(catalog, points);
            (!rewards.is_empty()).then_some((points, rewards))
        })
        .collect::<Vec<_>>();
    let all_rewards = configured
        .iter()
        .flat_map(|(_, rewards)| rewards.iter().copied())
        .collect::<Vec<_>>();
    if all_rewards.is_empty() || !can_grant_typed_task_rewards(account, &all_rewards) {
        return reply(method, encode_rewards_list(&[]));
    }
    let points = configured
        .iter()
        .map(|(points, _)| *points)
        .collect::<Vec<_>>();
    for point in points {
        account.sports_meet.received_points.insert(point);
    }
    let mut rewards = Vec::new();
    for reward in all_rewards {
        if grant_typed_task_reward(account, &reward) {
            rewards.push(reward);
        } else {
            return HandlerResult::Error(GameError::InvalidState(
                "sports meet reward could not be granted",
            ));
        }
    }
    effects.push_pre(Response::raw(
        "user.UpdateUserInfo",
        UserInfoCodec::encode(&user_info_from_typed_account(server_state, account)),
    ));
    effects.push_pre(Response::raw(
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_typed_account(account)),
    ));
    reply(method, encode_rewards_list(&rewards))
}

fn sportsmeet_rewards(catalog: &GameplayCatalog, points: u64) -> Vec<ShopReward> {
    catalog
        .sportsmeet_awards
        .values()
        .find(|row| u64::try_from(row.score).ok() == Some(points))
        .and_then(|row| catalog.rewards_by_id.get(&row.reward_id))
        .cloned()
        .unwrap_or_default()
}

fn owner_rank_payload_typed(_account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    for _ in 0..3 {
        append_message_field(&mut output, 1, &owner_rank_node(1, 0, 0));
    }
    output
}

fn owner_rank_node(rank: u64, score: u64, copy_id: u64) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, rank);
    append_varint_field(&mut output, 2, score);
    append_varint_field(&mut output, 3, copy_id);
    output
}

fn rank_payload_typed(account: &blueoath_domain::AccountState, rank: u64) -> Vec<u8> {
    let node = owner_rank_node(account.character.uid, 0, rank);
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &node);
    append_message_field(&mut output, 2, &node);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_sports_rewards_are_idempotent() {
        let state = ServerState::new("sports", "Captain", "1.4.0");
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("sports").unwrap(),
            "Sports",
        );
        account.sports_meet.points = 20;
        let mut catalog = GameplayCatalog::default();
        catalog.sportsmeet_awards.insert(
            1,
            SportsMeetAwardConfig {
                score: 20,
                reward_id: 1,
            },
        );
        catalog.rewards_by_id.insert(
            1,
            vec![ShopReward {
                goods_type: 5,
                item_id: 1,
                num: 2,
                instance_id: 0,
            }],
        );
        let mut effects = ResponseEffects::default();
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                &catalog,
                "sportsmeet.ReceivePointsReward",
                &[8, 20],
                &mut effects,
            ),
            HandlerResult::Reply(_)
        ));
        assert!(account.sports_meet.received_points.contains(&20));
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                &catalog,
                "sportsmeet.ReceivePointsReward",
                &[8, 20],
                &mut effects,
            ),
            HandlerResult::Reply(_)
        ));
    }
}
