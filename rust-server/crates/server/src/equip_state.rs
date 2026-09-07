#![allow(dead_code)]

use serde_json::{json, Value};

use super::*;

pub(super) fn auto_select_enhancement_materials(
    account: &Value,
    catalog: Option<&EquipCatalog>,
    equip_id: u64,
) -> Vec<(i32, i32)> {
    let Some(catalog) = catalog else {
        return Vec::new();
    };
    let Some(items) = account
        .get("bag")
        .and_then(|bag| bag.get("items"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    // One-click enhancement must only select materials valid for current level.
    // Selecting every quality at once makes enhance_equip_state reject the request
    // because config_equip_enhance_item.enhance_level_limit is mutually exclusive.
    let current_level = account
        .get("equip")
        .and_then(|equip| equip.get("items"))
        .and_then(Value::as_array)
        .and_then(|equipments| {
            equipments
                .iter()
                .find(|item| json_u64(item, "equipId") == Some(equip_id))
        })
        .and_then(|item| json_i32(item, "enhanceLv"))
        .unwrap_or_default()
        .max(0);
    let mut selected = Vec::new();
    for item in items {
        let Some(template_id) = json_i32(item, "templateId") else {
            continue;
        };
        let Some((_, limits)) = catalog.enhance_materials.get(&template_id) else {
            continue;
        };
        if limits.is_some_and(|(min_level, max_level)| {
            current_level < min_level || current_level > max_level
        }) {
            continue;
        }
        let count = json_i32(item, "num").unwrap_or_default().max(0);
        if count > 0 {
            selected.push((template_id, count));
        }
    }
    selected
}

pub(super) fn encode_equip_enhance_response(equip_id: u64, level: i32, exp: i32) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, equip_id);
    append_varint_field(&mut output, 2, level.max(0) as u64);
    append_varint_field(&mut output, 3, exp.max(0) as u64);
    output
}

pub(super) fn encode_study_skill_response(hero_id: u64, skill_id: i32) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, hero_id);
    append_varint_field(&mut output, 2, skill_id.max(0) as u64);
    output
}

pub(super) fn enhance_equip_state(
    account: &mut Value,
    catalog: Option<&EquipCatalog>,
    equip_id: u64,
    materials: &[(i32, i32)],
) -> Option<(i32, i32)> {
    if equip_id == 0
        || materials.is_empty()
        || materials.iter().any(|(id, count)| *id <= 0 || *count <= 0)
    {
        return None;
    }
    let mut totals = std::collections::BTreeMap::<i32, i32>::new();
    for (id, count) in materials {
        let entry = totals.entry(*id).or_default();
        *entry = entry.saturating_add(*count);
    }
    if totals
        .iter()
        .any(|(id, count)| bag_item_count(account, *id) < i64::from(*count))
    {
        return None;
    }
    let target = account
        .get("equip")
        .and_then(|equip| equip.get("items"))
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| json_u64(item, "equipId") == Some(equip_id))
        });
    let target = target?;
    let catalog = catalog?;
    let template_id = json_i32(target, "templateId")?;
    let quality = catalog
        .quality_by_template
        .get(&template_id)
        .copied()
        .unwrap_or_default();
    if quality == 5 {
        let next_level = json_i32(target, "enhanceLv")
            .unwrap_or_default()
            .max(0)
            .saturating_add(1);
        let costs = catalog.enhance_level_ur.get(&next_level)?;
        let mut selected = std::collections::BTreeMap::<i32, i32>::new();
        for (id, count) in materials {
            let entry = selected.entry(*id).or_default();
            *entry = entry.saturating_add(*count);
        }
        for (goods_type, item_id, amount) in costs {
            if !resource_available(account, *goods_type, *item_id, *amount)
                || (*goods_type != 5
                    && selected.get(item_id).copied().unwrap_or_default() < *amount)
            {
                return None;
            }
        }
        for (goods_type, item_id, amount) in costs {
            consume_resource(account, *goods_type, *item_id, *amount);
        }
        let item = account
            .get_mut("equip")
            .and_then(|equip| equip.get_mut("items"))
            .and_then(Value::as_array_mut)
            .and_then(|items| {
                items
                    .iter_mut()
                    .find(|item| json_u64(item, "equipId") == Some(equip_id))
            })?;
        item["enhanceLv"] = json!(next_level);
        item["enhanceExp"] = json!(0);
        return Some((next_level, 0));
    }
    let current_level = json_i32(target, "enhanceLv").unwrap_or_default().max(0);
    let max_level = *catalog.enhance_max_by_template.get(&template_id)?;
    if current_level >= max_level {
        return None;
    }
    let mut added_exp = 0i64;
    for (id, count) in &totals {
        let (exp, limits) = catalog.enhance_materials.get(id)?;
        if let Some((min_level, max_level)) = limits {
            if current_level < *min_level || current_level > *max_level {
                return None;
            }
        }
        added_exp = added_exp.saturating_add(i64::from(*exp).saturating_mul(i64::from(*count)));
    }
    let mut level = current_level;
    let mut exp = i64::from(json_i32(target, "enhanceExp").unwrap_or_default().max(0));
    let completed_exp = |level: i32| {
        (1..=level)
            .filter_map(|item_level| catalog.enhance_level_exp.get(&item_level))
            .map(|value| i64::from(*value))
            .sum::<i64>()
    };
    let base_exp = completed_exp(current_level);
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
        return None;
    }
    for (id, count) in totals {
        let consumed = consume_bag_item(account, id, count);
        if consumed != count {
            return None;
        }
    }
    let item = account
        .get_mut("equip")
        .and_then(|equip| equip.get_mut("items"))
        .and_then(Value::as_array_mut)
        .and_then(|items| {
            items
                .iter_mut()
                .find(|item| json_u64(item, "equipId") == Some(equip_id))
        });
    let item = item?;
    item["enhanceLv"] = json!(level);
    item["enhanceExp"] = json!(exp.min(i64::from(i32::MAX)) as i32);
    Some((level, exp.min(i64::from(i32::MAX)) as i32))
}

pub(super) fn enhance_bind_equip_state(
    account: &mut Value,
    catalog: Option<&EquipCatalog>,
    equip_id: u64,
) -> Option<(i32, i32)> {
    let catalog = catalog?;
    let target = account
        .get("equip")
        .and_then(|equip| equip.get("items"))
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| json_u64(item, "equipId") == Some(equip_id))
        })?;
    if json_u64(target, "heroId").unwrap_or_default() == 0 {
        return None;
    }
    let template_id = json_i32(target, "templateId")?;
    let quality = catalog
        .quality_by_template
        .get(&template_id)
        .copied()
        .unwrap_or_default();
    let current_level = json_i32(target, "enhanceLv").unwrap_or_default().max(0);
    let (next_level, costs) = if quality == 5 {
        let rule = catalog.levelbreak_rules.get(&2)?;
        let (min_level, max_level) = rule.level_rank?;
        if current_level < min_level {
            let next = current_level.saturating_add(1);
            (next, catalog.enhance_level_ur.get(&next)?.clone())
        } else {
            if current_level >= max_level {
                return None;
            }
            (current_level.saturating_add(1), rule.costs.clone())
        }
    } else {
        let rule = catalog.levelbreak_rules.get(&1)?;
        let (min_level, max_level) = rule.level_rank?;
        if current_level < min_level || current_level >= max_level {
            return None;
        }
        (current_level.saturating_add(1), rule.costs.clone())
    };
    if costs.is_empty()
        || costs
            .iter()
            .any(|(kind, id, amount)| !resource_available(account, *kind, *id, *amount))
    {
        return None;
    }
    for (kind, id, amount) in costs {
        consume_resource(account, kind, id, amount);
    }
    let item = account
        .get_mut("equip")
        .and_then(|equip| equip.get_mut("items"))
        .and_then(Value::as_array_mut)
        .and_then(|items| {
            items
                .iter_mut()
                .find(|item| json_u64(item, "equipId") == Some(equip_id))
        })?;
    item["enhanceLv"] = json!(next_level);
    item["enhanceExp"] = json!(0);
    Some((next_level, 0))
}

pub(super) fn renovate_equip_state(
    account: &mut Value,
    catalog: Option<&EquipCatalog>,
    equip_id: u64,
    consume_ids: &[u64],
) -> bool {
    if equip_id == 0 || consume_ids.is_empty() {
        return false;
    }
    let Some(catalog) = catalog else {
        return false;
    };
    let Some(items) = account
        .get("equip")
        .and_then(|equip| equip.get("items"))
        .and_then(Value::as_array)
    else {
        return false;
    };
    let Some(target) = items
        .iter()
        .find(|item| json_u64(item, "equipId") == Some(equip_id))
    else {
        return false;
    };
    let template_id = json_i32(target, "templateId").unwrap_or_default();
    let current_star = json_i32(target, "star").unwrap_or_default().max(0);
    let next_star = current_star.saturating_add(1);
    let star_max = *catalog.star_max_by_template.get(&template_id).unwrap_or(&5);
    let Some(rule) = catalog.renovate_rules.get(&next_star) else {
        return false;
    };
    if next_star > star_max
        || json_i32(target, "enhanceLv").unwrap_or_default() < rule.need_level
        || consume_ids.len() != rule.self_count
    {
        return false;
    }
    let consume_set = consume_ids
        .iter()
        .copied()
        .filter(|id| *id != 0 && *id != equip_id)
        .collect::<std::collections::HashSet<_>>();
    if consume_set.len() != consume_ids.len()
        || consume_set.iter().any(|id| {
            !items.iter().any(|item| {
                let same_template = json_i32(item, "templateId") == Some(template_id);
                let universal_core = catalog
                    .quality_by_template
                    .get(&template_id)
                    .is_some_and(|quality| *quality >= 3)
                    && catalog
                        .type_by_template
                        .get(&json_i32(item, "templateId").unwrap_or_default())
                        == Some(&129)
                    && catalog
                        .quality_by_template
                        .get(&json_i32(item, "templateId").unwrap_or_default())
                        == catalog.quality_by_template.get(&template_id);
                json_u64(item, "equipId") == Some(*id)
                    && json_u64(item, "heroId").unwrap_or_default() == 0
                    && (same_template || universal_core)
            })
        })
    {
        return false;
    }
    for (goods_type, item_id, count) in &rule.costs {
        if *count <= 0 {
            return false;
        }
        let enough = if *goods_type == 5 {
            currency_character_key(*item_id)
                .and_then(|key| {
                    account
                        .get("character")
                        .and_then(|value| json_i64(value, key))
                })
                .is_some_and(|value| value >= i64::from(*count))
        } else if *goods_type == 1 || *goods_type == 6 {
            bag_item_count(account, *item_id) >= i64::from(*count)
        } else {
            false
        };
        if !enough {
            return false;
        }
    }
    let Some(items) = account
        .get_mut("equip")
        .and_then(|equip| equip.get_mut("items"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    items.retain(|item| {
        json_u64(item, "equipId")
            .map(|id| !consume_set.contains(&id))
            .unwrap_or(true)
    });
    let Some(target) = items
        .iter_mut()
        .find(|item| json_u64(item, "equipId") == Some(equip_id))
    else {
        return false;
    };
    let star = current_star.saturating_add(1);
    target["star"] = json!(star);
    for (goods_type, item_id, count) in &rule.costs {
        if *goods_type == 5 {
            if let Some(key) = currency_character_key(*item_id) {
                add_character_i64(account, key, -*count);
            }
        } else {
            let _ = consume_bag_item(account, *item_id, *count);
        }
    }
    true
}

pub(super) fn dismantle_equip_state(
    account: &mut Value,
    catalog: Option<&EquipCatalog>,
    requested_ids: &[u64],
) -> (Vec<ShopReward>, Vec<u64>) {
    let Some(catalog) = catalog else {
        return (Vec::new(), Vec::new());
    };
    let Some(items) = account
        .get("equip")
        .and_then(|equip| equip.get("items"))
        .and_then(Value::as_array)
    else {
        return (Vec::new(), Vec::new());
    };
    let requested = requested_ids
        .iter()
        .copied()
        .filter(|id| *id > 0)
        .collect::<std::collections::HashSet<_>>();
    if requested.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut remove_ids = Vec::new();
    let mut remove_indexes = Vec::new();
    let mut totals = std::collections::BTreeMap::<(i32, i32), i32>::new();
    for (index, item) in items.iter().enumerate() {
        let Some(equip_id) = json_u64(item, "equipId") else {
            continue;
        };
        if !requested.contains(&equip_id) {
            continue;
        }
        let Some(template_id) = json_i32(item, "templateId") else {
            continue;
        };
        // Missing config is rejected. no_resolve follows client CanDelect behavior.
        let Some(rewards) = catalog.dismantle_rewards_by_template.get(&template_id) else {
            continue;
        };
        if catalog.no_resolve_templates.contains(&template_id) {
            continue;
        }
        remove_ids.push(equip_id);
        remove_indexes.push(index);
        for &(goods_type, item_id, num) in rewards {
            let total = totals.entry((goods_type, item_id)).or_default();
            *total = total.saturating_add(num);
        }
    }
    if let Some(items) = account
        .get_mut("equip")
        .and_then(|equip| equip.get_mut("items"))
        .and_then(Value::as_array_mut)
    {
        for index in remove_indexes.into_iter().rev() {
            items.remove(index);
        }
    }
    let rewards = totals
        .into_iter()
        .map(|((goods_type, item_id), num)| {
            if goods_type == 5 {
                if let Some(key) = currency_character_key(item_id) {
                    add_character_i64(account, key, num);
                }
            } else {
                add_bag_item(account, item_id, num);
            }
            ShopReward {
                goods_type,
                item_id,
                num,
                instance_id: 0,
            }
        })
        .collect();
    (rewards, remove_ids)
}

pub(super) fn set_equip_hero_id(account: &mut Value, equip_id: u64, hero_id: u64) {
    if equip_id == 0 {
        return;
    }
    if let Some(items) = account
        .get_mut("equip")
        .and_then(|value| value.get_mut("items"))
        .and_then(Value::as_array_mut)
    {
        if let Some(item) = items
            .iter_mut()
            .find(|item| json_u64(item, "equipId") == Some(equip_id))
            .and_then(Value::as_object_mut)
        {
            item.insert("heroId".to_owned(), json!(hero_id));
        }
    }
}
