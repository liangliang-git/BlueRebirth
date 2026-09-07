#![allow(dead_code)]

use blueoath_protocol::*;
use serde_json::Value;

use super::*;

const ZERO_TRACKED_BAG_ITEMS: &[i32] = &[
    10182, 10185, 10187, // skill books
    10007, 10181, 12201, // construction/draw tickets
    10029, 10030, 10031, // construction resources
];

pub(super) fn json_i64_any(value: &Value) -> i64 {
    value.as_i64().unwrap_or_default()
}

pub(super) fn hero_array<'a>(value: &'a Value, key: &str) -> Option<&'a Vec<Value>> {
    value.get(key).and_then(Value::as_array)
}

pub(super) fn hero_bag_from_account(account: &Value) -> HeroBag {
    let dock = account.get("dock");
    let heroes = dock
        .and_then(|value| value.get("heroes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|hero| {
            let equip_groups = equip_groups_from_hero(hero);
            let equip_effects = hero
                .get("equipEffects")
                .or_else(|| hero.get("equipEffect"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|effect| EquipEffectInfo {
                    effect_type: json_i32(effect, "type")
                        .or_else(|| json_i32(effect, "effectType"))
                        .unwrap_or_default(),
                    effect_ids: effect
                        .get("effectIds")
                        .or_else(|| effect.get("effectId"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_i64)
                        .filter_map(|value| i32::try_from(value).ok())
                        .collect(),
                })
                .collect();
            let combination_info = hero
                .get("combinationInfo")
                .or_else(|| hero.get("CombinationInfo"))
                .map(|info| HeroCombinationInfo {
                    com_lv: json_i32(info, "comLv")
                        .or_else(|| json_i32(info, "ComLv"))
                        .unwrap_or_default(),
                    com_grade: json_i32(info, "comGrade")
                        .or_else(|| json_i32(info, "ComGrade"))
                        .unwrap_or_default(),
                    combine: json_u64(info, "combine")
                        .or_else(|| json_u64(info, "Combine"))
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default(),
                    be_combined: json_u64(info, "beCombined")
                        .or_else(|| json_u64(info, "BeCombined"))
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default(),
                })
                .unwrap_or_default();
            HeroGrid {
                hero_id: json_u64(hero, "heroId")
                    .and_then(|value| u32::try_from(value).ok())
                    .unwrap_or_default(),
                template_id: json_i32(hero, "templateId").unwrap_or_default(),
                level: json_i32(hero, "level").unwrap_or_default(),
                fashioning: json_i32(hero, "fashioning").unwrap_or_default(),
                exp: json_i32(hero, "exp").unwrap_or_default(),
                create_time: json_i32(hero, "createTime").unwrap_or_default(),
                update_time: json_i32(hero, "updateTime").unwrap_or_default(),
                affection: json_i32(hero, "affection").unwrap_or_default(),
                marry_time: json_i32(hero, "marryTime").unwrap_or_default(),
                cur_hp: hero
                    .get("curHp")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
                mood: json_i32(hero, "mood").unwrap_or(MOOD_INITIAL),
                marry_type: json_i32(hero, "marryType").unwrap_or_default(),
                equip_slots: hero
                    .get("equipSlots")
                    .and_then(Value::as_array)
                    .map(|slots| {
                        slots
                            .iter()
                            .filter_map(Value::as_u64)
                            .filter_map(|value| u32::try_from(value).ok())
                            .collect()
                    })
                    .unwrap_or_default(),
                name: json_string(hero, "name").unwrap_or_default(),
                change_name_time: json_i32(hero, "changeNameTime").unwrap_or_default(),
                lock: hero.get("lock").and_then(Value::as_bool).unwrap_or(false),
                advance: json_i32(hero, "advance").unwrap_or_default(),
                adv_lv: json_i32(hero, "advLv").unwrap_or_default(),
                remould_effects: hero
                    .get("remouldEffects")
                    .and_then(Value::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(Value::as_i64)
                            .filter_map(|value| i32::try_from(value).ok())
                            .collect()
                    })
                    .unwrap_or_default(),
                remould_level: json_i32(hero, "remouldLevel").unwrap_or_default(),
                pskills: hero_array(hero, "pSkills")
                    .or_else(|| hero_array(hero, "pskills"))
                    .into_iter()
                    .flatten()
                    .map(|skill| PSkillEntry {
                        pskill_id: json_u64(skill, "pSkillId")
                            .or_else(|| json_u64(skill, "pskillId"))
                            .and_then(|value| u32::try_from(value).ok())
                            .unwrap_or_default(),
                        pskill_exp: json_u64(skill, "pSkillExp")
                            .or_else(|| json_u64(skill, "pskillExp"))
                            .and_then(|value| u32::try_from(value).ok())
                            .unwrap_or_default(),
                        level: json_i32(skill, "level")
                            .or_else(|| json_i32(skill, "pSkillLv"))
                            .or_else(|| json_i32(skill, "pskillLv"))
                            .unwrap_or(1),
                        replace: json_i32(skill, "replace").unwrap_or_default(),
                    })
                    .collect(),
                intensify: hero_array(hero, "intensify")
                    .into_iter()
                    .flatten()
                    .map(|attr| AttrIntensify {
                        attr_type: json_i32(attr, "attrType").unwrap_or_default(),
                        intensify_level: json_i32(attr, "intensifyLvl")
                            .or_else(|| json_i32(attr, "intensifyLevel"))
                            .unwrap_or_default(),
                        cur_exp: json_i32(attr, "curExp").unwrap_or_default(),
                    })
                    .collect(),
                equip_groups,
                equip_effects,
                combination_info,
            }
        })
        .collect();
    HeroBag {
        heroes,
        bag_size: dock
            .and_then(|value| json_i32(value, "bagSize"))
            .unwrap_or_default(),
    }
}

pub(super) fn hero_bag_from_typed_account(account: &blueoath_domain::AccountState) -> HeroBag {
    let heroes = account
        .dock
        .heroes
        .values()
        .map(|hero| HeroGrid {
            hero_id: u32::try_from(hero.id.get()).unwrap_or(u32::MAX),
            template_id: i32::try_from(hero.template_id.get()).unwrap_or(i32::MAX),
            marry_time: i32::try_from(
                account
                    .activities
                    .progress
                    .get(&format!("compat:hero:{}:marryTime", hero.id.get()))
                    .copied()
                    .unwrap_or_default(),
            )
            .unwrap_or(i32::MAX),
            marry_type: i32::try_from(
                account
                    .activities
                    .progress
                    .get(&format!("compat:hero:{}:marryType", hero.id.get()))
                    .copied()
                    .unwrap_or_default(),
            )
            .unwrap_or(i32::MAX),
            name: hero.name.clone(),
            change_name_time: i32::try_from(hero.change_name_time).unwrap_or(i32::MAX),
            level: i32::try_from(hero.level).unwrap_or(i32::MAX),
            exp: i32::try_from(hero.exp).unwrap_or(i32::MAX),
            affection: i32::try_from(hero.affection).unwrap_or(i32::MAX),
            cur_hp: i64::try_from(hero.hp).unwrap_or(i64::MAX),
            mood: i32::try_from(hero.mood).unwrap_or(i32::MAX),
            equip_slots: hero
                .equip_slots
                .iter()
                .map(|slot| {
                    slot.map(|id| u32::try_from(id.get()).unwrap_or(u32::MAX))
                        .unwrap_or(0)
                })
                .collect(),
            pskills: hero
                .pskills
                .iter()
                .map(|(skill_id, level)| PSkillEntry {
                    pskill_id: u32::try_from(*skill_id).unwrap_or(u32::MAX),
                    pskill_exp: 0,
                    level: i32::try_from(*level).unwrap_or(i32::MAX),
                    replace: 0,
                })
                .collect(),
            combination_info: HeroCombinationInfo {
                com_lv: i32::try_from(
                    account
                        .activities
                        .progress
                        .get(&format!("compat:hero:{}:combination:level", hero.id.get()))
                        .copied()
                        .unwrap_or_default(),
                )
                .unwrap_or(i32::MAX),
                com_grade: i32::try_from(
                    account
                        .activities
                        .progress
                        .get(&format!("compat:hero:{}:combination:grade", hero.id.get()))
                        .copied()
                        .unwrap_or_default(),
                )
                .unwrap_or(i32::MAX),
                combine: u32::try_from(
                    account
                        .activities
                        .progress
                        .get(&format!(
                            "compat:hero:{}:combination:combine",
                            hero.id.get()
                        ))
                        .copied()
                        .unwrap_or_default(),
                )
                .unwrap_or(u32::MAX),
                be_combined: u32::try_from(
                    account
                        .activities
                        .progress
                        .get(&format!(
                            "compat:hero:{}:combination:beCombined",
                            hero.id.get()
                        ))
                        .copied()
                        .unwrap_or_default(),
                )
                .unwrap_or(u32::MAX),
            },
            lock: hero.locked,
            ..HeroGrid::default()
        })
        .collect();
    HeroBag {
        heroes,
        bag_size: 200,
    }
}

fn equip_groups_from_hero(hero: &Value) -> Vec<HeroEquipGroup> {
    let normal_ids = hero
        .get("equipSlots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let normal_states = hero
        .get("equipStates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let group = |equip_type: i32, ids: &[Value], states: &[Value]| HeroEquipGroup {
        equip_type,
        slots: (0..6)
            .map(|index| HeroEquipSlot {
                equip_id: ids
                    .get(index)
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .unwrap_or_default(),
                state: states
                    .get(index)
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                    .unwrap_or_default(),
            })
            .collect(),
    };
    let mut groups = vec![group(1, &normal_ids, &normal_states)];
    let slot_map = hero.get("equipSlotsByType").and_then(Value::as_object);
    let state_map = hero.get("equipStatesByType").and_then(Value::as_object);
    if let Some(slot_map) = slot_map {
        let mut types = slot_map
            .keys()
            .filter_map(|key| key.parse::<i32>().ok())
            .filter(|value| *value > 1)
            .collect::<Vec<_>>();
        types.sort_unstable();
        for equip_type in types {
            let key = equip_type.to_string();
            let ids = slot_map
                .get(&key)
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let states = state_map
                .and_then(|map| map.get(&key))
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            groups.push(group(equip_type, ids, states));
        }
    }
    groups
}

pub(super) fn bag_info_from_account(account: &Value) -> BagInfo {
    // Keep zero-count rows for consumables whose client widgets cache the last
    // value.  This also repairs snapshots created before zero tombstones were
    // persisted: client receives explicit deletion markers on next refresh.
    let bag = account.get("bag");
    let mut items = bag
        .and_then(|value| value.get("items"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| BagGrid {
            template_id: json_i32(item, "templateId").unwrap_or_default(),
            num: json_i32(item, "num").unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    for template_id in ZERO_TRACKED_BAG_ITEMS {
        if !items.iter().any(|item| item.template_id == *template_id) {
            items.push(BagGrid {
                template_id: *template_id,
                num: 0,
            });
        }
    }
    BagInfo {
        bag_type: 1,
        bag_size: bag
            .and_then(|value| json_i32(value, "bagSize"))
            .unwrap_or(100),
        items,
    }
}

pub(super) fn bag_info_from_typed_account(account: &blueoath_domain::AccountState) -> BagInfo {
    let mut items = account
        .inventory
        .items
        .iter()
        .filter_map(|(template_id, amount)| {
            Some(BagGrid {
                template_id: i32::try_from(template_id.get()).ok()?,
                num: i32::try_from(*amount).unwrap_or(i32::MAX),
            })
        })
        .collect::<Vec<_>>();
    for template_id in ZERO_TRACKED_BAG_ITEMS {
        if !items.iter().any(|item| item.template_id == *template_id) {
            items.push(BagGrid {
                template_id: *template_id,
                num: 0,
            });
        }
    }
    BagInfo {
        bag_type: 1,
        bag_size: 100,
        items,
    }
}

pub(super) fn fashion_list_from_account(
    account: &Value,
    catalog: Option<&FashionList>,
) -> FashionList {
    let fashion = account.get("fashion");
    let stored = fashion
        .and_then(|value| value.get("entries"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|entry| FashionInfo {
            sf_id: json_i32(entry, "sfId").unwrap_or_default(),
            fashion_tids: entry
                .get("fashionTids")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_i64)
                        .filter_map(|value| i32::try_from(value).ok())
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    let Some(catalog) = catalog else {
        return FashionList { items: stored };
    };
    let mut items = catalog.items.clone();
    for entry in stored {
        if let Some(existing) = items.iter_mut().find(|item| item.sf_id == entry.sf_id) {
            existing.fashion_tids.extend(entry.fashion_tids);
            existing.fashion_tids.sort_unstable();
            existing.fashion_tids.dedup();
        } else {
            items.push(entry);
        }
    }
    items.sort_unstable_by_key(|item| item.sf_id);
    FashionList { items }
}

pub(super) fn fashion_list_from_typed_account(
    account: &blueoath_domain::AccountState,
    catalog: Option<&FashionList>,
) -> FashionList {
    let stored = account
        .fashion
        .entries
        .iter()
        .map(|(sf_id, fashion_tids)| FashionInfo {
            sf_id: i32::try_from(*sf_id).unwrap_or(i32::MAX),
            fashion_tids: fashion_tids
                .iter()
                .map(|fashion_tid| i32::try_from(fashion_tid.get()).unwrap_or(i32::MAX))
                .collect(),
        })
        .collect::<Vec<_>>();
    let Some(catalog) = catalog else {
        return FashionList { items: stored };
    };
    let mut items = catalog.items.clone();
    for entry in stored {
        if let Some(existing) = items.iter_mut().find(|item| item.sf_id == entry.sf_id) {
            existing.fashion_tids.extend(entry.fashion_tids);
            existing.fashion_tids.sort_unstable();
            existing.fashion_tids.dedup();
        } else {
            items.push(entry);
        }
    }
    items.sort_unstable_by_key(|item| item.sf_id);
    FashionList { items }
}

pub(super) fn equip_list_from_account(
    account: &Value,
    catalog: Option<&EquipCatalog>,
) -> EquipList {
    let equip = account.get("equip");
    let items = equip
        .and_then(|value| value.get("items"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| {
            let template_id = json_i32(item, "templateId").unwrap_or_default();
            let star = json_i32(item, "star").unwrap_or_default();
            let stored_pskills = item
                .get("pSkills")
                .or_else(|| item.get("pskills"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|skill| EquipPSkill {
                    pskill_id: json_i32(skill, "pSkillId")
                        .or_else(|| json_i32(skill, "pskillId"))
                        .unwrap_or_default(),
                    level: json_i32(skill, "pSkillLv")
                        .or_else(|| json_i32(skill, "pskillLv"))
                        .unwrap_or_default(),
                })
                .collect::<Vec<_>>();
            let pskills = if stored_pskills.is_empty() && star > 0 {
                catalog
                    .and_then(|value| value.skills_by_template.get(&template_id))
                    .map(|skills| {
                        skills
                            .iter()
                            .map(|(skill_id, max_level)| EquipPSkill {
                                pskill_id: *skill_id,
                                level: star.min(*max_level),
                            })
                            .collect()
                    })
                    .unwrap_or(stored_pskills)
            } else {
                stored_pskills
            };
            EquipInfo {
                equip_id: json_u64(item, "equipId")
                    .and_then(|value| u32::try_from(value).ok())
                    .unwrap_or_default(),
                template_id,
                enhance_level: json_i32(item, "enhanceLv").unwrap_or_default(),
                star: json_i32(item, "star").unwrap_or_default(),
                hero_id: json_u64(item, "heroId")
                    .and_then(|value| u32::try_from(value).ok())
                    .unwrap_or_default(),
                enhance_exp: json_i32(item, "enhanceExp").unwrap_or_default(),
                pskills,
                rise_common_equips: item
                    .get("riseCommonEquips")
                    .or_else(|| item.get("rise_common_equips"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|value| EquipNum {
                        template_id: json_i32(value, "templateId")
                            .or_else(|| json_i32(value, "template_id"))
                            .unwrap_or_default(),
                        num: json_i32(value, "num").unwrap_or_default(),
                    })
                    .collect(),
            }
        })
        .collect();
    // EquipService's one-click selector reads TEquipList.EquipNum (field 3),
    // not bag.GetBagInfo. Expose enhancement consumables here so the client
    // can calculate/select materials before sending equip.Enhance.
    let mut nums_by_template = std::collections::BTreeMap::<i32, i32>::new();
    for item in account
        .get("bag")
        .and_then(|bag| bag.get("items"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(template_id) = json_i32(item, "templateId") else {
            continue;
        };
        let num = json_i32(item, "num").unwrap_or_default().max(0);
        if catalog
            .and_then(|value| value.enhance_materials.get(&template_id))
            .is_some()
        {
            nums_by_template.insert(template_id, num);
        }
    }
    // Include configured enhancement materials absent from old snapshots as
    // zero tombstones, clearing stale one-click selector counts client-side.
    if let Some(catalog) = catalog {
        for template_id in catalog.enhance_materials.keys() {
            nums_by_template.entry(*template_id).or_insert(0);
        }
    }
    let nums = nums_by_template
        .into_iter()
        .map(|(template_id, num)| EquipNum { template_id, num })
        .collect();
    EquipList {
        bag_size: equip
            .and_then(|value| json_i32(value, "equipBagSize"))
            .unwrap_or(2000),
        items,
        nums,
    }
}

pub(super) fn equip_list_from_typed_account(account: &blueoath_domain::AccountState) -> EquipList {
    let items = account
        .dock
        .equipments
        .values()
        .map(equip_info_from_typed_equipment)
        .collect();
    EquipList {
        bag_size: 2000,
        items,
        nums: Vec::new(),
    }
}

pub(super) fn equip_info_from_typed_equipment(
    equipment: &blueoath_domain::EquipmentState,
) -> EquipInfo {
    EquipInfo {
        equip_id: u32::try_from(equipment.id.get()).unwrap_or(u32::MAX),
        template_id: i32::try_from(equipment.template_id.get()).unwrap_or(i32::MAX),
        enhance_level: i32::try_from(equipment.enhance_level).unwrap_or(i32::MAX),
        star: i32::try_from(equipment.star).unwrap_or(i32::MAX),
        hero_id: equipment
            .hero_id
            .map(|id| u32::try_from(id.get()).unwrap_or(u32::MAX))
            .unwrap_or_default(),
        enhance_exp: i32::try_from(equipment.enhance_exp).unwrap_or(i32::MAX),
        ..EquipInfo::default()
    }
}

pub(super) fn building_info_from_account(account: &Value, now: u32) -> UserBuildingInfo {
    let building = account.get("building");
    let buildings: Vec<BuildingInfo> = building
        .and_then(|value| value.get("buildings"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|value| {
            let building_id = json_i32(value, "id").unwrap_or_default();
            let last_update_time = json_i64(value, "lastUpdateTime")
                .or_else(|| json_i64(value, "last_update_time"))
                .unwrap_or(i64::from(now));
            let tactic_list = value
                .get("tacticList")
                .or_else(|| value.get("tactic_list"))
                .and_then(Value::as_array)
                .map(|tactics| {
                    tactics
                        .iter()
                        .map(|tactic| BuildingTactic {
                            building_id: json_i32(tactic, "buildingId").unwrap_or(building_id),
                            name: json_string(tactic, "name")
                                .or_else(|| json_string(tactic, "tacticName"))
                                .unwrap_or_default(),
                            hero_ids: json_i32_array(tactic, "heroIds")
                                .into_iter()
                                .filter_map(|id| u32::try_from(id).ok())
                                .collect(),
                            index: json_i32(tactic, "index").unwrap_or_default(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            BuildingInfo {
                id: building_id,
                template_id: json_i32(value, "tid")
                    .or_else(|| json_i32(value, "templateId"))
                    .unwrap_or_default(),
                level: json_i32(value, "level").unwrap_or_default(),
                hero_ids: value
                    .get("heroIds")
                    .or_else(|| value.get("hero_ids"))
                    .and_then(Value::as_array)
                    .map(|ids| {
                        ids.iter()
                            .filter_map(Value::as_u64)
                            .filter_map(|id| u32::try_from(id).ok())
                            .collect()
                    })
                    .unwrap_or_default(),
                productivity: json_i32(value, "productivity").unwrap_or_default(),
                produce_speed: json_i32(value, "produceSpeed")
                    .or_else(|| json_i32(value, "produce_speed"))
                    .unwrap_or_default(),
                product_count: json_i32(value, "productCount")
                    .or_else(|| json_i32(value, "product_count"))
                    .unwrap_or_default(),
                status: json_i32(value, "status").unwrap_or(1),
                last_update_time,
                recipe_id: json_i32(value, "recipeId")
                    .or_else(|| json_i32(value, "recipe_id"))
                    .unwrap_or_default(),
                item_count: json_i32(value, "itemCount")
                    .or_else(|| json_i32(value, "item_count"))
                    .unwrap_or_default(),
                last_mood_update_time: json_i64(value, "lastMoodUpdateTime")
                    .or_else(|| json_i64(value, "last_mood_update_time"))
                    .unwrap_or(last_update_time),
                last_build_update_time: json_i64(value, "lastBuildUpdateTime")
                    .or_else(|| json_i64(value, "last_build_update_time"))
                    .unwrap_or_default(),
                recipe_time: json_i32(value, "recipeTime")
                    .or_else(|| json_i32(value, "recipe_time"))
                    .unwrap_or_default(),
                float_count: json_i32(value, "floatCount")
                    .or_else(|| json_i32(value, "float_count"))
                    .unwrap_or_default(),
                tactic_list,
            }
        })
        .collect();
    let lands: Vec<BuildingLandInfo> = building
        .and_then(|value| value.get("lands"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|value| BuildingLandInfo {
            index: json_i32(value, "index").unwrap_or_default(),
            building_id: json_i32(value, "buildingId").unwrap_or_default(),
        })
        .collect();
    if building.is_none() || buildings.is_empty() || lands.is_empty() {
        return default_building_info(now);
    }
    let mut buildings = buildings;
    buildings.sort_by_key(|value| value.id);
    let mut lands = lands;
    lands.sort_by_key(|value| value.index);
    let office_level = buildings
        .iter()
        .find(|value| (1..=5).contains(&value.template_id))
        .map(|value| value.level.max(1))
        .unwrap_or(1);
    let default_worker_strength = (100_i32
        .saturating_add(50_i32.saturating_mul(office_level.saturating_sub(1))))
    .saturating_mul(10_000);
    UserBuildingInfo {
        buildings,
        lands,
        worker_strength: default_worker_strength,
        worker_recover: building
            .and_then(|value| json_i32(value, "workerRecover"))
            .or_else(|| building.and_then(|value| json_i32(value, "worker_recover")))
            .unwrap_or(10),
        food_max: building
            .and_then(|value| json_i32(value, "foodMax"))
            .or_else(|| building.and_then(|value| json_i32(value, "food_max")))
            .unwrap_or(100),
        electric_max: building
            .and_then(|value| json_i32(value, "electricMax"))
            .or_else(|| building.and_then(|value| json_i32(value, "electric_max")))
            .unwrap_or(100),
        worker_update_time: i64::from(now),
    }
}

pub(super) fn default_building_info(now: u32) -> UserBuildingInfo {
    UserBuildingInfo {
        buildings: vec![
            BuildingInfo {
                id: 1,
                template_id: 2,
                level: 2,
                status: 1,
                last_update_time: i64::from(now),
                last_build_update_time: i64::from(now),
                ..BuildingInfo::default()
            },
            BuildingInfo {
                id: 2,
                template_id: 41,
                level: 1,
                status: 1,
                last_update_time: i64::from(now),
                last_build_update_time: i64::from(now),
                ..BuildingInfo::default()
            },
        ],
        lands: vec![
            BuildingLandInfo {
                index: 1,
                building_id: 1,
            },
            BuildingLandInfo {
                index: 6,
                building_id: 2,
            },
        ],
        worker_strength: 1_500_000,
        worker_recover: 10,
        food_max: 100,
        electric_max: 100,
        worker_update_time: i64::from(now),
    }
}

pub(super) fn building_info_from_typed_account(
    account: &blueoath_domain::AccountState,
    now: u32,
) -> UserBuildingInfo {
    if account.buildings.levels.is_empty() {
        return default_building_info(now);
    }
    let buildings = account
        .buildings
        .levels
        .iter()
        .filter_map(|(building_id, level)| {
            let production = account.buildings.productions.get(building_id);
            Some(BuildingInfo {
                id: i32::try_from(*building_id).ok()?,
                template_id: i32::try_from(
                    account
                        .buildings
                        .template_ids
                        .get(building_id)
                        .copied()
                        .unwrap_or(*building_id),
                )
                .ok()?,
                level: i32::try_from(*level).ok()?,
                hero_ids: account
                    .buildings
                    .hero_assignments
                    .get(building_id)
                    .into_iter()
                    .flatten()
                    .filter_map(|hero_id| u32::try_from(hero_id.get()).ok())
                    .collect(),
                productivity: production
                    .and_then(|value| i32::try_from(value.productivity).ok())
                    .unwrap_or_default(),
                produce_speed: production
                    .and_then(|value| i32::try_from(value.produce_speed).ok())
                    .unwrap_or_default(),
                product_count: production
                    .and_then(|value| i32::try_from(value.product_count).ok())
                    .unwrap_or_default(),
                status: production
                    .and_then(|value| i32::try_from(value.status).ok())
                    .unwrap_or(1),
                last_update_time: production
                    .and_then(|value| i64::try_from(value.last_update_at).ok())
                    .filter(|value| *value > 0)
                    .unwrap_or(i64::from(now)),
                recipe_id: production
                    .and_then(|value| i32::try_from(value.recipe_id).ok())
                    .unwrap_or_default(),
                item_count: production
                    .and_then(|value| i32::try_from(value.item_count).ok())
                    .unwrap_or_default(),
                recipe_time: production
                    .and_then(|value| i32::try_from(value.recipe_time).ok())
                    .unwrap_or_default(),
                last_build_update_time: i64::from(now),
                ..BuildingInfo::default()
            })
        })
        .collect::<Vec<_>>();
    if buildings.is_empty() {
        return default_building_info(now);
    }
    let lands = buildings
        .iter()
        .enumerate()
        .map(|(index, building)| BuildingLandInfo {
            index: account
                .buildings
                .land_indices
                .get(&(building.id as u64))
                .copied()
                .and_then(|value| i32::try_from(value).ok())
                .unwrap_or_else(|| i32::try_from(index + 1).unwrap_or(i32::MAX)),
            building_id: building.id,
        })
        .collect();
    UserBuildingInfo {
        buildings,
        lands,
        worker_strength: 1_500_000,
        worker_recover: 10,
        food_max: 100,
        electric_max: 100,
        worker_update_time: i64::from(now),
    }
}

pub(super) fn fleet_info_from_account(account: &Value) -> FleetInfo {
    let fleet = account.get("fleet");
    let tactics: Vec<FleetTactic> = fleet
        .and_then(|value| value.get("tactics"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|value| FleetTactic {
            tactic_name: json_string(value, "tacticName").unwrap_or_default(),
            hero_ids: json_i32_array(value, "heroInfo")
                .into_iter()
                .chain(json_i32_array(value, "heroIds"))
                .collect(),
            mode_id: json_i32(value, "modeId").unwrap_or_default(),
            strategy_id: json_i32(value, "strategyId").unwrap_or_default(),
            formation_id: json_i32(value, "formationId").unwrap_or(2),
            tactic_type: json_i32(value, "type").unwrap_or(1),
            ex_hero_ids: json_i32_array(value, "exHeroInfo")
                .into_iter()
                .chain(json_i32_array(value, "exHeroIds"))
                .collect(),
        })
        .collect();
    if tactics.is_empty() {
        return default_fleet_info();
    }
    FleetInfo {
        tactics,
        max_power: fleet
            .and_then(|value| json_i32(value, "maxPower"))
            .or_else(|| fleet.and_then(|value| json_i32(value, "max_power")))
            .unwrap_or_default(),
        min_power: fleet
            .and_then(|value| json_i32(value, "minPower"))
            .or_else(|| fleet.and_then(|value| json_i32(value, "min_power")))
            .unwrap_or_default(),
    }
}

pub(super) fn fleet_info_from_typed_account(account: &blueoath_domain::AccountState) -> FleetInfo {
    let tactics = account
        .fleet
        .fleets
        .iter()
        .filter_map(|(fleet_id, fleet)| {
            Some(FleetTactic {
                tactic_name: String::new(),
                hero_ids: fleet
                    .members
                    .iter()
                    .filter_map(|hero_id| i32::try_from(hero_id.get()).ok())
                    .collect(),
                mode_id: i32::try_from(fleet_id.get()).ok()?,
                strategy_id: i32::try_from(fleet.tactic_id).ok()?,
                formation_id: i32::try_from(fleet.formation_id).ok()?,
                tactic_type: 1,
                ex_hero_ids: Vec::new(),
            })
        })
        .collect::<Vec<_>>();
    if tactics.is_empty() {
        return default_fleet_info();
    }
    FleetInfo {
        tactics,
        ..FleetInfo::default()
    }
}

pub(super) fn set_fleet_on_typed_account(
    account: &mut blueoath_domain::AccountState,
    fleet: &FleetInfo,
) -> bool {
    let mut fleets = std::collections::BTreeMap::new();
    for tactic in &fleet.tactics {
        let Ok(raw_fleet_id) = u64::try_from(tactic.mode_id) else {
            return false;
        };
        let Ok(fleet_id) = blueoath_domain::FleetId::new(raw_fleet_id) else {
            return false;
        };
        let Ok(formation_id) = u32::try_from(tactic.formation_id) else {
            return false;
        };
        let Ok(tactic_id) = u32::try_from(tactic.strategy_id) else {
            return false;
        };
        let mut members = Vec::new();
        for hero_id in &tactic.hero_ids {
            let Ok(hero_id) = u64::try_from(*hero_id) else {
                return false;
            };
            let Ok(hero_id) = blueoath_domain::HeroId::new(hero_id) else {
                return false;
            };
            if !account.dock.heroes.contains_key(&hero_id) {
                return false;
            }
            members.push(hero_id);
        }
        if fleets
            .insert(
                fleet_id,
                blueoath_domain::FleetRecord {
                    formation_id,
                    tactic_id,
                    members,
                },
            )
            .is_some()
        {
            return false;
        }
    }
    account.fleet.fleets = fleets;
    true
}

#[cfg(test)]
pub(super) fn set_fleet_from_account(account: &mut Value, fleet: &FleetInfo) {
    let tactics = fleet
        .tactics
        .iter()
        .map(|tactic| {
            json!({
                "modeId": tactic.mode_id,
                "type": tactic.tactic_type,
                "tacticName": tactic.tactic_name,
                "heroInfo": tactic.hero_ids,
                "exHeroInfo": tactic.ex_hero_ids,
                "strategyId": tactic.strategy_id,
                "formationId": tactic.formation_id
            })
        })
        .collect::<Vec<_>>();
    let Some(root) = account.as_object_mut() else {
        return;
    };
    let fleet_value = root.entry("fleet".to_owned()).or_insert_with(|| json!({}));
    let Some(fleet_object) = fleet_value.as_object_mut() else {
        return;
    };
    fleet_object.insert("tactics".to_owned(), Value::Array(tactics));
    fleet_object.insert("maxPower".to_owned(), json!(fleet.max_power));
    fleet_object.insert("minPower".to_owned(), json!(fleet.min_power));
}

pub(super) fn default_fleet_info() -> FleetInfo {
    FleetInfo {
        tactics: (1..=5)
            .map(|mode_id| FleetTactic {
                mode_id,
                formation_id: 2,
                tactic_type: 1,
                ..FleetTactic::default()
            })
            .collect(),
        ..FleetInfo::default()
    }
}

#[cfg(test)]
pub(super) fn copy_record_list_from_account(account: &Value, copy_id: i32) -> CopyRecordList {
    let character = account.get("character").unwrap_or(account);
    let uid = json_i64(character, "uid").unwrap_or_default().max(0) as u64;
    let user_name = json_string(character, "name").unwrap_or_default();
    let level = json_i32(character, "level").unwrap_or_default();
    let mut stored_records = Vec::new();
    if let Some(records) = account.get("copyRecords").and_then(Value::as_array) {
        stored_records.extend(records.iter());
    }
    if let Some(records) = account
        .get("dailyCopy")
        .and_then(|daily| daily.get("records"))
        .and_then(Value::as_array)
    {
        stored_records.extend(records.iter());
    }
    let records = stored_records
        .into_iter()
        .filter(|record| json_i32(record, "copyId") == Some(copy_id))
        .map(|record| {
            let hero_ids = json_i32_array(record, "heroIds");
            let fallback_tactic = fleet_info_from_account(account)
                .tactics
                .first()
                .cloned()
                .unwrap_or_default();
            let hero_ids = if hero_ids.is_empty() {
                fallback_tactic.hero_ids
            } else {
                hero_ids
            };
            let tactics = hero_ids
                .into_iter()
                .filter_map(|hero_id| {
                    account
                        .get("dock")
                        .and_then(|dock| dock.get("heroes"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .find(|hero| json_i32(hero, "heroId") == Some(hero_id))
                        .map(|hero| CopyRecordHero {
                            template_id: json_i32(hero, "templateId").unwrap_or_default(),
                            level: json_i32(hero, "level").unwrap_or_default(),
                            advance_level: json_i32(hero, "advLv").unwrap_or_default(),
                            cur_hp: hero
                                .get("curHp")
                                .and_then(Value::as_u64)
                                .unwrap_or_default(),
                            equips: hero
                                .get("equipSlots")
                                .and_then(Value::as_array)
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_i64)
                                .filter_map(|equip_id| {
                                    account
                                        .get("equip")
                                        .and_then(|equip| equip.get("items"))
                                        .and_then(Value::as_array)
                                        .into_iter()
                                        .flatten()
                                        .find(|equip| json_i64(equip, "equipId") == Some(equip_id))
                                        .map(|equip| CopyRecordEquip {
                                            template_id: json_i32(equip, "templateId")
                                                .unwrap_or_default(),
                                            level: json_i32(equip, "enhanceLv").unwrap_or_default(),
                                            star_level: json_i32(equip, "star").unwrap_or_default(),
                                        })
                                })
                                .collect(),
                            point: 0,
                        })
                })
                .collect();
            CopyRecord {
                uid,
                user_name: user_name.clone(),
                level,
                pass_time: json_i32(record, "passTime").unwrap_or_default(),
                secret_id: json_i32(record, "secretId").unwrap_or_default(),
                strategy_id: json_i32(record, "strategyId").unwrap_or_default(),
                tactics,
                power: json_i32(record, "power").unwrap_or_default(),
                record_time: json_i32(record, "recTime").unwrap_or_default(),
                ex_buff: json_i32_array(record, "exBuff"),
            }
        })
        .collect();
    CopyRecordList { copy_id, records }
}

pub(super) fn copy_record_list_from_typed_account(
    account: &blueoath_domain::AccountState,
    copy_id: i32,
) -> CopyRecordList {
    let records = account
        .battle
        .records
        .iter()
        .filter(|record| i32::try_from(record.copy_id.get()).ok() == Some(copy_id))
        .map(|record| CopyRecord {
            uid: account.character.uid,
            user_name: account.character.name.clone(),
            level: i32::try_from(account.character.level).unwrap_or(i32::MAX),
            pass_time: i32::try_from(record.pass_time).unwrap_or(i32::MAX),
            secret_id: i32::try_from(record.secret_id).unwrap_or(i32::MAX),
            strategy_id: i32::try_from(record.strategy_id).unwrap_or(i32::MAX),
            tactics: record
                .hero_ids
                .iter()
                .filter_map(|hero_id| account.dock.heroes.get(hero_id))
                .map(|hero| CopyRecordHero {
                    template_id: i32::try_from(hero.template_id.get()).unwrap_or(i32::MAX),
                    level: i32::try_from(hero.level).unwrap_or(i32::MAX),
                    advance_level: 0,
                    cur_hp: hero.hp,
                    equips: hero
                        .equip_slots
                        .iter()
                        .flatten()
                        .filter_map(|equip_id| account.dock.equipments.get(equip_id))
                        .map(|equip| CopyRecordEquip {
                            template_id: i32::try_from(equip.template_id.get()).unwrap_or(i32::MAX),
                            level: i32::try_from(equip.enhance_level).unwrap_or(i32::MAX),
                            star_level: i32::try_from(equip.star).unwrap_or(i32::MAX),
                        })
                        .collect(),
                    point: 0,
                })
                .collect(),
            power: i32::try_from(record.power).unwrap_or(i32::MAX),
            record_time: i32::try_from(record.record_time).unwrap_or(i32::MAX),
            ex_buff: record
                .ex_buffs
                .iter()
                .filter_map(|buff| i32::try_from(*buff).ok())
                .collect(),
        })
        .collect();
    CopyRecordList { copy_id, records }
}

pub(super) fn copy_info_response_from_typed_account(
    account: &blueoath_domain::AccountState,
    copy_id: i32,
) -> CopyInfoResponse {
    let records = copy_record_list_from_typed_account(account, copy_id).records;
    let first = records.first().cloned();
    let fast = records
        .iter()
        .min_by_key(|record| record.pass_time)
        .cloned();
    CopyInfoResponse {
        max_ex_star: 0,
        max_ex_star_first: None,
        max_ex_star_fast: None,
        first,
        fast,
        atk_grad: None,
    }
}

#[cfg(test)]
pub(super) fn copy_info_response_from_account(account: &Value, copy_id: i32) -> CopyInfoResponse {
    let records = copy_record_list_from_account(account, copy_id).records;
    let first = records.first().cloned();
    let fast = records
        .iter()
        .min_by_key(|record| record.pass_time)
        .cloned();
    let max_ex_star = account
        .get("dailyCopy")
        .and_then(|daily| daily.get("chapters"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|chapter| json_i32(chapter, "exStar"))
        .max()
        .unwrap_or_default()
        .max(0);
    CopyInfoResponse {
        max_ex_star,
        max_ex_star_first: first.clone(),
        max_ex_star_fast: fast.clone(),
        first,
        fast,
        atk_grad: None,
    }
}

#[cfg(test)]
pub(super) fn delete_copy_record(account: &mut Value, copy_id: i32, index: i32) -> bool {
    let Some(records) = account.get_mut("copyRecords").and_then(Value::as_array_mut) else {
        return false;
    };
    let matching = records
        .iter()
        .enumerate()
        .filter(|(_, record)| json_i32(record, "copyId") == Some(copy_id))
        .map(|(position, _)| position)
        .collect::<Vec<_>>();
    let Some(position) = matching.get(usize::try_from(index.max(0)).unwrap_or_default()) else {
        return false;
    };
    records.remove(*position);
    true
}

#[cfg(test)]
pub(super) fn apply_copy_record_to_fleet(
    account: &mut Value,
    copy_id: i32,
    index: i32,
) -> Option<FleetInfo> {
    let record = account
        .get("copyRecords")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|record| json_i32(record, "copyId") == Some(copy_id))
        .nth(usize::try_from(index.max(0)).ok()?)?;
    let template_ids = record
        .get("templateIds")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
        .unwrap_or_default();
    let hero_ids = record
        .get("heroIds")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut fleet = fleet_info_from_account(account);
    let tactic = fleet.tactics.first_mut()?;
    let mut selected = if !hero_ids.is_empty() {
        hero_ids
            .into_iter()
            .filter_map(|id| i32::try_from(id).ok())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if selected.is_empty() && !template_ids.is_empty() {
        let heroes = account
            .get("dock")
            .and_then(|dock| dock.get("heroes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for template_id in template_ids {
            if let Some(hero_id) = heroes.iter().find_map(|hero| {
                (json_i64(hero, "templateId") == Some(template_id))
                    .then(|| json_i32(hero, "heroId"))
                    .flatten()
            }) {
                if !selected.contains(&hero_id) {
                    selected.push(hero_id);
                }
            }
        }
    }
    if selected.is_empty() {
        return None;
    }
    tactic.hero_ids = selected;
    if let Some(strategy_id) = json_i32(record, "strategyId") {
        tactic.strategy_id = strategy_id;
    }
    set_fleet_from_account(account, &fleet);
    Some(fleet)
}

pub(super) fn preset_fleet_info_from_account(account: &Value) -> PresetFleetInfo {
    let preset = account
        .get("presetFleet")
        .or_else(|| account.get("presetfleet"));
    let fleets = preset
        .and_then(|value| {
            value
                .get("presetfleet")
                .or_else(|| value.get("fleets"))
                .or_else(|| value.get("presetFleets"))
        })
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|value| PresetFleet {
            name: json_string(value, "Name")
                .or_else(|| json_string(value, "name"))
                .unwrap_or_default(),
            hero_ids: json_i32_array(value, "heroList")
                .into_iter()
                .chain(json_i32_array(value, "heroIds"))
                .collect(),
            ex_hero_ids: json_i32_array(value, "exHeroList")
                .into_iter()
                .chain(json_i32_array(value, "exHeroIds"))
                .collect(),
            mode_id: json_i32(value, "modeId").unwrap_or_default(),
            strategy_id: json_i32(value, "strategyId").unwrap_or_default(),
        })
        .collect();
    PresetFleetInfo {
        fleets,
        name_num: preset
            .and_then(|value| json_i32(value, "NameNum"))
            .or_else(|| preset.and_then(|value| json_i32(value, "nameNum")))
            .unwrap_or_default(),
        red_dot: preset
            .and_then(|value| json_i32(value, "redDot"))
            .unwrap_or_default(),
    }
}

#[cfg(test)]
pub(super) fn set_preset_fleet_from_account(account: &mut Value, value: &PresetFleetInfo) {
    let Some(root) = account.as_object_mut() else {
        return;
    };
    root.insert(
        "presetFleet".to_owned(),
        json!({
            "presetfleet": value
                .fleets
                .iter()
                .map(|fleet| json!({
                    "Name": fleet.name,
                    "heroList": fleet.hero_ids,
                    "exHeroList": fleet.ex_hero_ids,
                    "modeId": fleet.mode_id,
                    "strategyId": fleet.strategy_id
                }))
                .collect::<Vec<_>>(),
            "NameNum": value.name_num,
            "redDot": value.red_dot
        }),
    );
}

pub(super) fn preset_fleet_info_from_typed_account(
    account: &blueoath_domain::AccountState,
) -> PresetFleetInfo {
    PresetFleetInfo {
        fleets: account
            .fleet
            .presets
            .iter()
            .map(|preset| PresetFleet {
                name: preset.name.clone(),
                hero_ids: preset
                    .hero_ids
                    .iter()
                    .filter_map(|id| i32::try_from(id.get()).ok())
                    .collect(),
                ex_hero_ids: preset
                    .ex_hero_ids
                    .iter()
                    .filter_map(|id| i32::try_from(id.get()).ok())
                    .collect(),
                mode_id: i32::try_from(preset.mode_id).unwrap_or(i32::MAX),
                strategy_id: i32::try_from(preset.strategy_id).unwrap_or(i32::MAX),
            })
            .collect(),
        name_num: i32::try_from(account.fleet.preset_name_num).unwrap_or(i32::MAX),
        red_dot: i32::try_from(account.fleet.preset_red_dot).unwrap_or(i32::MAX),
    }
}

pub(super) fn set_preset_fleet_on_typed_account(
    account: &mut blueoath_domain::AccountState,
    value: &PresetFleetInfo,
) -> bool {
    if value.fleets.len() > 100 || value.name_num < 0 || value.red_dot < 0 {
        return false;
    }
    let mut presets = Vec::with_capacity(value.fleets.len());
    for fleet in &value.fleets {
        let Some(hero_ids) = fleet
            .hero_ids
            .iter()
            .map(|id| {
                u64::try_from(*id)
                    .ok()
                    .and_then(|id| blueoath_domain::HeroId::new(id).ok())
                    .filter(|id| account.dock.heroes.contains_key(id))
            })
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        let Some(ex_hero_ids) = fleet
            .ex_hero_ids
            .iter()
            .map(|id| {
                u64::try_from(*id)
                    .ok()
                    .and_then(|id| blueoath_domain::HeroId::new(id).ok())
                    .filter(|id| account.dock.heroes.contains_key(id))
            })
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        presets.push(blueoath_domain::PresetFleetState {
            name: fleet.name.clone(),
            hero_ids,
            ex_hero_ids,
            mode_id: u32::try_from(fleet.mode_id).unwrap_or_default(),
            strategy_id: u32::try_from(fleet.strategy_id).unwrap_or_default(),
        });
    }
    account.fleet.presets = presets;
    account.fleet.preset_name_num = value.name_num as u32;
    account.fleet.preset_red_dot = value.red_dot as u32;
    true
}

#[cfg(test)]
pub(super) fn sync_typed_preset_fleet_state(
    account: &mut blueoath_domain::AccountState,
    legacy: &Value,
) -> bool {
    let value = preset_fleet_info_from_account(legacy);
    let mut presets = Vec::with_capacity(value.fleets.len());
    for fleet in value.fleets {
        let hero_ids = fleet
            .hero_ids
            .into_iter()
            .filter_map(|id| u64::try_from(id).ok())
            .filter_map(|id| blueoath_domain::HeroId::new(id).ok())
            .collect();
        let ex_hero_ids = fleet
            .ex_hero_ids
            .into_iter()
            .filter_map(|id| u64::try_from(id).ok())
            .filter_map(|id| blueoath_domain::HeroId::new(id).ok())
            .collect();
        presets.push(blueoath_domain::PresetFleetState {
            name: fleet.name,
            hero_ids,
            ex_hero_ids,
            mode_id: u32::try_from(fleet.mode_id.max(0)).unwrap_or_default(),
            strategy_id: u32::try_from(fleet.strategy_id.max(0)).unwrap_or_default(),
        });
    }
    let name_num = u32::try_from(value.name_num.max(0)).unwrap_or_default();
    let red_dot = u32::try_from(value.red_dot.max(0)).unwrap_or_default();
    let changed = account.fleet.presets != presets
        || account.fleet.preset_name_num != name_num
        || account.fleet.preset_red_dot != red_dot;
    account.fleet.presets = presets;
    account.fleet.preset_name_num = name_num;
    account.fleet.preset_red_dot = red_dot;
    changed
}

pub(super) fn json_i32_array(value: &Value, key: &str) -> Vec<i32> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_i64)
                .filter_map(|item| i32::try_from(item).ok())
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn daily_copy_progress_from_account(
    account: Option<&Value>,
    now: u32,
) -> Vec<DailyCopyProgress> {
    let daily = account.and_then(|value| value.get("dailyCopy"));
    let reset = daily_copy_reset_required(daily, now);
    daily
        .and_then(|value| value.get("chapters"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            Some(DailyCopyProgress {
                chapter_id: json_i32(value, "chapterId")?,
                challenge_times: if reset {
                    0
                } else {
                    json_i32(value, "challengeTimes").unwrap_or_default().max(0)
                },
                pass_copy: {
                    let mut ids = Vec::new();
                    for id in json_i32_array(value, "passCopy") {
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                    }
                    ids
                },
                select_ex: value
                    .get("selectEx")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                ex_star: json_i32(value, "exStar").unwrap_or_default().max(0),
            })
        })
        .collect()
}

pub(super) fn daily_copy_progress_from_typed_account(
    account: &blueoath_domain::AccountState,
    now: u32,
) -> Vec<DailyCopyProgress> {
    let reset_day = (u64::from(now) + 8 * 60 * 60) / 86_400;
    let challenge_times = if u64::from(account.daily_copy.reset_day) == reset_day {
        account.daily_copy.challenge_times.clone()
    } else {
        std::collections::BTreeMap::new()
    };
    let chapter_ids = challenge_times
        .keys()
        .chain(account.daily_copy.select_ex.keys())
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    chapter_ids
        .into_iter()
        .filter_map(|chapter_id| {
            Some(DailyCopyProgress {
                chapter_id: i32::try_from(chapter_id.get()).ok()?,
                challenge_times: i32::try_from(
                    challenge_times
                        .get(&chapter_id)
                        .copied()
                        .unwrap_or_default(),
                )
                .unwrap_or(i32::MAX),
                pass_copy: Vec::new(),
                select_ex: account
                    .daily_copy
                    .select_ex
                    .get(&chapter_id)
                    .copied()
                    .unwrap_or(false),
                ex_star: if u64::from(account.daily_copy.reset_day) == reset_day {
                    i32::try_from(
                        account
                            .daily_copy
                            .ex_stars
                            .get(&chapter_id)
                            .copied()
                            .unwrap_or_default(),
                    )
                    .unwrap_or(i32::MAX)
                } else {
                    0
                },
            })
        })
        .collect()
}

pub(super) fn daily_copy_snapshot_payload_from_typed_account(
    account: &blueoath_domain::AccountState,
    chapter_catalog: Option<&ChapterCatalog>,
    now: u32,
) -> Vec<u8> {
    let fallback;
    let catalog = match chapter_catalog {
        Some(catalog) => catalog,
        None => {
            fallback = ChapterCatalog::fallback();
            &fallback
        }
    };
    DailyCopyCodec::encode_with_progress(
        &catalog.daily_chapters,
        &catalog.daily_groups,
        &daily_copy_progress_from_typed_account(account, now),
        &daily_copy_group_progress_from_typed_account(account, false, now),
        &daily_copy_group_progress_from_typed_account(account, true, now),
    )
}

pub(super) fn daily_copy_group_progress_from_typed_account(
    account: &blueoath_domain::AccountState,
    extra: bool,
    now: u32,
) -> Vec<DailyCopyGroupProgress> {
    let reset_day = (u64::from(now) + 8 * 60 * 60) / 86_400;
    let reset = u64::from(account.daily_copy.reset_day) != reset_day;
    let values = if extra {
        &account.daily_copy.extra_group_success_times
    } else {
        &account.daily_copy.group_success_times
    };
    values
        .iter()
        .filter_map(|(group_id, success_times)| {
            Some(DailyCopyGroupProgress {
                group_id: i32::try_from(*group_id).ok()?,
                success_times: if reset && !extra {
                    0
                } else {
                    i32::try_from(*success_times).unwrap_or(i32::MAX)
                },
            })
        })
        .collect()
}

pub(super) fn sync_typed_daily_copy_state(
    account: &mut blueoath_domain::AccountState,
    legacy: &Value,
    now: u32,
) -> bool {
    let Some(daily) = legacy.get("dailyCopy") else {
        return false;
    };
    let reset_day = (u64::from(now) + 8 * 60 * 60) / 86_400;
    let stored_reset_day = json_i32(daily, "resetDay")
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or_default();
    let next_reset_day = u32::try_from(reset_day).unwrap_or(u32::MAX);
    let is_current_day = stored_reset_day == next_reset_day;
    let mut next_challenges = std::collections::BTreeMap::new();
    let mut next_select_ex = std::collections::BTreeMap::new();
    let mut next_ex_stars = std::collections::BTreeMap::new();
    let mut next_groups = std::collections::BTreeMap::new();
    let mut next_extra_groups = std::collections::BTreeMap::new();
    if is_current_day {
        for chapter in daily
            .get("chapters")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(chapter_id) = json_i32(chapter, "chapterId")
                .and_then(|value| u64::try_from(value).ok())
                .and_then(|value| blueoath_domain::ChapterId::new(value).ok())
            else {
                continue;
            };
            let challenge_times = json_i32(chapter, "challengeTimes")
                .and_then(|value| u32::try_from(value.max(0)).ok())
                .unwrap_or_default();
            next_challenges.insert(chapter_id, challenge_times);
            next_select_ex.insert(
                chapter_id,
                chapter
                    .get("selectEx")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            );
            next_ex_stars.insert(
                chapter_id,
                u32::try_from(json_i32(chapter, "exStar").unwrap_or_default().max(0))
                    .unwrap_or_default(),
            );
            for copy_id in json_i32_array(chapter, "passCopy") {
                if let Some(copy_id) = u64::try_from(copy_id)
                    .ok()
                    .and_then(|value| blueoath_domain::CopyId::new(value).ok())
                {
                    account.battle.passed_copies.insert(copy_id);
                }
            }
        }
        for (key, target) in [
            ("groups", &mut next_groups),
            ("extraGroups", &mut next_extra_groups),
        ] {
            for group in daily
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(group_id) = json_i32(group, "dailyGroupId")
                    .and_then(|value| u64::try_from(value).ok())
                    .filter(|value| *value > 0)
                else {
                    continue;
                };
                target.insert(
                    group_id,
                    u32::try_from(json_i32(group, "successTimes").unwrap_or_default().max(0))
                        .unwrap_or_default(),
                );
            }
        }
    }
    let changed = account.daily_copy.reset_day != next_reset_day
        || account.daily_copy.challenge_times != next_challenges
        || account.daily_copy.select_ex != next_select_ex
        || account.daily_copy.ex_stars != next_ex_stars
        || account.daily_copy.group_success_times != next_groups
        || account.daily_copy.extra_group_success_times != next_extra_groups;
    account.daily_copy.reset_day = next_reset_day;
    account.daily_copy.challenge_times = next_challenges;
    account.daily_copy.select_ex = next_select_ex;
    account.daily_copy.ex_stars = next_ex_stars;
    account.daily_copy.group_success_times = next_groups;
    account.daily_copy.extra_group_success_times = next_extra_groups;
    changed
}

pub(super) fn daily_copy_group_progress_from_account(
    account: Option<&Value>,
    key: &str,
    now: u32,
) -> Vec<DailyCopyGroupProgress> {
    let daily = account.and_then(|value| value.get("dailyCopy"));
    let reset = daily_copy_reset_required(daily, now);
    daily
        .and_then(|value| value.get(key))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            Some(DailyCopyGroupProgress {
                group_id: json_i32(value, "dailyGroupId")?,
                success_times: if key == "groups" && reset {
                    0
                } else {
                    json_i32(value, "successTimes").unwrap_or_default().max(0)
                },
            })
        })
        .collect()
}

pub(super) fn daily_copy_reset_required(daily: Option<&Value>, now: u32) -> bool {
    let reset_day = (u64::from(now) + 8 * 60 * 60) / 86_400;
    daily
        .and_then(|value| json_i32(value, "resetDay"))
        .is_none_or(|stored| i64::from(stored) != reset_day as i64)
}

pub(super) fn daily_copy_snapshot_payload(
    account: &Value,
    chapter_catalog: Option<&ChapterCatalog>,
    now: u32,
) -> Vec<u8> {
    let fallback;
    let catalog = match chapter_catalog {
        Some(catalog) => catalog,
        None => {
            fallback = ChapterCatalog::fallback();
            &fallback
        }
    };
    DailyCopyCodec::encode_with_progress(
        &catalog.daily_chapters,
        &catalog.daily_groups,
        &daily_copy_progress_from_account(Some(account), now),
        &daily_copy_group_progress_from_account(Some(account), "groups", now),
        &daily_copy_group_progress_from_account(Some(account), "extraGroups", now),
    )
}

pub(super) fn goods_copy_snapshot_payload(
    account: &Value,
    chapter_catalog: Option<&ChapterCatalog>,
) -> Vec<u8> {
    let fallback;
    let catalog = match chapter_catalog {
        Some(catalog) => catalog,
        None => {
            fallback = ChapterCatalog::fallback();
            &fallback
        }
    };
    let stored = account
        .get("goodsCopy")
        .and_then(Value::as_object)
        .and_then(|value| value.get("entries"))
        .and_then(Value::as_array);
    let mut output = Vec::new();
    for copy_id in &catalog.goods_copy {
        let entry = stored
            .into_iter()
            .flatten()
            .find(|value| json_i32(value, "copyId") == Some(*copy_id));
        let empty = Value::Null;
        let entry = entry.unwrap_or(&empty);
        let mut info = Vec::new();
        append_varint_field(
            &mut info,
            1,
            json_i32(entry, "todayMaxDamage").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut info,
            2,
            json_i32(entry, "todayGetGoods").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut info,
            3,
            json_i32(entry, "percent").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(&mut info, 4, (*copy_id).max(0) as u64);
        append_message_field(&mut output, 1, &info);
    }
    output
}

/// Normalize persisted DailyCopy state to configured chapters/groups and current China day.
/// C# performs this before both GetData and SelectEx; doing it here keeps server-owned state
/// stable after client disconnects and ensures unknown client IDs cannot be persisted.
#[cfg(test)]
pub(super) fn normalize_daily_copy_state(
    account: &mut Value,
    chapter_catalog: Option<&ChapterCatalog>,
    now: u32,
) -> bool {
    let fallback;
    let catalog = match chapter_catalog {
        Some(catalog) => catalog,
        None => {
            fallback = ChapterCatalog::fallback();
            &fallback
        }
    };
    let reset_day = ((u64::from(now) + 8 * 60 * 60) / 86_400) as i32;
    let before = account.get("dailyCopy").cloned();
    let old = before
        .as_ref()
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let reset = old
        .get("resetDay")
        .and_then(Value::as_i64)
        .map(|day| day != i64::from(reset_day))
        .unwrap_or(true);
    let old_chapters = old
        .get("chapters")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let old_groups = old
        .get("groups")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let old_extra_groups = old
        .get("extraGroups")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut chapters = Vec::with_capacity(catalog.daily_chapters.len());
    for (chapter_id, _) in &catalog.daily_chapters {
        let existing = old_chapters
            .iter()
            .find(|value| json_i32(value, "chapterId") == Some(*chapter_id));
        let mut chapter = existing.cloned().unwrap_or_else(|| json!({}));
        chapter["chapterId"] = json!(*chapter_id);
        chapter["challengeTimes"] = json!(if reset {
            0
        } else {
            json_i32(&chapter, "challengeTimes")
                .unwrap_or_default()
                .max(0)
        });
        let mut passed = json_i32_array(&chapter, "passCopy");
        passed.sort_unstable();
        passed.dedup();
        chapter["passCopy"] = json!(passed);
        chapter["selectEx"] = json!(json_bool(&chapter, "selectEx"));
        chapter["exStar"] = json!(json_i32(&chapter, "exStar").unwrap_or_default().max(0));
        chapters.push(chapter);
    }
    let normalize_groups = |source: &[Value], reset_counts: bool| {
        catalog
            .daily_groups
            .iter()
            .map(|group_id| {
                let existing = source
                    .iter()
                    .find(|value| json_i32(value, "dailyGroupId") == Some(*group_id));
                let success_times = if reset_counts {
                    0
                } else {
                    existing
                        .and_then(|value| json_i32(value, "successTimes"))
                        .unwrap_or_default()
                        .max(0)
                };
                json!({"dailyGroupId": group_id, "successTimes": success_times})
            })
            .collect::<Vec<_>>()
    };
    let mut normalized = old;
    normalized.insert("resetDay".to_owned(), json!(reset_day));
    normalized.insert("chapters".to_owned(), json!(chapters));
    normalized.insert(
        "groups".to_owned(),
        json!(normalize_groups(&old_groups, reset)),
    );
    normalized.insert(
        "extraGroups".to_owned(),
        json!(normalize_groups(&old_extra_groups, false)),
    );
    let daily = Value::Object(normalized);
    let changed = before.as_ref() != Some(&daily);
    if let Some(root) = account.as_object_mut() {
        root.insert("dailyCopy".to_owned(), daily);
    }
    changed
}
