use super::common::error::GameError;
use super::common::response::HandlerResult;
use super::*;
use blueoath_domain::AccountState;

pub(super) fn handle_typed(
    account: &mut AccountState,
    state: &ServerState,
    method: &str,
    request_args: &[u8],
    post_pushes: &mut Vec<Vec<u8>>,
) -> HandlerResult {
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    match method {
        "shiptask.GetShipTaskReward" => {
            let request = match ShipTaskRewardRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidRequest("ship task reward"))
                }
            };
            let ship_tid = request.ship_tid;
            let task_id = request.task_id;
            if let Some(task) = account
                .ship_task
                .tasks
                .iter_mut()
                .find(|task| task.ship_tid == ship_tid && task.task_id == task_id)
            {
                task.status = 1;
            } else {
                account
                    .ship_task
                    .tasks
                    .push(blueoath_domain::ShipTaskTaskState {
                        ship_tid,
                        task_id,
                        status: 1,
                        count: 1,
                    });
            }
            account.ship_task.current_ship_tid = ship_tid;
            append_shiptask_typed_push(post_pushes, account);
            HandlerResult::PushOnly
        }
        "shiptask.GetAchievementReward" => {
            let request = match ShipTaskRewardRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidRequest("ship task achievement"))
                }
            };
            let ship_tid = request.ship_tid;
            let achievement_id = request.task_id;
            let already_claimed = account.ship_task.achievements.iter().any(|achievement| {
                achievement.ship_tid == ship_tid
                    && achievement.id == achievement_id
                    && achievement.claimed
            });
            if !already_claimed {
                let Some(reward_id) = catalog
                    .testship_rewards
                    .get(&(achievement_id as i32))
                    .map(|row| row.reward_id)
                else {
                    return HandlerResult::Error(GameError::NotFound("ship task reward"));
                };
                let rewards = catalog
                    .rewards_by_id
                    .get(&reward_id)
                    .cloned()
                    .unwrap_or_default();
                if rewards.is_empty() || !can_grant_typed_task_rewards(account, &rewards) {
                    return HandlerResult::Error(GameError::InvalidState(
                        "ship task reward is unsupported",
                    ));
                }
                for reward in &rewards {
                    let _ = grant_typed_task_reward(account, reward);
                }
                append_method_push(
                    post_pushes,
                    "user.UpdateUserInfo",
                    UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
                );
            }
            if let Some(achievement) =
                account
                    .ship_task
                    .achievements
                    .iter_mut()
                    .find(|achievement| {
                        achievement.ship_tid == ship_tid && achievement.id == achievement_id
                    })
            {
                achievement.claimed = true;
            } else {
                account
                    .ship_task
                    .achievements
                    .push(blueoath_domain::ShipTaskAchievementState {
                        ship_tid,
                        id: achievement_id,
                        claimed: true,
                    });
            }
            account.ship_task.current_ship_tid = ship_tid;
            append_shiptask_typed_push(post_pushes, account);
            HandlerResult::PushOnly
        }
        "shiptask.SetCurrentShip" => {
            let request = match ShipTaskCurrentShipRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "ship task current ship",
                    ))
                }
            };
            account.ship_task.current_ship_tid = request.ship_tid;
            account.ship_task.current_hero_template_id = request.hero_template_id;
            account.ship_task.set_ship_time = current_unix_seconds() as u64;
            append_shiptask_typed_push(post_pushes, account);
            HandlerResult::PushOnly
        }
        _ => HandlerResult::Empty,
    }
}

fn append_shiptask_typed_push(post_pushes: &mut Vec<Vec<u8>>, account: &AccountState) {
    append_method_push(
        post_pushes,
        "shiptask.UpdateShipTaskInfo",
        typed_shiptask_info_payload(account),
    );
}

fn typed_shiptask_info_payload(account: &AccountState) -> Vec<u8> {
    let state = &account.ship_task;
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, state.current_ship_tid);
    for task in &state.tasks {
        let mut row = Vec::new();
        append_varint_field(&mut row, 1, task.task_id);
        append_varint_field(&mut row, 2, task.status);
        append_varint_field(&mut row, 3, task.count);
        append_message_field(&mut output, 2, &row);
    }
    for achievement in &state.achievements {
        let mut row = Vec::new();
        append_varint_field(&mut row, 1, achievement.id);
        append_varint_field(&mut row, 2, u64::from(achievement.claimed));
        append_message_field(&mut output, 3, &row);
    }
    for extra in &state.extra_mvp {
        let mut row = Vec::new();
        append_varint_field(&mut row, 1, extra.copy_id);
        append_varint_field(&mut row, 2, extra.count);
        append_message_field(&mut output, 4, &row);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_shiptask_updates_state_and_pushes_info() {
        let mut account = AccountState::default();
        let mut pushes = Vec::new();
        let result = handle_typed(
            &mut account,
            &ServerState::new("shiptask", "Captain", "test"),
            "shiptask.GetShipTaskReward",
            &[0x08, 12, 0x10, 3],
            &mut pushes,
        );
        assert!(matches!(result, HandlerResult::PushOnly));
        assert_eq!(account.ship_task.current_ship_tid, 12);
        assert_eq!(account.ship_task.tasks[0].task_id, 3);
        assert_eq!(pushes.len(), 1);
    }

    #[test]
    fn shiptask_payload_contains_current_ship_and_rows() {
        let mut account = AccountState::default();
        account.ship_task.current_ship_tid = 12;
        account
            .ship_task
            .tasks
            .push(blueoath_domain::ShipTaskTaskState {
                ship_tid: 12,
                task_id: 3,
                status: 1,
                count: 2,
            });
        account
            .ship_task
            .achievements
            .push(blueoath_domain::ShipTaskAchievementState {
                ship_tid: 12,
                id: 4,
                claimed: true,
            });
        let payload = typed_shiptask_info_payload(&account);
        assert!(payload.windows(2).any(|window| window == [0x08, 0x0C]));
        assert!(payload.contains(&0x12));
        assert!(payload.contains(&0x1A));
    }
}
