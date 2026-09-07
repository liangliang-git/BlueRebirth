use serde_json::{json, Value};

use super::*;

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let state = context.state;
    let pre_pushes = &mut *context.pre_pushes;
    let account = &mut *context.account;
    let response_err = &mut *context.response_err;
    let response_err_msg = &mut *context.response_err_msg;
    let Some(account) = account.as_deref_mut() else {
        *response_err = 1;
        *response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };

    match method {
        "outpost.GetOutPostInfo" => Some(outpost_info_payload(account)),
        "outpost.UpgradeBuilding" | "outpost.DegradeBuilding" => {
            let building_id = decode_varint_field(request_args, 1);
            let delta = if method.ends_with("UpgradeBuilding") {
                1
            } else {
                -1
            };
            let level_result = update_building_level(
                account,
                super::extended_handler::gameplay_catalog(),
                building_id,
                delta,
            );
            match level_result {
                Ok(()) => push_outpost_refresh(state, pre_pushes, account),
                Err(message) => {
                    *response_err = 1;
                    *response_err_msg = message.to_owned();
                }
            }
            Some(outpost_info_payload(account))
        }
        "outpost.SetHero" => {
            let building_id = decode_varint_field(request_args, 1);
            let hero_ids = decode_repeated_varint_field(request_args, 2);
            if !update_building(account, building_id, |building| {
                building["heroList"] = json!(hero_ids);
            }) {
                *response_err = 1;
                *response_err_msg = "outpost building was not found".to_owned();
            }
            Some(outpost_info_payload(account))
        }
        "outpost.SetUseCoin" => {
            let building_id = decode_varint_field(request_args, 1);
            let use_coin = decode_varint_field(request_args, 2) != 0;
            if !update_building(account, building_id, |building| {
                building["useCoin"] = json!(use_coin);
            }) {
                *response_err = 1;
                *response_err_msg = "outpost building was not found".to_owned();
            }
            Some(outpost_info_payload(account))
        }
        "outpost.SaveTactic" => {
            let tactics = decode_repeated_message_field(request_args, 1);
            account["outpostTactics"] = json!(tactics.clone());
            let _ = update_tactics(account, tactics);
            Some(outpost_info_payload(account))
        }
        "outpost.RemoveTactic" => {
            remove_tactic(
                account,
                decode_varint_field(request_args, 1),
                decode_varint_field(request_args, 2),
            );
            Some(outpost_info_payload(account))
        }
        "outpost.ChangeTacticName" => {
            change_tactic_name(
                account,
                decode_varint_field(request_args, 1),
                decode_varint_field(request_args, 2),
                decode_string_field(request_args, 3).unwrap_or_default(),
            );
            Some(outpost_info_payload(account))
        }
        "outpost.ReceiveItem" => {
            let rewards =
                collect_outpost_items(account, Some(decode_varint_field(request_args, 1)));
            push_outpost_refresh(state, pre_pushes, account);
            Some(encode_receive_result(&rewards))
        }
        "outpost.ReceiveAll" => {
            let rewards = collect_outpost_items(account, None);
            push_outpost_refresh(state, pre_pushes, account);
            Some(encode_receive_result(&rewards))
        }
        "outpost.SpeedUpProduction" => {
            let building_id = decode_varint_field(request_args, 1);
            match speed_up_production(
                account,
                super::extended_handler::gameplay_catalog(),
                building_id,
            ) {
                Ok(rewards) => {
                    push_outpost_refresh(state, pre_pushes, account);
                    Some(encode_receive_result(&rewards))
                }
                Err(message) => {
                    *response_err = 1;
                    *response_err_msg = message.to_owned();
                    Some(Vec::new())
                }
            }
        }
        _ => None,
    }
}

fn outpost_info_payload(account: &mut Value) -> Vec<u8> {
    let now = current_unix_seconds();
    let outpost = account
        .as_object_mut()
        .expect("account must be an object")
        .entry("outpost".to_owned())
        .or_insert_with(|| json!({"buildings": [], "speedUpTime": 0}));
    let buildings = outpost
        .get_mut("buildings")
        .and_then(Value::as_array_mut)
        .expect("outpost buildings must be an array");
    if buildings.is_empty() {
        buildings.push(json!({
            "id": 1,
            "level": 0,
            "heroList": [],
            "state": 1,
            "useCoin": false,
            "itemInfo": [],
            "tacticList": []
        }));
    }
    let mut output = Vec::new();
    for building in buildings.iter() {
        let mut encoded = Vec::new();
        append_varint_field(
            &mut encoded,
            1,
            json_i32(building, "id").unwrap_or(0).max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            2,
            json_i32(building, "level").unwrap_or(0).max(0) as u64,
        );
        for hero_id in json_i32_array(building, "heroList") {
            append_varint_field(&mut encoded, 3, hero_id.max(0) as u64);
        }
        append_varint_field(
            &mut encoded,
            4,
            json_i32(building, "state").unwrap_or(1).max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            5,
            u64::from(
                building
                    .get("useCoin")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
        );
        for item in building
            .get("itemInfo")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let mut reward = Vec::new();
            append_varint_field(
                &mut reward,
                1,
                json_i32_alias(item, &["type", "goodsType"])
                    .unwrap_or(0)
                    .max(0) as u64,
            );
            append_varint_field(
                &mut reward,
                2,
                json_i32_alias(item, &["configId", "itemId"])
                    .unwrap_or(0)
                    .max(0) as u64,
            );
            append_varint_field(
                &mut reward,
                3,
                json_i32(item, "num").unwrap_or(0).max(0) as u64,
            );
            append_message_field(&mut encoded, 6, &reward);
        }
        for tactic in building
            .get("tacticList")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            append_message_field(&mut encoded, 7, &json_bytes(tactic));
        }
        append_message_field(&mut output, 1, &encoded);
    }
    let speed_up_time = account
        .get("outpost")
        .and_then(|value| value.get("speedUpTime"))
        .and_then(Value::as_i64)
        .unwrap_or(i64::from(now));
    append_varint_field(&mut output, 2, speed_up_time.max(0) as u64);
    output
}

fn update_building_level(
    account: &mut Value,
    catalog: &GameplayCatalog,
    building_id: i32,
    delta: i32,
) -> Result<(), &'static str> {
    let _ = outpost_info_payload(account);
    let level = account
        .get("outpost")
        .and_then(|outpost| outpost.get("buildings"))
        .and_then(Value::as_array)
        .and_then(|buildings| {
            buildings
                .iter()
                .find(|building| json_i32(building, "id") == Some(building_id))
        })
        .and_then(|building| json_i32(building, "level"))
        .ok_or("outpost building was not found")?;
    if delta > 0 {
        let Some(config) = outpost_level_config(catalog, building_id, level) else {
            return Err("outpost level config was not found");
        };
        if json_i32(config, "next_id").unwrap_or_default() <= 0 {
            return Err("outpost building is at maximum level");
        }
        let costs = reward_triplets(config, "item_cost");
        if costs.is_empty() {
            return Err("outpost upgrade cost is not configured");
        }
        if costs
            .iter()
            .any(|(kind, item, amount)| !resource_available(account, *kind, *item, *amount))
        {
            return Err("outpost upgrade cost is insufficient");
        }
        for (kind, item, amount) in costs {
            consume_resource(account, kind, item, amount);
        }
    } else if level <= 0 {
        return Err("outpost building is at minimum level");
    }
    if !update_building(account, building_id, |building| {
        let current = json_i32(building, "level").unwrap_or(0);
        building["level"] = json!(current.saturating_add(delta).max(0));
    }) {
        return Err("outpost building was not found");
    }
    Ok(())
}

fn update_building<F>(account: &mut Value, building_id: i32, update: F) -> bool
where
    F: FnOnce(&mut Value),
{
    let _ = outpost_info_payload(account);
    let Some(buildings) = account
        .get_mut("outpost")
        .and_then(|outpost| outpost.get_mut("buildings"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let Some(building) = buildings
        .iter_mut()
        .find(|building| json_i32(building, "id") == Some(building_id))
    else {
        return false;
    };
    update(building);
    true
}

fn outpost_level_config(catalog: &GameplayCatalog, building_id: i32, level: i32) -> Option<&Value> {
    catalog.outpost_levels.values().find(|config| {
        json_i32(config, "outpost_id") == Some(building_id)
            && json_i32(config, "level") == Some(level)
    })
}

fn reward_triplets(value: &Value, key: &str) -> Vec<(i32, i32, i32)> {
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

fn json_i32_alias(value: &Value, keys: &[&str]) -> Option<i32> {
    keys.iter().find_map(|key| json_i32(value, key))
}

fn json_bytes(value: &Value) -> Vec<u8> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .filter_map(|byte| u8::try_from(byte).ok())
        .collect()
}

fn update_tactics(account: &mut Value, tactics: Vec<Vec<u8>>) -> bool {
    let Some(buildings) = account
        .get_mut("outpost")
        .and_then(|outpost| outpost.get_mut("buildings"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    for building in buildings {
        let building_id = json_i32(building, "id");
        let selected = tactics
            .iter()
            .filter(|tactic| building_id == Some(decode_varint_field(tactic, 1)))
            .map(|tactic| json!(tactic))
            .collect::<Vec<_>>();
        building["tacticList"] = json!(selected);
    }
    true
}

fn remove_tactic(account: &mut Value, building_id: i32, index: i32) {
    let _ = update_building(account, building_id, |building| {
        if let Some(tactics) = building.get_mut("tacticList").and_then(Value::as_array_mut) {
            tactics.retain(|tactic| decode_varint_field(&json_bytes(tactic), 4) != index);
        }
    });
}

fn change_tactic_name(account: &mut Value, building_id: i32, index: i32, name: String) {
    let _ = update_building(account, building_id, |building| {
        let Some(tactics) = building.get_mut("tacticList").and_then(Value::as_array_mut) else {
            return;
        };
        for tactic in tactics {
            let bytes = json_bytes(tactic);
            if decode_varint_field(&bytes, 4) == index {
                let mut updated = Vec::new();
                append_varint_field(&mut updated, 1, decode_varint_field(&bytes, 1) as u64);
                append_string_field_local(&mut updated, 2, &name);
                for hero_id in decode_repeated_varint_field(&bytes, 3) {
                    append_varint_field(&mut updated, 3, hero_id as u64);
                }
                append_varint_field(&mut updated, 4, index as u64);
                *tactic = json!(updated);
                break;
            }
        }
    });
}

fn collect_outpost_items(account: &mut Value, building_id: Option<i32>) -> Vec<ShopReward> {
    let Some(buildings) = account
        .get_mut("outpost")
        .and_then(|outpost| outpost.get_mut("buildings"))
        .and_then(Value::as_array_mut)
    else {
        return Vec::new();
    };
    let mut pending_items = Vec::new();
    for building in buildings {
        if building_id.is_some() && json_i32(building, "id") != building_id {
            continue;
        }
        let Some(items) = building.get_mut("itemInfo").and_then(Value::as_array_mut) else {
            continue;
        };
        let pending = std::mem::take(items);
        pending_items.extend(pending);
    }
    let mut rewards = Vec::new();
    for item in pending_items {
        let goods_type = json_i32_alias(&item, &["type", "goodsType"]).unwrap_or(0);
        let item_id = json_i32_alias(&item, &["configId", "itemId"]).unwrap_or(0);
        let num = json_i32(&item, "num").unwrap_or(0);
        if goods_type > 0 && item_id > 0 && num > 0 {
            let reward = ShopReward {
                goods_type,
                item_id,
                num,
                instance_id: 0,
            };
            rewards.push(grant_reward(account, reward, current_unix_seconds(), None));
        }
    }
    rewards
}

fn speed_up_production(
    account: &mut Value,
    catalog: &GameplayCatalog,
    building_id: i32,
) -> Result<Vec<ShopReward>, &'static str> {
    let level = account
        .get("outpost")
        .and_then(|outpost| outpost.get("buildings"))
        .and_then(Value::as_array)
        .and_then(|buildings| {
            buildings
                .iter()
                .find(|building| json_i32(building, "id") == Some(building_id))
        })
        .and_then(|building| json_i32(building, "level"))
        .ok_or("outpost building was not found")?;
    let has_pending = account
        .get("outpost")
        .and_then(|outpost| outpost.get("buildings"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|building| json_i32(building, "id") == Some(building_id))
        .and_then(|building| building.get("itemInfo"))
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty());
    if !has_pending {
        return Err("outpost has no production reward");
    }
    let config = outpost_level_config(catalog, building_id, level)
        .ok_or("outpost level config was not found")?;
    let costs = reward_triplets(config, "speedup_cost");
    if costs.is_empty() {
        return Err("outpost speedup cost is not configured");
    }
    if costs
        .iter()
        .any(|(kind, item, amount)| !resource_available(account, *kind, *item, *amount))
    {
        return Err("outpost speedup cost is insufficient");
    }
    for (kind, item, amount) in costs {
        consume_resource(account, kind, item, amount);
    }
    Ok(collect_outpost_items(account, Some(building_id)))
}

fn append_string_field_local(output: &mut Vec<u8>, field: u8, value: &str) {
    append_message_field(output, field, value.as_bytes());
}

fn encode_receive_result(rewards: &[ShopReward]) -> Vec<u8> {
    let mut output = Vec::new();
    for reward in rewards {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, reward.goods_type.max(0) as u64);
        append_varint_field(&mut item, 2, reward.item_id.max(0) as u64);
        append_varint_field(&mut item, 3, reward.num.max(0) as u64);
        append_varint_field(&mut item, 4, reward.instance_id.max(0) as u64);
        append_message_field(&mut output, 6, &item);
    }
    output
}

fn push_outpost_refresh(state: &ServerState, pre_pushes: &mut Vec<Vec<u8>>, account: &Value) {
    let user = UserInfoCodec::encode(&user_info_from_account(state, Some(account)));
    let bag = BagInfoCodec::encode(&bag_info_from_account(account));
    append_method_push(pre_pushes, "user.UpdateUserInfo", user);
    append_method_push(pre_pushes, "bag.UpdateBagData", bag);
    append_method_push(
        pre_pushes,
        "outpost.UpdateOutPostInfo",
        outpost_info_payload(&mut account.clone()),
    );
}
