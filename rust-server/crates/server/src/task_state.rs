use serde_json::{json, Value};

use super::*;

pub(super) fn complete_task(
    account: &mut Value,
    task_type: i32,
    task_id: i32,
    goal: i32,
    now: u32,
) {
    let Some(account) = account.as_object_mut() else {
        return;
    };
    let tasks = account
        .entry("tasks".to_owned())
        .or_insert_with(|| json!({"records": []}));
    let Some(tasks) = tasks.as_object_mut() else {
        return;
    };
    let records = tasks
        .entry("records".to_owned())
        .or_insert_with(|| json!([]));
    let Some(records) = records.as_array_mut() else {
        return;
    };
    if let Some(record) = records.iter_mut().find(|r| {
        json_i32(r, "taskType") == Some(task_type) && json_i32(r, "taskId") == Some(task_id)
    }) {
        record["count"] = json!(goal);
        record["finishTime"] = json!(now);
        record["completed"] = json!(1);
        record["rewardTime"] = json!(now);
    } else {
        records.push(json!({"taskType": task_type, "taskId": task_id, "count": goal, "finishTime": now, "completed": 1, "rewardTime": now}));
    }
}

pub(super) fn task_claimed(account: &Value, task_type: i32, task_id: i32) -> bool {
    account
        .get("tasks")
        .and_then(|tasks| tasks.get("records"))
        .and_then(Value::as_array)
        .is_some_and(|records| {
            records.iter().any(|record| {
                json_i32(record, "taskType") == Some(task_type)
                    && json_i32(record, "taskId") == Some(task_id)
                    && json_i64(record, "rewardTime").unwrap_or_default() > 0
            })
        })
}

pub(super) fn task_completed(account: &Value, task_type: i32, task_id: i32, goal: i32) -> bool {
    account
        .get("tasks")
        .and_then(|tasks| tasks.get("records"))
        .and_then(Value::as_array)
        .is_some_and(|records| {
            records.iter().any(|record| {
                json_i32(record, "taskType") == Some(task_type)
                    && json_i32(record, "taskId") == Some(task_id)
                    && (json_i32(record, "completed").unwrap_or_default() == 1
                        || json_i32(record, "finishTime").unwrap_or_default() > 0
                        || json_i32(record, "count").unwrap_or_default() >= goal)
            })
        })
}

/// Keep legacy UserInfo.AchievePoint consistent with claimed achievement records.
/// Achievement points are derived from the client catalog, never accepted from TaskTrigger.
pub(super) fn sync_achievement_points(account: &mut Value, catalog: &TaskCatalog) -> bool {
    let points = catalog
        .definitions
        .iter()
        .filter(|definition| definition.task_type == 5)
        .filter(|definition| task_claimed(account, definition.task_type, definition.id))
        .map(|definition| definition.point.max(0))
        .sum::<i32>();
    let Some(character) = account.get_mut("character").and_then(Value::as_object_mut) else {
        return false;
    };
    if character
        .get("achievePoint")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        == i64::from(points)
    {
        return false;
    }
    character.insert("achievePoint".to_owned(), json!(points));
    true
}

pub(super) fn task_is_visible(
    account: &Value,
    catalog: &TaskCatalog,
    definition: &TaskDefinition,
) -> bool {
    if definition.abandoned != 0 {
        return false;
    }
    let level = value_i64_any(account.get("character").unwrap_or(account), &["level"]);
    if (definition.level_min > 0 && level < i64::from(definition.level_min))
        || (definition.level_max > 0 && level > i64::from(definition.level_max))
    {
        return false;
    }
    if matches!(definition.task_type, 2 | 3 | 8 | 9) || definition.previous_task_id <= 0 {
        return true;
    }
    task_claimed(account, definition.task_type, definition.id)
        || catalog.definitions.iter().any(|previous| {
            previous.task_type == definition.task_type
                && previous.id == definition.previous_task_id
                && task_claimed(account, previous.task_type, previous.id)
        })
}

/// Advance task counters from trusted server-side actions. Client TaskTrigger is rejected.
pub(super) fn advance_task_event(
    account: &mut Value,
    catalog: Option<&TaskCatalog>,
    event_type: i32,
    delta: i32,
    now: u32,
) -> bool {
    advance_task_event_impl(account, catalog, event_type, None, delta, now)
}

/// Advance trusted event counters, optionally restricted to goal[1].
pub(super) fn advance_task_event_with_param(
    account: &mut Value,
    catalog: Option<&TaskCatalog>,
    event_type: i32,
    target_param: i32,
    delta: i32,
    now: u32,
) -> bool {
    advance_task_event_impl(account, catalog, event_type, Some(target_param), delta, now)
}

fn advance_task_event_impl(
    account: &mut Value,
    catalog: Option<&TaskCatalog>,
    event_type: i32,
    target_param: Option<i32>,
    delta: i32,
    now: u32,
) -> bool {
    let Some(catalog) = catalog else {
        return false;
    };
    normalize_task_state(account, now);
    let definitions = catalog
        .definitions
        .iter()
        .filter(|definition| definition.event_type == event_type)
        .filter(|definition| {
            definition.event_param.is_none() || definition.event_param == target_param
        })
        .filter(|definition| task_is_visible(account, catalog, definition))
        .cloned()
        .collect::<Vec<_>>();
    let mut changed = false;
    let Some(account) = account.as_object_mut() else {
        return false;
    };
    let tasks = account
        .entry("tasks".to_owned())
        .or_insert_with(|| json!({"records": []}));
    let Some(tasks) = tasks.as_object_mut() else {
        return false;
    };
    let records = tasks
        .entry("records".to_owned())
        .or_insert_with(|| json!([]));
    let Some(records) = records.as_array_mut() else {
        return false;
    };
    for definition in definitions {
        let current = records
            .iter()
            .find(|record| {
                json_i32(record, "taskType") == Some(definition.task_type)
                    && json_i32(record, "taskId") == Some(definition.id)
            })
            .and_then(|record| json_i32(record, "count"))
            .unwrap_or_default();
        let next = current
            .saturating_add(delta.max(0))
            .min(definition.goal.max(0));
        if next <= current {
            continue;
        }
        if let Some(record) = records.iter_mut().find(|record| {
            json_i32(record, "taskType") == Some(definition.task_type)
                && json_i32(record, "taskId") == Some(definition.id)
        }) {
            record["count"] = json!(next);
            if next >= definition.goal {
                record["completed"] = json!(1);
                record["finishTime"] = json!(now);
            }
        } else {
            records.push(json!({
                "taskType": definition.task_type,
                "taskId": definition.id,
                "count": next,
                "completed": if next >= definition.goal { 1 } else { 0 },
                "finishTime": if next >= definition.goal { now } else { 0 },
                "rewardTime": 0
            }));
        }
        changed = true;
    }
    changed
}

pub(super) fn task_rewards(
    catalog: Option<&TaskCatalog>,
    task_type: i32,
    task_id: i32,
) -> Vec<ShopReward> {
    let Some(catalog) = catalog else {
        return Vec::new();
    };
    let Some(definition) = catalog
        .definitions
        .iter()
        .find(|d| d.task_type == task_type && d.id == task_id)
    else {
        return Vec::new();
    };
    let configured = if definition.reward_id > 0 {
        catalog
            .rewards_by_id
            .get(&definition.reward_id)
            .cloned()
            .unwrap_or_default()
    } else {
        definition.inline_rewards.clone()
    };
    configured
        .into_iter()
        .map(|(goods_type, item_id, num)| ShopReward {
            goods_type,
            item_id,
            num,
            instance_id: 0,
        })
        .collect()
}

pub(super) fn grant_reward(
    account: &mut Value,
    reward: ShopReward,
    now: u32,
    fashion_catalog: Option<&FashionList>,
) -> ShopReward {
    match reward.goods_type {
        16 => {
            add_medal(account, reward.item_id, now);
            reward
        }
        5 => {
            if let Some(key) = currency_character_key(reward.item_id) {
                add_character_i64(account, key, reward.num);
            }
            reward
        }
        3 => {
            let id = add_ship_items(account, reward.item_id, reward.num.max(0), now);
            ShopReward {
                instance_id: id,
                ..reward
            }
        }
        2 => {
            let mut id = 0;
            for _ in 0..reward.num.max(0) {
                id = add_equip_item(account, reward.item_id);
            }
            ShopReward {
                instance_id: id,
                ..reward
            }
        }
        18 => {
            add_fashion_item(account, reward.item_id, fashion_catalog);
            reward
        }
        _ => {
            add_bag_item(account, reward.item_id, reward.num);
            reward
        }
    }
}

pub(super) fn encode_task_reward(task_id: i32, rewards: &[ShopReward]) -> Vec<u8> {
    let mut out = Vec::new();
    append_varint_field(&mut out, 1, task_id.max(0) as u64);
    for reward in rewards {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, reward.goods_type.max(0) as u64);
        append_varint_field(&mut item, 2, reward.item_id.max(0) as u64);
        append_varint_field(&mut item, 3, reward.num.max(0) as u64);
        append_message_field(&mut out, 2, &item);
    }
    out
}

pub(super) fn encode_task_reward_list(rewards: &[ShopReward]) -> Vec<u8> {
    let mut out = Vec::new();
    for reward in rewards {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, reward.goods_type.max(0) as u64);
        append_varint_field(&mut item, 2, reward.item_id.max(0) as u64);
        append_varint_field(&mut item, 3, reward.num.max(0) as u64);
        append_message_field(&mut out, 1, &item);
    }
    out
}

pub(super) fn task_info_payload(account: &Value, catalog: Option<&TaskCatalog>) -> Vec<u8> {
    let mut output = Vec::new();
    let records = account
        .get("tasks")
        .and_then(|tasks| tasks.get("records"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let record_for = |definition: &TaskDefinition| {
        records.iter().find(|record| {
            value_i64_any(record, &["taskType", "task_type"]) == i64::from(definition.task_type)
                && value_i64_any(record, &["taskId", "task_id"]) == i64::from(definition.id)
        })
    };
    let claimed = |definition: &TaskDefinition| {
        record_for(definition)
            .is_some_and(|record| value_i64_any(record, &["rewardTime", "reward_time"]) > 0)
    };
    let Some(catalog) = catalog.filter(|catalog| !catalog.definitions.is_empty()) else {
        if let Some(ids) = account
            .get("tasks")
            .and_then(|tasks| tasks.get("teachingPtRewardIds"))
            .and_then(Value::as_array)
        {
            for id in ids.iter().filter_map(Value::as_i64) {
                append_varint_field(&mut output, 11, id.max(0) as u64);
            }
        }
        // TaskStageInfo: TeachingStage type 17, stage id 1.
        output.extend_from_slice(&[0x62, 0x02, 0x08, 0x11, 0x10, 0x01, 0x68, 0x00]);
        return output;
    };
    let level = value_i64_any(account.get("character").unwrap_or(account), &["level"]);
    let mut selected = catalog
        .definitions
        .iter()
        .filter(|definition| {
            matches!(definition.task_type, 1 | 2 | 3 | 4 | 5 | 8 | 9)
                && definition.abandoned == 0
                && (definition.level_min <= 0 || level >= i64::from(definition.level_min))
                && (definition.level_max <= 0 || level <= i64::from(definition.level_max))
                && (matches!(definition.task_type, 2 | 3 | 8 | 9)
                    || definition.previous_task_id <= 0
                    || claimed(definition)
                    || catalog.definitions.iter().any(|candidate| {
                        candidate.task_type == definition.task_type
                            && candidate.id == definition.previous_task_id
                            && claimed(candidate)
                    }))
        })
        .collect::<Vec<_>>();
    selected.sort_by_key(|definition| (definition.task_type, definition.id));
    let tag_for = |task_type| match task_type {
        1 => 0x0A,
        2 => 0x12,
        3 => 0x1A,
        4 => 0x3A,
        5 => 0x22,
        8 => 0x4A,
        9 => 0x52,
        _ => 0,
    };
    for task_type in [1, 2, 3, 4, 5, 8, 9] {
        let type_defs = selected
            .iter()
            .copied()
            .filter(|definition| definition.task_type == task_type)
            .collect::<Vec<_>>();
        let mut event_types = type_defs
            .iter()
            .map(|definition| definition.event_type)
            .collect::<Vec<_>>();
        event_types.sort_unstable();
        event_types.dedup();
        for event_type in event_types {
            let event_defs = type_defs
                .iter()
                .copied()
                .filter(|definition| definition.event_type == event_type)
                .collect::<Vec<_>>();
            let mut event_info = Vec::new();
            append_varint_field(&mut event_info, 1, event_type.max(0) as u64);
            let max_goal = event_defs
                .iter()
                .map(|definition| definition.goal)
                .max()
                .unwrap_or(0);
            let progress = event_defs
                .iter()
                .filter_map(|definition| record_for(definition))
                .map(|record| value_i64_any(record, &["count"]).max(0))
                .max()
                .unwrap_or_default()
                .min(i64::from(max_goal));
            append_varint_field(&mut event_info, 2, progress as u64);
            for definition in event_defs {
                let mut task = Vec::new();
                append_varint_field(&mut task, 1, definition.id.max(0) as u64);
                if let Some(record) = record_for(definition) {
                    append_varint_field(
                        &mut task,
                        2,
                        value_i64_any(record, &["rewardTime", "reward_time"]).max(0) as u64,
                    );
                    append_varint_field(
                        &mut task,
                        3,
                        value_i64_any(record, &["finishTime", "finish_time"]).max(0) as u64,
                    );
                    append_varint_field(
                        &mut task,
                        4,
                        value_i64_any(record, &["count"]).max(0) as u64,
                    );
                } else {
                    append_varint_field(&mut task, 2, 0);
                    append_varint_field(&mut task, 3, 0);
                    append_varint_field(&mut task, 4, 0);
                }
                append_varint_field(&mut task, 7, 0);
                append_varint_field(&mut task, 8, 0);
                append_message_field(&mut event_info, 3, &task);
            }
            if let Some(tag) = (tag_for(task_type) != 0).then_some(tag_for(task_type)) {
                append_message_field(&mut output, tag >> 3, &event_info);
            }
        }
    }
    for definition in selected.iter().filter(|definition| {
        definition.task_type == 5 && definition.medal_id > 0 && claimed(definition)
    }) {
        append_varint_field(&mut output, 5, definition.medal_id as u64);
    }
    if let Some(ids) = account
        .get("tasks")
        .and_then(|tasks| tasks.get("teachingPtRewardIds"))
        .and_then(Value::as_array)
    {
        for id in ids.iter().filter_map(Value::as_i64) {
            append_varint_field(&mut output, 11, id.max(0) as u64);
        }
    }
    // TaskStageInfo: TeachingStage type 17, stage id 1.
    output.extend_from_slice(&[0x62, 0x02, 0x08, 0x11, 0x10, 0x01, 0x68, 0x00]);
    output
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ShopReward {
    pub(super) goods_type: i32,
    pub(super) item_id: i32,
    pub(super) num: i32,
    pub(super) instance_id: i32,
}
