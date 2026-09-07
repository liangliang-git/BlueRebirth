#![allow(dead_code)]

use serde_json::{json, Value};

use super::*;

pub(super) fn hero_intensify_state(
    account: &mut Value,
    catalog: &ShipIntensifyCatalog,
    hero_id: u64,
    consumed_ids: &[u64],
    super_intensify: bool,
) -> Result<Vec<u64>, &'static str> {
    if hero_id == 0
        || consumed_ids.is_empty()
        || consumed_ids.len() > 12
        || consumed_ids.iter().any(|id| *id == 0 || *id == hero_id)
        || consumed_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != consumed_ids.len()
    {
        return Err("intensify request is invalid");
    }
    let heroes = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .ok_or("hero bag is unavailable")?
        .clone();
    let target = heroes
        .iter()
        .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
        .ok_or("intensify target does not exist")?;
    if heroes
        .iter()
        .filter(|hero| json_u64(hero, "heroId") == Some(hero_id))
        .count()
        != 1
    {
        return Err("intensify target does not exist");
    }
    let consumed = consumed_ids
        .iter()
        .map(|id| {
            heroes
                .iter()
                .find(|hero| json_u64(hero, "heroId") == Some(*id))
                .ok_or("intensify material is unavailable")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if consumed.iter().any(|material| {
        json_bool(material, "lock")
            || json_i32(material, "level").unwrap_or(1) != 1
            || json_i32(material, "advance").unwrap_or_default() > 1
            || hero_array(material, "intensify").is_some_and(|attrs| !attrs.is_empty())
            || hero_is_in_use(account, json_u64(material, "heroId").unwrap_or_default())
    }) {
        return Err("intensify material is unavailable");
    }
    let template_id = json_i32(target, "templateId").unwrap_or_default();
    let (target_type, need_rows) = catalog
        .need_power_by_template
        .get(&template_id)
        .ok_or("intensify target config is missing")?;
    let max_rows = catalog
        .max_power_by_template
        .get(&template_id)
        .ok_or("intensify target config is missing")?;
    let diamond_cost = if super_intensify {
        catalog
            .diamond_cost_per_hero
            .checked_mul(i64::try_from(consumed_ids.len()).unwrap_or(i64::MAX))
            .ok_or("intensify diamond cost overflow")?
    } else {
        0
    };
    if character_i64(account, "diamond") < diamond_cost {
        return Err("insufficient diamonds");
    }
    let mut added_exp = std::collections::BTreeMap::<i32, i64>::new();
    for material in &consumed {
        let material_template = json_i32(material, "templateId").unwrap_or_default();
        let (material_type, _) = catalog
            .need_power_by_template
            .get(&material_template)
            .ok_or("intensify material config is missing")?;
        let provides = catalog
            .provide_power_by_template
            .get(&material_template)
            .ok_or("intensify material config is missing")?;
        let ratio = if material_type == target_type {
            catalog.same_type_ratio
        } else {
            10_000
        };
        for &(attr_type, amount) in provides {
            let mut amount = amount.saturating_mul(ratio) / 10_000;
            if super_intensify {
                amount = amount.saturating_mul(2);
            }
            let entry = added_exp.entry(attr_type).or_default();
            *entry = entry.saturating_add(amount);
        }
    }
    let mut attrs = std::collections::BTreeMap::<i32, (i64, i64)>::new();
    for attr in hero_array(target, "intensify").into_iter().flatten() {
        let attr_type = json_i32(attr, "attrType").unwrap_or_default();
        if attr_type > 0 {
            attrs.insert(
                attr_type,
                (
                    i64::from(json_i32(attr, "intensifyLvl").unwrap_or_default().max(0)),
                    i64::from(json_i32(attr, "curExp").unwrap_or_default().max(0)),
                ),
            );
        }
    }
    let max_by_attr = max_rows
        .iter()
        .copied()
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut gained = false;
    for &(attr_type, need_exp) in need_rows {
        if need_exp <= 0 {
            continue;
        }
        let Some(&max_level) = max_by_attr.get(&attr_type) else {
            continue;
        };
        let Some(&add) = added_exp.get(&attr_type) else {
            continue;
        };
        if max_level <= 0 || add <= 0 {
            continue;
        }
        let (level, cur_exp) = attrs.get(&attr_type).copied().unwrap_or_default();
        let current_total = level
            .max(0)
            .saturating_mul(need_exp)
            .saturating_add(cur_exp.max(0));
        let cap = max_level.saturating_mul(need_exp);
        if current_total >= cap {
            continue;
        }
        let next_total = current_total.saturating_add(add).min(cap);
        attrs.insert(attr_type, (next_total / need_exp, next_total % need_exp));
        gained |= next_total > current_total;
    }
    if !gained {
        return Err("all intensify attributes are capped");
    }
    let intensify = attrs
        .into_iter()
        .map(|(attr_type, (level, cur_exp))| {
            json!({"attrType": attr_type, "intensifyLvl": level, "curExp": cur_exp})
        })
        .collect::<Vec<_>>();
    let consumed_set = consumed_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let Some(heroes) = account
        .get_mut("dock")
        .and_then(|dock| dock.get_mut("heroes"))
        .and_then(Value::as_array_mut)
    else {
        return Err("hero bag is unavailable");
    };
    heroes.retain(|hero| !consumed_set.contains(&json_u64(hero, "heroId").unwrap_or_default()));
    let Some(target) = heroes
        .iter_mut()
        .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
    else {
        return Err("intensify target disappeared");
    };
    target["intensify"] = Value::Array(intensify);
    if diamond_cost > 0 {
        adjust_character_i64(account, "diamond", -diamond_cost);
    }
    if let Some(items) = account
        .get_mut("equip")
        .and_then(|equip| equip.get_mut("items"))
        .and_then(Value::as_array_mut)
    {
        for item in items {
            if json_u64(item, "heroId").is_some_and(|id| consumed_set.contains(&id)) {
                item["heroId"] = json!(0);
            }
        }
    }
    Ok(consumed_ids.to_vec())
}

pub(super) fn hero_change_equip_state(
    account: &mut Value,
    hero_id: u64,
    slot: u64,
    equip_id: u64,
) -> Result<(), &'static str> {
    hero_change_equip_state_for_type(account, hero_id, slot, equip_id, 1)
}

pub(super) fn hero_change_equip_state_for_type(
    account: &mut Value,
    hero_id: u64,
    slot: u64,
    equip_id: u64,
    equip_type: u64,
) -> Result<(), &'static str> {
    if hero_id == 0 || !(1..=6).contains(&slot) {
        return Err("equipment slot is invalid");
    }
    if equip_type == 0 {
        return Err("equipment fleet type is invalid");
    }
    let heroes = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .ok_or("hero bag is unavailable")?;
    if heroes
        .iter()
        .filter(|hero| json_u64(hero, "heroId") == Some(hero_id))
        .count()
        != 1
    {
        return Err("hero does not exist");
    }
    let old_equip_id = heroes
        .iter()
        .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
        .and_then(|hero| hero_slot_ids(hero, equip_type))
        .and_then(Value::as_array)
        .and_then(|slots| slots.get(slot as usize - 1))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let old_state = heroes
        .iter()
        .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
        .and_then(|hero| hero_slot_states(hero, equip_type))
        .and_then(Value::as_array)
        .and_then(|states| states.get(slot as usize - 1))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    if old_state == 1 && old_equip_id != equip_id {
        return Err("locked equipment cannot be replaced");
    }

    if equip_id > 0 {
        let equip_items = account
            .get("equip")
            .and_then(|equip| equip.get("items"))
            .and_then(Value::as_array)
            .ok_or("equipment bag is unavailable")?;
        let matches = equip_items
            .iter()
            .filter(|item| json_u64(item, "equipId") == Some(equip_id))
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err("equipment does not exist");
        }
        let owner = json_u64(matches[0], "heroId").unwrap_or_default();
        if equip_type == 1 && owner != 0 && owner != hero_id {
            return Err("equipment belongs to another hero");
        }
    }

    if let Some(hero) = find_hero_mut(account, hero_id) {
        let slots = hero_slots_mut(hero, equip_type);
        let slots = slots.as_array_mut().ok_or("equipment slots are invalid")?;
        while slots.len() < 6 {
            slots.push(json!(0));
        }
        for value in slots.iter_mut() {
            if value.as_u64() == Some(equip_id) && equip_id != old_equip_id {
                *value = json!(0);
            }
        }
        slots[slot as usize - 1] = json!(equip_id);
        if old_equip_id != equip_id {
            let states = hero_states_mut(hero, equip_type);
            let states = states
                .as_array_mut()
                .ok_or("equipment states are invalid")?;
            while states.len() < 6 {
                states.push(json!(0));
            }
            states[slot as usize - 1] = json!(0);
        }
    } else {
        return Err("hero does not exist");
    }
    if equip_type == 1 && old_equip_id != equip_id {
        set_equip_hero_id(account, old_equip_id, 0);
        set_equip_hero_id(account, equip_id, hero_id);
    }
    Ok(())
}

fn hero_slot_ids(hero: &Value, equip_type: u64) -> Option<&Value> {
    if equip_type == 1 {
        Some(hero.get("equipSlots")?)
    } else {
        hero.get("equipSlotsByType")?.get(equip_type.to_string())
    }
}

fn hero_slot_states(hero: &Value, equip_type: u64) -> Option<&Value> {
    if equip_type == 1 {
        hero.get("equipStates")
    } else {
        hero.get("equipStatesByType")?.get(equip_type.to_string())
    }
}

fn hero_slots_mut(hero: &mut serde_json::Map<String, Value>, equip_type: u64) -> &mut Value {
    if equip_type == 1 {
        hero.entry("equipSlots".to_owned())
            .or_insert_with(|| json!([0, 0, 0, 0, 0, 0]))
    } else {
        hero.entry("equipSlotsByType".to_owned())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("equipment slots by type must be an object")
            .entry(equip_type.to_string())
            .or_insert_with(|| json!([0, 0, 0, 0, 0, 0]))
    }
}

fn hero_states_mut(hero: &mut serde_json::Map<String, Value>, equip_type: u64) -> &mut Value {
    if equip_type == 1 {
        hero.entry("equipStates".to_owned())
            .or_insert_with(|| json!([0, 0, 0, 0, 0, 0]))
    } else {
        hero.entry("equipStatesByType".to_owned())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("equipment states by type must be an object")
            .entry(equip_type.to_string())
            .or_insert_with(|| json!([0, 0, 0, 0, 0, 0]))
    }
}

pub(super) fn hero_auto_equip_state(
    account: &mut Value,
    equip_type: u64,
    units: &[(u64, Vec<(u64, u64)>)],
) -> Result<(), &'static str> {
    if units.is_empty() {
        return Err("auto equipment request is empty");
    }
    let mut candidate = account.clone();
    for (hero_id, equips) in units {
        for (slot, equip_id) in equips {
            hero_change_equip_state_for_type(
                &mut candidate,
                *hero_id,
                *slot,
                *equip_id,
                equip_type,
            )?;
        }
    }
    *account = candidate;
    Ok(())
}

pub(super) fn hero_auto_unequip_state(
    account: &mut Value,
    equip_type: u64,
    hero_ids: &[u64],
) -> Result<(), &'static str> {
    if hero_ids.is_empty() {
        return Err("auto unequipment request is empty");
    }
    let mut candidate = account.clone();
    for hero_id in hero_ids {
        for slot in 1u64..=6 {
            let old_equip_id = candidate
                .get("dock")
                .and_then(|dock| dock.get("heroes"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|hero| json_u64(hero, "heroId") == Some(*hero_id))
                .and_then(|hero| hero_slot_ids(hero, equip_type))
                .and_then(Value::as_array)
                .and_then(|slots| slots.get(usize::try_from(slot - 1).unwrap_or_default()))
                .and_then(Value::as_u64)
                .unwrap_or_default();
            hero_change_equip_state_for_type(&mut candidate, *hero_id, slot, 0, equip_type)?;
            if equip_type == 1 {
                set_equip_hero_id(&mut candidate, old_equip_id, 0);
            }
        }
    }
    *account = candidate;
    Ok(())
}

pub(super) fn hero_equip_binding_state(
    account: &mut Value,
    hero_id: u64,
    equip_id: u64,
    equip_type: u64,
) -> Result<(), &'static str> {
    if equip_type != 1 || hero_id == 0 || equip_id == 0 {
        return Err("equipment binding request is invalid");
    }
    let hero = find_hero_mut(account, hero_id).ok_or("hero does not exist")?;
    let slot = hero
        .get("equipSlots")
        .and_then(Value::as_array)
        .and_then(|slots| {
            slots
                .iter()
                .position(|value| value.as_u64() == Some(equip_id))
        })
        .ok_or("equipment is not equipped by hero")?;
    let states = hero_states_mut(hero, equip_type)
        .as_array_mut()
        .ok_or("equipment states are invalid")?;
    while states.len() < 6 {
        states.push(json!(0));
    }
    states[slot] = json!(1);
    Ok(())
}

pub(super) fn hero_equip_unbinding_state(
    account: &mut Value,
    hero_id: u64,
    equip_id: u64,
    equip_type: u64,
) -> Result<Vec<ShopReward>, &'static str> {
    if equip_type != 1 || hero_id == 0 || equip_id == 0 {
        return Err("equipment unbinding request is invalid");
    }
    let hero = find_hero_mut(account, hero_id).ok_or("hero does not exist")?;
    let slot = hero
        .get("equipSlots")
        .and_then(Value::as_array)
        .and_then(|slots| {
            slots
                .iter()
                .position(|value| value.as_u64() == Some(equip_id))
        })
        .ok_or("equipment is not equipped by hero")?;
    let states = hero_states_mut(hero, equip_type)
        .as_array_mut()
        .ok_or("equipment states are invalid")?;
    while states.len() < 6 {
        states.push(json!(0));
    }
    if states[slot].as_i64().unwrap_or_default() != 1 {
        return Err("equipment is not bound");
    }
    states[slot] = json!(0);
    Ok(Vec::new())
}

pub(super) fn hero_equip_effect_state(
    account: &mut Value,
    hero_id: u64,
    effects: &[(i32, Vec<i32>)],
) -> Result<(), &'static str> {
    let hero = find_hero_mut(account, hero_id).ok_or("hero does not exist")?;
    hero.insert(
        "equipEffects".to_owned(),
        json!(effects
            .iter()
            .filter(|(effect_type, effect_ids)| *effect_type > 0 && !effect_ids.is_empty())
            .map(|(effect_type, effect_ids)| json!({"type": effect_type, "effectIds": effect_ids}))
            .collect::<Vec<_>>()),
    );
    Ok(())
}

pub(super) fn hero_equip_lock_transplant_state(
    account: &mut Value,
    hero_ids: &[u64],
    equip_type: u64,
) -> Result<(), &'static str> {
    if !matches!(equip_type, 2 | 3) || hero_ids.is_empty() {
        return Err("equipment transplant request is invalid");
    }
    let mut candidate = account.clone();
    for hero_id in hero_ids {
        let (normal_ids, normal_states) = candidate
            .get("dock")
            .and_then(|dock| dock.get("heroes"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|hero| json_u64(hero, "heroId") == Some(*hero_id))
            .map(|hero| {
                (
                    hero.get("equipSlots")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                    hero.get("equipStates")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                )
            })
            .ok_or("hero does not exist")?;
        let ids = (0..6)
            .map(|index| {
                if normal_states
                    .get(index)
                    .and_then(Value::as_i64)
                    .unwrap_or_default()
                    == 1
                {
                    normal_ids.get(index).cloned().unwrap_or_else(|| json!(0))
                } else {
                    json!(0)
                }
            })
            .collect::<Vec<_>>();
        let states = ids
            .iter()
            .map(|id| json!(i32::from(id.as_u64().unwrap_or_default() > 0)))
            .collect::<Vec<_>>();
        let hero = find_hero_mut(&mut candidate, *hero_id).ok_or("hero does not exist")?;
        hero.entry("equipSlotsByType".to_owned())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("equipment slots by type are invalid")?
            .insert(equip_type.to_string(), json!(ids));
        hero.entry("equipStatesByType".to_owned())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("equipment states by type are invalid")?
            .insert(equip_type.to_string(), json!(states));
    }
    *account = candidate;
    Ok(())
}

pub(super) fn hero_advance_max_level_state(
    account: &mut Value,
    catalog: &ShipAdvanceCatalog,
    hero_id: u64,
) -> Result<(), &'static str> {
    let heroes = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .ok_or("hero bag is unavailable")?;
    let target = heroes
        .iter()
        .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
        .ok_or("hero does not exist")?;
    if heroes
        .iter()
        .filter(|hero| json_u64(hero, "heroId") == Some(hero_id))
        .count()
        != 1
    {
        return Err("hero does not exist");
    }
    let next_level = json_i32(target, "advLv")
        .unwrap_or_default()
        .checked_add(1)
        .ok_or("max-level breakthrough is capped")?;
    if !catalog.by_level.is_empty() {
        let config = catalog
            .by_level
            .get(&next_level)
            .ok_or("max-level breakthrough config is missing")?;
        let initial_level = config.initial_level;
        let max_level = config.max_level;
        if initial_level <= 0 || max_level <= initial_level {
            return Err("max-level breakthrough config is invalid");
        }
        if json_i32(target, "level").unwrap_or_default() < initial_level {
            return Err("hero level is too low for max-level breakthrough");
        }
    }
    let target = find_hero_mut(account, hero_id).ok_or("hero does not exist")?;
    target.insert("advLv".to_owned(), json!(next_level));
    Ok(())
}

pub(super) fn hero_advance_mub_state(
    account: &mut Value,
    catalog: &ShipBreakCatalog,
    hero_id: u64,
    consume_items: &[(i32, i32)],
) -> Result<(), &'static str> {
    let heroes = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .ok_or("hero bag is unavailable")?;
    let hero = heroes
        .iter()
        .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
        .cloned()
        .ok_or("advance target does not exist")?;
    if heroes
        .iter()
        .filter(|value| json_u64(value, "heroId") == Some(hero_id))
        .count()
        != 1
    {
        return Err("advance target does not exist");
    }
    let template_id = json_i32(&hero, "templateId").unwrap_or_default();
    let config = catalog
        .by_template
        .get(&template_id)
        .ok_or("advance config is missing")?;
    if json_i32(&hero, "level").unwrap_or_default() < config.min_level {
        return Err("hero level is too low for advance");
    }
    let break_to = config.break_to;
    if break_to <= 0 {
        return Err("advance target has no next template");
    }
    let (fragment_id, required_count) = config
        .break_item_mub
        .ok_or("advance fragment config is missing")?;
    if fragment_id <= 0 || required_count <= 0 {
        return Err("advance fragment config is invalid");
    }
    let allowed = config
        .break_usableitem_mub
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let conversion = |item_id: i32| match item_id {
        // Client config parameter 507/508 defaults used by MubConversionLoader.
        18051 => 1,
        17512 => 30,
        _ => 1,
    };
    let mut effective = 0i64;
    for (item_id, amount) in consume_items {
        if *amount <= 0 || (*item_id != fragment_id && !allowed.contains(item_id)) {
            return Err("advance fragment is invalid");
        }
        if bag_item_count(account, *item_id) < i64::from(*amount) {
            return Err("advance fragment is unavailable");
        }
        effective = effective
            .checked_add(i64::from(*amount) * i64::from(conversion(*item_id)))
            .ok_or("advance fragment count is too large")?;
    }
    if effective != i64::from(required_count) {
        return Err("advance fragment count is invalid");
    }
    let (currency_type, currency_id, currency_cost) = config
        .currency_cost
        .ok_or("advance currency config is invalid")?;
    if currency_type != 5 || currency_cost < 0 {
        return Err("advance currency config is invalid");
    }
    let currency_key =
        currency_character_key(currency_id).ok_or("advance currency is unsupported")?;
    if character_i64(account, currency_key) < currency_cost {
        return Err("insufficient advance currency");
    }
    for (item_id, amount) in consume_items {
        consume_bag_item(account, *item_id, *amount);
    }
    adjust_character_i64(account, currency_key, -currency_cost);
    let target = find_hero_mut(account, hero_id).ok_or("advance target does not exist")?;
    target["advance"] = json!(json_i32(&hero, "advance")
        .unwrap_or_default()
        .saturating_add(1));
    target["templateId"] = json!(break_to);
    Ok(())
}

pub(super) fn hero_remould_state(
    account: &mut Value,
    catalog: &ShipRemouldCatalog,
    hero_id: u64,
    effect_id: i32,
) -> Result<(), &'static str> {
    if hero_id == 0 || effect_id <= 0 {
        return Err("remould request is invalid");
    }
    let heroes = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .ok_or("hero bag is unavailable")?;
    let hero = heroes
        .iter()
        .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
        .cloned()
        .ok_or("hero was not found")?;
    let template_id = json_i32(&hero, "templateId").unwrap_or_default();
    let sf_id = template_id.saturating_sub(1) / 10;
    let ship_info = catalog
        .ship_info_by_sf_id
        .get(&sf_id)
        .ok_or("this hero cannot be remoulded")?;
    let stage_ids = &ship_info.remould_template;
    if stage_ids.is_empty() {
        return Err("this hero cannot be remoulded");
    }
    let effect = catalog
        .effects
        .get(&effect_id)
        .ok_or("remould effect was not found")?;
    let mut effect_stage = None;
    for (index, stage_id) in stage_ids.iter().enumerate() {
        if catalog
            .templates
            .get(stage_id)
            .map(|stage| stage.remould_item_group.contains(&effect_id))
            .unwrap_or(false)
        {
            effect_stage = Some(index);
            break;
        }
    }
    let effect_stage = effect_stage.ok_or("remould effect does not belong to this hero")?;
    let mut completed = json_i32_array(&hero, "remouldEffects")
        .into_iter()
        .filter(|id| *id > 0)
        .collect::<std::collections::BTreeSet<_>>();
    if !completed.insert(effect_id) {
        return Err("remould effect is already active");
    }
    let mut current_stage = 0;
    for stage_id in stage_ids {
        let group = catalog
            .templates
            .get(stage_id)
            .map(|stage| stage.remould_item_group.clone())
            .unwrap_or_default();
        if group.iter().any(|id| !completed.contains(id)) {
            break;
        }
        current_stage += 1;
    }
    // completed currently includes requested node. Compare against state before request:
    // requested node must be in first not-yet-complete stage.
    let mut before = completed.clone();
    before.remove(&effect_id);
    let mut expected_stage = 0;
    for stage_id in stage_ids {
        let group = catalog
            .templates
            .get(stage_id)
            .map(|stage| stage.remould_item_group.clone())
            .unwrap_or_default();
        if group.iter().any(|id| !before.contains(id)) {
            break;
        }
        expected_stage += 1;
    }
    if effect_stage != expected_stage {
        return Err("remould effect is not in the current stage");
    }
    let prerequisites = &effect.remould_prev;
    if prerequisites.iter().any(|id| *id > 0) && !prerequisites.iter().any(|id| before.contains(id))
    {
        return Err("remould prerequisite is not complete");
    }
    if json_i32(&hero, "level").unwrap_or_default() < effect.limit_level {
        return Err("hero level is too low for this remould effect");
    }
    if json_i32(&hero, "advance").unwrap_or_default() < effect.limit_star {
        return Err("hero advance level is too low for this remould effect");
    }

    let mut costs = std::collections::BTreeMap::<(i32, i32), i64>::new();
    for (goods_type, item_id, amount) in &effect.costs {
        if *amount <= 0 {
            return Err("remould cost configuration is invalid");
        }
        if *item_id <= 0 || matches!(*goods_type, 2 | 3) {
            return Err("unsupported remould cost type");
        }
        let entry = costs.entry((*goods_type, *item_id)).or_default();
        *entry = entry
            .checked_add(*amount)
            .ok_or("remould cost is too large")?;
    }
    for ((goods_type, item_id), amount) in &costs {
        if *goods_type == 5 {
            let key = currency_character_key(*item_id).ok_or("remould currency is unsupported")?;
            if character_i64(account, key) < *amount {
                return Err("not enough currency for remoulding");
            }
        } else if matches!(*goods_type, 1 | 6) {
            if bag_item_count(account, *item_id) < *amount {
                return Err("not enough items for remoulding");
            }
        } else {
            return Err("unsupported remould cost type");
        }
    }
    for ((goods_type, item_id), amount) in costs {
        let amount = i32::try_from(amount).map_err(|_| "remould cost is too large")?;
        consume_resource(account, goods_type, item_id, amount);
    }

    let mut skills = hero
        .get("pSkills")
        .or_else(|| hero.get("pskills"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for values in &effect.remould_effect_type {
        let kind = values.first().copied().unwrap_or_default() as i64;
        let first = values.get(1).copied().unwrap_or_default() as i64;
        if kind == 4 && first > 0 {
            if !skills.iter().any(|skill| {
                json_i32(skill, "pSkillId").or_else(|| json_i32(skill, "pskillId"))
                    == i32::try_from(first).ok()
            }) {
                skills.push(json!({"pSkillId": first, "level": 1, "pSkillExp": 0, "replace": 0}));
            }
        } else if kind == 5 && values.len() >= 3 && first > 0 {
            let replacement = values[2];
            for skill in &mut skills {
                if json_i32(skill, "pSkillId").or_else(|| json_i32(skill, "pskillId"))
                    == i32::try_from(first).ok()
                {
                    skill["replace"] = json!(replacement);
                }
            }
        }
    }
    let target = find_hero_mut(account, hero_id).ok_or("hero was not found")?;
    target["remouldEffects"] = json!(completed.into_iter().collect::<Vec<_>>());
    target["remouldLevel"] = json!(current_stage);
    target["pSkills"] = Value::Array(skills);
    Ok(())
}

#[allow(dead_code)]
pub(super) fn fashion_equip_state(
    account: &mut Value,
    catalog: Option<&FashionList>,
    hero_id: u64,
    fashion_tid: i32,
    equip_status: i32,
) -> Result<(), &'static str> {
    let hero = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .and_then(|heroes| {
            heroes
                .iter()
                .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
        })
        .ok_or("hero was not found")?;
    let template_id = json_i32(hero, "templateId").unwrap_or_default();
    let sf_id = template_id.saturating_sub(1) / 10;
    let selected = if fashion_tid > 0 {
        fashion_tid
    } else if equip_status == 0 {
        sf_id
    } else {
        return Err("fashion is invalid");
    };
    if let Some(catalog) = catalog {
        let belongs = catalog
            .items
            .iter()
            .find(|item| item.sf_id == sf_id)
            .is_some_and(|item| item.fashion_tids.contains(&selected) || selected == sf_id);
        if !belongs {
            return Err("fashion is not for this hero");
        }
        let owned = selected == sf_id
            || account
                .get("fashion")
                .and_then(|fashion| fashion.get("entries"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|entry| {
                    json_i32(entry, "sfId") == Some(sf_id)
                        && entry
                            .get("fashionTids")
                            .and_then(Value::as_array)
                            .is_some_and(|tids| {
                                tids.iter()
                                    .any(|tid| tid.as_i64() == Some(i64::from(selected)))
                            })
                });
        if !owned {
            return Err("fashion is not owned by hero");
        }
    }
    find_hero_mut(account, hero_id)
        .ok_or("hero was not found")?
        .insert("fashioning".to_owned(), json!(selected));
    Ok(())
}

pub(super) fn hero_advance_state(
    account: &mut Value,
    catalog: &ShipBreakCatalog,
    hero_id: u64,
    consumed_ids: &[u64],
    consume_item_ids: &[i32],
) -> Result<(u64, Vec<u64>), &'static str> {
    let heroes = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .ok_or("hero bag is unavailable")?
        .clone();
    let target = heroes
        .iter()
        .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
        .ok_or("advance target does not exist")?;
    if heroes
        .iter()
        .filter(|hero| json_u64(hero, "heroId") == Some(hero_id))
        .count()
        != 1
    {
        return Err("advance target does not exist");
    }
    let template_id = json_i32(target, "templateId").unwrap_or_default();
    let config = catalog
        .by_template
        .get(&template_id)
        .ok_or("advance config is missing")?;
    if json_i32(target, "level").unwrap_or_default() < config.min_level {
        return Err("hero level is too low for advance");
    }
    let break_to = config.break_to;
    if break_to <= 0 {
        return Err("advance target has no next template");
    }
    let allowed_templates = config
        .break_item
        .as_ref()
        .map(|(templates, _)| templates.iter().copied())
        .into_iter()
        .flatten()
        .filter(|value| *value > 0)
        .collect::<std::collections::HashSet<_>>();
    let required_hero_count = config
        .break_item
        .as_ref()
        .map(|(_, count)| *count)
        .unwrap_or(if config.break_item_optional_count > 0 {
            1
        } else {
            0
        });
    if consumed_ids.len() != required_hero_count
        || consumed_ids.iter().any(|id| *id == 0 || *id == hero_id)
        || consumed_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != consumed_ids.len()
    {
        return Err("advance material count is invalid");
    }
    let consumed = consumed_ids
        .iter()
        .map(|id| {
            heroes
                .iter()
                .find(|hero| json_u64(hero, "heroId") == Some(*id))
                .ok_or("advance material does not exist")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if consumed.iter().any(|material| {
        json_bool(material, "lock")
            || hero_is_in_use(account, json_u64(material, "heroId").unwrap_or_default())
            || (!allowed_templates.is_empty()
                && !allowed_templates
                    .contains(&json_i32(material, "templateId").unwrap_or_default()))
    }) {
        return Err("advance material is invalid");
    }
    let item_requirement = config
        .break_item_mub
        .map(|(item_id, count)| (item_id, usize::try_from(count).unwrap_or_default()));
    if let Some((required_item, required_count)) = item_requirement {
        if consume_item_ids.len() != required_count
            || consume_item_ids.iter().any(|id| *id != required_item)
        {
            return Err("advance item count is invalid");
        }
        if consume_item_ids
            .iter()
            .any(|item_id| bag_item_count(account, *item_id) < 1)
        {
            return Err("advance item is unavailable");
        }
    } else if !consume_item_ids.is_empty() {
        return Err("advance does not accept items");
    }
    let (currency_type, currency_id, currency_cost) = config
        .currency_cost
        .ok_or("advance currency config is missing")?;
    if currency_type != 5 {
        return Err("advance currency config is invalid");
    }
    let currency_cost = currency_cost.max(0);
    let Some(currency_key) = currency_character_key(currency_id) else {
        return Err("advance currency is unsupported");
    };
    if character_i64(account, currency_key) < currency_cost {
        return Err("insufficient advance currency");
    }
    let consumed_set = consumed_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let Some(heroes) = account
        .get_mut("dock")
        .and_then(|dock| dock.get_mut("heroes"))
        .and_then(Value::as_array_mut)
    else {
        return Err("hero bag is unavailable");
    };
    heroes.retain(|hero| !consumed_set.contains(&json_u64(hero, "heroId").unwrap_or_default()));
    let Some(target) = heroes
        .iter_mut()
        .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
    else {
        return Err("advance target disappeared");
    };
    target["advance"] = json!(json_i32(target, "advance")
        .unwrap_or_default()
        .saturating_add(1));
    target["templateId"] = json!(break_to);
    for item_id in consume_item_ids {
        consume_bag_item(account, *item_id, 1);
    }
    adjust_character_i64(account, currency_key, -currency_cost);
    if let Some(items) = account
        .get_mut("equip")
        .and_then(|equip| equip.get_mut("items"))
        .and_then(Value::as_array_mut)
    {
        for item in items {
            if json_u64(item, "heroId").is_some_and(|id| consumed_set.contains(&id)) {
                item["heroId"] = json!(0);
            }
        }
    }
    Ok((hero_id, consumed_ids.to_vec()))
}

pub(super) fn consume_hero_skill_upgrade_materials(
    account: &mut Value,
    catalog: &HeroSkillUpgradeCatalog,
    skill_id: i32,
    current_level: i32,
) -> bool {
    let Some(level_costs) = catalog.costs_by_skill.get(&skill_id) else {
        // Level-1 display-only skills (for example 4001/4004) have no upgrade
        // material row. Preserve compatibility and let the state route handle them.
        return true;
    };
    let Some(costs) = current_level
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| level_costs.get(index))
    else {
        return false;
    };
    if costs.iter().any(|(goods_type, item_id, amount)| {
        !resource_available(account, *goods_type, *item_id, *amount)
    }) {
        return false;
    }
    for (goods_type, item_id, amount) in costs {
        consume_resource(account, *goods_type, *item_id, *amount);
    }
    true
}

pub(super) fn retire_heroes_state(
    account: &mut Value,
    hero_ids: &[u64],
    is_dis_equip: bool,
) -> Vec<u64> {
    if hero_ids.is_empty() {
        return Vec::new();
    }
    let requested = hero_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let Some(root) = account.as_object_mut() else {
        return Vec::new();
    };
    let (retired, remaining_secretary) = {
        let Some(dock) = root.get_mut("dock").and_then(Value::as_object_mut) else {
            return Vec::new();
        };
        let Some(heroes) = dock.get_mut("heroes").and_then(Value::as_array_mut) else {
            return Vec::new();
        };
        let retired = heroes
            .iter()
            .filter_map(|hero| json_u64(hero, "heroId"))
            .filter(|id| requested.contains(id))
            .collect::<Vec<_>>();
        if retired.is_empty() {
            return Vec::new();
        }
        let retired_set = retired
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        heroes.retain(|hero| {
            json_u64(hero, "heroId")
                .map(|id| !retired_set.contains(&id))
                .unwrap_or(true)
        });
        let remaining = heroes
            .first()
            .and_then(|hero| json_u64(hero, "heroId"))
            .unwrap_or_default();
        (retired, remaining)
    };
    if retired.is_empty() {
        return Vec::new();
    }
    let retired_set = retired
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    if let Some(items) = root
        .get_mut("equip")
        .and_then(|equip| equip.get_mut("items"))
        .and_then(Value::as_array_mut)
    {
        if is_dis_equip {
            items.retain(|item| {
                json_u64(item, "heroId")
                    .map(|id| !retired_set.contains(&id))
                    .unwrap_or(true)
            });
        } else {
            for item in items {
                if json_u64(item, "heroId").is_some_and(|id| retired_set.contains(&id)) {
                    item["heroId"] = json!(0);
                }
            }
        }
    }
    if let Some(tactics) = root
        .get_mut("fleet")
        .and_then(|fleet| fleet.get_mut("tactics"))
        .and_then(Value::as_array_mut)
    {
        for tactic in tactics {
            for key in ["heroInfo", "exHeroInfo"] {
                if let Some(ids) = tactic.get_mut(key).and_then(Value::as_array_mut) {
                    ids.retain(|id| {
                        id.as_u64()
                            .map(|id| !retired_set.contains(&id))
                            .unwrap_or(true)
                    });
                }
            }
        }
    }
    if let Some(hero_list) = root
        .get_mut("bath")
        .and_then(|bath| bath.get_mut("heroList"))
        .and_then(Value::as_array_mut)
    {
        hero_list.retain(|hero| {
            json_u64(hero, "heroId")
                .map(|id| !retired_set.contains(&id))
                .unwrap_or(true)
        });
    }
    if let Some(character) = root.get_mut("character").and_then(Value::as_object_mut) {
        if character
            .get("secretaryId")
            .and_then(Value::as_u64)
            .is_some_and(|id| retired_set.contains(&id))
        {
            character.insert("secretaryId".to_owned(), json!(remaining_secretary));
        }
    }
    retired
}

pub(super) fn encode_hero_delete_payload(account: &Value, hero_ids: &[u64]) -> Vec<u8> {
    let bag_size = account
        .get("dock")
        .and_then(|dock| json_i32(dock, "bagSize"))
        .unwrap_or_default();
    let heroes = hero_ids
        .iter()
        .filter_map(|hero_id| u32::try_from(*hero_id).ok())
        .map(|hero_id| HeroGrid {
            hero_id,
            template_id: 0,
            ..HeroGrid::default()
        })
        .collect::<Vec<_>>();
    HeroBagCodec::encode(&HeroBag { heroes, bag_size })
}

pub(super) fn encode_hero_intensify_payload(
    account: &Value,
    target_id: u64,
    consumed_ids: &[u64],
) -> Vec<u8> {
    let bag = hero_bag_from_account(account);
    let mut heroes = bag
        .heroes
        .into_iter()
        .filter(|hero| u64::from(hero.hero_id) == target_id)
        .collect::<Vec<_>>();
    heroes.extend(consumed_ids.iter().filter_map(|hero_id| {
        u32::try_from(*hero_id).ok().map(|hero_id| HeroGrid {
            hero_id,
            template_id: 0,
            ..HeroGrid::default()
        })
    }));
    HeroBagCodec::encode(&HeroBag {
        heroes,
        bag_size: bag.bag_size,
    })
}

pub(super) fn apply_hero_breakdown_rewards(
    account: &mut Value,
    retired_templates: &[i32],
    catalog: Option<&HeroBreakdownCatalog>,
) -> Vec<ShopReward> {
    let mut totals = std::collections::BTreeMap::<(i32, i32), i32>::new();
    for template_id in retired_templates {
        for &(goods_type, config_id, num) in catalog
            .and_then(|catalog| catalog.rewards_by_template.get(template_id))
            .into_iter()
            .flatten()
        {
            let total = totals.entry((goods_type, config_id)).or_default();
            *total = total.saturating_add(num);
        }
    }
    let mut rewards = Vec::new();
    for ((goods_type, config_id), num) in totals {
        if goods_type == 5 {
            if let Some(key) = currency_character_key(config_id) {
                add_character_i64(account, key, num);
            }
        } else {
            add_bag_item(account, config_id, num);
        }
        rewards.push(ShopReward {
            goods_type,
            item_id: config_id,
            num,
            instance_id: 0,
        });
    }
    rewards
}

pub(super) fn encode_retire_hero_response(rewards: &[ShopReward]) -> Vec<u8> {
    let mut output = Vec::new();
    for reward in rewards {
        let mut nested = Vec::new();
        append_varint_field(&mut nested, 1, reward.goods_type.max(0) as u64);
        append_varint_field(&mut nested, 2, reward.item_id.max(0) as u64);
        append_varint_field(&mut nested, 3, reward.num.max(0) as u64);
        append_message_field(&mut output, 1, &nested);
    }
    output
}

pub(super) fn encode_hero_bag_push(account: &Value) -> Vec<u8> {
    TMessageCodec::encode_response(&TResponse {
        method: "hero.UpdateHeroBagData".to_owned(),
        ret: Some(HeroBagCodec::encode(&hero_bag_from_account(account))),
        time: current_unix_seconds(),
        ..TResponse::default()
    })
}
pub(super) fn illustrate_info_payload(
    account: &Value,
    handbook_behaviours: Option<&[i32]>,
    hero_memories: Option<&[(i32, i32)]>,
) -> Vec<u8> {
    let now = current_unix_seconds();
    let mut output = Vec::new();
    if let Some(heroes) = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
    {
        let mut seen = std::collections::HashSet::new();
        for hero in heroes {
            let Some(template_id) = json_i32(hero, "templateId") else {
                continue;
            };
            let illustrate_id = (template_id.saturating_sub(1)) / 10;
            if illustrate_id <= 0 || !seen.insert(illustrate_id) {
                continue;
            }
            let persisted = account
                .get("illustrate")
                .and_then(|illustrate| illustrate.get("entries"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|entry| json_i32(entry, "illustrateId") == Some(illustrate_id))
                .and_then(|entry| entry.get("behaviourList"))
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_i64)
                        .filter_map(|id| i32::try_from(id).ok())
                        .collect::<Vec<_>>()
                });
            if let Some(persisted) = persisted {
                append_illustrate_info_item(&mut output, illustrate_id, now, Some(&persisted));
            } else {
                append_illustrate_info_item(&mut output, illustrate_id, now, handbook_behaviours);
            }
        }
    }
    if let Some(memories) = hero_memories.filter(|items| !items.is_empty()) {
        for (hero_id, plot_id) in memories {
            let mut memory = Vec::new();
            append_varint_field(&mut memory, 1, (*hero_id).max(0) as u64);
            append_varint_field(&mut memory, 2, (*plot_id).max(0) as u64);
            append_message_field(&mut output, 8, &memory);
        }
    }
    // IllustrateEquipList repeated field must be non-nil on client bootstrap.
    append_message_field(&mut output, 9, &[]);
    output
}

pub(super) fn illustrate_info_payload_for_templates(
    template_ids: &[i32],
    handbook_behaviours: Option<&[i32]>,
) -> Vec<u8> {
    let now = current_unix_seconds();
    let mut output = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for template_id in template_ids {
        let illustrate_id = (template_id.saturating_sub(1)) / 10;
        if illustrate_id <= 0 || !seen.insert(illustrate_id) {
            continue;
        }
        append_illustrate_info_item(&mut output, illustrate_id, now, handbook_behaviours);
    }
    // IllustrateEquipList repeated field must be non-nil on client incremental updates too.
    append_message_field(&mut output, 9, &[]);
    output
}

pub(super) fn illustrate_info_payload_for_entries(entries: &[(i32, Vec<i32>)]) -> Vec<u8> {
    let now = current_unix_seconds();
    let mut output = Vec::new();
    for (illustrate_id, behaviours) in entries {
        if *illustrate_id <= 0 {
            continue;
        }
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, *illustrate_id as u64);
        append_varint_field(&mut item, 2, u64::from(now));
        append_varint_field(&mut item, 3, 0);
        for behaviour in behaviours.iter().filter(|id| **id > 0) {
            append_varint_field(&mut item, 5, *behaviour as u64);
        }
        append_varint_field(&mut item, 6, 0);
        append_message_field(&mut output, 1, &item);
    }
    append_message_field(&mut output, 9, &[]);
    output
}

pub(super) fn illustrate_info_payload_for_rewards(
    rewards: &[ShopReward],
    handbook_behaviours: Option<&[i32]>,
) -> Option<Vec<u8>> {
    let template_ids = rewards
        .iter()
        .filter(|reward| reward.goods_type == 3)
        .map(|reward| reward.item_id)
        .collect::<Vec<_>>();
    (!template_ids.is_empty())
        .then(|| illustrate_info_payload_for_templates(&template_ids, handbook_behaviours))
}

fn append_illustrate_info_item(
    output: &mut Vec<u8>,
    illustrate_id: i32,
    now: u32,
    handbook_behaviours: Option<&[i32]>,
) {
    let mut item = Vec::new();
    append_varint_field(&mut item, 1, illustrate_id as u64);
    append_varint_field(&mut item, 2, u64::from(now));
    append_varint_field(&mut item, 3, 0);
    // BehaviourList and MarryCount are always present in the C# payload. Use the
    // client catalog when available so all illustration actions unlock consistently.
    if let Some(behaviours) = handbook_behaviours.filter(|items| !items.is_empty()) {
        for behaviour in behaviours {
            append_varint_field(&mut item, 5, (*behaviour).max(0) as u64);
        }
    } else {
        append_varint_field(&mut item, 5, 0);
    }
    append_varint_field(&mut item, 6, 0);
    append_message_field(output, 1, &item);
}

pub(super) fn story_memory_payload(memories: Option<&[(i32, i32)]>) -> Vec<u8> {
    let mut output = Vec::new();
    if let Some(memories) = memories {
        for (chapter_id, index) in memories {
            let mut memory = Vec::new();
            append_varint_field(&mut memory, 1, (*chapter_id).max(0) as u64);
            append_varint_field(&mut memory, 2, (*index).max(0) as u64);
            append_message_field(&mut output, 1, &memory);
        }
    }
    output
}
