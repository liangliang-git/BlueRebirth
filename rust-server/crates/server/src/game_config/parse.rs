use serde_json::Value;

use super::*;
use crate::common::json::*;

pub(super) fn config_triplets(value: &Value, key: &str) -> Vec<(i32, i32, i32)> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            Some((
                i32::try_from(row.first()?.as_i64()?).ok()?,
                i32::try_from(row.get(1)?.as_i64()?).ok()?,
                i32::try_from(row.get(2)?.as_i64()?).ok()?,
            ))
        })
        .filter(|(kind, item, amount)| *kind > 0 && *item > 0 && *amount > 0)
        .collect()
}

pub(super) fn config_i32_array(value: &Value, key: &str) -> Vec<i32> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .filter_map(|value| i32::try_from(value).ok())
        .collect()
}

pub(super) fn config_drop_entries(value: &Value) -> Vec<DropEntry> {
    value
        .get("drop")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            Some(DropEntry {
                goods_type: i32::try_from(row.first()?.as_i64()?).ok()?,
                item_id: i32::try_from(row.get(1)?.as_i64()?).ok()?,
                min: i32::try_from(row.get(2)?.as_i64()?).ok()?,
                max: i32::try_from(row.get(3)?.as_i64()?).ok()?,
                rate: row.get(4)?.as_i64()?,
            })
        })
        .filter(|entry| {
            entry.goods_type > 0
                && entry.item_id > 0
                && entry.min > 0
                && entry.max >= entry.min
                && entry.rate > 0
        })
        .collect()
}

pub(super) fn config_extract(value: &Value) -> ActivityExtractConfig {
    let cost = value
        .get("item_cost")
        .and_then(Value::as_array)
        .and_then(|values| {
            Some((
                i32::try_from(values.first()?.as_i64()?).ok()?,
                i32::try_from(values.get(1)?.as_i64()?).ok()?,
                i32::try_from(values.get(2)?.as_i64()?).ok()?,
            ))
        });
    let rewards = value
        .get("drop_reward_id")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            Some((
                i32::try_from(row.first()?.as_i64()?).ok()?,
                i32::try_from(row.get(1)?.as_i64()?).ok()?,
            ))
        })
        .filter(|(reward_id, amount)| *reward_id > 0 && *amount > 0)
        .collect();
    ActivityExtractConfig { cost, rewards }
}

pub(super) fn config_i32_pairs(value: &Value, key: &str) -> Vec<(i32, i32)> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let values = item.as_array()?;
            Some((
                i32::try_from(values.first()?.as_i64()?).ok()?,
                i32::try_from(values.get(1)?.as_i64()?).ok()?,
            ))
        })
        .collect()
}

pub(super) fn config_i64_pairs(value: &Value, key: &str) -> Vec<(i32, i64)> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let values = item.as_array()?;
            Some((
                i32::try_from(values.first()?.as_i64()?).ok()?,
                values.get(1)?.as_i64()?,
            ))
        })
        .filter(|(attr_id, _)| *attr_id > 0)
        .collect()
}

pub(super) fn config_i32_nested_array(value: &Value, key: &str) -> Vec<Vec<i32>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some(
                item.as_array()?
                    .iter()
                    .filter_map(|value| value.as_i64().and_then(|value| i32::try_from(value).ok()))
                    .collect(),
            )
        })
        .collect()
}

pub(super) fn config_i32_triple(value: &Value, key: &str) -> Option<(i32, i32, i32)> {
    let values = value.get(key)?.as_array()?.first()?.as_array()?;
    Some((
        i32::try_from(values.first()?.as_i64()?).ok()?,
        i32::try_from(values.get(1)?.as_i64()?).ok()?,
        i32::try_from(values.get(2)?.as_i64()?).ok()?,
    ))
}

pub(super) fn config_activity(id: i32, value: &Value) -> ActivityConfig {
    let p6 = config_i32_nested_array(value, "p6")
        .into_iter()
        .flatten()
        .collect();
    ActivityConfig {
        id: json_i32(value, "id").unwrap_or(id),
        activity_type: json_i32(value, "type").unwrap_or_default(),
        is_open: json_i32(value, "is_open").unwrap_or_default(),
        p1: config_i32_array(value, "p1"),
        p2: config_i32_pairs(value, "p2").into_iter().next(),
        p3: config_i32_pairs(value, "p3").into_iter().next(),
        p4: config_i32_nested_array(value, "p4"),
        p5: config_i32_pairs(value, "p5"),
        p6,
        p14: config_i32_triple(value, "p14"),
    }
}

pub(super) fn config_world_event(value: &Value) -> WorldEventConfig {
    WorldEventConfig {
        server_stage_rewards: config_i32_pairs(value, "server_stage_rewards"),
    }
}

pub(super) fn config_valentine_gift(value: &Value) -> ValentineGiftConfig {
    ValentineGiftConfig {
        ship_fleet_id: json_i32(value, "ship_fleet_id").unwrap_or_default(),
        attach_reward: json_i32(value, "attach_reward").unwrap_or_default(),
    }
}

pub(super) fn config_testship_reward(value: &Value) -> TestShipRewardConfig {
    TestShipRewardConfig {
        reward_id: json_i32(value, "reward").unwrap_or_default(),
    }
}
