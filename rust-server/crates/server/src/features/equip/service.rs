#![allow(dead_code)]

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

fn typed_compat_currency(item_id: i32) -> bool {
    matches!(item_id, 8..=15 | 18 | 22..=29 | 31..=33)
}

fn credit_typed_currency(
    account: &mut blueoath_domain::AccountState,
    item_id: i32,
    amount: u64,
) -> bool {
    if let Some(kind) = typed_currency_kind(item_id) {
        return account.resources.credit(kind, amount).is_ok();
    }
    if !typed_compat_currency(item_id) {
        return false;
    }
    let key = format!("compat:currency:{item_id}");
    let current = account
        .activities
        .progress
        .get(&key)
        .copied()
        .unwrap_or_default();
    let Some(next) = current.checked_add(amount) else {
        return false;
    };
    account.activities.progress.insert(key, next);
    true
}

pub(crate) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    state: &ServerState,
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
                    let _ = credit_typed_currency(
                        account,
                        reward.item_id,
                        u64::try_from(reward.num).unwrap_or_default(),
                    );
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
            effects.push_pre(Response::raw(
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
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
                    EquipListCodec::encode_item(&equip_info_from_typed_equipment(
                        equipment,
                        EQUIP_CATALOG.get(),
                    ))
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
