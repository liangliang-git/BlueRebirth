use super::super::*;

use crate::common::error::GameError;
use crate::common::response::{HandlerResult, Response, ResponseEffects};

// 处理星级奖励
pub(crate) fn handle(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    chapter_catalog: Option<&ChapterCatalog>,
    task_catalog: Option<&TaskCatalog>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let Ok(request) = CopyStarRewardRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "copy star reward request is invalid",
        ));
    };
    let chapter_id = request.chapter_id;
    let indexes: Vec<i64> = request
        .indexes
        .iter()
        .map(|index| i64::from(*index))
        .collect();
    let Some(chapter_rewards) =
        chapter_catalog.and_then(|catalog| catalog.star_rewards_by_chapter.get(&chapter_id))
    else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "copy star reward chapter was not found",
        ));
    };
    let Some(task_catalog) = task_catalog else {
        return HandlerResult::Error(GameError::InvalidState(
            "copy reward catalog is unavailable",
        ));
    };
    if chapter_id <= 0 || indexes.is_empty() {
        return HandlerResult::Error(GameError::InvalidRequest(
            "copy star reward request is invalid",
        ));
    }
    let Ok(chapter_id_u32) = u32::try_from(chapter_id) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "copy star reward chapter is invalid",
        ));
    };
    let requested_indexes = indexes.clone();
    let star_num = chapter_rewards
        .level_ids
        .iter()
        .filter_map(|copy_id| {
            let copy_id = u64::try_from(*copy_id)
                .ok()
                .and_then(|id| blueoath_domain::CopyId::new(id).ok())?;
            if !account.battle.passed_copies.contains(&copy_id) {
                return None;
            }
            Some(
                account
                    .battle
                    .copy_stars
                    .get(&copy_id)
                    .copied()
                    .unwrap_or(7),
            )
        })
        .map(|stars| i32::try_from(stars.min(7).count_ones()).unwrap_or(i32::MAX))
        .sum::<i32>();
    let mut pending = Vec::new();
    let mut pending_indexes = std::collections::BTreeSet::new();
    for index in indexes {
        let Some(position) = usize::try_from(index.saturating_sub(1)).ok() else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "copy star reward index is invalid",
            ));
        };
        let Some(required_stars) = chapter_rewards.star_conditions.get(position).copied() else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "copy star reward index is invalid",
            ));
        };
        let Some(reward_id) = chapter_rewards.reward_ids.get(position).copied() else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "copy star reward is not configured",
            ));
        };
        let reward_index = u32::try_from(index).unwrap_or_default();
        if !pending_indexes.insert(index)
            || star_num < required_stars
            || account
                .battle
                .claimed_star_rewards
                .contains(&(chapter_id_u32, reward_index))
        {
            return HandlerResult::Error(GameError::InvalidState(
                "copy star reward is unavailable",
            ));
        }
        let Some(rewards) = task_catalog.rewards_by_id.get(&reward_id) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "copy star reward is not configured",
            ));
        };
        if rewards.is_empty() {
            return HandlerResult::Error(GameError::InvalidRequest(
                "copy star reward is not configured",
            ));
        }
        pending.extend(rewards.iter().map(|(goods_type, item_id, num)| ShopReward {
            goods_type: *goods_type,
            item_id: *item_id,
            num: *num,
            instance_id: 0,
        }));
    }
    if !can_grant_typed_task_rewards(account, &pending) {
        return HandlerResult::Error(GameError::InvalidState(
            "copy star reward type is unsupported",
        ));
    }
    let mut pending = pending;
    if !grant_typed_task_rewards_with_fashion(account, &mut pending, None) {
        return HandlerResult::Error(GameError::InvalidState(
            "copy star reward could not be granted",
        ));
    }
    for index in requested_indexes {
        account
            .battle
            .claimed_star_rewards
            .insert((chapter_id_u32, u32::try_from(index).unwrap_or_default()));
    }
    effects.push_pre(Response::raw(
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_typed_account(account)),
    ));
    HandlerResult::Reply(Response::raw(method, encode_task_reward_list(&pending)))
}
