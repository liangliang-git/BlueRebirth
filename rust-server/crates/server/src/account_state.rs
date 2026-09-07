use serde_json::{json, Value};

use super::*;

pub(super) fn set_character_i64(account: &mut Value, key: &str, value: i32) {
    if let Some(character) = account.get_mut("character").and_then(Value::as_object_mut) {
        character.insert(key.to_owned(), json!(value));
    }
}

pub(super) fn set_character_string(account: &mut Value, key: &str, value: String) {
    if let Some(character) = account.get_mut("character").and_then(Value::as_object_mut) {
        character.insert(key.to_owned(), Value::String(value));
    }
}

pub(super) fn find_hero_mut(
    account: &mut Value,
    hero_id: u64,
) -> Option<&mut serde_json::Map<String, Value>> {
    account
        .get_mut("dock")?
        .get_mut("heroes")?
        .as_array_mut()?
        .iter_mut()
        .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
        .and_then(Value::as_object_mut)
}

pub(super) fn hero_is_in_use(account: &Value, hero_id: u64) -> bool {
    if hero_id == 0 {
        return false;
    }
    let secretary_used = account
        .get("character")
        .and_then(|character| json_u64(character, "secretaryId"))
        == Some(hero_id);
    let fleet_used = account
        .get("fleet")
        .and_then(|fleet| fleet.get("tactics"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|tactic| {
            ["heroInfo", "heroIds", "exHeroInfo"]
                .into_iter()
                .flat_map(move |key| json_i32_array(tactic, key))
        })
        .any(|id| u64::try_from(id).ok() == Some(hero_id));
    let building_used = account
        .get("building")
        .and_then(|building| building.get("buildings"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|building| json_i32_array(building, "heroIds"))
        .any(|id| u64::try_from(id).ok() == Some(hero_id));
    let bath_used = account
        .get("bath")
        .and_then(|bath| bath.get("heroList"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|hero| json_u64(hero, "heroId") == Some(hero_id));
    secretary_used || fleet_used || building_used || bath_used
}

pub(super) fn resource_available(
    account: &Value,
    goods_type: i32,
    item_id: i32,
    amount: i32,
) -> bool {
    amount > 0
        && if goods_type == 5 {
            currency_character_key(item_id)
                .map(|key| character_i64(account, key) >= i64::from(amount))
                .unwrap_or(false)
        } else if goods_type == 1 || goods_type == 6 {
            bag_item_count(account, item_id) >= i64::from(amount)
        } else {
            false
        }
}

pub(super) fn consume_resource(account: &mut Value, goods_type: i32, item_id: i32, amount: i32) {
    if goods_type == 5 {
        if let Some(key) = currency_character_key(item_id) {
            adjust_character_i64(account, key, -i64::from(amount));
        }
    } else if goods_type == 1 || goods_type == 6 {
        let _ = consume_bag_item(account, item_id, amount);
    }
}

pub(super) fn currency_character_key(currency_type: i32) -> Option<&'static str> {
    Some(match currency_type {
        1 => "gold",
        2 => "diamond",
        5 => "supply",
        8 => "mainGun",
        9 => "torpedo",
        10 => "plane",
        11 => "other",
        12 => "retire",
        13 => "bath",
        14 => "strategy",
        15 => "medal",
        18 => "tower",
        22 => "copyTrainPoint",
        23 => "fashionPoint",
        24 => "guildContri",
        25 => "lucky",
        26 => "teacherMedal",
        27 => "teacherPrestige",
        28 => "battlePassExp",
        29 => "battlePassGold",
        30 => "pvePt",
        31 => "guildCoinII",
        32 => "urEquipCoin",
        33 => "activityBattlePassExp",
        _ => return None,
    })
}

pub(super) fn add_character_i64(account: &mut Value, key: &str, amount: i32) {
    adjust_character_i64(account, key, i64::from(amount));
}

pub(super) fn character_i64(account: &Value, key: &str) -> i64 {
    account
        .get("character")
        .and_then(|character| character.get(key))
        .and_then(Value::as_i64)
        .unwrap_or_default()
}

pub(super) fn adjust_character_i64(account: &mut Value, key: &str, amount: i64) {
    if let Some(character) = account.get_mut("character").and_then(Value::as_object_mut) {
        let current = character
            .get(key)
            .and_then(Value::as_i64)
            .unwrap_or_default();
        let next = current.saturating_add(amount);
        let next = if key == "supply" { next.max(0) } else { next };
        character.insert(key.to_owned(), json!(next));
    }
}

pub(super) fn add_bag_item(account: &mut Value, template_id: i32, amount: i32) {
    let Some(root) = account.as_object_mut() else {
        return;
    };
    let bag = root
        .entry("bag".to_owned())
        .or_insert_with(|| json!({"items": [], "bagSize": 100}));
    let Some(items) = bag.get_mut("items").and_then(Value::as_array_mut) else {
        return;
    };
    if let Some(item) = items
        .iter_mut()
        .find(|item| json_i32(item, "templateId") == Some(template_id))
        .and_then(Value::as_object_mut)
    {
        let current = item.get("num").and_then(Value::as_i64).unwrap_or_default();
        item.insert(
            "num".to_owned(),
            json!(current.saturating_add(i64::from(amount))),
        );
    } else {
        items.push(json!({"templateId": template_id, "num": amount}));
    }
    // config_expand_item.db: 140001 expands ship dock, 140002 expands equipment dock.
    // Keep item in bag (client displays ownership) and apply capacity immediately.
    if amount > 0 {
        let expansion = i64::from(amount).saturating_mul(10);
        match template_id {
            140001 => {
                if let Some(dock) = root.get_mut("dock").and_then(Value::as_object_mut) {
                    let current = dock.get("bagSize").and_then(Value::as_i64).unwrap_or(200);
                    dock.insert(
                        "bagSize".to_owned(),
                        json!(current.saturating_add(expansion)),
                    );
                }
            }
            140002 => {
                if let Some(equip) = root.get_mut("equip").and_then(Value::as_object_mut) {
                    let current = equip
                        .get("equipBagSize")
                        .and_then(Value::as_i64)
                        .unwrap_or(2000);
                    equip.insert(
                        "equipBagSize".to_owned(),
                        json!(current.saturating_add(expansion)),
                    );
                }
            }
            _ => {}
        }
    }
}

pub(super) fn add_medal(account: &mut Value, medal_id: i32, time: u32) {
    if medal_id <= 0 || time == 0 {
        return;
    }
    let Some(root) = account.as_object_mut() else {
        return;
    };
    let medals = root.entry("medals".to_owned()).or_insert_with(|| json!([]));
    let Some(medals) = medals.as_array_mut() else {
        return;
    };
    if medals.iter().any(|medal| {
        json_i32(medal, "medalId") == Some(medal_id)
            || json_i32(medal, "medal_id") == Some(medal_id)
    }) {
        return;
    }
    medals.push(json!({"medalId": medal_id, "time": time}));
}

/// Populate missing hero skill rows from client ship configuration. Existing skill levels stay
/// untouched; this only repairs fresh/legacy snapshots where the client would otherwise render
/// its dummy level-1 skill.
pub(super) fn ensure_hero_pskills(
    account: &mut Value,
    catalog: &std::collections::BTreeMap<i32, Vec<i32>>,
) -> bool {
    let Some(heroes) = account
        .get_mut("dock")
        .and_then(|dock| dock.get_mut("heroes"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    for hero in heroes {
        let template_id = json_i32(hero, "templateId").unwrap_or_default();
        let Some(hero_obj) = hero.as_object_mut() else {
            continue;
        };
        let Some(configured) = catalog.get(&template_id) else {
            continue;
        };
        let skills = hero_obj
            .entry("pSkills".to_owned())
            .or_insert_with(|| json!([]));
        let Some(skills) = skills.as_array_mut() else {
            continue;
        };
        if skills.is_empty() {
            *skills = configured
                .iter()
                .map(|id| json!({"pSkillId": id, "pSkillExp": 0, "level": 1, "replace": 0}))
                .collect();
            changed = true;
            continue;
        }
        for skill_id in configured {
            if let Some(skill) = skills.iter_mut().find(|skill| {
                json_i32(skill, "pSkillId").or_else(|| json_i32(skill, "pskillId"))
                    == Some(*skill_id)
            }) {
                if json_i32(skill, "level").is_none() {
                    let level = json_i32(skill, "pSkillLv")
                        .or_else(|| json_i32(skill, "pskillLv"))
                        .unwrap_or(1)
                        .max(1);
                    skill["level"] = json!(level);
                    changed = true;
                }
            } else {
                skills
                    .push(json!({"pSkillId": skill_id, "pSkillExp": 0, "level": 1, "replace": 0}));
                changed = true;
            }
        }
    }
    changed
}

pub(super) fn ensure_hero_mood_state(account: &mut Value) -> bool {
    let Some(heroes) = account
        .get_mut("dock")
        .and_then(|dock| dock.get_mut("heroes"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    for hero in heroes {
        let Some(hero_obj) = hero.as_object_mut() else {
            continue;
        };
        let mood = hero_obj
            .get("mood")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .filter(|value| *value >= MOOD_MIN)
            .map(|value| value.clamp(MOOD_MIN, MOOD_MAX))
            .unwrap_or(MOOD_INITIAL);
        if hero_obj.get("mood").and_then(Value::as_i64) != Some(i64::from(mood)) {
            hero_obj.insert("mood".to_owned(), json!(mood));
            changed = true;
        }
    }
    changed
}

pub(super) fn apply_natural_mood_recovery(account: &mut Value, now: u32, multiplier: f64) -> bool {
    let Some(heroes) = account
        .get_mut("dock")
        .and_then(|dock| dock.get_mut("heroes"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    let now = i64::from(now);
    for hero in heroes {
        let Some(hero_obj) = hero.as_object_mut() else {
            continue;
        };
        let Some(last_update) = hero_obj.get("moodUpdateTime").and_then(Value::as_i64) else {
            hero_obj.insert("moodUpdateTime".to_owned(), json!(now));
            changed = true;
            continue;
        };
        if last_update >= now {
            if last_update > now {
                hero_obj.insert("moodUpdateTime".to_owned(), json!(now));
                changed = true;
            }
            continue;
        }
        let elapsed = now.saturating_sub(last_update);
        let intervals = elapsed / MOOD_RECOVERY_INTERVAL_SECONDS;
        if intervals <= 0 {
            continue;
        }
        let next_update =
            last_update.saturating_add(intervals.saturating_mul(MOOD_RECOVERY_INTERVAL_SECONDS));
        let mood = i64::from(
            hero_obj
                .get("mood")
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok())
                .unwrap_or(MOOD_INITIAL),
        );
        let married_bonus = if hero_obj
            .get("marryTime")
            .and_then(Value::as_i64)
            .is_some_and(|value| value > 0)
        {
            MOOD_MARRIED_RECOVERY_BONUS
        } else {
            0
        };
        let base = MOOD_NORMAL_RECOVERY.saturating_add(married_bonus);
        let per_interval = scale_reward(i64::from(base), multiplier);
        let recovery = per_interval.saturating_mul(intervals);
        let next_mood = mood
            .saturating_add(recovery)
            .clamp(i64::from(MOOD_MIN), i64::from(MOOD_NORMAL_LIMIT));
        if next_mood != mood {
            hero_obj.insert("mood".to_owned(), json!(next_mood as i32));
            changed = true;
        }
        if hero_obj.get("moodUpdateTime").and_then(Value::as_i64) != Some(next_update) {
            hero_obj.insert("moodUpdateTime".to_owned(), json!(next_update));
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
pub(super) fn restore_hero_mood(account: &mut Value, hero_id: u64) -> bool {
    let Some(hero) = account
        .get_mut("dock")
        .and_then(|dock| dock.get_mut("heroes"))
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
    else {
        return false;
    };
    let old = json_i32(hero, "mood").unwrap_or(MOOD_INITIAL);
    hero["mood"] = json!(MOOD_INITIAL);
    let changed = old != MOOD_INITIAL;
    if changed {
        hero["moodUpdateTime"] = json!(current_unix_seconds());
    }
    changed
}

pub(super) fn recover_hero_mood_from_bath(
    account: &mut Value,
    hero_id: u64,
    bath_seconds: i64,
    multiplier: f64,
    now: u32,
) -> bool {
    let Some(hero) = account
        .get_mut("dock")
        .and_then(|dock| dock.get_mut("heroes"))
        .and_then(Value::as_array_mut)
        .and_then(|heroes| {
            heroes
                .iter_mut()
                .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
        })
    else {
        return false;
    };
    let Some(hero) = hero.as_object_mut() else {
        return false;
    };
    let intervals = bath_seconds.max(0) / MOOD_BATH_INTERVAL_SECONDS;
    let base_recovery = if intervals > 0 {
        i64::from(MOOD_BATH_INTERVAL_RECOVERY)
            .saturating_mul(intervals)
            .min(i64::from(MOOD_BATH_RECOVERY))
    } else {
        i64::from(MOOD_BATH_RECOVERY)
    };
    let recovery = scale_reward(base_recovery, multiplier);
    let mood: i64 = i64::from(
        hero.get("mood")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(MOOD_INITIAL),
    );
    let next_mood = mood
        .saturating_add(recovery)
        .clamp(i64::from(MOOD_MIN), i64::from(MOOD_MAX));
    let mut changed = false;
    if next_mood != mood {
        hero.insert("mood".to_owned(), json!(next_mood as i32));
        changed = true;
    }
    if hero.get("moodUpdateTime").and_then(Value::as_u64) != Some(u64::from(now)) {
        hero.insert("moodUpdateTime".to_owned(), json!(now));
        changed = true;
    }
    changed
}

pub(super) fn consume_bag_item(account: &mut Value, template_id: i32, requested: i32) -> i32 {
    if requested <= 0 {
        return 0;
    }
    let Some(items) = account
        .get_mut("bag")
        .and_then(|bag| bag.get_mut("items"))
        .and_then(Value::as_array_mut)
    else {
        return 0;
    };
    let Some(index) = items
        .iter()
        .position(|item| {
            json_i32(item, "templateId") == Some(template_id)
                && json_i32(item, "num").unwrap_or_default() > 0
        })
        .or_else(|| {
            items
                .iter()
                .position(|item| json_i32(item, "templateId") == Some(template_id))
        })
    else {
        return 0;
    };
    let current = json_i32(&items[index], "num").unwrap_or_default().max(0);
    let consumed = current.min(requested);
    // Keep exhausted rows as explicit zero-count tombstones.  Bag/equip update
    // payloads use num=0 to clear client-side cached inventory entries; removing
    // row leaves Lua client cache showing stale count until full relogin.
    if let Some(item) = items[index].as_object_mut() {
        item.insert("num".to_owned(), json!(current - consumed));
    }
    consumed
}

pub(super) fn bag_item_count(account: &Value, template_id: i32) -> i64 {
    account
        .get("bag")
        .and_then(|bag| bag.get("items"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| json_i32(item, "templateId") == Some(template_id))
                .map(|item| json_i32(item, "num").unwrap_or_default().max(0) as i64)
                .sum()
        })
        .unwrap_or_default()
}

pub(super) fn add_fashion_item(
    account: &mut Value,
    fashion_tid: i32,
    catalog: Option<&FashionList>,
) {
    let sf_id = catalog
        .and_then(|catalog| {
            catalog
                .items
                .iter()
                .find(|item| item.fashion_tids.contains(&fashion_tid))
                .map(|item| item.sf_id)
        })
        .unwrap_or(fashion_tid);
    let Some(root) = account.as_object_mut() else {
        return;
    };
    let fashion = root
        .entry("fashion".to_owned())
        .or_insert_with(|| json!({"entries": []}));
    let Some(entries) = fashion.get_mut("entries").and_then(Value::as_array_mut) else {
        return;
    };
    if let Some(entry) = entries
        .iter_mut()
        .find(|entry| json_i32(entry, "sfId") == Some(sf_id))
        .and_then(Value::as_object_mut)
    {
        let tids = entry
            .entry("fashionTids".to_owned())
            .or_insert_with(|| json!([]));
        if let Some(tids) = tids.as_array_mut() {
            if !tids
                .iter()
                .any(|value| value.as_i64() == Some(i64::from(fashion_tid)))
            {
                tids.push(json!(fashion_tid));
            }
        }
    } else {
        entries.push(json!({"sfId": sf_id, "fashionTids": [fashion_tid]}));
    }
}

pub(super) fn next_account_instance_id(account: &Value, path: &str, key: &str) -> i32 {
    account
        .get(path)
        .and_then(|value| value.get(key))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| json_i32(value, if path == "dock" { "heroId" } else { "equipId" }))
        .max()
        .unwrap_or_default()
        .saturating_add(1)
}

pub(super) fn add_equip_item(account: &mut Value, template_id: i32) -> i32 {
    let equip_id = next_account_instance_id(account, "equip", "items");
    let Some(root) = account.as_object_mut() else {
        return equip_id;
    };
    let equip = root
        .entry("equip".to_owned())
        .or_insert_with(|| json!({"items": [], "equipBagSize": 2000}));
    if let Some(items) = equip.get_mut("items").and_then(Value::as_array_mut) {
        items.push(json!({
            "equipId": equip_id,
            "templateId": template_id,
            "enhanceLv": 0,
            "star": 0,
            "heroId": 0,
            "enhanceExp": 0
        }));
    }
    equip_id
}

pub(super) fn add_ship_items(account: &mut Value, template_id: i32, amount: i32, now: u32) -> i32 {
    let mut last_id = 0;
    let Some(root) = account.as_object_mut() else {
        return last_id;
    };
    let dock = root
        .entry("dock".to_owned())
        .or_insert_with(|| json!({"heroes": [], "bagSize": 200}));
    if let Some(heroes) = dock.get_mut("heroes").and_then(Value::as_array_mut) {
        for _ in 0..amount {
            let id = heroes
                .iter()
                .filter_map(|hero| json_i32(hero, "heroId"))
                .max()
                .unwrap_or_default()
                .saturating_add(1);
            heroes.push(json!({
                "heroId": id,
                "templateId": template_id,
                "level": 1,
                "fashioning": (template_id.saturating_sub(1)) / 10,
                "exp": 0,
                "createTime": now,
                "updateTime": now,
                "moodUpdateTime": now,
                "affection": 500000,
                "marryTime": 0,
                "mood": MOOD_INITIAL,
                "marryType": 0,
                "curHp": 10000000000i64,
                "equipSlots": [0, 0, 0, 0, 0, 0],
                "name": "",
                "changeNameTime": 0,
                "lock": false,
                "advance": 0,
                "advLv": 0,
                "remouldLevel": 0
            }));
            last_id = id;
        }
    }
    last_id
}

pub(super) fn apply_ship_defaults(
    account: &mut Value,
    catalog: &BuildShipCatalog,
    template_id: i32,
    hero_id: i32,
) {
    if hero_id <= 0 {
        return;
    }
    let Some(defaults) = catalog.ship_defaults.get(&template_id) else {
        return;
    };
    let mut equip_ids = Vec::new();
    for template in defaults {
        equip_ids.push(add_equip_item(account, *template));
    }
    if let Some(hero) = find_hero_mut(account, hero_id as u64) {
        let slots = hero
            .entry("equipSlots".to_owned())
            .or_insert_with(|| json!([]));
        let Some(slots) = slots.as_array_mut() else {
            return;
        };
        while slots.len() < 6 {
            slots.push(json!(0));
        }
        for (index, equip_id) in equip_ids.into_iter().enumerate().take(6) {
            slots[index] = json!(equip_id);
        }
    }
}

pub(super) fn json_i32(value: &Value, key: &str) -> Option<i32> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
}

pub(super) fn json_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

pub(super) fn json_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

pub(super) fn apply_strategy_state(account: &mut Value, method: &str, args: &[u8]) -> bool {
    let strategy_id = decode_varint_field(args, 1);
    match method {
        "strategy.Learn" | "strategy.Upgrade" => {
            if strategy_id <= 0 {
                return false;
            }
            let level = decode_varint_field(args, 2).max(1);
            let list = account
                .as_object_mut()
                .map(|root| {
                    root.entry("strategy".to_owned())
                        .or_insert_with(|| json!({"list": [], "resetNum": 0, "curCost": 0}))
                })
                .and_then(|strategy| strategy.get_mut("list"))
                .and_then(Value::as_array_mut);
            let Some(list) = list else { return false };
            if let Some(entry) = list
                .iter_mut()
                .find(|entry| json_i32(entry, "id") == Some(strategy_id))
            {
                entry["level"] = json!(level);
            } else {
                list.push(json!({"id": strategy_id, "level": level}));
            }
            true
        }
        "strategy.Reset" => account
            .as_object_mut()
            .map(|root| {
                let strategy = root
                    .entry("strategy".to_owned())
                    .or_insert_with(|| json!({"list": [], "resetNum": 0, "curCost": 0}));
                let Some(strategy) = strategy.as_object_mut() else {
                    return false;
                };
                let list = strategy
                    .entry("list".to_owned())
                    .or_insert_with(|| json!([]));
                let Some(list) = list.as_array_mut() else {
                    return false;
                };
                list.clear();
                let reset_num = strategy
                    .get("resetNum")
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                    .unwrap_or_default()
                    .saturating_add(1);
                strategy.insert("resetNum".to_owned(), json!(reset_num));
                true
            })
            .unwrap_or(false),
        "strategy.Apply" => {
            if strategy_id <= 0 {
                return false;
            }
            let fleet_id = decode_varint_field(args, 3);
            let tactic_type = decode_varint_field(args, 4);
            account
                .get_mut("fleet")
                .and_then(|fleet| fleet.get_mut("tactics"))
                .and_then(Value::as_array_mut)
                .and_then(|tactics| {
                    tactics.iter_mut().find(|tactic| {
                        json_i32(tactic, "modeId") == Some(fleet_id)
                            && json_i32(tactic, "type").unwrap_or(1) == tactic_type
                    })
                })
                .map(|tactic| {
                    tactic["strategyId"] = json!(strategy_id);
                    true
                })
                .unwrap_or(false)
        }
        _ => false,
    }
}

pub(super) fn strategy_info_payload(account: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    if let Some(list) = account
        .get("strategy")
        .and_then(|strategy| strategy.get("list"))
        .and_then(Value::as_array)
    {
        for entry in list {
            let mut encoded = Vec::new();
            append_varint_field(
                &mut encoded,
                1,
                json_i32(entry, "id").unwrap_or_default().max(0) as u64,
            );
            append_varint_field(
                &mut encoded,
                2,
                json_i32(entry, "level").unwrap_or_default().max(0) as u64,
            );
            append_message_field(&mut output, 1, &encoded);
        }
    }
    append_varint_field(
        &mut output,
        2,
        account
            .get("strategy")
            .and_then(|strategy| json_i32(strategy, "curCost"))
            .unwrap_or_default()
            .max(0) as u64,
    );
    append_varint_field(
        &mut output,
        3,
        account
            .get("strategy")
            .and_then(|strategy| json_i32(strategy, "resetNum"))
            .unwrap_or_default()
            .max(0) as u64,
    );
    output
}

pub(super) fn start_support_state(account: &mut Value, args: &[u8], now: u32) -> Option<i32> {
    let support_id = decode_varint_field(args, 1);
    let hero_ids = decode_repeated_varint_field(args, 2);
    if support_id <= 0 || hero_ids.is_empty() || hero_ids.iter().any(|id| *id <= 0) {
        return None;
    }
    let owned = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .map(|heroes| {
            heroes
                .iter()
                .filter_map(|hero| json_i32(hero, "heroId"))
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    if hero_ids.iter().any(|id| !owned.contains(id)) {
        return None;
    }
    let root = account.as_object_mut()?;
    let support = root
        .entry("support".to_owned())
        .or_insert_with(|| json!({"items": []}));
    let items = support.get_mut("items")?.as_array_mut()?;
    let id = items
        .iter()
        .filter_map(|item| json_i32(item, "id"))
        .max()
        .unwrap_or_default()
        .saturating_add(1);
    items.push(json!({
        "id": id,
        "supportId": support_id,
        "startTime": now,
        "heroList": hero_ids
    }));
    Some(id)
}

#[cfg(test)]
pub(super) fn complete_support_state(account: &mut Value, id: i32) -> bool {
    account
        .get_mut("support")
        .and_then(|support| support.get_mut("items"))
        .and_then(Value::as_array_mut)
        .map(|items| {
            let old_len = items.len();
            items.retain(|item| json_i32(item, "id") != Some(id));
            old_len != items.len()
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SupportSettlement {
    pub(super) reward_type: i32,
    pub(super) hero_ids: Vec<u64>,
    pub(super) base_rewards: Vec<(i32, i32, i32)>,
    pub(super) random_rewards: Vec<(i32, i32, i32)>,
}

pub(super) fn settle_support_state(
    account: &mut Value,
    id: i32,
    completion_type: i32,
    now: u32,
    catalog: &SupportCatalog,
) -> Option<SupportSettlement> {
    if id <= 0 || !matches!(completion_type, 1..=3) {
        return None;
    }
    let items = account
        .get("support")
        .and_then(|support| support.get("items"))
        .and_then(Value::as_array)?;
    let item = items.iter().find(|item| json_i32(item, "id") == Some(id))?;
    let support_id = json_i32(item, "supportId")?;
    let hero_ids = item
        .get("heroList")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .filter_map(|id| u64::try_from(id).ok())
        .collect::<Vec<_>>();
    let start_time = json_i64(item, "startTime")?.max(0) as u32;
    let config = catalog.items.get(&support_id).cloned().unwrap_or_default();
    let elapsed = u64::from(now.saturating_sub(start_time));
    if completion_type == 1 && elapsed < config.duration_seconds as u64 {
        return None;
    }
    let big_success = completion_type != 3
        && config.big_success_ratio > 0
        && (u64::from(now).saturating_add(u64::from(id as u32)) % 10_000)
            < u64::try_from(config.big_success_ratio).unwrap_or_default();
    let base_rewards = if big_success && !config.big_success_base_rewards.is_empty() {
        config.big_success_base_rewards.clone()
    } else {
        config.base_rewards.clone()
    };
    let mut random_rewards = Vec::new();
    if completion_type != 3 {
        let drop_id = if big_success {
            config.big_success_extra_drop_id
        } else {
            config.extra_drop_id
        };
        if let Some(reward) = catalog
            .drop_rewards
            .get(&drop_id)
            .and_then(|rewards| rewards.first())
        {
            random_rewards.push(*reward);
        }
    }
    if completion_type == 2 {
        if let Some((goods_type, item_id, amount)) = config.fast_consumption.or(config.consumption)
        {
            if !resource_available(account, goods_type, item_id, amount) {
                return None;
            }
            consume_resource(account, goods_type, item_id, amount);
        }
    }
    let support_items = account
        .get_mut("support")
        .and_then(|support| support.get_mut("items"))
        .and_then(Value::as_array_mut)?;
    let old_len = support_items.len();
    support_items.retain(|entry| json_i32(entry, "id") != Some(id));
    (support_items.len() != old_len).then_some(SupportSettlement {
        reward_type: if completion_type == 3 {
            0
        } else if big_success {
            2
        } else {
            1
        },
        hero_ids,
        base_rewards: if completion_type == 3 {
            Vec::new()
        } else {
            base_rewards
        },
        random_rewards: if completion_type == 3 {
            Vec::new()
        } else {
            random_rewards
        },
    })
}

pub(super) fn support_info_payload(account: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    let Some(items) = account
        .get("support")
        .and_then(|support| support.get("items"))
        .and_then(Value::as_array)
    else {
        return output;
    };
    for item in items {
        let mut encoded = Vec::new();
        append_varint_field(
            &mut encoded,
            1,
            json_i32(item, "id").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            2,
            json_i32(item, "supportId").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            3,
            json_i64(item, "startTime").unwrap_or_default().max(0) as u64,
        );
        for hero_id in json_i32_array(item, "heroList") {
            append_varint_field(&mut encoded, 4, hero_id.max(0) as u64);
        }
        append_message_field(&mut output, 1, &encoded);
    }
    output
}
