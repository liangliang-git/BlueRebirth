use serde_json::{json, Value};

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let payload = handle_legacy(context, method, request_args);
    if let Some(error) = context.handler_error.clone() {
        HandlerResult::Error(error)
    } else {
        match payload {
            Some(payload) => HandlerResult::Reply(Response::raw(method, payload)),
            None => HandlerResult::Empty,
        }
    }
}

fn handle_legacy<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let state = context.state;
    let account = &mut *context.account;
    let catalogs = context.catalogs;
    let GameLoginCatalogs {
        fashion: fashion_catalog,
        equip: equip_catalog,
        tasks: task_catalog,
        equip_new_test: equip_new_test_catalog,
        ..
    } = catalogs;
    let account_view = account.as_deref();
    let pre_pushes = &mut *context.pre_pushes;
    let handler_error = &mut *context.handler_error;

    match method {
        "equip.UpdateEquipBagData" => Some(EquipListCodec::encode(&equip_list_from_account(
            account_view.unwrap_or(&Value::Null),
            equip_catalog,
        ))),
        "equiptestcopy.UpdateData" => Some(equip_test_copy_payload(
            account_view.unwrap_or(&Value::Null),
        )),
        "equiptestcopy.ReceiveRewards" => {
            let reward_id = decode_varint_field(request_args, 1);
            if reward_id <= 0 {
                *handler_error = Some(GameError::Internal(
                    "equipment test reward id is invalid".to_owned(),
                ));
                Some(Vec::new())
            } else if let Some(account) = account.as_deref_mut() {
                let received = account
                    .get("equipTestCopy")
                    .and_then(|value| value.get("receivedRewards"))
                    .and_then(Value::as_array)
                    .is_some_and(|values| {
                        values
                            .iter()
                            .any(|value| json_i32(value, "rewardId") == Some(reward_id))
                    });
                if received {
                    Some(encode_retire_hero_response(&[]))
                } else {
                    let rewards = task_catalog
                        .and_then(|catalog| catalog.rewards_by_id.get(&reward_id))
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(goods_type, item_id, num)| ShopReward {
                            goods_type,
                            item_id,
                            num,
                            instance_id: 0,
                        })
                        .map(|reward| {
                            grant_reward(account, reward, current_unix_seconds(), fashion_catalog)
                        })
                        .collect::<Vec<_>>();
                    if rewards.is_empty() && task_catalog.is_some() {
                        *handler_error = Some(GameError::Internal(
                            "equipment test reward is not configured".to_owned(),
                        ));
                        return Some(Vec::new());
                    }
                    account
                        .as_object_mut()
                        .expect("account must be an object")
                        .entry("equipTestCopy")
                        .or_insert_with(|| json!({"maxDamage": 0, "receivedRewards": []}));
                    account["equipTestCopy"]["receivedRewards"]
                        .as_array_mut()
                        .expect("equipment test rewards must be an array")
                        .push(json!({
                            "rewardId": reward_id,
                            "receiveTime": current_unix_seconds()
                        }));
                    if !rewards.is_empty() {
                        append_method_push(
                            pre_pushes,
                            "bag.UpdateBagData",
                            BagInfoCodec::encode(&bag_info_from_account(account)),
                        );
                        append_method_push(
                            pre_pushes,
                            "user.UpdateUserInfo",
                            UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                        );
                        append_method_push(
                            pre_pushes,
                            "equip.UpdateEquipBagData",
                            EquipListCodec::encode(&equip_list_from_account(
                                account,
                                equip_catalog,
                            )),
                        );
                    }
                    Some(encode_retire_hero_response(&rewards))
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "equipnewtestcopy.UpdateEquipNewData" => Some(equip_new_test_copy_payload(
            account_view.unwrap_or(&Value::Null),
        )),
        "equipnewtestcopy.ReceiveRewards" => {
            let copy_index = decode_varint_field(request_args, 1);
            let damage_index = decode_varint_field(request_args, 2);
            if copy_index <= 0 || damage_index <= 0 {
                *handler_error = Some(GameError::Internal(
                    "equipment new test reward index is invalid".to_owned(),
                ));
                Some(Vec::new())
            } else if let Some(account) = account.as_deref_mut() {
                let Some(equip_new_test_catalog) = equip_new_test_catalog else {
                    *handler_error = Some(GameError::Internal(
                        "equipment new test reward is not configured".to_owned(),
                    ));
                    return Some(Vec::new());
                };
                let reward_id = match resolve_new_test_reward(
                    equip_new_test_catalog,
                    account,
                    copy_index,
                    damage_index,
                ) {
                    Ok(reward_id) => reward_id,
                    Err(message) => {
                        *handler_error = Some(GameError::Internal(format!(
                            "equipment new test reward {message}"
                        )));
                        return Some(Vec::new());
                    }
                };
                let Some(reward_defs) = task_catalog
                    .and_then(|catalog| catalog.rewards_by_id.get(&reward_id))
                    .filter(|rewards| !rewards.is_empty())
                else {
                    *handler_error = Some(GameError::Internal(
                        "equipment new test reward is not configured".to_owned(),
                    ));
                    return Some(Vec::new());
                };
                let _rewards = reward_defs
                    .iter()
                    .map(|(goods_type, item_id, num)| ShopReward {
                        goods_type: *goods_type,
                        item_id: *item_id,
                        num: *num,
                        instance_id: 0,
                    })
                    .map(|reward| {
                        grant_reward(account, reward, current_unix_seconds(), fashion_catalog)
                    })
                    .collect::<Vec<_>>();
                if !mark_new_test_reward(account, copy_index, damage_index) {
                    *handler_error = Some(GameError::Internal(
                        "equipment new test reward already claimed".to_owned(),
                    ));
                    return Some(Vec::new());
                }
                append_method_push(
                    pre_pushes,
                    "equipnewtestcopy.UpdateEquipNewData",
                    equip_new_test_copy_payload(account),
                );
                append_method_push(
                    pre_pushes,
                    "bag.UpdateBagData",
                    BagInfoCodec::encode(&bag_info_from_account(account)),
                );
                append_method_push(
                    pre_pushes,
                    "user.UpdateUserInfo",
                    UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                );
                append_method_push(
                    pre_pushes,
                    "equip.UpdateEquipBagData",
                    EquipListCodec::encode(&equip_list_from_account(account, equip_catalog)),
                );
                Some(Vec::new())
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "equipactivity.UpdateEquipActivityInfo" => {
            if let Some(account) = account.as_deref_mut() {
                ensure_equip_activity_state(account, equip_catalog);
                Some(equip_activity_payload(account))
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "equipactivity.GetReward" => {
            let equip_id = decode_varint_u64_field(request_args, 1);
            if equip_id == 0 {
                *handler_error = Some(GameError::Internal(
                    "equipment activity id is invalid".to_owned(),
                ));
                Some(Vec::new())
            } else if let Some(account) = account.as_deref_mut() {
                ensure_equip_activity_state(account, equip_catalog);
                let template_id = account
                    .get("equip")
                    .and_then(|equip| equip.get("items"))
                    .and_then(Value::as_array)
                    .and_then(|items| {
                        items.iter().find_map(|item| {
                            (json_u64(item, "equipId") == Some(equip_id))
                                .then(|| json_i32(item, "templateId"))
                                .flatten()
                        })
                    });
                let reward_id = template_id.and_then(|template_id| {
                    equip_catalog.and_then(|catalog| {
                        catalog
                            .activity_reward_by_template
                            .get(&template_id)
                            .copied()
                    })
                });
                let reward_defs = reward_id.and_then(|reward_id| {
                    task_catalog.and_then(|catalog| catalog.rewards_by_id.get(&reward_id))
                });
                if let Some(reward_defs) = reward_defs.filter(|rewards| !rewards.is_empty()) {
                    if !mark_equip_activity_reward(account, equip_id) {
                        *handler_error = Some(GameError::Internal(
                            "equipment activity reward is already claimed or unavailable"
                                .to_owned(),
                        ));
                        return Some(Vec::new());
                    }
                    let _rewards = reward_defs
                        .iter()
                        .map(|(goods_type, item_id, num)| ShopReward {
                            goods_type: *goods_type,
                            item_id: *item_id,
                            num: *num,
                            instance_id: 0,
                        })
                        .map(|reward| {
                            grant_reward(account, reward, current_unix_seconds(), fashion_catalog)
                        })
                        .collect::<Vec<_>>();
                    append_method_push(
                        pre_pushes,
                        "equipactivity.UpdateEquipActivityInfo",
                        equip_activity_payload(account),
                    );
                    append_method_push(
                        pre_pushes,
                        "bag.UpdateBagData",
                        BagInfoCodec::encode(&bag_info_from_account(account)),
                    );
                    append_method_push(
                        pre_pushes,
                        "user.UpdateUserInfo",
                        UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                    );
                    Some(Vec::new())
                } else {
                    *handler_error = Some(GameError::Internal(
                        "equipment activity reward is not configured".to_owned(),
                    ));
                    Some(Vec::new())
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "equip.Dismantle" => {
            let equip_ids = decode_repeated_varint_field(request_args, 1)
                .into_iter()
                .filter_map(|id| u64::try_from(id).ok())
                .collect::<Vec<_>>();
            if let Some(account) = account.as_deref_mut() {
                let (rewards, removed_ids) =
                    dismantle_equip_state(account, equip_catalog, &equip_ids);
                if removed_ids.is_empty() {
                    *handler_error = Some(GameError::Internal(
                        "no dismantlable equipment was selected".to_owned(),
                    ));
                    Some(Vec::new())
                } else {
                    let mut equip_push = equip_list_from_account(account, equip_catalog);
                    equip_push.items.extend(removed_ids.iter().filter_map(|id| {
                        u32::try_from(*id).ok().map(|equip_id| EquipInfo {
                            equip_id,
                            template_id: 0,
                            ..EquipInfo::default()
                        })
                    }));
                    append_method_push(
                        pre_pushes,
                        "equip.UpdateEquipBagData",
                        EquipListCodec::encode(&equip_push),
                    );
                    append_method_push(
                        pre_pushes,
                        "bag.UpdateBagData",
                        BagInfoCodec::encode(&bag_info_from_account(account)),
                    );
                    append_method_push(
                        pre_pushes,
                        "user.UpdateUserInfo",
                        UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                    );
                    Some(encode_retire_hero_response(&rewards))
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "equip.Enhance" => {
            let (equip_id, mut materials) = decode_equip_enhance_request(request_args);
            if let Some(account) = account.as_deref_mut() {
                if materials.is_empty() {
                    materials = auto_select_enhancement_materials(account, equip_catalog, equip_id);
                }
                if std::env::var_os("BLUEOATH_TRACE_METHODS").is_some() {
                    eprintln!(
                        "equip.Enhance state equip={} materials={:?}",
                        equip_id, materials
                    );
                }
                if let Some((level, exp)) =
                    enhance_equip_state(account, equip_catalog, equip_id, &materials)
                {
                    advance_task_event(account, task_catalog, 2726, 1, current_unix_seconds());
                    append_method_push(
                        pre_pushes,
                        "bag.UpdateBagData",
                        BagInfoCodec::encode(&bag_info_from_account(account)),
                    );
                    append_method_push(
                        pre_pushes,
                        "equip.UpdateEquipBagData",
                        EquipListCodec::encode(&equip_list_from_account(account, equip_catalog)),
                    );
                    append_method_push(
                        pre_pushes,
                        "user.UpdateUserInfo",
                        UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                    );
                    append_method_push(
                        pre_pushes,
                        "task.TaskInfo",
                        task_info_payload(account, task_catalog),
                    );
                    Some(encode_equip_enhance_response(equip_id, level, exp))
                } else {
                    if std::env::var_os("BLUEOATH_TRACE_METHODS").is_some() {
                        eprintln!("equip.Enhance rejected equip={}", equip_id);
                    }
                    *handler_error = Some(GameError::Internal(
                        "equipment enhancement requirements are not met".to_owned(),
                    ));
                    Some(Vec::new())
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "equip.EnhanceBind" => {
            let equip_id = decode_varint_u64_field(request_args, 1);
            if let Some(account) = account.as_deref_mut() {
                if let Some((level, exp)) =
                    enhance_bind_equip_state(account, equip_catalog, equip_id)
                {
                    advance_task_event(account, task_catalog, 2726, 1, current_unix_seconds());
                    append_method_push(
                        pre_pushes,
                        "user.UpdateUserInfo",
                        UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                    );
                    append_method_push(
                        pre_pushes,
                        "bag.UpdateBagData",
                        BagInfoCodec::encode(&bag_info_from_account(account)),
                    );
                    append_method_push(
                        pre_pushes,
                        "equip.UpdateEquipBagData",
                        EquipListCodec::encode(&equip_list_from_account(account, equip_catalog)),
                    );
                    append_method_push(
                        pre_pushes,
                        "task.TaskInfo",
                        task_info_payload(account, task_catalog),
                    );
                    Some(encode_equip_enhance_response(equip_id, level, exp))
                } else {
                    *handler_error = Some(GameError::Internal(
                        "bound equipment enhancement requirements are not met".to_owned(),
                    ));
                    Some(Vec::new())
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "equip.RiseStar" => {
            let equip_id = decode_varint_u64_field(request_args, 1);
            let consume_ids = decode_repeated_varint_field(request_args, 2)
                .into_iter()
                .filter_map(|id| u64::try_from(id).ok())
                .collect::<Vec<_>>();
            if let Some(account) = account.as_deref_mut() {
                if renovate_equip_state(account, equip_catalog, equip_id, &consume_ids) {
                    advance_task_event(account, task_catalog, 2727, 1, current_unix_seconds());
                    let response = equip_list_from_account(account, equip_catalog)
                        .items
                        .into_iter()
                        .find(|item| u64::from(item.equip_id) == equip_id)
                        .map(|item| EquipListCodec::encode_item(&item))
                        .unwrap_or_default();
                    append_method_push(pre_pushes, "equip.UpdateEquipBagData", {
                        let mut equip_push = equip_list_from_account(account, equip_catalog);
                        equip_push.items.extend(consume_ids.iter().filter_map(|id| {
                            u32::try_from(*id).ok().map(|equip_id| EquipInfo {
                                equip_id,
                                template_id: 0,
                                ..EquipInfo::default()
                            })
                        }));
                        EquipListCodec::encode(&equip_push)
                    });
                    append_method_push(
                        pre_pushes,
                        "task.TaskInfo",
                        task_info_payload(account, task_catalog),
                    );
                    Some(response)
                } else {
                    *handler_error = Some(GameError::Internal(
                        "equipment renovation requirements are not met".to_owned(),
                    ));
                    Some(Vec::new())
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        _ => None,
    }
}

pub(crate) fn equip_test_copy_payload(account: &Value) -> Vec<u8> {
    let state = account.get("equipTestCopy").unwrap_or(&Value::Null);
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        json_i32(state, "maxDamage").unwrap_or_default().max(0) as u64,
    );
    if let Some(rewards) = state.get("receivedRewards").and_then(Value::as_array) {
        for reward in rewards {
            let mut encoded = Vec::new();
            append_varint_field(
                &mut encoded,
                1,
                json_i32(reward, "rewardId").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut encoded,
                2,
                json_i32(reward, "receiveTime").unwrap_or_default().max(0) as u64,
            );
            append_message_field(&mut output, 2, &encoded);
        }
    }
    output
}

pub(crate) fn equip_new_test_copy_payload(account: &Value) -> Vec<u8> {
    let state = account.get("equipNewTestCopy").unwrap_or(&Value::Null);
    let mut output = Vec::new();
    if let Some(infos) = state.get("infos").and_then(Value::as_array) {
        for info in infos {
            let mut encoded = Vec::new();
            append_varint_field(
                &mut encoded,
                1,
                json_i32(info, "id").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut encoded,
                2,
                json_i32(info, "maxDamage").unwrap_or_default().max(0) as u64,
            );
            if let Some(rewards) = info.get("receivedRewards").and_then(Value::as_array) {
                for reward in rewards {
                    let mut reward_encoded = Vec::new();
                    append_varint_field(
                        &mut reward_encoded,
                        1,
                        json_i32(reward, "damageIndex").unwrap_or_default().max(0) as u64,
                    );
                    append_varint_field(
                        &mut reward_encoded,
                        2,
                        json_i32(reward, "receiveTime").unwrap_or_default().max(0) as u64,
                    );
                    append_message_field(&mut encoded, 3, &reward_encoded);
                }
            }
            append_message_field(&mut output, 1, &encoded);
        }
    }
    output
}

pub(crate) fn equip_activity_payload(account: &Value) -> Vec<u8> {
    let state = account.get("equipActivity").unwrap_or(&Value::Null);
    let mut output = Vec::new();
    if let Some(infos) = state.get("infos").and_then(Value::as_array) {
        for info in infos {
            let mut encoded = Vec::new();
            append_varint_field(
                &mut encoded,
                1,
                json_u64(info, "equipId").unwrap_or_default(),
            );
            append_varint_field(
                &mut encoded,
                2,
                json_i32(info, "templateId").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut encoded,
                3,
                json_i32(info, "powerPoint").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut encoded,
                4,
                json_i32(info, "isReward").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut encoded,
                5,
                json_i32(info, "extraRule").unwrap_or_default().max(0) as u64,
            );
            append_message_field(&mut output, 1, &encoded);
        }
    }
    output
}

fn ensure_equip_activity_state(account: &mut Value, equip_catalog: Option<&EquipCatalog>) {
    let Some(catalog) = equip_catalog else {
        return;
    };
    let activity_items = account
        .get("equip")
        .and_then(|equip| equip.get("items"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let equip_id = json_u64(item, "equipId")?;
                    let template_id = json_i32(item, "templateId")?;
                    catalog
                        .activity_equip_by_template
                        .contains(&template_id)
                        .then_some((equip_id, template_id))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let state = account
        .as_object_mut()
        .expect("account must be an object")
        .entry("equipActivity")
        .or_insert_with(|| json!({"infos": []}));
    let infos = state
        .as_object_mut()
        .expect("equipment activity state must be an object")
        .entry("infos")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("equipment activity infos must be an array");
    for (equip_id, template_id) in activity_items {
        if !infos
            .iter()
            .any(|info| json_u64(info, "equipId") == Some(equip_id))
        {
            infos.push(json!({
                "equipId": equip_id,
                "templateId": template_id,
                "powerPoint": 0,
                "isReward": 0,
                "extraRule": 0
            }));
        }
    }
}

pub(crate) fn mark_new_test_reward(
    account: &mut Value,
    copy_index: i32,
    damage_index: i32,
) -> bool {
    let root = account
        .as_object_mut()
        .expect("account must be an object")
        .entry("equipNewTestCopy")
        .or_insert_with(|| json!({"infos": []}));
    let infos = root
        .as_object_mut()
        .expect("equipment new test state must be an object")
        .entry("infos")
        .or_insert_with(|| json!([]));
    let infos = infos
        .as_array_mut()
        .expect("equipment new test infos must be an array");
    let info = if let Some(info) = infos
        .iter_mut()
        .find(|info| json_i32(info, "id") == Some(copy_index))
    {
        info
    } else {
        infos.push(json!({"id": copy_index, "maxDamage": 0, "receivedRewards": []}));
        infos.last_mut().expect("new test info was inserted")
    };
    let rewards = info
        .as_object_mut()
        .expect("equipment new test info must be an object")
        .entry("receivedRewards")
        .or_insert_with(|| json!([]));
    let rewards = rewards
        .as_array_mut()
        .expect("equipment new test rewards must be an array");
    if rewards
        .iter()
        .any(|reward| json_i32(reward, "damageIndex") == Some(damage_index))
    {
        return false;
    }
    rewards.push(json!({
        "damageIndex": damage_index,
        "receiveTime": current_unix_seconds()
    }));
    true
}

pub(crate) fn resolve_new_test_reward(
    catalog: &EquipNewTestCatalog,
    account: &Value,
    copy_index: i32,
    damage_index: i32,
) -> Result<i32, &'static str> {
    let (threshold, reward_id) = catalog
        .reward_id(copy_index, damage_index)
        .ok_or("index is invalid")?;
    let info = account
        .get("equipNewTestCopy")
        .and_then(|state| state.get("infos"))
        .and_then(Value::as_array)
        .and_then(|infos| {
            infos
                .iter()
                .find(|info| json_i32(info, "id") == Some(copy_index))
        });
    let max_damage = info
        .and_then(|info| json_i32(info, "maxDamage"))
        .unwrap_or_default();
    if max_damage < threshold {
        return Err("damage threshold not reached");
    }
    if info
        .and_then(|info| info.get("receivedRewards"))
        .and_then(Value::as_array)
        .is_some_and(|rewards| {
            rewards
                .iter()
                .any(|reward| json_i32(reward, "damageIndex") == Some(damage_index))
        })
    {
        return Err("already claimed");
    }
    Ok(reward_id)
}

pub(crate) fn update_new_test_max_damage(
    account: &mut Value,
    catalog: &EquipNewTestCatalog,
    copy_id: i32,
    damage: i32,
) -> Option<i32> {
    let copy_index = catalog
        .copy_ids
        .iter()
        .position(|configured_id| *configured_id == copy_id)
        .and_then(|index| i32::try_from(index + 1).ok())?;
    let root = account
        .as_object_mut()?
        .entry("equipNewTestCopy")
        .or_insert_with(|| json!({"infos": []}));
    let infos = root
        .as_object_mut()?
        .entry("infos")
        .or_insert_with(|| json!([]))
        .as_array_mut()?;
    let info = if let Some(info) = infos
        .iter_mut()
        .find(|info| json_i32(info, "id") == Some(copy_index))
    {
        info
    } else {
        infos.push(json!({"id": copy_index, "maxDamage": 0, "receivedRewards": []}));
        infos.last_mut()?
    };
    let current = json_i32(info, "maxDamage").unwrap_or_default().max(0);
    let next = current.max(damage.max(0));
    if next > current {
        info["maxDamage"] = json!(next);
    }
    Some(next)
}

pub(crate) fn mark_equip_activity_reward(account: &mut Value, equip_id: u64) -> bool {
    let infos = account
        .as_object_mut()
        .expect("account must be an object")
        .entry("equipActivity")
        .or_insert_with(|| json!({"infos": []}))
        .as_object_mut()
        .expect("equipment activity state must be an object")
        .entry("infos")
        .or_insert_with(|| json!([]));
    let Some(info) = infos
        .as_array_mut()
        .expect("equipment activity infos must be an array")
        .iter_mut()
        .find(|info| json_u64(info, "equipId") == Some(equip_id))
    else {
        return false;
    };
    if json_i32(info, "isReward").unwrap_or_default() > 0 {
        return false;
    }
    info["isReward"] = json!(1);
    true
}
