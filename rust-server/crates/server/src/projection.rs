#![allow(dead_code)]

use blueoath_protocol::*;
use serde_json::Value;

use super::*;

const ZERO_TRACKED_BAG_ITEMS: &[i32] = &[
    10182, 10185, 10187, // skill books
    10007, 10181, 12201, // construction/draw tickets
    10029, 10030, 10031, // construction resources
];

// The client stores HeroGrid.CurHp as a fixed-point ratio.  A full-health ship
// must receive 10_000_000_000, while the typed account stores absolute HP.
const HERO_HP_COEFFICIENT: u64 = 10_000_000_000;

fn hero_hp_to_client_ratio(current_hp: u64, max_hp: u64) -> i64 {
    let max_hp = max_hp.max(1);
    let current_hp = current_hp.min(max_hp);
    let ratio = current_hp
        .saturating_mul(HERO_HP_COEFFICIENT)
        .checked_div(max_hp)
        .unwrap_or_default()
        .min(HERO_HP_COEFFICIENT);

    // CurHp is read unconditionally by the client.  Preserve a non-zero wire
    // value even for a sunk ship so Lua does not see nil.
    i64::try_from(ratio.max(1)).unwrap_or(i64::MAX)
}

/// Convert client battle CurHp fixed-point ratio back to persisted absolute HP.
/// Battle result CurHp uses same 1e10 ratio as HeroGrid; typed storage is HP units.
pub(crate) fn typed_hero_hp_from_client_ratio(client_ratio: i64, max_hp: u64) -> u64 {
    let ratio = u64::try_from(client_ratio.max(0))
        .unwrap_or_default()
        .min(HERO_HP_COEFFICIENT);
    max_hp
        .max(1)
        .saturating_mul(ratio)
        .saturating_add(HERO_HP_COEFFICIENT.saturating_sub(1))
        / HERO_HP_COEFFICIENT
}

pub(crate) fn typed_hero_cur_hp_for_client(
    account: &blueoath_domain::AccountState,
    hero: &blueoath_domain::HeroState,
) -> i64 {
    hero_hp_to_client_ratio(
        hero.hp,
        ship_max_hp_for_typed_hero_with_heroes(
            hero,
            &account.activities.progress,
            &account.dock.equipments,
            SHIP_STAT_CATALOG.get(),
            EQUIP_CATALOG.get(),
            SHIP_REMOULD_CATALOG.get(),
            SHIP_STAT_MULTIPLIER.get().copied().unwrap_or(1.0),
            Some(&account.dock.heroes),
        ),
    )
}

pub(super) fn hero_array<'a>(value: &'a Value, key: &str) -> Option<&'a Vec<Value>> {
    value.get(key).and_then(Value::as_array)
}

pub(super) fn hero_bag_from_typed_account(account: &blueoath_domain::AccountState) -> HeroBag {
    let now = current_unix_seconds();
    let create_time = u32::try_from(account.character.create_time)
        .ok()
        .filter(|value| *value != 0)
        .unwrap_or(now);
    let hero_pskills = |hero: &blueoath_domain::HeroState| {
        let stored = hero
            .pskills
            .iter()
            .map(|(skill_id, level)| PSkillEntry {
                pskill_id: u32::try_from(*skill_id).unwrap_or(u32::MAX),
                pskill_exp: u32::try_from(
                    account
                        .activities
                        .progress
                        .get(&format!(
                            "compat:hero:{}:pskill:{}:exp",
                            hero.id.get(),
                            skill_id
                        ))
                        .copied()
                        .unwrap_or_default(),
                )
                .unwrap_or(u32::MAX),
                level: i32::try_from(*level).unwrap_or(i32::MAX),
                replace: i32::try_from(
                    account
                        .activities
                        .progress
                        .get(&format!(
                            "compat:hero:{}:pskill:{}:replace",
                            hero.id.get(),
                            skill_id
                        ))
                        .copied()
                        .unwrap_or_default(),
                )
                .unwrap_or(i32::MAX),
            })
            .collect::<Vec<_>>();
        if !stored.is_empty() {
            return stored;
        }
        HERO_SKILL_CATALOG
            .get()
            .and_then(|catalog| catalog.get(&i32::try_from(hero.template_id.get()).ok()?))
            .into_iter()
            .flatten()
            .map(|skill_id| PSkillEntry {
                pskill_id: u32::try_from(*skill_id).unwrap_or(u32::MAX),
                pskill_exp: 0,
                level: 1,
                replace: 0,
            })
            .collect()
    };
    let hero_intensify = |hero: &blueoath_domain::HeroState| {
        let prefix = format!("compat:hero:{}:intensify:", hero.id.get());
        let mut attrs = std::collections::BTreeSet::new();
        for key in account.activities.progress.keys() {
            if let Some(value) = key.strip_prefix(&prefix) {
                if let Some(attr) = value
                    .strip_suffix(":level")
                    .and_then(|id| id.parse::<i32>().ok())
                {
                    attrs.insert(attr);
                }
            }
        }
        attrs
            .into_iter()
            .map(|attr_type| AttrIntensify {
                attr_type,
                intensify_level: i32::try_from(
                    account
                        .activities
                        .progress
                        .get(&format!("{prefix}{attr_type}:level"))
                        .copied()
                        .unwrap_or_default(),
                )
                .unwrap_or(i32::MAX),
                cur_exp: i32::try_from(
                    account
                        .activities
                        .progress
                        .get(&format!("{prefix}{attr_type}:exp"))
                        .copied()
                        .unwrap_or_default(),
                )
                .unwrap_or(i32::MAX),
            })
            .collect::<Vec<_>>()
    };
    let heroes = account
        .dock
        .heroes
        .values()
        .map(|hero| HeroGrid {
            hero_id: u32::try_from(hero.id.get()).unwrap_or(u32::MAX),
            template_id: i32::try_from(hero.template_id.get()).unwrap_or(i32::MAX),
            fashioning: i32::try_from(if hero.fashioning == 0 {
                hero.template_id.get().saturating_sub(1) / 10
            } else {
                u64::from(hero.fashioning)
            })
            .unwrap_or(i32::MAX),
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
            create_time: i32::try_from(create_time).unwrap_or(i32::MAX),
            update_time: i32::try_from(now).unwrap_or(i32::MAX),
            affection: i32::try_from(hero.affection).unwrap_or(i32::MAX),
            cur_hp: typed_hero_cur_hp_for_client(account, hero),
            mood: i32::try_from(hero.mood).unwrap_or(i32::MAX),
            equip_slots: hero
                .equip_slots
                .iter()
                .map(|slot| {
                    slot.map(|id| u32::try_from(id.get()).unwrap_or(u32::MAX))
                        .unwrap_or(0)
                })
                .collect(),
            pskills: hero_pskills(hero),
            advance: i32::try_from(
                account
                    .activities
                    .progress
                    .get(&format!("compat:hero:{}:advance", hero.id.get()))
                    .copied()
                    .unwrap_or_default(),
            )
            .unwrap_or(i32::MAX),
            adv_lv: i32::try_from(
                account
                    .activities
                    .progress
                    .get(&format!("compat:hero:{}:advLv", hero.id.get()))
                    .copied()
                    .unwrap_or_default(),
            )
            .unwrap_or(i32::MAX),
            remould_effects: account
                .activities
                .progress
                .iter()
                .filter_map(|(key, value)| {
                    key.strip_prefix(&format!("compat:hero:{}:remould:effect:", hero.id.get()))
                        .and_then(|id| id.parse::<i32>().ok())
                        .filter(|_| *value > 0)
                })
                .collect(),
            remould_level: i32::try_from(
                account
                    .activities
                    .progress
                    .get(&format!("compat:hero:{}:remould:level", hero.id.get()))
                    .copied()
                    .unwrap_or_default(),
            )
            .unwrap_or(i32::MAX),
            intensify: hero_intensify(hero),
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
        bag_size: i32::try_from(account.ship_dock_capacity()).unwrap_or(i32::MAX),
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

pub(super) fn bag_info_from_typed_account_with_tombstones(
    account: &blueoath_domain::AccountState,
    removed_template_ids: &[i32],
) -> BagInfo {
    let mut bag = bag_info_from_typed_account(account);
    for template_id in removed_template_ids {
        if *template_id > 0
            && !bag
                .items
                .iter()
                .any(|item| item.template_id == *template_id)
        {
            bag.items.push(BagGrid {
                template_id: *template_id,
                num: 0,
            });
        }
    }
    bag
}

pub(super) fn fashion_list_from_typed_account(
    account: &blueoath_domain::AccountState,
    _catalog: Option<&FashionList>,
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
    FashionList { items: stored }
}

pub(super) fn equip_list_from_typed_account(account: &blueoath_domain::AccountState) -> EquipList {
    let items = account
        .dock
        .equipments
        .values()
        .map(|equipment| equip_info_from_typed_equipment(equipment, EQUIP_CATALOG.get()))
        .collect();
    let nums = EQUIP_CATALOG
        .get()
        .map(|catalog| {
            catalog
                .enhance_materials
                .keys()
                .map(|template_id| EquipNum {
                    template_id: *template_id,
                    num: account
                        .inventory
                        .items
                        .iter()
                        .find(|(id, _)| i32::try_from(id.get()).ok() == Some(*template_id))
                        .and_then(|(_, amount)| i32::try_from(*amount).ok())
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    EquipList {
        bag_size: i32::try_from(account.equipment_dock_capacity()).unwrap_or(i32::MAX),
        items,
        nums,
    }
}

pub(super) fn equip_info_from_typed_equipment(
    equipment: &blueoath_domain::EquipmentState,
    catalog: Option<&EquipCatalog>,
) -> EquipInfo {
    let pskills = if equipment.star > 0 {
        catalog
            .and_then(|value| {
                value
                    .skills_by_template
                    .get(&i32::try_from(equipment.template_id.get()).ok()?)
            })
            .into_iter()
            .flatten()
            .map(|(skill_id, max_level)| EquipPSkill {
                pskill_id: *skill_id,
                level: i32::try_from(equipment.star)
                    .unwrap_or(i32::MAX)
                    .min(*max_level),
            })
            .collect()
    } else {
        Vec::new()
    };
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
        pskills,
        ..EquipInfo::default()
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
    building_info_from_typed_account_with_catalog(account, now, None)
}

pub(super) fn building_info_from_typed_account_with_catalog(
    account: &blueoath_domain::AccountState,
    now: u32,
    catalog: Option<&BuildingCatalog>,
) -> UserBuildingInfo {
    if account.buildings.levels.is_empty() {
        return default_building_info(now);
    }
    let buildings = account
        .buildings
        .levels
        .iter()
        .filter_map(|(building_id, level)| {
            let template_id = i32::try_from(
                account
                    .buildings
                    .template_ids
                    .get(building_id)
                    .copied()
                    .unwrap_or(*building_id),
            )
            .ok()?;
            let building_config =
                catalog.and_then(|value| value.typed_building_configs.get(&template_id));
            let production = account.buildings.productions.get(building_id);
            let is_auto_production =
                building_config.is_some_and(|config| matches!(config.building_type, 2 | 3 | 4 | 6));
            let default_productivity = building_config
                .map(|config| config.productivity.max(0))
                .unwrap_or_default();
            let default_produce_speed = building_config
                .map(crate::game_config::building_produce_speed)
                .unwrap_or_default();
            Some(BuildingInfo {
                id: i32::try_from(*building_id).ok()?,
                template_id,
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
                    .filter(|value| *value > 0)
                    .unwrap_or(default_productivity),
                produce_speed: production
                    .and_then(|value| i32::try_from(value.produce_speed).ok())
                    .filter(|value| *value > 0)
                    .unwrap_or(default_produce_speed),
                product_count: production
                    .and_then(|value| i32::try_from(value.product_count).ok())
                    .unwrap_or_default(),
                status: production
                    .and_then(|value| i32::try_from(value.status).ok())
                    .filter(|value| *value > 0)
                    .unwrap_or(if is_auto_production { 3 } else { 1 }),
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
                last_mood_update_time: i64::try_from(account.buildings.mood_update_at)
                    .ok()
                    .filter(|value| *value > 0)
                    .unwrap_or(i64::from(now)),
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
    let office_level = catalog
        .and_then(|catalog| {
            buildings.iter().find_map(|building| {
                catalog
                    .typed_building_configs
                    .get(&building.template_id)
                    .filter(|config| config.building_type == 1)
                    .map(|_| building.level.max(1))
            })
        })
        .unwrap_or(1);
    let worker_max_strength = catalog
        .map(|catalog| {
            let level_bonus = catalog
                .worker_hp_level_up
                .iter()
                .take(usize::try_from(office_level).unwrap_or_default())
                .fold(0_i32, |total, bonus| total.saturating_add(*bonus));
            catalog
                .worker_hp_max
                .saturating_add(level_bonus)
                .saturating_mul(10_000)
        })
        .unwrap_or(1_500_000);
    let worker_strength = account
        .buildings
        .worker_strength
        .try_into()
        .ok()
        .filter(|value: &u32| *value > 0)
        .unwrap_or(worker_max_strength as u32)
        .min(u32::try_from(worker_max_strength).unwrap_or(u32::MAX));
    UserBuildingInfo {
        buildings,
        lands,
        worker_strength: i32::try_from(worker_strength).unwrap_or(i32::MAX),
        worker_recover: catalog.map(|value| value.worker_recover).unwrap_or(10),
        food_max: 100,
        electric_max: 100,
        worker_update_time: i64::try_from(account.buildings.worker_update_at)
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(i64::from(now)),
    }
}

pub(super) fn fleet_info_from_typed_account(account: &blueoath_domain::AccountState) -> FleetInfo {
    let tactics = account
        .fleet
        .fleets
        .iter()
        .filter_map(|(fleet_id, fleet)| {
            Some(FleetTactic {
                tactic_name: fleet.tactic_name.clone(),
                hero_ids: fleet
                    .members
                    .iter()
                    .filter_map(|hero_id| i32::try_from(hero_id.get()).ok())
                    .collect(),
                mode_id: i32::try_from(fleet_id.get()).ok()?,
                strategy_id: i32::try_from(fleet.tactic_id).ok()?,
                formation_id: i32::try_from(fleet.formation_id).ok()?,
                tactic_type: i32::try_from(fleet.tactic_type.max(1)).ok()?,
                ex_hero_ids: fleet
                    .ex_members
                    .iter()
                    .filter_map(|hero_id| i32::try_from(hero_id.get()).ok())
                    .collect(),
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
        let mut ex_members = Vec::new();
        for hero_id in &tactic.ex_hero_ids {
            let Ok(hero_id) = u64::try_from(*hero_id) else {
                return false;
            };
            let Ok(hero_id) = blueoath_domain::HeroId::new(hero_id) else {
                return false;
            };
            if !account.dock.heroes.contains_key(&hero_id)
                || members.contains(&hero_id)
                || ex_members.contains(&hero_id)
            {
                return false;
            }
            ex_members.push(hero_id);
        }
        if fleets
            .insert(
                fleet_id,
                blueoath_domain::FleetRecord {
                    tactic_name: tactic.tactic_name.clone(),
                    formation_id,
                    tactic_id,
                    tactic_type: u32::try_from(tactic.tactic_type).unwrap_or_default().max(1),
                    members,
                    ex_members,
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
                    cur_hp: typed_hero_cur_hp_for_client(account, hero) as u64,
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

pub(super) fn daily_copy_progress_from_typed_account(
    account: &blueoath_domain::AccountState,
    chapter_catalog: &ChapterCatalog,
    now: u32,
) -> Vec<DailyCopyProgress> {
    let reset_day = (u64::from(now) + 8 * 60 * 60) / 86_400;
    let challenge_times = if u64::from(account.daily_copy.reset_day) == reset_day {
        account.daily_copy.challenge_times.clone()
    } else {
        std::collections::BTreeMap::new()
    };
    chapter_catalog
        .daily_chapters
        .iter()
        .filter_map(|(chapter_id, _)| {
            let chapter_id_key =
                blueoath_domain::ChapterId::new(u64::try_from(*chapter_id).ok()?).ok()?;
            let pass_copy = chapter_catalog
                .daily_level_ids_by_chapter
                .get(chapter_id)
                .into_iter()
                .flatten()
                .copied()
                .filter(|copy_id| {
                    u64::try_from(*copy_id)
                        .ok()
                        .and_then(|id| blueoath_domain::CopyId::new(id).ok())
                        .is_some_and(|copy_id| account.battle.passed_copies.contains(&copy_id))
                })
                .collect::<Vec<_>>();
            let select_ex = account
                .daily_copy
                .select_ex
                .get(&chapter_id_key)
                .copied()
                .unwrap_or(false);
            let ex_star = if u64::from(account.daily_copy.reset_day) == reset_day {
                i32::try_from(
                    account
                        .daily_copy
                        .ex_stars
                        .get(&chapter_id_key)
                        .copied()
                        .unwrap_or_default(),
                )
                .unwrap_or(i32::MAX)
            } else {
                0
            };
            let challenge_time = challenge_times
                .get(&chapter_id_key)
                .copied()
                .unwrap_or_default();
            let has_progress =
                !pass_copy.is_empty() || challenge_time > 0 || select_ex || ex_star > 0;
            has_progress.then(|| DailyCopyProgress {
                chapter_id: *chapter_id,
                challenge_times: i32::try_from(challenge_time).unwrap_or(i32::MAX),
                pass_copy,
                select_ex,
                ex_star,
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
        &daily_copy_progress_from_typed_account(account, catalog, now),
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
