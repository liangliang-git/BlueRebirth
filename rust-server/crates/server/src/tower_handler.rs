use serde_json::{json, Value};

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let Some(account) = context.account.as_deref_mut() else {
        return HandlerResult::Error(GameError::AccountUnavailable);
    };
    let now = current_unix_seconds();
    match method {
        "tower.GetTowerInfo" => reply(
            method,
            tower_info_payload(account, context.catalogs.chapters, now),
        ),
        "tower.Reset" => {
            let Some(tower) = account
                .as_object_mut()
                .map(|account| account.entry("tower").or_insert_with(|| json!({})))
            else {
                return invalid("tower state is invalid");
            };
            tower["dailyCount"] = json!(0);
            tower["dailyCountEx"] = json!(0);
            tower["resetTime"] = json!(now);
            tower["heroIds"] = json!([]);
            tower["lockEquipList"] = json!([]);
            HandlerResult::PushOnly
        }
        "tower.ResetChangeHeroIdList" => {
            if let Some(tower) = account.get_mut("tower") {
                tower["heroIds"] = json!([]);
            }
            HandlerResult::PushOnly
        }
        "tower.Receive" => {
            let Some(tower) = account.as_object_mut().map(|account| {
                account
                    .entry("tower".to_owned())
                    .or_insert_with(|| json!({}))
            }) else {
                return invalid("tower state is invalid");
            };
            let daily_count = tower
                .get("dailyCount")
                .and_then(Value::as_i64)
                .unwrap_or_default()
                .max(0);
            tower["dailyCount"] = json!(daily_count.saturating_add(1));
            reply(method, tower_reward_payload(tower, None))
        }
        "tower.Replacement" => {
            let Some(tower) = account.as_object_mut().map(|account| {
                account
                    .entry("tower".to_owned())
                    .or_insert_with(|| json!({}))
            }) else {
                return invalid("tower state is invalid");
            };
            let topic_index = tower
                .get("topicIndex")
                .and_then(Value::as_i64)
                .unwrap_or_default()
                .max(0);
            tower["topicIndex"] = json!(topic_index.saturating_add(1));
            reply(
                method,
                tower_info_payload(account, context.catalogs.chapters, now),
            )
        }
        "tower.SendUpgrade" => {
            let Some(tower) = account.as_object_mut().map(|account| {
                account
                    .entry("tower".to_owned())
                    .or_insert_with(|| json!({}))
            }) else {
                return invalid("tower state is invalid");
            };
            let max_level = tower
                .get("maxLevel")
                .and_then(Value::as_i64)
                .unwrap_or_default()
                .max(0);
            tower["maxLevel"] = json!(max_level.saturating_add(1));
            tower["isNewLevel"] = json!(true);
            reply(
                method,
                tower_info_payload(account, context.catalogs.chapters, now),
            )
        }
        "tower.ReceiveBuff" => {
            let copy_id = decode_varint_field(request_args, 1);
            let Some(tower) = account.as_object_mut().map(|account| {
                account
                    .entry("tower".to_owned())
                    .or_insert_with(|| json!({}))
            }) else {
                return invalid("tower state is invalid");
            };
            let ids = tower
                .as_object_mut()
                .expect("tower state must be an object")
                .entry("savePassCopyId".to_owned())
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .expect("tower save pass ids must be an array");
            if copy_id > 0 && !ids.iter().any(|id| id.as_i64() == Some(i64::from(copy_id))) {
                ids.push(json!(copy_id));
            }
            reply(
                method,
                tower_reward_payload(tower, (copy_id > 0).then_some(copy_id)),
            )
        }
        _ => HandlerResult::Empty,
    }
}

pub(super) fn handle_activity<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let Some(account) = context.account.as_deref_mut() else {
        return HandlerResult::Error(GameError::AccountUnavailable);
    };
    let now = current_unix_seconds();
    match method {
        "activityTower.ActivityTower" | "activityTower.GetActivityTower" => {
            reply(method, activity_tower_payload(account, now))
        }
        "activityTower.Reset" => {
            let Some(tower) = account.as_object_mut().map(|account| {
                account
                    .entry("activityTower".to_owned())
                    .or_insert_with(|| json!({}))
            }) else {
                return invalid("activity tower state is invalid");
            };
            tower["resetTime"] = json!(now);
            tower["smallResetNumber"] = json!(tower
                .get("smallResetNumber")
                .and_then(Value::as_i64)
                .unwrap_or_default()
                .max(0)
                .saturating_add(1));
            tower["quickNumber"] = json!(0);
            tower["passCopyIdList"] = json!([]);
            tower["savePassCopyIdList"] = json!([]);
            tower["savePassStageCopyIdList"] = json!([]);
            reply(method, activity_tower_payload(account, now))
        }
        "activityTower.ReceiveBuff" => {
            let copy_id = decode_varint_field(request_args, 1);
            let Some(tower) = account.as_object_mut().map(|account| {
                account
                    .entry("activityTower".to_owned())
                    .or_insert_with(|| json!({}))
            }) else {
                return invalid("activity tower state is invalid");
            };
            append_unique_id(tower, "passCopyIdList", copy_id);
            reply(method, activity_tower_payload(account, now))
        }
        "activityTower.QuickPass" => {
            let copy_id = decode_varint_field(request_args, 1);
            let Some(tower) = account.as_object_mut().map(|account| {
                account
                    .entry("activityTower".to_owned())
                    .or_insert_with(|| json!({}))
            }) else {
                return invalid("activity tower state is invalid");
            };
            append_unique_id(tower, "passCopyIdList", copy_id);
            let quick_number = tower
                .get("quickNumber")
                .and_then(Value::as_i64)
                .unwrap_or_default()
                .max(0);
            tower["quickNumber"] = json!(quick_number.saturating_add(1));
            let history_max = tower
                .get("historyMax")
                .and_then(Value::as_i64)
                .unwrap_or_default()
                .max(0);
            tower["historyMax"] = json!(history_max.max(quick_number.saturating_add(1)));
            reply(method, activity_tower_payload(account, now))
        }
        _ => HandlerResult::Empty,
    }
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn invalid(message: &'static str) -> HandlerResult {
    HandlerResult::Error(GameError::InvalidRequest(message))
}

pub(crate) fn tower_info_payload(
    account: &Value,
    chapter_catalog: Option<&ChapterCatalog>,
    now: u32,
) -> Vec<u8> {
    let tower = account.get("tower").and_then(Value::as_object);
    let chapter_id = tower
        .and_then(|value| value.get("chapterId"))
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .or_else(|| {
            chapter_catalog
                .map(|catalog| i64::from(catalog.tower_chapter_id))
                .filter(|value| *value > 0)
        })
        .unwrap_or(30_001) as u64;
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, chapter_id);
    append_nonnegative_field(&mut output, 2, tower, "areaIndex");
    append_nonnegative_field(&mut output, 3, tower, "copyIndex");
    append_nonnegative_field(&mut output, 4, tower, "topicIndex");
    append_nonnegative_field(&mut output, 5, tower, "dailyCount");
    append_varint_field(
        &mut output,
        6,
        tower
            .and_then(|value| value.get("resetTime"))
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
            .unwrap_or(i64::from(now)) as u64,
    );

    if let Some(entries) = tower
        .and_then(|value| value.get("sfId2Count"))
        .and_then(Value::as_array)
    {
        for entry in entries {
            let mut encoded = Vec::new();
            let entry = entry.as_object();
            append_nonnegative_field(&mut encoded, 1, entry, "sfId");
            append_nonnegative_field(&mut encoded, 2, entry, "count");
            append_message_field(&mut output, 7, &encoded);
        }
    }

    append_repeated_nonnegative_field(&mut output, 9, tower, "heroIds");
    append_repeated_nonnegative_field(&mut output, 10, tower, "lockEquipList");
    append_nonnegative_field(&mut output, 11, tower, "passLastChapterId");
    append_bool_field(&mut output, 12, tower, "isReset");
    append_nonnegative_field(&mut output, 13, tower, "maxLevel");
    append_nonnegative_field(&mut output, 14, tower, "maxArea");
    append_nonnegative_field(&mut output, 15, tower, "maxCopy");
    append_nonnegative_field(&mut output, 16, tower, "dailyCountEx");
    append_bool_field(&mut output, 17, tower, "isNewLevel");
    append_repeated_nonnegative_field(&mut output, 18, tower, "savePassCopyId");
    output
}

pub(crate) fn activity_tower_payload(account: &Value, now: u32) -> Vec<u8> {
    let tower = account.get("activityTower").and_then(Value::as_object);
    let mut output = Vec::new();
    append_nonnegative_field(&mut output, 1, tower, "activityId");
    append_varint_field(
        &mut output,
        2,
        tower
            .and_then(|value| value.get("resetTime"))
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
            .unwrap_or(i64::from(now)) as u64,
    );
    append_nonnegative_field(&mut output, 3, tower, "smallResetNumber");
    append_nonnegative_field(&mut output, 4, tower, "quickNumber");
    append_nonnegative_field(&mut output, 5, tower, "historyMax");
    append_repeated_nonnegative_field(&mut output, 6, tower, "savePassCopyIdList");
    append_repeated_nonnegative_field(&mut output, 7, tower, "passCopyIdList");
    append_repeated_nonnegative_field(&mut output, 9, tower, "lockEquipList");
    append_repeated_nonnegative_field(&mut output, 10, tower, "heroIds");
    append_repeated_nonnegative_field(&mut output, 11, tower, "savePassStageCopyIdList");
    output
}

fn append_nonnegative_field(
    output: &mut Vec<u8>,
    field: u8,
    object: Option<&serde_json::Map<String, Value>>,
    key: &str,
) {
    let value = object
        .and_then(|object| object.get(key))
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0) as u64;
    append_varint_field(output, field, value);
}

fn append_bool_field(
    output: &mut Vec<u8>,
    field: u8,
    object: Option<&serde_json::Map<String, Value>>,
    key: &str,
) {
    append_varint_field(
        output,
        field,
        u64::from(
            object
                .and_then(|object| object.get(key))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
    );
}

fn append_repeated_nonnegative_field(
    output: &mut Vec<u8>,
    field: u8,
    object: Option<&serde_json::Map<String, Value>>,
    key: &str,
) {
    if let Some(values) = object
        .and_then(|object| object.get(key))
        .and_then(Value::as_array)
    {
        for value in values
            .iter()
            .filter_map(Value::as_i64)
            .filter(|value| *value > 0)
        {
            append_varint_field(output, field, value as u64);
        }
    }
}

fn append_unique_id(object: &mut Value, key: &str, id: i32) {
    if id <= 0 {
        return;
    }
    let ids = object
        .as_object_mut()
        .expect("tower state must be an object")
        .entry(key.to_owned())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("tower id list must be an array");
    if !ids
        .iter()
        .any(|value| value.as_i64() == Some(i64::from(id)))
    {
        ids.push(json!(id));
    }
}

fn tower_reward_payload(tower: &Value, copy_id: Option<i32>) -> Vec<u8> {
    let object = tower.as_object();
    let mut output = Vec::new();
    append_nonnegative_field(&mut output, 1, object, "chapterId");
    append_nonnegative_field(&mut output, 2, object, "areaIndex");
    append_nonnegative_field(&mut output, 3, object, "copyIndex");
    append_nonnegative_field(&mut output, 4, object, "topicIndex");
    if let Some(rewards) = object
        .and_then(|value| value.get("pendingRewards"))
        .and_then(Value::as_array)
    {
        for reward in rewards {
            let mut encoded = Vec::new();
            append_nonnegative_field(&mut encoded, 1, reward.as_object(), "type");
            append_nonnegative_field(&mut encoded, 2, reward.as_object(), "configId");
            append_nonnegative_field(&mut encoded, 3, reward.as_object(), "num");
            append_nonnegative_field(&mut encoded, 4, reward.as_object(), "id");
            append_message_field(&mut output, 5, &encoded);
        }
    }
    if let Some(copy_id) = copy_id {
        append_varint_field(&mut output, 6, copy_id.max(0) as u64);
    }
    output
}

#[cfg(test)]
mod tests {
    use crate::common::response::HandlerResult;

    use super::*;

    #[test]
    fn handlers_expose_typed_results() {
        let _: for<'state, 'account, 'scratch> fn(
            &mut GameLoginRequestContext<'state, 'account, 'scratch>,
            &str,
            &[u8],
        ) -> HandlerResult = handle;
        let _: for<'state, 'account, 'scratch> fn(
            &mut GameLoginRequestContext<'state, 'account, 'scratch>,
            &str,
            &[u8],
        ) -> HandlerResult = handle_activity;
    }
}
