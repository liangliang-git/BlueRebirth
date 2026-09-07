use serde_json::{json, Value};

use super::*;

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    match method {
        "shiptask.GetShipTaskReward" => {
            let payload = {
                let account = context.account.as_deref_mut()?;
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
            Some(Vec::new())
        }
        "shiptask.GetAchievementReward" => {
            let payload = {
                let account = context.account.as_deref_mut()?;
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
            Some(Vec::new())
        }
        "shiptask.SetCurrentShip" => {
            let payload = {
                let account = context.account.as_deref_mut()?;
                let state = shiptask_state_mut(account);
                state["currentShipTid"] = json!(decode_varint_field(request_args, 1));
                state["currentHeroTemplateId"] = json!(decode_varint_field(request_args, 2));
                state["setShipTime"] = json!(current_unix_seconds());
                shiptask_info_payload(account)
            };
            append_shiptask_push(context, payload);
            Some(Vec::new())
        }
        _ => None,
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
