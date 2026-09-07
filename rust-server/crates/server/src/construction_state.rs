use serde_json::{json, Value};

use super::*;

pub(super) fn decode_construction_projects(payload: &[u8]) -> Vec<Value> {
    let mut projects = Vec::new();
    let mut index = 0;
    while index < payload.len() {
        let Ok((key, next)) = read_varint(payload, index) else {
            break;
        };
        index = next;
        if key >> 3 != 1 || key & 7 != 2 {
            index = skip_wire(payload, index, key & 7).unwrap_or(payload.len());
            continue;
        }
        let Ok((length, body_start)) = read_varint(payload, index) else {
            break;
        };
        let Ok(length) = usize::try_from(length) else {
            break;
        };
        let Some(end) = body_start.checked_add(length) else {
            break;
        };
        let Some(body) = payload.get(body_start..end) else {
            break;
        };
        let mut items = Vec::new();
        let mut gold = 0i64;
        let mut cursor = 0;
        while cursor < body.len() {
            let Ok((field_key, field_next)) = read_varint(body, cursor) else {
                break;
            };
            cursor = field_next;
            match (field_key >> 3, field_key & 7) {
                (1, 2) => {
                    let Ok((len, start)) = read_varint(body, cursor) else {
                        break;
                    };
                    let Ok(len) = usize::try_from(len) else { break };
                    let Some(stop) = start.checked_add(len) else {
                        break;
                    };
                    let Some(item) = body.get(start..stop) else {
                        break;
                    };
                    let id = decode_varint_field(item, 1);
                    let count = decode_varint_field(item, 2);
                    items.push(json!({"resId": id, "count": count}));
                    cursor = stop;
                }
                (2, 0) => {
                    if let Ok((value, next)) = read_varint(body, cursor) {
                        gold = value as i64;
                        cursor = next;
                    } else {
                        break;
                    }
                }
                _ => {
                    cursor = skip_wire(body, cursor, field_key & 7).unwrap_or(body.len());
                }
            }
        }
        projects.push(json!({"items": items, "gold": gold}));
        index = end;
    }
    projects
}

pub(super) fn start_construction(
    account: &mut Value,
    projects: &[Value],
    now: u32,
) -> Result<(), &'static str> {
    let existing = account
        .get("construction")
        .and_then(|v| v.get("jobs"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if existing.len() + projects.len() > 10 {
        return Err("construction queue is full");
    }
    for project in projects {
        let gold = json_i64(project, "gold").unwrap_or_default();
        if !(30..=999).contains(&gold) {
            return Err("construction gold is invalid");
        }
        let mut seen = std::collections::HashSet::new();
        let Some(items) = project.get("items").and_then(Value::as_array) else {
            return Err("construction materials are missing");
        };
        for item in items {
            let id = json_i64(item, "resId").unwrap_or_default();
            let count = json_i64(item, "count").unwrap_or_default();
            if !matches!(id, 10029 | 10030) || !(30..=999).contains(&count) || !seen.insert(id) {
                return Err("construction material project is invalid");
            }
        }
        if !seen.contains(&10029) || !seen.contains(&10030) {
            return Err("both construction materials are required");
        }
    }
    let total_gold: i64 = projects
        .iter()
        .map(|p| json_i64(p, "gold").unwrap_or_default())
        .sum();
    let total_steel: i64 = projects
        .iter()
        .flat_map(|p| p.get("items").and_then(Value::as_array))
        .flatten()
        .filter(|i| json_i64(i, "resId") == Some(10029))
        .map(|i| json_i64(i, "count").unwrap_or_default())
        .sum();
    let total_al: i64 = projects
        .iter()
        .flat_map(|p| p.get("items").and_then(Value::as_array))
        .flatten()
        .filter(|i| json_i64(i, "resId") == Some(10030))
        .map(|i| json_i64(i, "count").unwrap_or_default())
        .sum();
    if total_gold <= 0
        || character_i64(account, "gold") < total_gold
        || bag_item_count(account, 10029) < total_steel
        || bag_item_count(account, 10030) < total_al
    {
        return Err("not enough construction resources");
    }
    let mut jobs = existing;
    let mut sequence = jobs
        .iter()
        .filter_map(|j| json_i64(j, "sequence"))
        .max()
        .unwrap_or(0)
        + 1;
    let active = jobs
        .iter()
        .filter(|j| !json_bool(j, "completed") && json_i64(j, "endTime").unwrap_or_default() > 0)
        .count();
    for (offset, project) in projects.iter().enumerate() {
        let steel = project
            .get("items")
            .and_then(Value::as_array)
            .and_then(|items| items.iter().find(|i| json_i64(i, "resId") == Some(10029)))
            .and_then(|i| json_i64(i, "count"))
            .unwrap_or_default();
        let aluminium = project
            .get("items")
            .and_then(Value::as_array)
            .and_then(|items| items.iter().find(|i| json_i64(i, "resId") == Some(10030)))
            .and_then(|i| json_i64(i, "count"))
            .unwrap_or_default();
        let template_id = select_construction_template(
            json_i64(project, "gold").unwrap_or_default(),
            steel,
            aluminium,
        );
        let duration = construction_duration_seconds(template_id);
        let end = if active + offset < 2 {
            i64::from(now) + i64::from(duration)
        } else {
            0
        };
        jobs.push(json!({"sequence": sequence, "templateId": template_id, "durationSeconds": duration, "endTime": end, "completed": false, "project": project}));
        sequence += 1;
    }
    adjust_character_i64(account, "gold", -total_gold);
    consume_bag_item(account, 10029, total_steel as i32);
    consume_bag_item(account, 10030, total_al as i32);
    account["construction"] = json!({"jobs": jobs, "nextSequence": sequence, "lastProject": projects.last().cloned().unwrap_or(Value::Null)});
    Ok(())
}

pub(super) fn select_construction_template(gold: i64, steel: i64, aluminium: i64) -> i32 {
    if let Some(catalog) = BUILD_FORMULA_CATALOG.get() {
        for (r1, r2, r3, ships) in &catalog.0 {
            let in_range = |value: i64, range: &[i64]| {
                range.len() >= 2 && value >= range[0] && value <= range[1]
            };
            if in_range(gold, r1) && in_range(steel, r2) && in_range(aluminium, r3) {
                if let Some(template) = ships.first() {
                    return *template;
                }
            }
        }
    }
    match gold + steel + aluminium {
        total if total >= 2400 => 10210513,
        total if total >= 1200 => 10210512,
        _ => 10210511,
    }
}

pub(super) fn construction_duration_seconds(template_id: i32) -> i32 {
    let configured = BUILD_SHIP_CATALOG
        .get()
        .and_then(|catalog| catalog.ship_build_time.get(&template_id))
        .copied()
        .unwrap_or(60);
    configured.clamp(60, 7 * 24 * 60 * 60)
}

pub(super) fn build_notes_payload(now: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let Some(catalog) = BUILD_FORMULA_CATALOG.get() else {
        return out;
    };
    for (r1, r2, r3, ships) in &catalog.0 {
        let Some(&template_id) = ships.first() else {
            continue;
        };
        let gold = r1.first().copied().unwrap_or(30).clamp(30, 999);
        let steel = r2.first().copied().unwrap_or(30).clamp(30, 999);
        let aluminium = r3.first().copied().unwrap_or(30).clamp(30, 999);
        let mut project = Vec::new();
        for (res_id, count) in [(10029, steel), (10030, aluminium)] {
            let mut item = Vec::new();
            append_varint_field(&mut item, 1, res_id as u64);
            append_varint_field(&mut item, 2, count as u64);
            append_message_field(&mut project, 1, &item);
        }
        append_varint_field(&mut project, 2, gold as u64);
        let mut formula = Vec::new();
        append_varint_field(&mut formula, 1, u64::from(now));
        append_message_field(&mut formula, 2, &project);
        append_varint_field(&mut formula, 3, template_id as u64);
        let mut note = Vec::new();
        append_bytes_field(&mut note, 1, format!("Ship {template_id}").as_bytes());
        append_message_field(&mut note, 2, &formula);
        append_varint_field(&mut note, 3, 0);
        append_varint_field(&mut note, 4, 0);
        append_varint_field(&mut note, 5, template_id as u64);
        append_message_field(&mut out, 1, &note);
    }
    out
}

pub(super) fn discuss_payload(htid: i32) -> Vec<u8> {
    let mut out = Vec::new();
    append_varint_field(&mut out, 1, 0);
    append_varint_field(&mut out, 2, 0);
    append_varint_field(&mut out, 3, u64::from(current_unix_seconds()));
    append_varint_field(&mut out, 4, 0);
    if let Some(catalog) = BUILD_FORMULA_CATALOG.get() {
        if let Some((r1, r2, r3, ships)) = catalog.0.iter().find(|(_, _, _, ships)| {
            ships
                .iter()
                .any(|template| *template == htid || *template / 10 == htid)
        }) {
            let template_id = ships.first().copied().unwrap_or_default();
            let msg = format!(
                "Build formula: gold {} steel {} aluminium {}",
                r1.first().copied().unwrap_or(30),
                r2.first().copied().unwrap_or(30),
                r3.first().copied().unwrap_or(30)
            );
            let mut item = Vec::new();
            append_bytes_field(&mut item, 1, format!("Ship {template_id}").as_bytes());
            append_bytes_field(&mut item, 2, msg.as_bytes());
            for field in 3..=8 {
                append_varint_field(&mut item, field, 0);
            }
            append_message_field(&mut out, 5, &item);
        }
    }
    out
}

pub(super) fn encode_discuss_empty() -> Vec<u8> {
    let mut out = Vec::new();
    append_varint_field(&mut out, 1, 0);
    append_varint_field(&mut out, 2, 0);
    append_varint_field(&mut out, 3, u64::from(current_unix_seconds()));
    append_varint_field(&mut out, 4, 0);
    out
}

pub(super) fn finish_construction(account: &mut Value, indexes: &[i32], now: u32) -> bool {
    if indexes.is_empty() {
        return false;
    }
    let mut unique = indexes.to_vec();
    unique.sort_unstable();
    unique.dedup();
    if bag_item_count(account, 10031) < unique.len() as i64 {
        return false;
    }
    let building: Vec<i64> = account
        .get("construction")
        .and_then(|v| v.get("jobs"))
        .and_then(Value::as_array)
        .map(|jobs| {
            jobs.iter()
                .filter(|j| {
                    !json_bool(j, "completed")
                        && json_i64(j, "endTime").unwrap_or_default() > i64::from(now)
                })
                .filter_map(|j| json_i64(j, "sequence"))
                .collect()
        })
        .unwrap_or_default();
    let mut selected = Vec::new();
    for index in unique.iter().filter_map(|v| usize::try_from(*v).ok()) {
        if index == 0 {
            return false;
        }
        let Some(sequence) = building.get(index - 1) else {
            return false;
        };
        selected.push(*sequence);
    }
    let Some(jobs) = account
        .get_mut("construction")
        .and_then(|v| v.get_mut("jobs"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    for job in jobs.iter_mut() {
        if selected.contains(&json_i64(job, "sequence").unwrap_or_default()) {
            job["completed"] = Value::Bool(true);
            job["endTime"] = json!(now);
        }
    }
    consume_bag_item(account, 10031, unique.len() as i32);
    true
}

pub(super) fn receive_construction(
    account: &mut Value,
    indexes: &[i32],
    now: u32,
) -> (Vec<u8>, usize) {
    let completed: Vec<Value> = account
        .get("construction")
        .and_then(|v| v.get("jobs"))
        .and_then(Value::as_array)
        .map(|jobs| {
            jobs.iter()
                .filter(|j| {
                    json_bool(j, "completed")
                        || json_i64(j, "endTime").unwrap_or_default() <= i64::from(now)
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if completed.is_empty() {
        return (Vec::new(), 0);
    }
    let capacity = account
        .get("dock")
        .and_then(|dock| dock.get("bagSize"))
        .and_then(Value::as_i64)
        .unwrap_or(200);
    let current = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len) as i64;
    let wanted = if indexes.is_empty() {
        vec![1]
    } else {
        indexes
            .iter()
            .filter_map(|v| usize::try_from(*v).ok())
            .collect()
    };
    let mut wanted = wanted;
    wanted.sort_unstable();
    wanted.dedup();
    if current + wanted.len() as i64 > capacity
        || wanted
            .iter()
            .any(|index| *index == 0 || *index > completed.len())
    {
        return (Vec::new(), 0);
    }
    let mut rewards = Vec::new();
    for index in wanted.iter().copied() {
        if let Some(job) = completed.get(index.saturating_sub(1)) {
            let template_id = json_i32(job, "templateId").unwrap_or(10210511);
            let id = add_ship_items(account, template_id, 1, now);
            if let Some(catalog) = BUILD_SHIP_CATALOG.get() {
                apply_ship_defaults(account, catalog, template_id, id);
            }
            rewards.push(ShopReward {
                goods_type: 3,
                item_id: template_id,
                num: 1,
                instance_id: id,
            });
        }
    }
    let seqs: std::collections::HashSet<i64> = wanted
        .iter()
        .filter_map(|i| completed.get(i.saturating_sub(1)))
        .filter_map(|j| json_i64(j, "sequence"))
        .collect();
    if let Some(jobs) = account
        .get_mut("construction")
        .and_then(|v| v.get_mut("jobs"))
        .and_then(Value::as_array_mut)
    {
        jobs.retain(|j| !seqs.contains(&json_i64(j, "sequence").unwrap_or_default()));
    }
    (encode_rewards_list(&rewards), rewards.len())
}

pub(super) fn encode_rewards_list(rewards: &[ShopReward]) -> Vec<u8> {
    let mut out = Vec::new();
    for reward in rewards {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, reward.goods_type as u64);
        append_varint_field(&mut item, 2, reward.item_id as u64);
        append_varint_field(&mut item, 3, reward.num as u64);
        append_varint_field(&mut item, 4, reward.instance_id as u64);
        append_message_field(&mut out, 1, &item);
    }
    out
}

pub(super) fn encode_buildship_ret(rewards: &[ShopReward]) -> Vec<u8> {
    let mut out = Vec::new();
    for reward in rewards {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, reward.goods_type.max(0) as u64);
        append_varint_field(&mut item, 2, reward.item_id.max(0) as u64);
        append_varint_field(&mut item, 3, reward.num.max(0) as u64);
        append_varint_field(&mut item, 4, reward.instance_id.max(0) as u64);
        append_message_field(&mut out, 1, &item);
        append_message_field(&mut out, 3, &[]);
    }
    out
}
