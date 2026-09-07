use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(super) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    state: &ServerState,
    method: &str,
    request_args: &[u8],
    task_catalog: Option<&TaskCatalog>,
    post_pushes: &mut Vec<Vec<u8>>,
) -> HandlerResult {
    if !matches!(
        method,
        "task.TaskReward"
            | "task.TaskRewardByDaysActivity"
            | "task.TaskSevenDayActivity"
            | "task.TaskRewardByReturnActivity"
    ) {
        return HandlerResult::Empty;
    }
    let Ok(request) = TaskRewardRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest("task reward request is invalid"));
    };
    let Some(catalog) = task_catalog else {
        return HandlerResult::Error(GameError::CatalogUnavailable);
    };
    let task_type = if request.task_type == 0 {
        catalog
            .definitions
            .iter()
            .find(|definition| definition.id as u64 == request.task_id)
            .map(|definition| definition.task_type)
            .unwrap_or_default()
    } else {
        request.task_type as i32
    };
    let Some(definition) = catalog.definitions.iter().find(|definition| {
        definition.id as u64 == request.task_id && definition.task_type == task_type
    }) else {
        return HandlerResult::Error(GameError::NotFound("task"));
    };
    if typed_task_claimed(account, request.task_id) {
        return HandlerResult::Error(GameError::InvalidState("task reward was already claimed"));
    }
    if !typed_task_completed(account, request.task_id, definition.goal)
        || !typed_task_visible(account, catalog, definition)
    {
        return HandlerResult::Error(GameError::InvalidState("task is not complete"));
    }

    let mut rewards = task_rewards(Some(catalog), task_type, definition.id);
    if definition.medal_id > 0 {
        rewards.push(ShopReward {
            goods_type: 16,
            item_id: definition.medal_id,
            num: 1,
            instance_id: 0,
        });
    }
    if rewards.is_empty()
        || rewards.iter().any(|reward| {
            reward.goods_type == 16
                || (reward.goods_type != 5 && !matches!(reward.goods_type, 1 | 6))
        })
    {
        return HandlerResult::Error(GameError::InvalidState("typed task reward is unsupported"));
    }
    if rewards
        .iter()
        .any(|reward| !can_grant_typed_task_reward(account, reward))
    {
        return HandlerResult::Error(GameError::InvalidState(
            "typed task reward cannot be granted",
        ));
    }
    for reward in &rewards {
        // Preflight above makes this infallible for supported reward kinds.
        let _ = grant_typed_task_reward(account, reward);
    }
    complete_typed_task(account, request.task_id);

    append_method_push(
        post_pushes,
        "task.TaskInfo",
        task_info_payload_from_typed_account(account, Some(catalog)),
    );
    append_method_push(
        post_pushes,
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_typed_account(account)),
    );
    append_method_push(
        post_pushes,
        "user.UpdateUserInfo",
        UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
    );
    HandlerResult::Reply(Response::raw(
        method,
        encode_task_reward(request.task_id as i32, &rewards),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_task_reward_updates_currency_and_claim_state() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("task").unwrap(),
            "Task",
        );
        account.tasks.progress.insert(101, 1);
        let catalog = TaskCatalog {
            definitions: vec![TaskDefinition {
                task_type: 1,
                id: 101,
                goal: 1,
                inline_rewards: vec![(5, 1, 100)],
                ..TaskDefinition::default()
            }],
            ..TaskCatalog::default()
        };
        let state = ServerState::new("task", "Task", "test");
        let mut pushes = Vec::new();
        let result = handle_typed(
            &mut account,
            &state,
            "task.TaskReward",
            &[0x08, 101, 0x10, 1],
            Some(&catalog),
            &mut pushes,
        );
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert!(account.tasks.claimed.contains(&101));
        assert_eq!(
            account
                .resources
                .amount(blueoath_domain::CurrencyKind::Gold)
                .get(),
            100
        );
        assert_eq!(pushes.len(), 3);
    }
}
