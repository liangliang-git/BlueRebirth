use serde_json::{json, Value};

use super::*;

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    match method {
        "sportsmeet.GetSportsTickCount" => Some(tick_count_payload(context.account.as_deref()?)),
        "sportsmeet.GetPointsRewardDetail" => {
            Some(points_detail_payload(context.account.as_deref()?))
        }
        "sportsmeet.ReceivePointsReward" => {
            let points = decode_varint_field(request_args, 1);
            Some(receive_points_reward(context, catalog, Some(points.max(0))))
        }
        "sportsmeet.ReceiveAllPointsReward" => Some(receive_points_reward(context, catalog, None)),
        "sportsmeetrank.GetOwnerRankData" => Some(owner_rank_payload(context.account.as_deref()?)),
        "sportsmeetrank.GetAttackBeeRank" => Some(rank_payload(
            context.account.as_deref()?,
            2,
            RankKind::AttackBee,
        )),
        "sportsmeetrank.GetTrackRank" => Some(rank_payload(
            context.account.as_deref()?,
            2,
            RankKind::Track,
        )),
        "sportsmeetrank.GetSteeplechaseRank" => Some(rank_payload(
            context.account.as_deref()?,
            2,
            RankKind::Steeplechase,
        )),
        _ => None,
    }
}

fn sports_state_mut(account: &mut Value) -> &mut Value {
    account
        .as_object_mut()
        .expect("account must be an object")
        .entry("sportsMeet".to_owned())
        .or_insert_with(|| {
            json!({
                "tickCount": 10,
                "freeCounts": [],
                "points": 0,
                "receivedPoints": []
            })
        })
}

fn tick_count_payload(account: &Value) -> Vec<u8> {
    let state = account.get("sportsMeet").unwrap_or(&Value::Null);
    let mut output = Vec::new();
    for row in state
        .get("freeCounts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut encoded = Vec::new();
        append_varint_field(
            &mut encoded,
            1,
            json_i32(row, "copyId").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            2,
            json_i32(row, "freeCount").unwrap_or_default().max(0) as u64,
        );
        append_message_field(&mut output, 1, &encoded);
    }
    append_varint_field(
        &mut output,
        2,
        state
            .get("tickCount")
            .and_then(Value::as_i64)
            .unwrap_or_default()
            .max(0) as u64,
    );
    output
}

fn points_detail_payload(account: &Value) -> Vec<u8> {
    let state = account.get("sportsMeet").unwrap_or(&Value::Null);
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        state
            .get("points")
            .and_then(Value::as_i64)
            .unwrap_or_default()
            .max(0) as u64,
    );
    for point in state
        .get("receivedPoints")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
    {
        append_varint_field(&mut output, 2, point.max(0) as u64);
    }
    output
}

fn receive_points_reward<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    catalog: &GameplayCatalog,
    requested_points: Option<i32>,
) -> Vec<u8> {
    let Some(account) = context.account.as_deref_mut() else {
        return encode_rewards_list(&[]);
    };
    let total_points = account
        .get("sportsMeet")
        .and_then(|state| state.get("points"))
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .clamp(0, i64::from(i32::MAX)) as i32;
    let received = account
        .get("sportsMeet")
        .and_then(|state| state.get("receivedPoints"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_i64)
                .filter_map(|value| i32::try_from(value).ok())
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let points_to_receive = if let Some(points) = requested_points {
        if points <= 0 || points > total_points || received.contains(&points) {
            Vec::new()
        } else {
            vec![points]
        }
    } else {
        let mut points = catalog
            .sportsmeet_awards
            .values()
            .filter_map(|row| json_i32(row, "score"))
            .filter(|score| *score > 0 && *score <= total_points && !received.contains(score))
            .collect::<Vec<_>>();
        points.sort_unstable();
        points.dedup();
        points
    };
    if points_to_receive.is_empty() {
        return encode_rewards_list(&[]);
    }
    let configured = points_to_receive
        .into_iter()
        .filter_map(|points| {
            let rewards = sportsmeet_rewards(catalog, points);
            (!rewards.is_empty()).then_some((points, rewards))
        })
        .collect::<Vec<_>>();
    if configured.is_empty() {
        return encode_rewards_list(&[]);
    }
    let state = sports_state_mut(account);
    let received_points = state
        .get_mut("receivedPoints")
        .and_then(Value::as_array_mut)
        .expect("received sports points array");
    for (points, _) in &configured {
        received_points.push(json!(points));
    }
    let mut rewards = Vec::new();
    for (_, configured_rewards) in configured {
        rewards.extend(configured_rewards.into_iter().map(|reward| {
            grant_reward(
                account,
                reward,
                current_unix_seconds(),
                context.catalogs.fashion,
            )
        }));
    }
    append_method_push(
        context.pre_pushes,
        "user.UpdateUserInfo",
        UserInfoCodec::encode(&user_info_from_account(context.state, Some(account))),
    );
    append_method_push(
        context.pre_pushes,
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_account(account)),
    );
    encode_rewards_list(&rewards)
}

fn sportsmeet_rewards(catalog: &GameplayCatalog, points: i32) -> Vec<ShopReward> {
    catalog
        .sportsmeet_awards
        .values()
        .find(|row| json_i32(row, "score") == Some(points))
        .and_then(|row| {
            json_i32(row, "rewards")
                .or_else(|| json_i32(row, "reward"))
                .and_then(|reward_id| catalog.rewards_by_id.get(&reward_id))
        })
        .cloned()
        .unwrap_or_default()
}

fn owner_rank_payload(account: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &owner_rank_node(1, 0, 0));
    append_message_field(&mut output, 2, &owner_rank_node(1, 0, 0));
    append_message_field(&mut output, 3, &owner_rank_node(1, 0, 0));
    let _ = account;
    output
}

fn owner_rank_node(rank: i32, score: i32, copy_id: i32) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, rank.max(0) as u64);
    append_varint_field(&mut output, 2, score.max(0) as u64);
    append_varint_field(&mut output, 3, copy_id.max(0) as u64);
    output
}

#[derive(Clone, Copy)]
enum RankKind {
    AttackBee,
    Track,
    Steeplechase,
}

fn rank_payload(account: &Value, rank: i32, kind: RankKind) -> Vec<u8> {
    let uid = account
        .get("character")
        .and_then(|value| json_u64(value, "uid"))
        .unwrap_or(1);
    let mut node = Vec::new();
    append_varint_field(&mut node, 1, uid);
    append_varint_field(&mut node, 2, 0);
    append_varint_field(&mut node, 3, rank.max(0) as u64);
    let mut output = Vec::new();
    append_message_field(&mut output, 1, &node);
    append_message_field(&mut output, 2, &node);
    let _ = kind;
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_reward_is_idempotent() {
        let mut account =
            json!({"sportsMeet": {"points": 20, "receivedPoints": []}, "character": {"gold": 0}});
        let state = ServerState::new("sports", "Captain", "1.4.0");
        let mut pre = Vec::new();
        let mut post = Vec::new();
        let mut err = 0;
        let mut msg = String::new();
        let mut details = None;
        let mut rewards = Vec::new();
        let mut hero_ids = Vec::new();
        let mut mvp = None;
        let mut wrecked = std::collections::HashSet::new();
        let mut account_slot = Some(&mut account);
        let mut ctx = GameLoginRequestContext {
            state: &state,
            account: &mut account_slot,
            catalogs: GameLoginCatalogs::empty(),
            pre_pushes: &mut pre,
            post_pushes: &mut post,
            response_err: &mut err,
            response_err_msg: &mut msg,
            pass_details: &mut details,
            pass_rewards: &mut rewards,
            pass_hero_ids: &mut hero_ids,
            pass_mvp_hero_id: &mut mvp,
            pass_shipwrecked_ids: &mut wrecked,
        };
        let mut catalog = GameplayCatalog::default();
        catalog
            .sportsmeet_awards
            .insert(1, json!({"score": 20, "rewards": 1}));
        catalog.rewards_by_id.insert(
            1,
            vec![ShopReward {
                goods_type: 5,
                item_id: 1,
                num: 2,
                instance_id: 0,
            }],
        );
        let first = receive_points_reward(&mut ctx, &catalog, Some(20));
        assert!(!first.is_empty());
        let second = receive_points_reward(&mut ctx, &catalog, Some(20));
        assert!(second.is_empty());
    }
}
