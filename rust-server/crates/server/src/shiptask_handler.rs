use serde_json::{json, Value};

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
            let ship_tid = decode_varint_field(request_args, 1).max(0) as u64;
            let task_id = decode_varint_field(request_args, 2).max(0) as u64;
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
            let ship_tid = decode_varint_field(request_args, 1).max(0) as u64;
            let achievement_id = decode_varint_field(request_args, 2).max(0) as u64;
            let already_claimed = account.ship_task.achievements.iter().any(|achievement| {
                achievement.ship_tid == ship_tid
                    && achievement.id == achievement_id
                    && achievement.claimed
            });
            if !already_claimed {
                let Some(reward_id) = catalog
                    .testship_rewards
                    .get(&(achievement_id as i32))
                    .and_then(|row| json_i32(row, "reward"))
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
            account.ship_task.current_ship_tid = decode_varint_field(request_args, 1).max(0) as u64;
            account.ship_task.current_hero_template_id =
                decode_varint_field(request_args, 2).max(0) as u64;
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

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let Some(account) = context.account.as_deref_mut() else {
        return HandlerResult::Error(GameError::AccountUnavailable);
    };
    match method {
        "shiptask.GetShipTaskReward" => {
            let payload = {
                let ship_tid = decode_varint_field(request_args, 1);
                let task_id = decode_varint_field(request_args, 2);
                let state = shiptask_state_mut(account);
                state["currentShipTid"] = json!(ship_tid);
                let tasks = state["tasks"].as_array_mut().expect("shiptask tasks array");
                if !tasks.iter().any(|task| {
                    json_i32(task, "shipTid") == Some(ship_tid)
                        && json_i32(task, "taskId") == Some(task_id)
                }) {
                    tasks.push(json!({
                        "shipTid": ship_tid,
                        "taskId": task_id,
                        "status": 1,
                        "count": 1,
                    }));
                } else {
                    for task in tasks.iter_mut().filter(|task| {
                        json_i32(task, "shipTid") == Some(ship_tid)
                            && json_i32(task, "taskId") == Some(task_id)
                    }) {
                        task["status"] = json!(1);
                    }
                }
                shiptask_info_payload(account)
            };
            append_shiptask_push(context, payload);
            HandlerResult::PushOnly
        }
        "shiptask.GetAchievementReward" => {
            let payload = {
                let ship_tid = decode_varint_field(request_args, 1);
                let achievement_id = decode_varint_field(request_args, 2);
                let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
                let already_claimed = account
                    .get("shiptask")
                    .and_then(|value| value.get("achievements"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|row| {
                        json_i32(row, "shipTid") == Some(ship_tid)
                            && json_i32(row, "id") == Some(achievement_id)
                            && json_i32(row, "get") == Some(1)
                    });
                if !already_claimed {
                    if let Some(reward_id) = catalog
                        .testship_rewards
                        .get(&achievement_id)
                        .and_then(|row| json_i32(row, "reward"))
                    {
                        let granted = catalog
                            .rewards_by_id
                            .get(&reward_id)
                            .cloned()
                            .unwrap_or_default()
                            .into_iter()
                            .map(|reward| {
                                grant_reward(
                                    account,
                                    reward,
                                    current_unix_seconds(),
                                    context.catalogs.fashion,
                                )
                            })
                            .collect::<Vec<_>>();
                        if !granted.is_empty() {
                            append_method_push(
                                context.post_pushes,
                                "user.UpdateUserInfo",
                                UserInfoCodec::encode(&user_info_from_account(
                                    context.state,
                                    Some(account),
                                )),
                            );
                        }
                    }
                }
                let state = shiptask_state_mut(account);
                let rewards = state["achievements"]
                    .as_array_mut()
                    .expect("shiptask achievements array");
                if let Some(row) = rewards.iter_mut().find(|row| {
                    json_i32(row, "shipTid") == Some(ship_tid)
                        && json_i32(row, "id") == Some(achievement_id)
                }) {
                    row["get"] = json!(1);
                } else {
                    rewards.push(json!({
                        "shipTid": ship_tid,
                        "id": achievement_id,
                        "get": 1,
                    }));
                }
                state["currentShipTid"] = json!(ship_tid);
                shiptask_info_payload(account)
            };
            append_shiptask_push(context, payload);
            HandlerResult::PushOnly
        }
        "shiptask.SetCurrentShip" => {
            let payload = {
                let state = shiptask_state_mut(account);
                state["currentShipTid"] = json!(decode_varint_field(request_args, 1));
                state["currentHeroTemplateId"] = json!(decode_varint_field(request_args, 2));
                state["setShipTime"] = json!(current_unix_seconds());
                shiptask_info_payload(account)
            };
            append_shiptask_push(context, payload);
            HandlerResult::PushOnly
        }
        _ => HandlerResult::Empty,
    }
}

fn append_shiptask_push<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    payload: Vec<u8>,
) {
    append_method_push(context.post_pushes, "shiptask.UpdateShipTaskInfo", payload);
}

fn shiptask_state_mut(account: &mut Value) -> &mut Value {
    account
        .as_object_mut()
        .expect("account must be an object")
        .entry("shiptask".to_owned())
        .or_insert_with(|| {
            json!({
                "currentShipTid": 0,
                "currentHeroTemplateId": 0,
                "setShipTime": 0,
                "tasks": [],
                "achievements": [],
                "extraMvp": [],
            })
        })
}

fn shiptask_info_payload(account: &Value) -> Vec<u8> {
    let state = account.get("shiptask").unwrap_or(&Value::Null);
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        state
            .get("currentShipTid")
            .and_then(Value::as_i64)
            .unwrap_or_default()
            .max(0) as u64,
    );
    for task in state
        .get("tasks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut row = Vec::new();
        append_varint_field(
            &mut row,
            1,
            json_i32(task, "taskId").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut row,
            2,
            json_i32(task, "status").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut row,
            3,
            json_i32(task, "count").unwrap_or_default().max(0) as u64,
        );
        append_message_field(&mut output, 2, &row);
    }
    for achievement in state
        .get("achievements")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut row = Vec::new();
        append_varint_field(
            &mut row,
            1,
            json_i32(achievement, "id").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut row,
            2,
            json_i32(achievement, "get").unwrap_or_default().max(0) as u64,
        );
        append_message_field(&mut output, 3, &row);
    }
    for extra in state
        .get("extraMvp")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut row = Vec::new();
        append_varint_field(
            &mut row,
            1,
            json_i32(extra, "copyId").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut row,
            2,
            json_i32(extra, "count").unwrap_or_default().max(0) as u64,
        );
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
    fn handler_exposes_typed_result() {
        let _: for<'state, 'account, 'scratch> fn(
            &mut GameLoginRequestContext<'state, 'account, 'scratch>,
            &str,
            &[u8],
        ) -> HandlerResult = handle;
    }

    #[test]
    fn shiptask_payload_contains_current_ship_and_rows() {
        let account = json!({
            "shiptask": {
                "currentShipTid": 12,
                "tasks": [{"taskId": 3, "status": 1, "count": 2}],
                "achievements": [{"id": 4, "get": 1}],
                "extraMvp": []
            }
        });
        let payload = shiptask_info_payload(&account);
        assert!(payload.windows(2).any(|window| window == [0x08, 0x0C]));
        assert!(payload.contains(&0x12));
        assert!(payload.contains(&0x1A));
    }
}
