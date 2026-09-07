#![allow(dead_code)]

use serde_json::{json, Value};

use super::*;

pub(super) fn build_project_payload(project: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    if let Some(items) = project.get("items").and_then(Value::as_array) {
        for item in items {
            let mut encoded = Vec::new();
            append_varint_field(
                &mut encoded,
                1,
                json_i64(item, "resId").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut encoded,
                2,
                json_i64(item, "count").unwrap_or_default().max(0) as u64,
            );
            append_message_field(&mut output, 1, &encoded);
        }
    }
    append_varint_field(
        &mut output,
        2,
        json_i64(project, "gold").unwrap_or_default().max(0) as u64,
    );
    output
}

pub(super) fn build_formula_payload(value: &Value, include_hero_id: bool) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        json_i64(value, "endTime").unwrap_or_default().max(0) as u64,
    );
    if let Some(project) = value.get("project") {
        append_message_field(&mut output, 2, &build_project_payload(project));
    }
    if include_hero_id {
        append_varint_field(
            &mut output,
            3,
            json_i64(value, "templateId")
                .or_else(|| json_i64(value, "heroId"))
                .unwrap_or_default()
                .max(0) as u64,
        );
    }
    output
}

pub(super) fn construction_info_payload(account: &Value, now: u32) -> Vec<u8> {
    let Some(construction) = account.get("construction") else {
        return Vec::new();
    };
    let mut output = Vec::new();
    let mut jobs = construction
        .get("jobs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    jobs.sort_by_key(|job| json_i64(job, "sequence").unwrap_or_default());
    loop {
        let due_time = jobs
            .iter()
            .filter(|job| {
                !json_bool(job, "completed")
                    && json_i64(job, "endTime").unwrap_or_default() > 0
                    && json_i64(job, "endTime").unwrap_or_default() <= i64::from(now)
            })
            .filter_map(|job| json_i64(job, "endTime"))
            .min();
        let Some(due_time) = due_time else { break };
        for job in &mut jobs {
            if !json_bool(job, "completed") && json_i64(job, "endTime") == Some(due_time) {
                job["completed"] = Value::Bool(true);
            }
        }
        promote_construction_waiting(&mut jobs, due_time);
    }
    promote_construction_waiting(&mut jobs, i64::from(now));
    let mut grouped = [Vec::new(), Vec::new(), Vec::new()];
    for job in jobs {
        let completed = json_bool(&job, "completed");
        let formula = build_formula_payload(&job, completed);
        let group = if completed {
            &mut grouped[0]
        } else if json_i64(&job, "endTime").unwrap_or_default() > 0 {
            &mut grouped[1]
        } else {
            &mut grouped[2]
        };
        group.push((json_i64(&job, "sequence").unwrap_or_default(), formula));
    }
    for (index, field) in [(0usize, 1u8), (1, 2), (2, 3)] {
        let group = &mut grouped[index];
        group.sort_by_key(|(sequence, _)| *sequence);
        for (_, formula) in group {
            append_message_field(&mut output, field, formula);
        }
    }
    if let Some(last_project) = construction
        .get("lastProject")
        .filter(|value| !value.is_null())
    {
        let mut last = Vec::new();
        append_varint_field(&mut last, 1, 0);
        append_message_field(&mut last, 2, &build_project_payload(last_project));
        append_message_field(&mut output, 4, &last);
    }
    output
}

pub(super) fn promote_construction_waiting(jobs: &mut [Value], transition_time: i64) {
    let active = jobs
        .iter()
        .filter(|job| {
            !json_bool(job, "completed") && json_i64(job, "endTime").unwrap_or_default() > 0
        })
        .count();
    let slots = 2usize.saturating_sub(active);
    if slots == 0 {
        return;
    }
    for (promoted, job) in jobs
        .iter_mut()
        .filter(|job| {
            !json_bool(job, "completed") && json_i64(job, "endTime").unwrap_or_default() == 0
        })
        .enumerate()
    {
        if promoted >= slots {
            break;
        }
        let duration = json_i64(job, "durationSeconds").unwrap_or_default().max(0);
        job["endTime"] = json!(transition_time.saturating_add(duration));
    }
}

pub(super) fn bathroom_end_payload(hero_id: u64, bath_time: i64) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, 0);
    append_varint_field(&mut output, 2, bath_time.max(0) as u64);
    append_varint_field(&mut output, 3, hero_id);
    output
}

pub(super) fn decode_building_assignments(
    request: &BuildingSetHeroListRequest,
) -> Vec<(i32, Vec<i32>)> {
    let building_ids = &request.building_ids;
    let hero_ids = &request.hero_ids;
    let mut assignments = Vec::new();
    let mut cursor = 0;
    for building_id in building_ids {
        let mut assigned = Vec::new();
        while cursor < hero_ids.len() && hero_ids[cursor] != -1 {
            assigned.push(hero_ids[cursor]);
            cursor += 1;
        }
        if cursor >= hero_ids.len() {
            return Vec::new();
        }
        cursor += 1;
        assignments.push((*building_id, assigned));
    }
    if cursor != hero_ids.len() {
        return Vec::new();
    }
    assignments
}

#[cfg(test)]
pub(super) fn update_building_assignments(
    account: &mut Value,
    assignments: &[(i32, Vec<i32>)],
    now: u32,
    catalog: Option<&BuildingCatalog>,
) -> bool {
    if assignments.is_empty() {
        return false;
    }
    let owned = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .map(|heroes| {
            heroes
                .iter()
                .filter_map(|hero| json_i64(hero, "heroId"))
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let mut moving = std::collections::HashSet::new();
    let mut seen_buildings = std::collections::HashSet::new();
    for (building_id, hero_ids) in assignments {
        if *building_id <= 0 || !seen_buildings.insert(*building_id) {
            return false;
        }
        for hero_id in hero_ids {
            if *hero_id <= 0 || !owned.contains(&i64::from(*hero_id)) || !moving.insert(*hero_id) {
                return false;
            }
        }
    }
    let Some(buildings) = account
        .get_mut("building")
        .and_then(|building| building.get_mut("buildings"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    for (building_id, _) in assignments {
        let Some(building) = buildings
            .iter()
            .find(|building| json_i32(building, "id") == Some(*building_id))
        else {
            return false;
        };
        // C# BuildingService rejects assignments while construction is mutating and
        // enforces config_buildinginfo.heronumber capacity.
        let status = json_i32(building, "status").unwrap_or(1);
        if status == 2 || status == 4 {
            return false;
        }
        let tid = json_i32(building, "tid")
            .or_else(|| json_i32(building, "templateId"))
            .unwrap_or_default();
        let level = json_i32(building, "level").unwrap_or(1);
        let hero_count = assignments
            .iter()
            .find(|(id, _)| id == building_id)
            .map(|(_, ids)| ids.len())
            .unwrap_or_default();
        if hero_count > building_capacity(tid, level, catalog) {
            return false;
        }
    }
    for building in buildings {
        let id = json_i32(building, "id").unwrap_or_default();
        let replacement = assignments
            .iter()
            .find(|(building_id, _)| *building_id == id)
            .map(|(_, hero_ids)| hero_ids);
        if let Some(hero_ids) = replacement {
            building["heroIds"] = json!(hero_ids);
            building["lastUpdateTime"] = json!(now);
        } else {
            let hero_ids = building
                .get("heroIds")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            building["heroIds"] = json!(hero_ids
                .into_iter()
                .filter(|hero| hero
                    .as_i64()
                    .is_none_or(|id| !moving.contains(&(id as i32))))
                .collect::<Vec<_>>());
            // C# writes LastUpdateTime for every building in replacement pass,
            // including buildings whose hero list stayed unchanged.
            building["lastUpdateTime"] = json!(now);
        }
    }
    true
}

pub(super) fn building_capacity(
    template_id: i32,
    level: i32,
    catalog: Option<&BuildingCatalog>,
) -> usize {
    // Bundled config_buildinginfo uses five slots for dormitory/production buildings;
    // office templates use level as capacity. Keep fallback deterministic when a
    // deployment omits optional building catalog rows.
    if let Some(capacity) = catalog.and_then(|catalog| catalog.capacities.get(&template_id)) {
        *capacity
    } else if (41..=45).contains(&template_id) {
        5
    } else if (1..=5).contains(&template_id) {
        level.max(1) as usize
    } else {
        0
    }
}

/// Apply one client building placement without relying on optional building config tables.
/// Persisted account remains source of truth; projection supplies protocol defaults.
#[cfg(test)]
pub(super) fn add_building_state(
    account: &mut Value,
    template_id: i32,
    land_index: i32,
    now: u32,
) -> Option<i32> {
    if template_id <= 0 || land_index <= 0 {
        return None;
    }
    let root = account.as_object_mut()?;
    let building = root
        .entry("building".to_owned())
        .or_insert_with(|| json!({"buildings": [], "lands": []}));
    let id = building
        .get("buildings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| json_i32(item, "id"))
        .max()
        .unwrap_or_default()
        .saturating_add(1);
    let land_occupied = building
        .get("lands")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|land| json_i32(land, "index") == Some(land_index));
    if land_occupied {
        return None;
    }
    if building
        .get("buildings")
        .and_then(Value::as_array)
        .is_none()
        || building.get("lands").and_then(Value::as_array).is_none()
    {
        return None;
    }
    {
        let buildings = building.get_mut("buildings")?.as_array_mut()?;
        buildings.push(json!({
            "id": id,
            "tid": template_id,
            "level": 1,
            "heroIds": [],
            "status": 1,
            "lastUpdateTime": now,
            "lastBuildUpdateTime": now
        }));
    }
    {
        let lands = building.get_mut("lands")?.as_array_mut()?;
        lands.push(json!({"index": land_index, "buildingId": id}));
    }
    Some(id)
}

#[cfg(test)]
pub(super) fn change_building_level(account: &mut Value, building_id: i32, delta: i32) -> bool {
    if building_id <= 0 || delta == 0 {
        return false;
    }
    let Some(buildings) = account
        .get_mut("building")
        .and_then(|building| building.get_mut("buildings"))
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
    let level = json_i32(building, "level").unwrap_or(1);
    let next = level.saturating_add(delta);
    if next <= 0 {
        return false;
    }
    building["level"] = json!(next);
    building["status"] = json!(1);
    true
}

#[cfg(test)]
pub(super) fn finish_building_state(account: &mut Value, building_id: i32, now: u32) -> bool {
    let Some(building) = account
        .get_mut("building")
        .and_then(|value| value.get_mut("buildings"))
        .and_then(Value::as_array_mut)
        .and_then(|buildings| {
            buildings
                .iter_mut()
                .find(|building| json_i32(building, "id") == Some(building_id))
        })
    else {
        return false;
    };
    building["status"] = json!(1);
    building["lastUpdateTime"] = json!(now);
    building["lastBuildUpdateTime"] = json!(now);
    true
}

#[cfg(test)]
pub(super) fn set_building_production(
    account: &mut Value,
    building_id: i32,
    recipe_id: i32,
    count: i32,
    now: u32,
) -> bool {
    if building_id <= 0 || recipe_id <= 0 || count <= 0 {
        return false;
    }
    let Some(building) = account
        .get_mut("building")
        .and_then(|value| value.get_mut("buildings"))
        .and_then(Value::as_array_mut)
        .and_then(|buildings| {
            buildings
                .iter_mut()
                .find(|building| json_i32(building, "id") == Some(building_id))
        })
    else {
        return false;
    };
    building["recipeId"] = json!(recipe_id);
    building["itemCount"] = json!(count);
    building["productCount"] = json!(0);
    // BuildingStatus.Working. Idle is 1; using idle here makes client-side
    // ProduceItem refuse to advance the newly started recipe.
    building["status"] = json!(3);
    building["lastUpdateTime"] = json!(now);
    true
}

/// Settle elapsed production and consume only completed output.
///
/// Client has no production-list RPC: it derives claimable entries from each
/// TBuildingInfo and uses ReceiveBuilding/ReceiveResource/ReceiveAll. Keep
/// this operation server-authoritative and idempotent.
#[cfg(test)]
pub(super) fn collect_building_rewards(
    account: &mut Value,
    catalog: Option<&BuildingCatalog>,
    building_id: Option<i32>,
    resource_id: Option<i32>,
    now: u32,
    oil_multiplier: f64,
    gold_multiplier: f64,
) -> Vec<ShopReward> {
    let Some(buildings) = account
        .get("building")
        .and_then(|value| value.get("buildings"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let building_count = buildings.len();
    let mut rewards = Vec::new();

    for index in 0..building_count {
        let Some(snapshot) = account
            .get("building")
            .and_then(|value| value.get("buildings"))
            .and_then(Value::as_array)
            .and_then(|values| values.get(index))
            .cloned()
        else {
            continue;
        };
        let id = json_i32(&snapshot, "id").unwrap_or_default();
        if building_id.is_some() && building_id != Some(id) {
            continue;
        }
        let tid = json_i32(&snapshot, "tid")
            .or_else(|| json_i32(&snapshot, "templateId"))
            .unwrap_or_default();
        let config = catalog.and_then(|value| value.building_configs.get(&tid));
        let building_type = config
            .and_then(|value| json_i32(value, "type"))
            .unwrap_or_default();

        let reward = if matches!(building_type, 3 | 4) {
            collect_resource_reward(
                &snapshot,
                config,
                catalog,
                resource_id,
                now,
                oil_multiplier,
                gold_multiplier,
            )
        } else if building_type == 7 {
            collect_item_reward(&snapshot, config, catalog, now)
        } else {
            None
        };
        let Some(reward) = reward else {
            continue;
        };

        if let Some(building) = account
            .get_mut("building")
            .and_then(|value| value.get_mut("buildings"))
            .and_then(Value::as_array_mut)
            .and_then(|values| values.get_mut(index))
        {
            apply_building_collection_state(
                building,
                config,
                catalog,
                now,
                oil_multiplier,
                gold_multiplier,
            );
        }
        rewards.push(reward);
    }
    rewards
}

#[cfg(test)]
fn collect_resource_reward(
    building: &Value,
    config: Option<&Value>,
    catalog: Option<&BuildingCatalog>,
    resource_id: Option<i32>,
    now: u32,
    oil_multiplier: f64,
    gold_multiplier: f64,
) -> Option<ShopReward> {
    let product_id = config.and_then(|value| json_i32_array(value, "productid").get(1).copied())?;
    if !matches!(product_id, 1 | 5) || resource_id.is_some_and(|id| id != product_id) {
        return None;
    }
    let product_max = config
        .and_then(|value| json_i32(value, "productmax"))
        .unwrap_or_default()
        .max(0);
    let mut product_count = json_i32(building, "productCount")
        .or_else(|| json_i32(building, "product_count"))
        .unwrap_or_default()
        .max(0)
        .min(product_max);
    let status = json_i32(building, "status").unwrap_or(1);
    let productivity = config
        .and_then(|value| json_i32(value, "productivity"))
        .unwrap_or_default()
        .max(0);
    let last_update = json_i64(building, "lastUpdateTime")
        .or_else(|| json_i64(building, "last_update_time"))
        .unwrap_or(i64::from(now));
    if status == 3 && productivity > 0 && product_max > product_count {
        let delta = i64::from(now)
            .saturating_sub(last_update)
            .saturating_sub(1)
            .max(0);
        let parameter_id = if product_id == 5 { 209 } else { 210 };
        let period = catalog
            .and_then(|value| value.resource_time_seconds.get(&parameter_id))
            .copied()
            .unwrap_or(600)
            .max(1);
        let produced = (i128::from(delta) * i128::from(productivity))
            .checked_div(i128::from(period) * 10_000)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(i32::MAX);
        let multiplier = if product_id == 5 {
            oil_multiplier
        } else {
            gold_multiplier
        };
        let produced =
            scale_reward(i64::from(produced), multiplier).clamp(0, i64::from(i32::MAX)) as i32;
        product_count = product_count.saturating_add(produced).min(product_max);
    }
    (product_count > 0).then_some(ShopReward {
        goods_type: 5,
        item_id: product_id,
        num: product_count,
        instance_id: 0,
    })
}

#[cfg(test)]
fn collect_item_reward(
    building: &Value,
    config: Option<&Value>,
    catalog: Option<&BuildingCatalog>,
    now: u32,
) -> Option<ShopReward> {
    let recipe_id = json_i32(building, "recipeId")
        .or_else(|| json_i32(building, "recipe_id"))
        .unwrap_or_default();
    let recipe = catalog.and_then(|value| value.recipe_configs.get(&recipe_id))?;
    let recipe_time = json_i32(recipe, "time")?.max(1);
    let item = recipe.get("item").and_then(Value::as_array)?;
    let goods_type = i32::try_from(item.first()?.as_i64()?).ok()?;
    let item_id = i32::try_from(item.get(1)?.as_i64()?).ok()?;
    let item_num = i32::try_from(item.get(2)?.as_i64()?).ok()?.max(1);
    let item_count = json_i32(building, "itemCount")
        .or_else(|| json_i32(building, "item_count"))
        .unwrap_or_default()
        .max(0);
    let product_count = json_i32(building, "productCount")
        .or_else(|| json_i32(building, "product_count"))
        .unwrap_or_default()
        .max(0);
    let product_max = config
        .and_then(|value| json_i32(value, "productmax"))
        .unwrap_or_default()
        .max(0);
    let status = json_i32(building, "status").unwrap_or(1);
    let last_update = json_i64(building, "lastUpdateTime")
        .or_else(|| json_i64(building, "last_update_time"))
        .unwrap_or(i64::from(now));
    let completed = if status == 3 && item_count > 0 {
        i32::try_from(i64::from(now).saturating_sub(last_update).max(0) / i64::from(recipe_time))
            .unwrap_or(i32::MAX)
            .min(item_count)
    } else {
        0
    };
    let total = product_count
        .saturating_add(completed)
        .min(if product_max > 0 {
            product_max
        } else {
            i32::MAX
        });
    (total > 0).then_some(ShopReward {
        goods_type,
        item_id,
        num: total.saturating_mul(item_num),
        instance_id: 0,
    })
}

#[cfg(test)]
fn apply_building_collection_state(
    building: &mut Value,
    config: Option<&Value>,
    catalog: Option<&BuildingCatalog>,
    now: u32,
    oil_multiplier: f64,
    gold_multiplier: f64,
) {
    let building_type = config
        .and_then(|value| json_i32(value, "type"))
        .unwrap_or_default();
    if matches!(building_type, 3 | 4) {
        let product_max = config
            .and_then(|value| json_i32(value, "productmax"))
            .unwrap_or_default()
            .max(0);
        let old_count = json_i32(building, "productCount")
            .or_else(|| json_i32(building, "product_count"))
            .unwrap_or_default()
            .max(0);
        let status = json_i32(building, "status").unwrap_or(1);
        let productivity = config
            .and_then(|value| json_i32(value, "productivity"))
            .unwrap_or_default()
            .max(0);
        let last_update = json_i64(building, "lastUpdateTime")
            .or_else(|| json_i64(building, "last_update_time"))
            .unwrap_or(i64::from(now));
        let product_id = config
            .and_then(|value| json_i32_array(value, "productid").get(1).copied())
            .unwrap_or_default();
        let period = if product_id == 5 { 209 } else { 210 };
        let period = catalog
            .and_then(|value| value.resource_time_seconds.get(&period))
            .copied()
            .unwrap_or(600)
            .max(1);
        let delta = i64::from(now)
            .saturating_sub(last_update)
            .saturating_sub(1)
            .max(0);
        let produced = if status == 3 && productivity > 0 {
            (i128::from(delta) * i128::from(productivity))
                .checked_div(i128::from(period) * 10_000)
                .and_then(|value| i32::try_from(value).ok())
                .unwrap_or(i32::MAX)
        } else {
            0
        };
        let multiplier = if product_id == 5 {
            oil_multiplier
        } else {
            gold_multiplier
        };
        let produced =
            scale_reward(i64::from(produced), multiplier).clamp(0, i64::from(i32::MAX)) as i32;
        let settled_count = old_count.saturating_add(produced).min(product_max);
        building["productCount"] = json!(0);
        building["lastUpdateTime"] = json!(now);
        building["status"] = json!(if settled_count >= product_max { 1 } else { 3 });
    } else if building_type == 7 {
        let item_count = json_i32(building, "itemCount")
            .or_else(|| json_i32(building, "item_count"))
            .unwrap_or_default()
            .max(0);
        let recipe_id = json_i32(building, "recipeId")
            .or_else(|| json_i32(building, "recipe_id"))
            .unwrap_or_default();
        let status = json_i32(building, "status").unwrap_or(1);
        if item_count == 0 {
            building["recipeId"] = json!(0);
            building["status"] = json!(1);
        }
        building["productCount"] = json!(0);
        if item_count > 0 {
            let recipe_time = catalog
                .and_then(|value| value.recipe_configs.get(&recipe_id))
                .and_then(|value| json_i32(value, "time"))
                .unwrap_or(1)
                .max(1);
            let old_last_update = json_i64(building, "lastUpdateTime")
                .or_else(|| json_i64(building, "last_update_time"))
                .unwrap_or(i64::from(now));
            let completed = if status == 3 {
                i32::try_from(
                    i64::from(now).saturating_sub(old_last_update).max(0) / i64::from(recipe_time),
                )
                .unwrap_or(i32::MAX)
                .min(item_count)
            } else {
                0
            };
            building["itemCount"] = json!(item_count.saturating_sub(completed));
            let finished = item_count <= completed;
            if finished {
                building["lastUpdateTime"] = json!(i64::from(now));
            } else if status == 3 {
                building["lastUpdateTime"] =
                    json!(old_last_update
                        .saturating_add(i64::from(completed) * i64::from(recipe_time)));
            }
            building["status"] = json!(if finished { 1 } else { status });
            if finished {
                building["recipeId"] = json!(0);
            }
        }
    }
}

pub(super) fn bathroom_service_payload(hero_id: u64, pos: i64, _bath_time: i64) -> Vec<u8> {
    let mut output = Vec::new();
    if pos > 0 {
        append_varint_field(&mut output, 1, pos as u64);
    }
    append_varint_field(&mut output, 2, hero_id);
    append_varint_field(&mut output, 3, 0);
    output
}

#[cfg(test)]
pub(super) fn bathroom_start_all_payload(account: &Value, args: &[u8]) -> Vec<u8> {
    let heroes = account
        .get("bath")
        .and_then(|bath| bath.get("heroList"))
        .and_then(Value::as_array);
    let mut output = Vec::new();
    for nested in decode_repeated_message_field(args, 1) {
        let hero_id = decode_varint_u64_field(&nested, 1);
        let bath_time = heroes
            .and_then(|items| {
                items
                    .iter()
                    .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
            })
            .and_then(|hero| json_i64(hero, "bathTime"))
            .unwrap_or_default();
        append_message_field(&mut output, 1, &bathroom_end_payload(hero_id, bath_time));
    }
    output
}

#[cfg(test)]
pub(super) fn update_bathroom_state(account: &mut Value, method: &str, args: &[u8], now: u32) {
    let owned_hero_ids = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .map(|heroes| {
            heroes
                .iter()
                .filter_map(|hero| json_u64(hero, "heroId"))
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let start_hero = |hero_list: &mut Vec<Value>, hero_id: u64, pos: u64, validate: bool| {
        if hero_id == 0 || (validate && !owned_hero_ids.contains(&hero_id)) {
            return;
        }
        hero_list.retain(|hero| json_u64(hero, "heroId") != Some(hero_id));
        hero_list.push(json!({
            "heroId": hero_id,
            "pos": pos,
            "isAuto": 0,
            "startTime": now,
            "bathTime": 0,
            "buffId": 0,
            "buffTime": 0,
            "power": 0,
        }));
    };
    let Some(root) = account.as_object_mut() else {
        return;
    };
    let bath = root
        .entry("bath".to_owned())
        .or_insert_with(|| json!({"heroList": [], "isAllAuto": 0}));
    let Some(bath) = bath.as_object_mut() else {
        return;
    };
    let hero_list = bath
        .entry("heroList".to_owned())
        .or_insert_with(|| json!([]));
    let Some(hero_list) = hero_list.as_array_mut() else {
        return;
    };

    match method {
        "bathroom.BathAllAuto" => {
            bath.insert(
                "isAllAuto".to_owned(),
                json!(decode_varint_u64_field(args, 1)),
            );
        }
        "bathroom.BathAuto" => {
            let hero_id = decode_varint_u64_field(args, 1);
            let is_auto = decode_varint_u64_field(args, 2);
            if hero_id != 0 {
                if let Some(hero) = hero_list
                    .iter_mut()
                    .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
                    .and_then(Value::as_object_mut)
                {
                    hero.insert("isAuto".to_owned(), json!(is_auto));
                }
            }
        }
        "bathroom.BathStart" => {
            let hero_id = decode_varint_u64_field(args, 1);
            let pos = decode_varint_u64_field(args, 2);
            start_hero(hero_list, hero_id, pos, true);
        }
        "bathroom.BathChangeHero" => {
            let old_hero_id = decode_varint_u64_field(args, 1);
            if old_hero_id != 0 {
                hero_list.retain(|hero| json_u64(hero, "heroId") != Some(old_hero_id));
            }
        }
        "bathroom.BathEnd" => {
            let hero_id = decode_varint_u64_field(args, 1);
            if hero_id != 0 {
                hero_list.retain(|hero| json_u64(hero, "heroId") != Some(hero_id));
            }
        }
        "bathroom.BathStartAll" => {
            for nested in decode_repeated_message_field(args, 1) {
                start_hero(
                    hero_list,
                    decode_varint_u64_field(&nested, 1),
                    decode_varint_u64_field(&nested, 2),
                    false,
                );
            }
        }
        "bathroom.BathService" => {}
        _ => {}
    }
}

pub(super) fn bathroom_info_payload(account: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    let Some(bath) = account.get("bath") else {
        output.extend_from_slice(&[0x0A, 0x00]);
        return output;
    };
    let heroes = bath.get("heroList").and_then(Value::as_array);
    if let Some(heroes) = heroes.filter(|items| !items.is_empty()) {
        for hero in heroes {
            let mut encoded = Vec::new();
            append_varint_field(
                &mut encoded,
                1,
                json_i64(hero, "heroId").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut encoded,
                2,
                json_i64(hero, "pos").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut encoded,
                3,
                json_i64(hero, "isAuto").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut encoded,
                4,
                json_i64(hero, "startTime").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut encoded,
                5,
                json_i64(hero, "bathTime").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut encoded,
                6,
                json_i64(hero, "buffId").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut encoded,
                7,
                json_i64(hero, "buffTime").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut encoded,
                8,
                json_i64(hero, "power").unwrap_or_default().max(0) as u64,
            );
            append_message_field(&mut output, 1, &encoded);
        }
    } else {
        output.extend_from_slice(&[0x0A, 0x00]);
    }
    let auto = json_i64(bath, "isAllAuto").unwrap_or_default();
    if auto != 0 {
        append_varint_field(&mut output, 2, auto.max(0) as u64);
    }
    output
}
