#![allow(dead_code)]

#[cfg(test)]
use serde_json::{json, Value};

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;

fn typed_currency_kind(item_id: i32) -> Option<blueoath_domain::CurrencyKind> {
    match item_id {
        1 => Some(blueoath_domain::CurrencyKind::Gold),
        2 => Some(blueoath_domain::CurrencyKind::Diamond),
        5 => Some(blueoath_domain::CurrencyKind::Supply),
        30 => Some(blueoath_domain::CurrencyKind::PvePoint),
        _ => None,
    }
}

pub(crate) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
    equip_catalog: Option<&EquipCatalog>,
    task_catalog: Option<&TaskCatalog>,
) -> HandlerResult {
    match method {
        "equip.UpdateEquipBagData" => HandlerResult::Reply(Response::raw(
            method,
            EquipListCodec::encode(&equip_list_from_typed_account(account)),
        )),
        "equip.Dismantle" => {
            let Some(catalog) = equip_catalog else {
                return HandlerResult::Empty;
            };
            let Ok(request) = EquipDismantleRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "equipment dismantle request is invalid",
                ));
            };
            let requested = request
                .equip_ids
                .into_iter()
                .filter_map(|id| blueoath_domain::EquipId::new(id).ok())
                .collect::<std::collections::BTreeSet<_>>();
            if requested.is_empty() {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "equipment dismantle request is invalid",
                ));
            }
            let mut removed = Vec::new();
            let mut rewards = Vec::new();
            for equip_id in &requested {
                let Some(equipment) = account.dock.equipments.get(equip_id) else {
                    continue;
                };
                let template_id = i32::try_from(equipment.template_id.get()).unwrap_or_default();
                if catalog.no_resolve_templates.contains(&template_id) {
                    continue;
                }
                let Some(definitions) = catalog.dismantle_rewards_by_template.get(&template_id)
                else {
                    continue;
                };
                removed.push(*equip_id);
                rewards.extend(
                    definitions
                        .iter()
                        .filter_map(|(goods_type, item_id, amount)| {
                            (*amount > 0).then_some(ShopReward {
                                goods_type: *goods_type,
                                item_id: *item_id,
                                num: *amount,
                                instance_id: 0,
                            })
                        }),
                );
            }
            if removed.is_empty() {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "no dismantlable equipment was selected",
                ));
            }
            let removed_set = removed
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            account
                .dock
                .equipments
                .retain(|equip_id, _| !removed_set.contains(equip_id));
            for hero in account.dock.heroes.values_mut() {
                for equip_id in &mut hero.equip_slots {
                    if equip_id.is_some_and(|id| removed_set.contains(&id)) {
                        *equip_id = None;
                    }
                }
            }
            for reward in &rewards {
                if reward.goods_type == 5 {
                    let kind = match reward.item_id {
                        1 => Some(blueoath_domain::CurrencyKind::Gold),
                        2 => Some(blueoath_domain::CurrencyKind::Diamond),
                        5 => Some(blueoath_domain::CurrencyKind::Supply),
                        30 => Some(blueoath_domain::CurrencyKind::PvePoint),
                        _ => None,
                    };
                    if let Some(kind) = kind {
                        let _ = account
                            .resources
                            .credit(kind, u64::try_from(reward.num).unwrap_or_default());
                    }
                } else if let Ok(template_id) = blueoath_domain::TemplateId::new(
                    u64::try_from(reward.item_id).unwrap_or_default(),
                ) {
                    let entry = account.inventory.items.entry(template_id).or_default();
                    *entry = entry.saturating_add(u64::try_from(reward.num).unwrap_or_default());
                }
            }
            let mut equip_push = equip_list_from_typed_account(account);
            equip_push.items.extend(removed.iter().filter_map(|id| {
                u32::try_from(id.get()).ok().map(|equip_id| EquipInfo {
                    equip_id,
                    template_id: 0,
                    ..EquipInfo::default()
                })
            }));
            effects.push_pre(Response::raw(
                "equip.UpdateEquipBagData",
                EquipListCodec::encode(&equip_push),
            ));
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            HandlerResult::Reply(Response::raw(method, encode_retire_hero_response(&rewards)))
        }
        "equip.RiseStar" => {
            let Some(catalog) = equip_catalog else {
                return HandlerResult::Empty;
            };
            let Ok(request) = EquipRiseStarRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "equipment rise star request is invalid",
                ));
            };
            let equip_id = request.equip_id;
            let consume_ids = request
                .consume_ids
                .into_iter()
                .filter_map(|id| blueoath_domain::EquipId::new(id).ok())
                .collect::<Vec<_>>();
            let Some(equip_id) = blueoath_domain::EquipId::new(equip_id).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest("equipment id is invalid"));
            };
            let Some(target) = account.dock.equipments.get(&equip_id).cloned() else {
                return HandlerResult::Error(GameError::InvalidRequest("equipment was not found"));
            };
            let template_id = i32::try_from(target.template_id.get()).unwrap_or_default();
            let current_star = i32::try_from(target.star).unwrap_or(i32::MAX);
            let next_star = current_star.saturating_add(1);
            let star_max = *catalog.star_max_by_template.get(&template_id).unwrap_or(&5);
            let Some(rule) = catalog.renovate_rules.get(&next_star) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "equipment renovation rule is missing",
                ));
            };
            if next_star > star_max
                || i32::try_from(target.enhance_level).unwrap_or(i32::MAX) < rule.need_level
                || consume_ids.len() != rule.self_count
                || consume_ids.contains(&equip_id)
                || consume_ids
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != consume_ids.len()
            {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "equipment renovation requirements are not met",
                ));
            }
            let target_quality = catalog.quality_by_template.get(&template_id).copied();
            let target_type = target_quality
                .map(|_| template_id)
                .and_then(|_| catalog.type_by_template.get(&template_id).copied());
            for material_id in &consume_ids {
                let Some(material) = account.dock.equipments.get(material_id) else {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "renovation material was not found",
                    ));
                };
                let material_template =
                    i32::try_from(material.template_id.get()).unwrap_or_default();
                let same_template = material_template == template_id;
                let universal_core = target_quality.is_some_and(|quality| quality >= 3)
                    && target_type.is_some()
                    && catalog.type_by_template.get(&material_template) == Some(&129)
                    && catalog.quality_by_template.get(&material_template)
                        == target_quality.as_ref();
                if material.hero_id.is_some() || !(same_template || universal_core) {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "renovation material is invalid",
                    ));
                }
            }
            let mut costs = Vec::new();
            for &(goods_type, item_id, amount) in &rule.costs {
                if amount <= 0 {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "renovation cost is invalid",
                    ));
                }
                let amount = u64::try_from(amount).unwrap_or_default();
                if goods_type == 5 {
                    let Some(kind) = typed_currency_kind(item_id) else {
                        return HandlerResult::Error(GameError::InvalidRequest(
                            "renovation currency is unsupported",
                        ));
                    };
                    if account.resources.amount(kind).get() < amount {
                        return HandlerResult::Error(GameError::InvalidRequest(
                            "renovation currency is insufficient",
                        ));
                    }
                    costs.push((goods_type, item_id, amount));
                } else if matches!(goods_type, 1 | 6) {
                    let Some(template_id) = blueoath_domain::TemplateId::new(
                        u64::try_from(item_id).unwrap_or_default(),
                    )
                    .ok() else {
                        return HandlerResult::Error(GameError::InvalidRequest(
                            "renovation item is invalid",
                        ));
                    };
                    if account
                        .inventory
                        .items
                        .get(&template_id)
                        .copied()
                        .unwrap_or_default()
                        < amount
                    {
                        return HandlerResult::Error(GameError::InvalidRequest(
                            "renovation item is insufficient",
                        ));
                    }
                    costs.push((goods_type, item_id, amount));
                } else {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "renovation cost is unsupported",
                    ));
                }
            }
            let consumed = consume_ids
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            account
                .dock
                .equipments
                .retain(|id, _| !consumed.contains(id));
            for hero in account.dock.heroes.values_mut() {
                for equipped in &mut hero.equip_slots {
                    if equipped.is_some_and(|id| consumed.contains(&id)) {
                        *equipped = None;
                    }
                }
            }
            if let Some(target) = account.dock.equipments.get_mut(&equip_id) {
                target.star = u32::try_from(next_star).unwrap_or(u32::MAX);
            }
            for (goods_type, item_id, amount) in costs {
                if goods_type == 5 {
                    if let Some(kind) = typed_currency_kind(item_id) {
                        let _ = account.resources.debit(kind, amount);
                    }
                } else if let Ok(template_id) =
                    blueoath_domain::TemplateId::new(u64::try_from(item_id).unwrap_or_default())
                {
                    if let Some(value) = account.inventory.items.get_mut(&template_id) {
                        *value = value.saturating_sub(amount);
                    }
                }
            }
            advance_typed_task_event(account, task_catalog, 2727, 1);
            let mut equip_push = equip_list_from_typed_account(account);
            equip_push.items.extend(consume_ids.iter().filter_map(|id| {
                u32::try_from(id.get()).ok().map(|equip_id| EquipInfo {
                    equip_id,
                    template_id: 0,
                    ..EquipInfo::default()
                })
            }));
            effects.push_pre(Response::raw(
                "equip.UpdateEquipBagData",
                EquipListCodec::encode(&equip_push),
            ));
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "task.TaskInfo",
                task_info_payload_from_typed_account(account, task_catalog),
            ));
            let response = account
                .dock
                .equipments
                .get(&equip_id)
                .map(|equipment| {
                    EquipListCodec::encode_item(&equip_info_from_typed_equipment(equipment))
                })
                .unwrap_or_default();
            HandlerResult::Reply(Response::raw(method, response))
        }
        "equip.Enhance" => {
            let Some(catalog) = equip_catalog else {
                return HandlerResult::Empty;
            };
            let Ok(request) = EquipEnhanceRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "equipment enhancement request is invalid",
                ));
            };
            let equip_id = request.equip_id;
            let materials = request
                .materials
                .iter()
                .map(|material| (material.template_id, material.amount))
                .collect::<Vec<_>>();
            if materials.is_empty() {
                return HandlerResult::Empty;
            }
            let Some(equip_id) = blueoath_domain::EquipId::new(equip_id).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest("equipment id is invalid"));
            };
            let Some(target) = account.dock.equipments.get(&equip_id).cloned() else {
                return HandlerResult::Error(GameError::InvalidRequest("equipment was not found"));
            };
            let template_id = i32::try_from(target.template_id.get()).unwrap_or_default();
            if catalog.quality_by_template.get(&template_id) == Some(&5) {
                return HandlerResult::Empty;
            }
            let max_level = *catalog
                .enhance_max_by_template
                .get(&template_id)
                .unwrap_or(&0);
            let current_level = i32::try_from(target.enhance_level).unwrap_or(i32::MAX);
            if max_level <= 0 || current_level >= max_level {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "equipment enhancement requirements are not met",
                ));
            }
            let mut totals = std::collections::BTreeMap::new();
            for (item_id, count) in materials {
                if item_id <= 0 || count <= 0 {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "equipment enhancement materials are invalid",
                    ));
                }
                let entry = totals.entry(item_id).or_insert(0i32);
                *entry = entry.saturating_add(count);
            }
            let mut added_exp = 0i64;
            for (item_id, count) in &totals {
                let Some((exp, limits)) = catalog.enhance_materials.get(item_id) else {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "equipment enhancement material is not configured",
                    ));
                };
                if limits.is_some_and(|(min, max)| current_level < min || current_level > max) {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "equipment enhancement material is not valid for level",
                    ));
                }
                let Some(item) =
                    blueoath_domain::TemplateId::new(u64::try_from(*item_id).unwrap_or_default())
                        .ok()
                else {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "material id is invalid",
                    ));
                };
                if account
                    .inventory
                    .items
                    .get(&item)
                    .copied()
                    .unwrap_or_default()
                    < u64::try_from(*count).unwrap_or(u64::MAX)
                {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "equipment enhancement materials are insufficient",
                    ));
                }
                added_exp =
                    added_exp.saturating_add(i64::from(*exp).saturating_mul(i64::from(*count)));
            }
            let completed_exp = |level: i32| {
                (1..=level)
                    .filter_map(|value| catalog.enhance_level_exp.get(&value))
                    .map(|value| i64::from(*value))
                    .sum::<i64>()
            };
            let mut level = current_level;
            let mut exp = i64::try_from(target.enhance_exp).unwrap_or(i64::MAX);
            let base_exp = completed_exp(level);
            if exp < base_exp {
                exp = base_exp.saturating_add(exp);
            }
            exp = exp.saturating_add(added_exp);
            while level < max_level {
                let next_exp = completed_exp(level.saturating_add(1));
                if next_exp <= 0 || exp < next_exp {
                    break;
                }
                level = level.saturating_add(1);
            }
            if level == current_level {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "equipment enhancement requirements are not met",
                ));
            }
            for (item_id, count) in totals {
                let item =
                    blueoath_domain::TemplateId::new(u64::try_from(item_id).unwrap_or_default())
                        .unwrap();
                if let Some(available) = account.inventory.items.get_mut(&item) {
                    *available = available.saturating_sub(u64::try_from(count).unwrap_or_default());
                }
            }
            if let Some(target) = account.dock.equipments.get_mut(&equip_id) {
                target.enhance_level = u32::try_from(level).unwrap_or(u32::MAX);
                target.enhance_exp = u64::try_from(exp).unwrap_or(u64::MAX);
            }
            advance_typed_task_event(account, task_catalog, 2726, 1);
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "equip.UpdateEquipBagData",
                EquipListCodec::encode(&equip_list_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "task.TaskInfo",
                task_info_payload_from_typed_account(account, task_catalog),
            ));
            HandlerResult::Reply(Response::raw(
                method,
                encode_equip_enhance_response(
                    equip_id.get(),
                    level,
                    i32::try_from(exp).unwrap_or(i32::MAX),
                ),
            ))
        }
        _ => HandlerResult::Empty,
    }
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_equip_list_reads_normalized_equipment() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("equip-typed").unwrap(),
            "Captain",
        );
        let equip_id = blueoath_domain::EquipId::new(4).unwrap();
        account.dock.equipments.insert(
            equip_id,
            blueoath_domain::EquipmentState {
                id: equip_id,
                template_id: blueoath_domain::TemplateId::new(300).unwrap(),
                enhance_level: 2,
                star: 3,
                enhance_exp: 5,
                hero_id: None,
            },
        );
        let HandlerResult::Reply(response) = handle_typed(
            &mut account,
            "equip.UpdateEquipBagData",
            &[],
            &mut ResponseEffects::default(),
            None,
            None,
        ) else {
            panic!("typed equipment route must reply");
        };
        assert!(response.payload.len() > 2);
    }

    #[test]
    fn typed_equip_enhance_consumes_material_and_updates_equipment() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("equip-enhance-typed").unwrap(),
            "Captain",
        );
        let item_id = blueoath_domain::TemplateId::new(10_182).unwrap();
        let before = account
            .inventory
            .items
            .get(&item_id)
            .copied()
            .unwrap_or_default();
        let mut catalog = EquipCatalog::default();
        catalog.enhance_max_by_template.insert(30_091, 5);
        catalog.enhance_materials.insert(10_182, (600, None));
        for level in 1..=5 {
            catalog.enhance_level_exp.insert(level, 500);
        }
        let mut material = Vec::new();
        append_varint_field(&mut material, 1, 10_182);
        append_varint_field(&mut material, 2, 1);
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_bytes_field(&mut args, 2, &material);
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "equip.Enhance",
            &args,
            &mut effects,
            Some(&catalog),
            None,
        );

        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(account.inventory.items.get(&item_id), Some(&(before - 1)));
        assert_eq!(
            account
                .dock
                .equipments
                .get(&blueoath_domain::EquipId::new(1).unwrap())
                .unwrap()
                .enhance_level,
            1
        );
        assert_eq!(
            account
                .dock
                .equipments
                .get(&blueoath_domain::EquipId::new(1).unwrap())
                .unwrap()
                .enhance_exp,
            600
        );
        assert_eq!(effects.into_parts().0.len(), 3);
    }

    #[test]
    fn typed_equip_dismantle_removes_equipment_and_grants_rewards() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("equip-dismantle-typed").unwrap(),
            "Captain",
        );
        let item_id = blueoath_domain::TemplateId::new(10_182).unwrap();
        let before = account
            .inventory
            .items
            .get(&item_id)
            .copied()
            .unwrap_or_default();
        let mut catalog = EquipCatalog::default();
        catalog
            .dismantle_rewards_by_template
            .insert(30_091, vec![(1, 10_182, 3)]);
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "equip.Dismantle",
            &args,
            &mut effects,
            Some(&catalog),
            None,
        );

        assert!(matches!(result, HandlerResult::Reply(_)));
        assert!(!account
            .dock
            .equipments
            .contains_key(&blueoath_domain::EquipId::new(1).unwrap()));
        assert_eq!(account.inventory.items.get(&item_id), Some(&(before + 3)));
        assert_eq!(
            account.dock.heroes.values().next().unwrap().equip_slots[0],
            None
        );
        assert_eq!(effects.into_parts().0.len(), 2);
    }

    #[test]
    fn typed_equip_rise_star_consumes_unbound_material() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("equip-star-typed").unwrap(),
            "Captain",
        );
        let hero_id = blueoath_domain::HeroId::new(1).unwrap();
        account.dock.heroes.get_mut(&hero_id).unwrap().equip_slots[2] = None;
        account
            .dock
            .equipments
            .get_mut(&blueoath_domain::EquipId::new(2).unwrap())
            .unwrap()
            .hero_id = None;
        let mut catalog = EquipCatalog::default();
        catalog.star_max_by_template.insert(30_091, 5);
        catalog.renovate_rules.insert(
            1,
            EquipRenovateRule {
                self_count: 1,
                ..EquipRenovateRule::default()
            },
        );
        catalog.quality_by_template.insert(30_091, 3);
        catalog.quality_by_template.insert(30_221, 3);
        catalog.type_by_template.insert(30_091, 1);
        catalog.type_by_template.insert(30_221, 129);
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_varint_field(&mut args, 2, 2);
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "equip.RiseStar",
            &args,
            &mut effects,
            Some(&catalog),
            None,
        );

        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(
            account
                .dock
                .equipments
                .get(&blueoath_domain::EquipId::new(1).unwrap())
                .unwrap()
                .star,
            1
        );
        assert!(!account
            .dock
            .equipments
            .contains_key(&blueoath_domain::EquipId::new(2).unwrap()));
        assert_eq!(effects.into_parts().0.len(), 3);
    }
}
