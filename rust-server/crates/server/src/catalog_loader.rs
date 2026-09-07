use rusqlite::{types::ValueRef, Connection};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use super::*;

fn read_config_rows(path: &Path) -> Vec<(i32, Value)> {
    if let Some(rows) = read_json_config_rows(path) {
        return rows;
    }
    let Ok(connection) =
        Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return Vec::new();
    };
    let Ok(mut statement) = connection.prepare("SELECT id, jsonbytes FROM DBObject") else {
        return Vec::new();
    };
    let Ok(rows) = statement.query_map([], |row| {
        let id = row
            .get::<_, String>(0)
            .ok()
            .and_then(|value| value.parse::<i32>().ok())
            .or_else(|| {
                row.get::<_, i64>(0)
                    .ok()
                    .and_then(|value| i32::try_from(value).ok())
            })
            .unwrap_or_default();
        let bytes = match row.get_ref(1)? {
            ValueRef::Blob(bytes) | ValueRef::Text(bytes) => bytes.to_vec(),
            _ => Vec::new(),
        };
        let decoded: Vec<u8> = bytes.into_iter().map(|byte| byte ^ 0x55).collect();
        let value: Value = serde_json::from_slice(&decoded).unwrap_or_default();
        Ok((id, value))
    }) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

fn read_json_config_rows(path: &Path) -> Option<Vec<(i32, Value)>> {
    let json_path = path.with_extension("json");
    let bytes = std::fs::read(json_path).ok()?;
    let document: Value = serde_json::from_slice(&bytes).ok()?;
    let rows = document.get("rows")?.as_array()?;
    Some(
        rows.iter()
            .filter_map(|row| {
                let id = row.get("id")?;
                let id = id
                    .as_i64()
                    .and_then(|value| i32::try_from(value).ok())
                    .or_else(|| id.as_str()?.parse::<i32>().ok())
                    .unwrap_or_default();
                Some((id, row.get("value")?.clone()))
            })
            .collect(),
    )
}

pub(super) fn config_dir(client_path: &Path) -> PathBuf {
    // Accept a server-local catalog directory containing config_*.db directly. This lets
    // deployments copy client catalogs once and run without the installed client tree.
    if client_path.join("config_chapter.db").is_file()
        || client_path.join("config_shop.db").is_file()
        || client_path.join("config_chapter.json").is_file()
        || client_path.join("config_shop.json").is_file()
    {
        return client_path.to_path_buf();
    }
    client_path
        .join("blueoath_Data")
        .join("StreamingAssets")
        .join("config")
}

pub(super) fn load_chapter_catalog(client_path: Option<&PathBuf>) -> ChapterCatalog {
    let Some(client_path) = client_path else {
        return ChapterCatalog::fallback();
    };
    let rows = read_config_rows(&config_dir(client_path).join("config_chapter.db"));
    if rows.is_empty() {
        ChapterCatalog::fallback().with_mini_game_rows(read_config_rows(
            &config_dir(client_path).join("config_minigame_copy.db"),
        ))
    } else {
        ChapterCatalog::from_rows(rows).with_mini_game_rows(read_config_rows(
            &config_dir(client_path).join("config_minigame_copy.db"),
        ))
    }
}

fn combination_costs(value: &Value, key: &str) -> Vec<(i32, i32, i32)> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            if row.len() < 3 {
                return None;
            }
            let goods_type = i32::try_from(row[0].as_i64()?).ok()?;
            let item_id = i32::try_from(row[1].as_i64()?).ok()?;
            let amount = i32::try_from(row[2].as_i64()?).ok()?;
            (goods_type > 0 && item_id > 0 && amount > 0).then_some((goods_type, item_id, amount))
        })
        .collect()
}

pub(super) fn load_combination_catalog(client_path: Option<&PathBuf>) -> CombinationCatalog {
    let Some(client_path) = client_path else {
        return CombinationCatalog::default();
    };
    let dir = config_dir(client_path);
    let open_sf_ids = read_config_rows(&dir.join("config_ship_fleet.db"))
        .into_iter()
        .filter(|(_, value)| json_i32(value, "combination_open") == Some(1))
        .map(|(id, _)| id)
        .collect();
    let rules_by_id = read_config_rows(&dir.join("config_combination_ship.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            let levels = json_i32_array(&value, "level");
            let sf_id = json_i32(&value, "sf_id")?;
            let level_start = *levels.first()?;
            let level_end = *levels.get(1).unwrap_or(&level_start);
            (id > 0 && sf_id > 0).then_some((
                id,
                CombinationRule {
                    level_end,
                    next_id: json_i32(&value, "next_id").unwrap_or_default(),
                    star: json_i32(&value, "star").unwrap_or_default(),
                    levelup_costs: combination_costs(&value, "levelup_item"),
                    break_costs: combination_costs(&value, "break_item"),
                },
            ))
        })
        .collect();
    CombinationCatalog {
        open_sf_ids,
        rules_by_id,
    }
}

pub(super) fn load_equip_new_test_catalog(client_path: Option<&PathBuf>) -> EquipNewTestCatalog {
    let Some(client_path) = client_path else {
        return EquipNewTestCatalog::default();
    };
    EquipNewTestCatalog::from_rows(read_config_rows(
        &config_dir(client_path).join("config_activity.db"),
    ))
}

pub(super) fn load_fashion_catalog(client_path: Option<&PathBuf>) -> FashionList {
    let Some(client_path) = client_path else {
        return FashionList::default();
    };
    let path = config_dir(client_path).join("config_fashion.db");
    let mut grouped = std::collections::BTreeMap::<i32, Vec<i32>>::new();
    for (fashion_tid, value) in read_config_rows(&path) {
        let sf_id = value
            .get("belongToShip")
            .or_else(|| value.get("belong_to_ship"))
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or_default();
        if fashion_tid != 0 && sf_id != 0 {
            grouped.entry(sf_id).or_default().push(fashion_tid);
        }
    }
    FashionList {
        items: grouped
            .into_iter()
            .map(|(sf_id, mut fashion_tids)| {
                fashion_tids.sort_unstable();
                fashion_tids.dedup();
                FashionInfo {
                    sf_id,
                    fashion_tids,
                }
            })
            .collect(),
    }
}

pub(super) fn load_equip_catalog(client_path: Option<&PathBuf>) -> EquipCatalog {
    let Some(client_path) = client_path else {
        return EquipCatalog::default();
    };
    let dir = config_dir(client_path);
    let equip_rows = read_config_rows(&dir.join("config_equip.db"));
    let skill_rows = read_config_rows(&config_dir(client_path).join("config_pskill_dict_group.db"));
    let max_levels = skill_rows
        .into_iter()
        .filter_map(|(skill_id, value)| {
            let max_level = value
                .get("maxLevel")
                .or_else(|| value.get("max_level"))
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok())?;
            Some((skill_id, if max_level > 0 { max_level } else { i32::MAX }))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut skills_by_template = std::collections::BTreeMap::new();
    let mut quality_by_template = std::collections::BTreeMap::new();
    let mut type_by_template = std::collections::BTreeMap::new();
    let mut enhance_max_by_template = std::collections::BTreeMap::new();
    let mut star_max_by_template = std::collections::BTreeMap::new();
    let mut dismantle_rewards_by_template = std::collections::BTreeMap::new();
    let mut activity_equip_by_template = std::collections::BTreeSet::new();
    let mut activity_reward_by_template = std::collections::BTreeMap::new();
    let mut no_resolve_templates = std::collections::BTreeSet::new();
    for (template_id, value) in equip_rows {
        let quality = json_i32(&value, "quality").unwrap_or_default();
        let equip_type = json_i32(&value, "equipTypeId")
            .or_else(|| json_i32(&value, "equip_type_id"))
            .unwrap_or_default();
        let enhance_max = json_i32(&value, "enhanceLevelMax")
            .or_else(|| json_i32(&value, "enhance_level_max"))
            .unwrap_or_default();
        let star_max = json_i32(&value, "starMax")
            .or_else(|| json_i32(&value, "star_max"))
            .unwrap_or_default();
        quality_by_template.insert(template_id, quality);
        type_by_template.insert(template_id, equip_type);
        enhance_max_by_template.insert(template_id, enhance_max);
        star_max_by_template.insert(template_id, star_max);
        let dismantle_rewards = value
            .get("dismantlingGet")
            .or_else(|| value.get("dismantling_get"))
            .and_then(Value::as_array)
            .map(|values| {
                if values.iter().all(|value| value.as_array().is_none()) {
                    values
                        .chunks(3)
                        .filter_map(|row| {
                            let goods_type = i32::try_from(row.first()?.as_i64()?).ok()?;
                            let item_id = i32::try_from(row.get(1)?.as_i64()?).ok()?;
                            let num = i32::try_from(row.get(2)?.as_i64()?).ok()?;
                            (goods_type > 0 && item_id > 0 && num > 0)
                                .then_some((goods_type, item_id, num))
                        })
                        .collect::<Vec<_>>()
                } else {
                    values
                        .iter()
                        .filter_map(|row| {
                            let row = row.as_array()?;
                            let goods_type = i32::try_from(row.first()?.as_i64()?).ok()?;
                            let item_id = i32::try_from(row.get(1)?.as_i64()?).ok()?;
                            let num = i32::try_from(row.get(2)?.as_i64()?).ok()?;
                            (goods_type > 0 && item_id > 0 && num > 0)
                                .then_some((goods_type, item_id, num))
                        })
                        .collect::<Vec<_>>()
                }
            })
            .unwrap_or_default();
        dismantle_rewards_by_template.insert(template_id, dismantle_rewards);
        if json_i32(&value, "activity_equip")
            .or_else(|| json_i32(&value, "activityEquip"))
            .unwrap_or_default()
            > 0
        {
            activity_equip_by_template.insert(template_id);
        }
        if let Some(reward_id) = json_i32(&value, "reward")
            .or_else(|| json_i32(&value, "reward_id"))
            .filter(|reward_id| *reward_id > 0)
        {
            activity_reward_by_template.insert(template_id, reward_id);
        }
        if json_i32(&value, "noResolve")
            .or_else(|| json_i32(&value, "no_resolve"))
            .unwrap_or_default()
            > 0
        {
            no_resolve_templates.insert(template_id);
        }
        let skills = value
            .get("renovateSkill")
            .or_else(|| value.get("renovate_skill"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_i64)
            .filter_map(|value| i32::try_from(value).ok())
            .filter(|value| *value > 0)
            .map(|skill_id| {
                (
                    skill_id,
                    max_levels.get(&skill_id).copied().unwrap_or(i32::MAX),
                )
            })
            .collect::<Vec<_>>();
        if !skills.is_empty() {
            skills_by_template.insert(template_id, skills);
        }
    }
    let enhance_materials = read_config_rows(&dir.join("config_equip_enhance_item.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            let exp = json_i32(&value, "exp")?;
            let limits = value_array_i64(&value, &["enhance_level_limit", "enhanceLevelLimit"]);
            let limits = if limits.len() >= 2 {
                Some((
                    i32::try_from(limits[0]).ok()?,
                    i32::try_from(limits[1]).ok()?,
                ))
            } else {
                None
            };
            Some((id, (exp.max(0), limits)))
        })
        .collect();
    let enhance_level_exp = read_config_rows(&dir.join("config_equip_enhance_level_exp.db"))
        .into_iter()
        .filter_map(|(id, value)| Some((id, json_i32(&value, "exp")?.max(0))))
        .collect();
    let enhance_level_ur = read_config_rows(&dir.join("config_equip_enhance_level_ur.db"))
        .into_iter()
        .filter_map(|(_, value)| {
            let level = json_i32(&value, "enchance_level")?;
            let costs = value
                .get("item_cost")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|row| {
                    let row = row.as_array()?;
                    Some((
                        i32::try_from(row.first()?.as_i64()?).ok()?,
                        i32::try_from(row.get(1)?.as_i64()?).ok()?,
                        i32::try_from(row.get(2)?.as_i64()?).ok()?,
                    ))
                })
                .filter(|(_, _, count)| *count > 0)
                .collect::<Vec<_>>();
            (!costs.is_empty()).then_some((level, costs))
        })
        .collect();
    let levelbreak_rules = read_config_rows(&dir.join("config_equip_levelbreak_item.db"))
        .into_iter()
        .map(|(id, value)| {
            let level_rank = value_array_i64(&value, &["level_rank", "levelRank"]);
            let level_rank = if level_rank.len() >= 2 {
                match (i32::try_from(level_rank[0]), i32::try_from(level_rank[1])) {
                    (Ok(min_level), Ok(max_level)) => Some((min_level, max_level)),
                    _ => None,
                }
            } else {
                None
            };
            let costs = value
                .get("item_cost")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|row| {
                    let row = row.as_array()?;
                    Some((
                        i32::try_from(row.first()?.as_i64()?).ok()?,
                        i32::try_from(row.get(1)?.as_i64()?).ok()?,
                        i32::try_from(row.get(2)?.as_i64()?).ok()?,
                    ))
                })
                .filter(|(_, _, count)| *count > 0)
                .collect::<Vec<_>>();
            (id, EquipLevelbreakRule { level_rank, costs })
        })
        .collect();
    let renovate_rules = read_config_rows(&dir.join("config_equip_enhance_renovate.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            let costs = value
                .get("item_array")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|row| {
                    let row = row.as_array()?;
                    Some((
                        i32::try_from(row.first()?.as_i64()?).ok()?,
                        i32::try_from(row.get(1)?.as_i64()?).ok()?,
                        i32::try_from(row.get(2)?.as_i64()?).ok()?,
                    ))
                })
                .collect::<Vec<_>>();
            Some((
                id,
                EquipRenovateRule {
                    costs,
                    self_count: usize::try_from(value_i64_any(&value, &["equip_self_count"]))
                        .ok()?,
                    need_level: json_i32(&value, "need_enhance_level").unwrap_or_default(),
                },
            ))
        })
        .collect();
    EquipCatalog {
        skills_by_template,
        quality_by_template,
        type_by_template,
        enhance_max_by_template,
        star_max_by_template,
        enhance_materials,
        enhance_level_exp,
        enhance_level_ur,
        levelbreak_rules,
        renovate_rules,
        dismantle_rewards_by_template,
        activity_equip_by_template,
        activity_reward_by_template,
        no_resolve_templates,
    }
}

#[cfg(test)]
pub(super) fn load_hero_skill_catalog(
    client_path: Option<&PathBuf>,
) -> std::collections::BTreeMap<i32, Vec<i32>> {
    let Some(client_path) = client_path else {
        return std::collections::BTreeMap::new();
    };
    read_config_rows(&config_dir(client_path).join("config_ship_main.db"))
        .into_iter()
        .filter_map(|(template_id, value)| {
            let mut skills = Vec::new();
            for key in [
                "pskill_show_id",
                "direct_activate_talent_id",
                "condition_activate_talent_id",
            ] {
                skills.extend(json_i32_array(&value, key));
            }
            skills.retain(|id| *id > 0);
            skills.sort_unstable();
            skills.dedup();
            (!skills.is_empty()).then_some((template_id, skills))
        })
        .collect()
}

pub(super) fn load_hero_skill_upgrade_catalog(
    client_path: Option<&PathBuf>,
) -> HeroSkillUpgradeCatalog {
    let Some(client_path) = client_path else {
        return HeroSkillUpgradeCatalog::default();
    };
    let mut catalog = HeroSkillUpgradeCatalog::default();
    for (group_id, value) in
        read_config_rows(&config_dir(client_path).join("config_pskill_dict_group.db"))
    {
        let costs = value
            .get("upgrade_materials")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| {
                        let row = row.as_array()?;
                        if row.len() < 3 {
                            return None;
                        }
                        let goods_type = i32::try_from(row[0].as_i64()?).ok()?;
                        let item_id = i32::try_from(row[1].as_i64()?).ok()?;
                        let amount = i32::try_from(row[2].as_i64()?).ok()?;
                        (goods_type > 0 && item_id > 0 && amount > 0)
                            .then_some((goods_type, item_id, amount))
                    })
                    .map(|cost| vec![cost])
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if costs.is_empty() {
            continue;
        }
        let mut ids = json_i32_array(&value, "skill_id_array");
        // Some client rows use group id itself as PSkillId (for example 10642).
        ids.push(group_id);
        ids.sort_unstable();
        ids.dedup();
        for skill_id in ids {
            catalog
                .costs_by_skill
                .entry(skill_id)
                .or_insert_with(|| costs.clone());
        }
    }
    catalog
}

pub(super) fn load_ship_stat_catalog(client_path: Option<&PathBuf>) -> ShipStatCatalog {
    let Some(client_path) = client_path else {
        return ShipStatCatalog::default();
    };
    let by_template = read_config_rows(&config_dir(client_path).join("config_ship_main.db"))
        .into_iter()
        .filter_map(|(template_id, value)| {
            (template_id > 0).then_some((
                template_id,
                ShipStat {
                    fixed_money: json_i64(&value, "fixed_money").unwrap_or_default().max(0),
                    hp: json_i64(&value, "hp").unwrap_or_default(),
                    hp_levelup: json_i64(&value, "hp_levelup").unwrap_or_default(),
                    attack: json_i64(&value, "attack").unwrap_or_default(),
                    attack_levelup: json_i64(&value, "attack_levelup").unwrap_or_default(),
                    defense: json_i64(&value, "defense").unwrap_or_default(),
                    defense_levelup: json_i64(&value, "defense_levelup").unwrap_or_default(),
                    torpedo_attack: json_i64(&value, "torpedo_attack").unwrap_or_default(),
                    torpedo_attack_levelup: json_i64(&value, "torpedo_attack_levelup")
                        .unwrap_or_default(),
                    torpedo_defense: json_i64(&value, "torpedo_defense").unwrap_or_default(),
                    torpedo_defense_levelup: json_i64(&value, "torpedo_defense_levelup")
                        .unwrap_or_default(),
                    ship_bomb_attack: json_i64(&value, "ship_bomb_attack").unwrap_or_default(),
                    ship_bomb_attack_levelup: json_i64(&value, "ship_bomb_attack_levelup")
                        .unwrap_or_default(),
                    ship_torpedo_attack: json_i64(&value, "ship_torpedo_attack")
                        .unwrap_or_default(),
                    ship_torpedo_attack_levelup: json_i64(&value, "ship_torpedo_attack_levelup")
                        .unwrap_or_default(),
                    carry_plane_count: json_i64(&value, "carry_plane_count").unwrap_or_default(),
                    hit: json_i64(&value, "hit").unwrap_or_default(),
                    dodge: json_i64(&value, "dodge").unwrap_or_default(),
                    crit: json_i64(&value, "crit").unwrap_or_default(),
                    anti_crit: json_i64(&value, "anti_crit").unwrap_or_default(),
                },
            ))
        })
        .collect();
    ShipStatCatalog { by_template }
}

pub(super) fn scaled_ship_stat(value: i64, multiplier: f64) -> i64 {
    let multiplier = normalize_multiplier(multiplier);
    ((value.max(0) as f64) * multiplier)
        .round()
        .clamp(0.0, i64::MAX as f64) as i64
}

#[cfg(test)]
pub(super) fn ship_attributes_for_template(
    template_id: i32,
    level: i64,
    catalog: Option<&ShipStatCatalog>,
    multiplier: f64,
) -> std::collections::BTreeMap<i32, i64> {
    ship_attributes_for_hero(None, template_id, level, catalog, multiplier)
}

pub(super) fn ship_attributes_for_hero(
    hero: Option<&Value>,
    template_id: i32,
    level: i64,
    catalog: Option<&ShipStatCatalog>,
    multiplier: f64,
) -> std::collections::BTreeMap<i32, i64> {
    let stats = catalog
        .and_then(|catalog| catalog.by_template.get(&template_id))
        .cloned()
        .unwrap_or_else(|| ShipStat {
            hp: 1_000,
            attack: 100,
            defense: 50,
            hit: 100,
            dodge: 35,
            ..ShipStat::default()
        });
    let level_delta = level.saturating_sub(1).max(0);
    let intensify = |attr_id: i32| {
        hero.and_then(|hero| hero_array(hero, "intensify"))
            .into_iter()
            .flatten()
            .find(|attr| json_i32(attr, "attrType") == Some(attr_id))
            .and_then(|attr| {
                json_i64(attr, "intensifyLvl").or_else(|| json_i64(attr, "intensifyLevel"))
            })
            .unwrap_or_default()
            .max(0)
    };
    let value = |attr_id: i32, base: i64, growth: i64| {
        let leveled = base.saturating_add(growth.saturating_mul(level_delta));
        scaled_ship_stat(leveled.saturating_add(intensify(attr_id)), multiplier)
    };
    let scout_num = if stats.carry_plane_count > 0 {
        stats.carry_plane_count
    } else {
        1
    };
    [
        (1, value(1, stats.hp, stats.hp_levelup)),
        (5, scout_num.saturating_add(intensify(5))),
        (8, value(8, stats.attack, stats.attack_levelup)),
        (9, value(9, stats.defense, stats.defense_levelup)),
        (
            10,
            value(10, stats.torpedo_attack, stats.torpedo_attack_levelup),
        ),
        (
            11,
            value(11, stats.torpedo_defense, stats.torpedo_defense_levelup),
        ),
        (
            14,
            value(14, stats.ship_bomb_attack, stats.ship_bomb_attack_levelup),
        ),
        (
            15,
            value(
                15,
                stats.ship_torpedo_attack,
                stats.ship_torpedo_attack_levelup,
            ),
        ),
        (
            17,
            scaled_ship_stat(stats.crit.saturating_add(intensify(17)), multiplier),
        ),
        (
            18,
            scaled_ship_stat(stats.anti_crit.saturating_add(intensify(18)), multiplier),
        ),
        (
            19,
            scaled_ship_stat(stats.hit.saturating_add(intensify(19)), multiplier),
        ),
        (
            20,
            scaled_ship_stat(stats.dodge.saturating_add(intensify(20)), multiplier),
        ),
    ]
    .into_iter()
    .collect()
}

pub(super) fn load_talent_catalog(client_path: Option<&PathBuf>) -> TalentCatalog {
    let Some(client_path) = client_path else {
        return TalentCatalog::default();
    };
    let dir = config_dir(client_path);
    let mut nodes = std::collections::BTreeMap::new();
    for (id, value) in read_config_rows(&dir.join("config_talent.db")) {
        if id <= 0 {
            continue;
        }
        nodes.insert(
            id,
            TalentNode {
                belong_talent: json_i32(&value, "belongtalent").unwrap_or_default(),
                next_talent: json_i32(&value, "nexttalent").unwrap_or_default(),
                precondition: json_i32_array(&value, "precondition"),
                costs: value
                    .get("levelup")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|cost| {
                        let values = cost.as_array()?;
                        Some((
                            i32::try_from(values.first()?.as_i64()?).ok()?,
                            i32::try_from(values.get(1)?.as_i64()?).ok()?,
                            values.get(2)?.as_i64()?.max(0),
                        ))
                    })
                    .filter(|(_, item_id, amount)| *item_id > 0 && *amount > 0)
                    .collect(),
            },
        );
    }
    let mut roots = Vec::new();
    for (_, value) in read_config_rows(&dir.join("config_talentmain.db")) {
        roots.extend(json_i32_array(&value, "talentlist"));
    }
    roots.extend(
        nodes
            .iter()
            .filter_map(|(id, node)| (node.belong_talent == 0).then_some(*id)),
    );
    roots.retain(|id| nodes.contains_key(id));
    roots.sort_unstable();
    roots.dedup();
    TalentCatalog { roots, nodes }
}

pub(super) fn compute_talent_target(
    catalog: &TalentCatalog,
    root_id: i32,
    reached: Option<i32>,
) -> (i32, Vec<i32>, i32) {
    let (target_id, is_operate) = match reached {
        Some(id) if id > 0 => match catalog.nodes.get(&id).map(|node| node.next_talent) {
            Some(next) if next > 0 => (next, 0),
            _ => (id, 1),
        },
        _ => (root_id, 0),
    };
    let pre = catalog
        .nodes
        .get(&target_id)
        .map(|node| node.precondition.clone())
        .unwrap_or_default();
    (target_id, pre, is_operate)
}

pub(super) fn talent_tree_payload_typed(
    account: &blueoath_domain::AccountState,
    catalog: &TalentCatalog,
) -> Vec<u8> {
    let mut out = Vec::new();
    for root in &catalog.roots {
        let reached = account
            .talents
            .active
            .get(&u64::try_from(*root).unwrap_or_default())
            .and_then(|id| i32::try_from(*id).ok());
        let (id, pre, operate) = compute_talent_target(catalog, *root, reached);
        append_message_field(&mut out, 1, &encode_talent_data(id, &pre, operate));
    }
    out
}

pub(super) fn talent_data_payload_typed(
    account: &blueoath_domain::AccountState,
    catalog: &TalentCatalog,
    talent_id: i32,
) -> Vec<u8> {
    let mut ret = Vec::new();
    if let Some(node) = catalog.nodes.get(&talent_id) {
        let root = if node.belong_talent > 0 {
            node.belong_talent
        } else {
            talent_id
        };
        let reached = account
            .talents
            .active
            .get(&u64::try_from(root).unwrap_or_default())
            .and_then(|id| i32::try_from(*id).ok());
        append_message_field(
            &mut ret,
            1,
            &encode_talent_data(
                talent_id,
                &node.precondition,
                i32::from(reached == Some(talent_id)),
            ),
        );
    }
    ret
}

pub(super) fn apply_talent_change_typed(
    account: &mut blueoath_domain::AccountState,
    catalog: &TalentCatalog,
    talent_id: i32,
) -> Result<(i32, Vec<i32>, i32), &'static str> {
    let node = catalog.nodes.get(&talent_id).ok_or("talent is invalid")?;
    let root = if node.belong_talent > 0 {
        node.belong_talent
    } else {
        talent_id
    };
    let root_key = u64::try_from(root).map_err(|_| "talent is invalid")?;
    let reached = account
        .talents
        .active
        .get(&root_key)
        .and_then(|id| i32::try_from(*id).ok());
    if let Some(current) = reached {
        if current == talent_id {
            return Err("talent is already active");
        }
        if catalog.nodes.get(&current).map(|node| node.next_talent) != Some(talent_id) {
            return Err("talent is not the next level");
        }
    } else if root != talent_id {
        return Err("talent root is not unlocked");
    }
    for (goods_type, item_id, amount) in &node.costs {
        let amount = u64::try_from(*amount).map_err(|_| "talent cost is invalid")?;
        let available = if *goods_type == 5 {
            talent_currency(*item_id)
                .map(|kind| account.resources.amount(kind).get())
                .ok_or("talent currency is unsupported")?
        } else if matches!(*goods_type, 1 | 6) {
            let template =
                blueoath_domain::TemplateId::new(u64::try_from(*item_id).unwrap_or_default())
                    .map_err(|_| "talent item is invalid")?;
            account
                .inventory
                .items
                .get(&template)
                .copied()
                .unwrap_or_default()
        } else {
            return Err("unsupported talent cost type");
        };
        if available < amount {
            return Err("not enough resource for talent");
        }
    }
    for (goods_type, item_id, amount) in &node.costs {
        let amount = u64::try_from(*amount).map_err(|_| "talent cost is invalid")?;
        if *goods_type == 5 {
            let kind = talent_currency(*item_id).ok_or("talent currency is unsupported")?;
            account
                .resources
                .debit(kind, amount)
                .map_err(|_| "not enough currency for talent")?;
        } else {
            let template =
                blueoath_domain::TemplateId::new(u64::try_from(*item_id).unwrap_or_default())
                    .map_err(|_| "talent item is invalid")?;
            let count = account
                .inventory
                .items
                .get_mut(&template)
                .ok_or("not enough items for talent")?;
            *count -= amount;
            if *count == 0 {
                account.inventory.items.remove(&template);
            }
        }
    }
    account
        .talents
        .active
        .insert(root_key, u64::try_from(talent_id).unwrap_or_default());
    Ok(compute_talent_target(catalog, root, Some(talent_id)))
}

fn talent_currency(item_id: i32) -> Option<blueoath_domain::CurrencyKind> {
    match item_id {
        1 => Some(blueoath_domain::CurrencyKind::Gold),
        2 => Some(blueoath_domain::CurrencyKind::Diamond),
        5 => Some(blueoath_domain::CurrencyKind::Supply),
        30 => Some(blueoath_domain::CurrencyKind::PvePoint),
        _ => None,
    }
}

pub(super) fn encode_talent_data(id: i32, precondition: &[i32], is_operate: i32) -> Vec<u8> {
    let mut out = Vec::new();
    append_varint_field(&mut out, 1, id.max(0) as u64);
    for pre in precondition {
        append_varint_field(&mut out, 2, (*pre).max(0) as u64);
    }
    append_varint_field(&mut out, 3, is_operate.max(0) as u64);
    out
}

pub(super) fn talent_change_payload(target: (i32, Vec<i32>, i32)) -> Vec<u8> {
    let mut out = Vec::new();
    append_message_field(
        &mut out,
        1,
        &encode_talent_data(target.0, &target.1, target.2),
    );
    out
}

pub(super) fn decode_talent_id(payload: &[u8]) -> i32 {
    decode_varint_field(payload, 1)
}

pub(super) fn load_shop_catalog(client_path: Option<&PathBuf>) -> ShopCatalog {
    let Some(client_path) = client_path else {
        return ShopCatalog::default();
    };
    let dir = config_dir(client_path);
    let valid_goods = read_config_rows(&dir.join("config_shop_goods.db"))
        .into_iter()
        .map(|(id, _)| id)
        .collect::<std::collections::HashSet<_>>();
    if valid_goods.is_empty() {
        return ShopCatalog::default();
    }
    let mut goods_by_shop = std::collections::BTreeMap::new();
    let mut costs_by_good_id = std::collections::BTreeMap::new();
    for (good_id, value) in read_config_rows(&dir.join("config_shop_goods.db")) {
        let costs = shop_costs_from_value(&value);
        if !costs.is_empty() {
            costs_by_good_id.insert(good_id, costs);
        }
    }
    for (shop_id, value) in read_config_rows(&dir.join("config_shop.db")) {
        if shop_id <= 0 {
            continue;
        }
        let mut goods = value_array_i64(&value, &["shelf_list", "shelfList"])
            .into_iter()
            .filter_map(|id| i32::try_from(id).ok())
            .filter(|id| valid_goods.contains(id))
            .collect::<Vec<_>>();
        goods.sort_unstable();
        goods.dedup();
        goods_by_shop.insert(shop_id, goods);
    }
    ShopCatalog {
        goods_by_shop,
        goods_by_id: std::collections::BTreeMap::new(),
        costs_by_good_id,
    }
}

pub(super) fn shop_costs_from_value(value: &Value) -> Vec<ShopCost> {
    let Some(currencies) = value.get("currency").and_then(Value::as_array) else {
        return Vec::new();
    };
    let Some(prices) = value.get("price").and_then(Value::as_array) else {
        return Vec::new();
    };
    currencies
        .iter()
        .zip(prices)
        .filter_map(|(currency, price)| {
            let currency = currency.as_array()?;
            let goods_type = i32::try_from(currency.first()?.as_i64()?).ok()?;
            let item_id = i32::try_from(currency.get(1)?.as_i64()?).ok()?;
            let amount = price.as_array()?.first()?.as_i64()?;
            (goods_type > 0 && item_id > 0 && amount > 0).then_some(ShopCost {
                goods_type,
                item_id,
                amount,
            })
        })
        .collect()
}

pub(super) fn load_server_shop_goods(catalog: &mut ShopCatalog, data_root: &Path) {
    // GM inventory is authoritative. Never retain client shelf IDs when the
    // server inventory is missing or malformed; those IDs cannot be purchased.
    catalog.goods_by_shop.clear();
    catalog.goods_by_id.clear();
    let path = data_root.join("gm-goods.json");
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let Ok(root) = serde_json::from_slice::<Value>(&bytes) else {
        return;
    };
    let Some(goods) = root.get("goods").and_then(Value::as_array) else {
        return;
    };
    for value in goods {
        let Some(good_id) = json_i32(value, "goodId") else {
            continue;
        };
        let Some(shop_id) = json_i32(value, "shopId") else {
            continue;
        };
        let Some(goods_type) = json_i32(value, "type") else {
            continue;
        };
        let Some(item_id) = json_i32(value, "itemId") else {
            continue;
        };
        let num = json_i32(value, "num").unwrap_or(1).max(1);
        if good_id > 0 && shop_id > 0 && item_id > 0 {
            catalog
                .goods_by_shop
                .entry(shop_id)
                .or_default()
                .push(good_id);
            catalog.goods_by_id.insert(
                good_id,
                ShopGood {
                    shop_id,
                    goods_type,
                    item_id,
                    num,
                    costs: catalog
                        .costs_by_good_id
                        .get(&good_id)
                        .cloned()
                        .unwrap_or_default(),
                },
            );
        }
    }
    for goods in catalog.goods_by_shop.values_mut() {
        goods.sort_unstable();
        goods.dedup();
    }
}

fn load_reward_definitions(dir: &Path) -> std::collections::BTreeMap<i32, Vec<ShopReward>> {
    read_config_rows(&dir.join("config_rewards.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            let rows = value.get("rewards")?.as_array()?;
            let rewards = rows
                .iter()
                .filter_map(|row| {
                    let row = row.as_array()?;
                    if row.len() < 3 {
                        return None;
                    }
                    let goods_type = i32::try_from(row[0].as_i64()?).ok()?;
                    let item_id = i32::try_from(row[1].as_i64()?).ok()?;
                    let num = i32::try_from(row[2].as_i64()?).ok()?;
                    (goods_type > 0 && item_id > 0 && num > 0).then_some(ShopReward {
                        goods_type,
                        item_id,
                        num,
                        instance_id: 0,
                    })
                })
                .collect::<Vec<_>>();
            (!rewards.is_empty()).then_some((id, rewards))
        })
        .collect()
}

pub(super) fn load_recharge_catalog(client_path: Option<&PathBuf>) -> RechargeCatalog {
    let Some(client_path) = client_path else {
        return RechargeCatalog::default();
    };
    let dir = config_dir(client_path);
    let rewards = load_reward_definitions(&dir);
    let mut catalog = RechargeCatalog::default();
    for (recharge_id, value) in read_config_rows(&dir.join("config_recharge.db")) {
        let Some(reward_id) = json_i32(&value, "reward") else {
            continue;
        };
        if let Some(reward) = rewards.get(&reward_id) {
            catalog
                .rewards_by_recharge_id
                .insert(recharge_id, reward.clone());
        }
    }
    catalog
}

pub(super) fn load_gameplay_catalog(client_path: Option<&PathBuf>) -> GameplayCatalog {
    let Some(client_path) = client_path else {
        return GameplayCatalog::default();
    };
    let dir = config_dir(client_path);
    let rows = |name: &str| {
        read_config_rows(&dir.join(name))
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    GameplayCatalog {
        rewards_by_id: load_reward_definitions(&dir),
        battlepass_levels: rows("config_battlepass_level.db"),
        battlepass_tasks: rows("config_battlepass_task.db"),
        battlepass_activity_levels: rows("config_battlepass_level_activity.db"),
        battlepass_activity_tasks: rows("config_battlepass_task_activity.db"),
        battlepass_param: read_config_rows(&dir.join("config_battlepass_param.db"))
            .into_iter()
            .next()
            .map(|(_, value)| value),
        battlepass_activity_param: read_config_rows(
            &dir.join("config_battlepass_param_activity.db"),
        )
        .into_iter()
        .next()
        .map(|(_, value)| value),
        activity: rows("config_activity.db"),
        parameters: rows("config_parameter.db"),
        activity_extract: rows("config_activity_extract.db"),
        activity_extract_ur: rows("config_activity_extract_ur.db"),
        anniversary_videos: rows("config_anniversary_video.db"),
        paper_cut_formulas: rows("config_interaction_paper_cut_fomula.db"),
        drop_items: rows("config_drop_item.db"),
        exchanges: rows("config_item_exchange.db"),
        food_recipes: rows("config_food_compose.db"),
        testship_tasks: rows("config_testship_task.db"),
        testship_rewards: rows("config_testship_reward.db"),
        world_events: rows("config_world_event.db"),
        world_event_tasks: rows("config_world_event_task.db"),
        guild_tasks: rows("config_task_guild.db"),
        guild_offer_info: read_config_rows(&dir.join("config_guildoffer_info.db"))
            .into_iter()
            .next()
            .map(|(_, value)| value),
        guild_offer_personal_rewards: rows("config_guildoffer_perscorereward.db"),
        guild_offer_rewards: rows("config_guildoffer_scorereward.db"),
        guild_war_base_info: rows("config_guildwar_base_info.db"),
        guild_war_rank: rows("config_guildwar_rank.db"),
        guild_war_rewards: rows("config_guildwar_reward.db"),
        magazine_info: rows("config_magazine_info.db"),
        magazine_pages: rows("config_magazine_page.db"),
        magazine_tasks: rows("config_task_magazine.db"),
        interaction_items: rows("config_interaction_item.db"),
        interaction_item_bags: rows("config_interaction_item_bag.db"),
        interaction_figures: rows("config_interaction_figurte.db"),
        guild_box_scores: rows("config_guildboxscore.db"),
        valentine_gifts: rows("config_item_valentine_gift.db"),
        sportsmeet_awards: rows("config_sportsmeet_award.db"),
        outpost_info: rows("config_outpost_info.db"),
        outpost_levels: rows("config_outpost_level.db"),
    }
}

pub(super) fn load_server_mail_templates(data_root: &Path) -> Vec<MailTemplate> {
    let path = data_root.join("gm-mails.json");
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let Ok(root) = serde_json::from_slice::<Value>(&bytes) else {
        return Vec::new();
    };
    let Some(mails) = root.get("mails").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut templates = mails
        .iter()
        .filter_map(|value| {
            let mid = value.get("mid").and_then(Value::as_u64)?;
            let goods_type = json_i32(value, "goodsType")?;
            let config_id = json_i32(value, "configId")?;
            Some(MailTemplate {
                mid,
                goods_type,
                config_id,
                num: json_i32(value, "num").unwrap_or(1).max(1),
                subject: json_string(value, "subject").unwrap_or_default(),
                content: json_string(value, "content").unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();
    templates.sort_unstable_by_key(|mail| mail.mid);
    templates.dedup_by_key(|mail| mail.mid);
    templates
}

pub(super) fn load_hero_level_catalog(client_path: Option<&PathBuf>) -> HeroLevelCatalog {
    let Some(client_path) = client_path else {
        return HeroLevelCatalog::default();
    };
    let dir = config_dir(client_path);
    let mut catalog = HeroLevelCatalog::default();
    for (item_id, value) in read_config_rows(&dir.join("config_ship_exp_item.db")) {
        if let Some(exp) = json_i32(&value, "exp") {
            catalog.exp_per_item.insert(item_id, exp.max(0));
        }
    }
    for (level, value) in read_config_rows(&dir.join("config_ship_levelup.db")) {
        if let Some(exp) = json_i32(&value, "exp") {
            catalog.exp_needed.insert(level, exp.max(0));
        }
    }
    catalog
}

pub(super) fn load_affection_catalog(client_path: Option<&PathBuf>) -> AffectionCatalog {
    let Some(client_path) = client_path else {
        return AffectionCatalog::default();
    };
    let exp_by_item = read_config_rows(&config_dir(client_path).join("config_affection_item.db"))
        .into_iter()
        .filter_map(|(item_id, value)| {
            let exp = json_i32(&value, "affection_exp")
                .or_else(|| json_i32(&value, "affectionExp"))
                .or_else(|| json_i32(&value, "exp"))?;
            (item_id > 0 && exp > 0).then_some((item_id, exp))
        })
        .collect();
    AffectionCatalog { exp_by_item }
}

pub(super) fn json_i64_pairs(value: &Value, key: &str) -> Vec<(i32, i64)> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            let attr_type = i32::try_from(row.first()?.as_i64()?).ok()?;
            let amount = row.get(1)?.as_i64()?;
            (attr_type > 0 && amount >= 0).then_some((attr_type, amount))
        })
        .collect()
}

pub(super) fn load_ship_intensify_catalog(client_path: Option<&PathBuf>) -> ShipIntensifyCatalog {
    let Some(client_path) = client_path else {
        return ShipIntensifyCatalog {
            same_type_ratio: 10_000,
            diamond_cost_per_hero: 5,
            ..ShipIntensifyCatalog::default()
        };
    };
    let dir = config_dir(client_path);
    let mut catalog = ShipIntensifyCatalog {
        same_type_ratio: 10_000,
        diamond_cost_per_hero: 5,
        ..ShipIntensifyCatalog::default()
    };
    for (template_id, value) in read_config_rows(&dir.join("config_ship_need_power_exp.db")) {
        let rows = json_i64_pairs(&value, "need_power_exp");
        if !rows.is_empty() {
            catalog.need_power_by_template.insert(
                template_id,
                (json_i32(&value, "enhance_type").unwrap_or_default(), rows),
            );
        }
    }
    for (template_id, value) in read_config_rows(&dir.join("config_ship_provide_power_exp.db")) {
        let rows = json_i64_pairs(&value, "provide_power_exp");
        if !rows.is_empty() {
            catalog.provide_power_by_template.insert(template_id, rows);
        }
    }
    for (template_id, value) in read_config_rows(&dir.join("config_ship_max_power.db")) {
        let rows = json_i64_pairs(&value, "max_power_prop");
        if !rows.is_empty() {
            catalog.max_power_by_template.insert(template_id, rows);
        }
    }
    for (id, value) in read_config_rows(&dir.join("config_parameter.db")) {
        match id {
            110 => {
                catalog.same_type_ratio = json_i64(&value, "value")
                    .filter(|value| *value > 0)
                    .unwrap_or(catalog.same_type_ratio)
            }
            31 => {
                catalog.diamond_cost_per_hero = json_i64(&value, "value")
                    .filter(|value| *value > 0)
                    .unwrap_or(catalog.diamond_cost_per_hero)
            }
            _ => {}
        }
    }
    catalog
}

pub(super) fn load_ship_break_catalog(client_path: Option<&PathBuf>) -> ShipBreakCatalog {
    let Some(client_path) = client_path else {
        return ShipBreakCatalog::default();
    };
    ShipBreakCatalog {
        by_template: read_config_rows(&config_dir(client_path).join("config_ship_break.db"))
            .into_iter()
            .filter(|(template_id, _)| *template_id > 0)
            .collect(),
    }
}

pub(super) fn load_ship_advance_catalog(client_path: Option<&PathBuf>) -> ShipAdvanceCatalog {
    let Some(client_path) = client_path else {
        return ShipAdvanceCatalog::default();
    };
    ShipAdvanceCatalog {
        by_level: read_config_rows(&config_dir(client_path).join("config_ship_advance.db"))
            .into_iter()
            .filter(|(level, _)| *level > 0)
            .collect(),
    }
}

pub(super) fn load_ship_remould_catalog(client_path: Option<&PathBuf>) -> ShipRemouldCatalog {
    let Some(client_path) = client_path else {
        return ShipRemouldCatalog::default();
    };
    let dir = config_dir(client_path);
    let mut ship_info_by_sf_id = std::collections::BTreeMap::new();
    for (id, value) in read_config_rows(&dir.join("config_ship_info.db")) {
        let sf_id = json_i32(&value, "sf_id").unwrap_or(id);
        if sf_id <= 0 {
            continue;
        }
        let has_remould = !json_i32_array(&value, "remould_template").is_empty();
        let replace = ship_info_by_sf_id
            .get(&sf_id)
            .map(|current: &Value| {
                has_remould && json_i32_array(current, "remould_template").is_empty()
            })
            .unwrap_or(true);
        if replace {
            ship_info_by_sf_id.insert(sf_id, value);
        }
    }
    let templates = read_config_rows(&dir.join("config_ship_remould_template.db"))
        .into_iter()
        .filter(|(id, _)| *id > 0)
        .collect();
    let effects = read_config_rows(&dir.join("config_ship_remould_effect.db"))
        .into_iter()
        .filter(|(id, _)| *id > 0)
        .collect();
    ShipRemouldCatalog {
        ship_info_by_sf_id,
        templates,
        effects,
    }
}

pub(super) fn load_commander_level_catalog(client_path: Option<&PathBuf>) -> CommanderLevelCatalog {
    let Some(client_path) = client_path else {
        return CommanderLevelCatalog::default();
    };
    let mut catalog = CommanderLevelCatalog::default();
    for (level, value) in
        read_config_rows(&config_dir(client_path).join("config_player_levelup.db"))
    {
        let level = json_i32(&value, "level").unwrap_or(level);
        let exp = json_i32(&value, "exp").unwrap_or_default();
        if level > 0 && exp > 0 {
            catalog.exp_needed.insert(level, exp);
        }
    }
    catalog
}

pub(super) fn load_hero_breakdown_catalog(client_path: Option<&PathBuf>) -> HeroBreakdownCatalog {
    let Some(client_path) = client_path else {
        return HeroBreakdownCatalog::default();
    };
    let mut catalog = HeroBreakdownCatalog::default();
    for (template_id, value) in
        read_config_rows(&config_dir(client_path).join("config_ship_main.db"))
    {
        let Some(entries) = value.get("break_down_get").and_then(Value::as_array) else {
            continue;
        };
        let rewards = entries
            .iter()
            .filter_map(|entry| {
                let values = entry.as_array()?;
                if values.len() < 3 {
                    return None;
                }
                let goods_type = i32::try_from(values[0].as_i64()?).ok()?;
                let config_id = i32::try_from(values[1].as_i64()?).ok()?;
                let num = i32::try_from(values[2].as_i64()?).ok()?;
                (goods_type > 0 && config_id > 0 && num > 0).then_some((goods_type, config_id, num))
            })
            .collect::<Vec<_>>();
        if !rewards.is_empty() {
            catalog.rewards_by_template.insert(template_id, rewards);
        }
    }
    catalog
}

pub(super) fn load_building_catalog(client_path: Option<&PathBuf>) -> BuildingCatalog {
    let Some(client_path) = client_path else {
        return BuildingCatalog::default();
    };
    let building_configs =
        read_config_rows(&config_dir(client_path).join("config_buildinginfo.db"))
            .into_iter()
            .filter(|(template_id, _)| *template_id > 0)
            .collect::<std::collections::BTreeMap<_, _>>();
    let capacities = building_configs
        .iter()
        .filter_map(|(template_id, value)| {
            let capacity = json_i32(value, "heronumber")?;
            (capacity >= 0).then_some((*template_id, capacity as usize))
        })
        .collect();
    let recipe_configs = read_config_rows(&config_dir(client_path).join("config_recipe.db"))
        .into_iter()
        .filter(|(recipe_id, _)| *recipe_id > 0)
        .collect();
    let resource_time_seconds =
        read_config_rows(&config_dir(client_path).join("config_parameter.db"))
            .into_iter()
            .filter_map(|(parameter_id, value)| {
                let seconds = json_i32(&value, "value")?;
                (parameter_id > 0 && seconds > 0).then_some((parameter_id, seconds))
            })
            .collect();
    BuildingCatalog {
        capacities,
        building_configs,
        recipe_configs,
        resource_time_seconds,
    }
}

pub(super) fn load_support_catalog(client_path: Option<&PathBuf>) -> SupportCatalog {
    let Some(client_path) = client_path else {
        return SupportCatalog::default();
    };
    let dir = config_dir(client_path);
    let rewards = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|row| {
                let row = row.as_array()?;
                let goods_type = i32::try_from(row.first()?.as_i64()?).ok()?;
                let item_id = i32::try_from(row.get(1)?.as_i64()?).ok()?;
                let num = i32::try_from(row.get(2)?.as_i64()?).ok()?;
                (goods_type > 0 && item_id > 0 && num > 0).then_some((goods_type, item_id, num))
            })
            .collect::<Vec<_>>()
    };
    let items = read_config_rows(&dir.join("config_support_fleet_item.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            let duration_seconds = value_i64_any(&value, &["time", "duration"]);
            (id > 0 && duration_seconds > 0).then_some((
                id,
                SupportFleetItem {
                    duration_seconds,
                    base_rewards: rewards(&value, "base_award"),
                    big_success_base_rewards: rewards(&value, "big_sucess_base_award"),
                    extra_drop_id: i32::try_from(value_i64_any(
                        &value,
                        &["extra_drop_id", "extraDropId"],
                    ))
                    .unwrap_or_default(),
                    big_success_extra_drop_id: i32::try_from(value_i64_any(
                        &value,
                        &["big_sucess_extra_drop_id", "bigSuccessExtraDropId"],
                    ))
                    .unwrap_or_default(),
                    big_success_ratio: i32::try_from(value_i64_any(
                        &value,
                        &["big_sucess_ratio", "bigSuccessRatio"],
                    ))
                    .unwrap_or_default(),
                    consumption: {
                        let values = value_array_i64(&value, &["consumption"]);
                        if values.len() >= 3 {
                            match (
                                i32::try_from(values[0]),
                                i32::try_from(values[1]),
                                i32::try_from(values[2]),
                            ) {
                                (Ok(goods_type), Ok(item_id), Ok(amount)) => {
                                    Some((goods_type, item_id, amount))
                                }
                                _ => None,
                            }
                        } else {
                            None
                        }
                    },
                    fast_consumption: {
                        let values = value_array_i64(&value, &["complete_item"]);
                        if values.len() >= 3 {
                            match (
                                i32::try_from(values[0]),
                                i32::try_from(values[1]),
                                i32::try_from(values[2]),
                            ) {
                                (Ok(goods_type), Ok(item_id), Ok(amount)) => {
                                    Some((goods_type, item_id, amount))
                                }
                                _ => None,
                            }
                        } else {
                            None
                        }
                    },
                },
            ))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let drop_rewards = read_config_rows(&dir.join("config_drop_item.db"))
        .into_iter()
        .filter_map(|(drop_id, value)| {
            let rows = ["drop", "drop_alone"]
                .into_iter()
                .filter_map(|key| value.get(key).and_then(Value::as_array))
                .flatten()
                .filter_map(|row| {
                    let row = row.as_array()?;
                    let goods_type = i32::try_from(row.first()?.as_i64()?).ok()?;
                    let item_id = i32::try_from(row.get(1)?.as_i64()?).ok()?;
                    let num = i32::try_from(row.get(2)?.as_i64()?).ok()?;
                    (goods_type > 0 && item_id > 0 && num > 0).then_some((goods_type, item_id, num))
                })
                .collect::<Vec<_>>();
            (!rows.is_empty()).then_some((drop_id, rows))
        })
        .collect();
    SupportCatalog {
        items,
        drop_rewards,
    }
}

pub(super) fn load_handbook_behaviours(client_path: Option<&PathBuf>) -> Vec<i32> {
    let Some(client_path) = client_path else {
        return Vec::new();
    };
    let mut ids =
        read_config_rows(&config_dir(client_path).join("config_handbook_behaviour_index.db"))
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| *id > 0)
            .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub(super) fn load_hero_memories(client_path: Option<&PathBuf>) -> Vec<(i32, i32)> {
    let Some(client_path) = client_path else {
        return Vec::new();
    };
    let mut memories =
        read_config_rows(&config_dir(client_path).join("config_building_character_story.db"))
            .into_iter()
            .filter_map(|(plot_id, value)| {
                let hero_id = value
                    .get("ship_fleet_id")
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())?;
                (hero_id > 0 && plot_id > 0).then_some((hero_id, plot_id))
            })
            .collect::<Vec<_>>();
    memories.sort_unstable();
    memories.dedup();
    memories
}

pub(super) fn value_i64_any(value: &Value, keys: &[&str]) -> i64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_i64))
        .unwrap_or_default()
}

pub(super) fn value_array_i64(value: &Value, keys: &[&str]) -> Vec<i64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_array))
        .map(|items| items.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default()
}

pub(super) fn task_definition_from_value(
    task_type: i32,
    id: i32,
    value: &Value,
) -> Option<TaskDefinition> {
    let goal = value_array_i64(value, &["goal"]);
    let event_type = i32::try_from(*goal.first()?).ok()?;
    let event_param = matches!(event_type, 16 | 17 | 24 | 900)
        .then(|| goal.get(1).and_then(|value| i32::try_from(*value).ok()))
        .flatten();
    let target = if goal.len() == 2 && matches!(event_type, 16 | 17) {
        1
    } else {
        i32::try_from(*goal.last()?).ok()?.max(0)
    };
    if id <= 0 || target <= 0 {
        return None;
    }
    Some(TaskDefinition {
        task_type,
        id,
        event_type,
        event_param,
        goal: target,
        level_min: i32::try_from(value_i64_any(
            value,
            &["playerLevelMin", "player_level_min"],
        ))
        .unwrap_or_default(),
        level_max: i32::try_from(value_i64_any(
            value,
            &["playerLevelMax", "player_level_max"],
        ))
        .unwrap_or_default(),
        abandoned: i32::try_from(value_i64_any(value, &["abandoned"])).unwrap_or_default(),
        next_task_id: i32::try_from(value_i64_any(value, &["nextTaskId", "next_task_id"]))
            .unwrap_or_default(),
        previous_task_id: 0,
        medal_id: i32::try_from(value_i64_any(value, &["medalId", "medal_id"])).unwrap_or_default(),
        point: i32::try_from(value_i64_any(value, &["point"])).unwrap_or_default(),
        reward_id: i32::try_from(value_i64_any(value, &["rewards", "reward_id"]))
            .unwrap_or_default(),
        inline_rewards: value
            .get("reward")
            .or_else(|| value.get("rewardsList"))
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| {
                        let a = row.as_array()?;
                        (a.len() >= 3).then_some((
                            i32::try_from(a[0].as_i64()?).ok()?,
                            i32::try_from(a[1].as_i64()?).ok()?,
                            i32::try_from(a[2].as_i64()?).ok()?,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

pub(super) fn load_task_catalog(client_path: Option<&PathBuf>) -> TaskCatalog {
    let Some(client_path) = client_path else {
        return TaskCatalog::default();
    };
    let dir = config_dir(client_path);
    let mut definitions = Vec::new();
    for (task_type, file) in [
        (1, "config_task_main.db"),
        (2, "config_task_daily.db"),
        (3, "config_task_weekly.db"),
        (4, "config_task_grow.db"),
        (12, "config_task_return.db"),
        (6, "config_task_activity.db"),
        (10, "config_task_treaty.db"),
    ] {
        for (id, value) in read_config_rows(&dir.join(file)) {
            if let Some(definition) = task_definition_from_value(task_type, id, &value) {
                definitions.push(definition);
            }
        }
    }
    for (id, value) in read_config_rows(&dir.join("config_achievement.db")) {
        if let Some(mut definition) = task_definition_from_value(5, id, &value) {
            definition.previous_task_id = i32::try_from(value_i64_any(
                &value,
                &["lastAchievement", "last_achievement"],
            ))
            .unwrap_or_default();
            definition.medal_id =
                i32::try_from(value_i64_any(&value, &["medalId", "medal_id"])).unwrap_or_default();
            definition.point = i32::try_from(value_i64_any(&value, &["point"])).unwrap_or_default();
            definitions.push(definition);
        }
    }
    let teaching = read_config_rows(&dir.join("config_task_teaching.db"))
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();
    for (_group_id, group) in read_config_rows(&dir.join("config_task_teaching_group.db")) {
        for (task_type, key) in [(8, "task_daily_id"), (9, "task_assess_id")] {
            for id in value_array_i64(&group, &[key]) {
                let Ok(id) = i32::try_from(id) else { continue };
                if let Some(value) = teaching.get(&id) {
                    if let Some(definition) = task_definition_from_value(task_type, id, value) {
                        definitions.push(definition);
                    }
                }
            }
        }
    }
    definitions.sort_by_key(|definition| (definition.task_type, definition.id));
    definitions.dedup_by_key(|definition| (definition.task_type, definition.id));
    // Normal task tables expose only next_task_id; derive previous links like C# catalog.
    let mut previous = std::collections::HashMap::new();
    for definition in &definitions {
        if definition.next_task_id > 0 {
            previous
                .entry((definition.task_type, definition.next_task_id))
                .or_insert(definition.id);
        }
    }
    for definition in &mut definitions {
        if let Some(previous_id) = previous.get(&(definition.task_type, definition.id)) {
            definition.previous_task_id = *previous_id;
        }
    }
    let rewards_by_id = read_config_rows(&dir.join("config_rewards.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            let rows = value
                .get("reward")
                .or_else(|| value.get("rewards"))
                .and_then(Value::as_array)?;
            let parsed = rows
                .iter()
                .filter_map(|row| {
                    let a = row.as_array()?;
                    (a.len() >= 3).then_some((
                        i32::try_from(a[0].as_i64()?).ok()?,
                        i32::try_from(a[1].as_i64()?).ok()?,
                        i32::try_from(a[2].as_i64()?).ok()?,
                    ))
                })
                .collect::<Vec<_>>();
            Some((id, parsed))
        })
        .collect();
    let teaching_rewards_by_id = read_config_rows(&dir.join("config_teaching_achievement.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            let reward_id = json_i32(&value, "rewards")?;
            (id > 0 && reward_id > 0).then_some((id, reward_id))
        })
        .collect();
    TaskCatalog {
        definitions,
        rewards_by_id,
        teaching_rewards_by_id,
    }
}

pub(super) fn load_build_ship_catalog(client_path: Option<&PathBuf>) -> BuildShipCatalog {
    let Some(client_path) = client_path else {
        return BuildShipCatalog::default();
    };
    let dir = config_dir(client_path);
    let mut catalog = BuildShipCatalog::default();
    for (pool_id, value) in read_config_rows(&dir.join("config_extract_ship.db")) {
        if let Some(extract_type) = json_i32(&value, "extract_type") {
            catalog.extract_type_by_pool.insert(pool_id, extract_type);
        }
        if let Some(drop_id) =
            value_i64_any_opt(&value, "drop_item_id").and_then(|v| i32::try_from(v).ok())
        {
            if drop_id > 0 {
                catalog.extract_to_drop.insert(pool_id, drop_id);
            }
        }
        let parse_costs = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|row| {
                    let a = row.as_array()?;
                    if a.len() < 3 {
                        return None;
                    }
                    Some((
                        i32::try_from(a[0].as_i64()?).ok()?,
                        i32::try_from(a[1].as_i64()?).ok()?,
                        i32::try_from(a[2].as_i64()?).ok()?,
                    ))
                })
                .filter(|(goods_type, item_id, num)| *goods_type > 0 && *item_id > 0 && *num > 0)
                .collect::<Vec<_>>()
        };
        let expend = parse_costs("expend");
        if !expend.is_empty() {
            catalog.expend_by_pool.insert(pool_id, expend);
        }
        let ten_expend = parse_costs("new_ten_expend");
        if !ten_expend.is_empty() {
            catalog.ten_expend_by_pool.insert(pool_id, ten_expend);
        }
        if let Some(rows) = value.get("twenty_drop").and_then(Value::as_array) {
            for row in rows {
                let Some(a) = row.as_array() else { continue };
                if a.len() >= 2 {
                    if let (Some(count), Some(drop_id)) = (a[0].as_i64(), a[1].as_i64()) {
                        if let (Ok(count), Ok(drop_id)) =
                            (i32::try_from(count), i32::try_from(drop_id))
                        {
                            if count > 0 && drop_id > 0 {
                                catalog
                                    .box_drop_by_pool_count
                                    .insert((pool_id, count), drop_id);
                            }
                        }
                    }
                }
            }
        }
        if let Some(rows) = value.get("hundred_reward").and_then(Value::as_array) {
            for row in rows {
                let Some(a) = row.as_array() else { continue };
                if a.len() >= 4 {
                    let parsed = [a[0].as_i64(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64()];
                    if let [Some(count), Some(goods_type), Some(item_id), Some(num)] = parsed {
                        if let (Ok(count), Ok(goods_type), Ok(item_id), Ok(num)) = (
                            i32::try_from(count),
                            i32::try_from(goods_type),
                            i32::try_from(item_id),
                            i32::try_from(num),
                        ) {
                            if count > 0 && goods_type > 0 && item_id > 0 && num > 0 {
                                catalog
                                    .reward_by_pool_count
                                    .insert((pool_id, count), (goods_type, item_id, num));
                            }
                        }
                    }
                }
            }
        }
    }
    for (ship_info_id, value) in read_config_rows(&dir.join("config_ship_info.db")) {
        let template_id = ship_info_id.saturating_mul(10).saturating_add(1);
        if template_id <= 1 {
            continue;
        }
        let defaults = (1..=6)
            .filter_map(|slot| {
                value
                    .get(format!("equip{slot}"))
                    .and_then(Value::as_i64)
                    .and_then(|v| i32::try_from(v).ok())
                    .filter(|id| *id > 0)
            })
            .collect::<Vec<_>>();
        if !defaults.is_empty() {
            catalog.ship_defaults.insert(template_id, defaults);
        }
        if let Some(quality) = json_i32(&value, "quality").filter(|v| *v > 0) {
            catalog.ship_quality.insert(template_id, quality);
        }
        if let Some(seconds) = value
            .get("build_time")
            .and_then(Value::as_i64)
            .and_then(|v| i32::try_from(v).ok())
            .filter(|v| *v > 0)
        {
            catalog.ship_build_time.insert(template_id, seconds);
        }
    }
    for (drop_id, value) in read_config_rows(&dir.join("config_drop_item.db")) {
        let mut entries = Vec::new();
        let mut rows = Vec::new();
        for key in ["drop", "drop_alone"] {
            if let Some(values) = value.get(key).and_then(Value::as_array) {
                rows.extend(values.iter());
            }
        }
        for row in rows {
            let Some(a) = row.as_array() else {
                continue;
            };
            if a.len() < 5 {
                continue;
            }
            let vals = a
                .iter()
                .take(5)
                .filter_map(Value::as_i64)
                .collect::<Vec<_>>();
            if vals.len() == 5 {
                entries.push((
                    vals[0] as i32,
                    vals[1] as i32,
                    vals[2] as i32,
                    vals[3] as i32,
                    vals[4] as i32,
                ));
            }
        }
        if !entries.is_empty() {
            catalog.pools.insert(drop_id, entries);
        }
    }
    for (item_id, value) in read_config_rows(&dir.join("config_item_info.db")) {
        if let Some(drop_id) = json_i32(&value, "drop_id").filter(|id| *id > 0) {
            catalog.treasure_drop_by_item.insert(item_id, drop_id);
        }
    }
    for (item_id, value) in read_config_rows(&dir.join("config_item_selected.db")) {
        let options = value
            .get("item_id")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|row| {
                let values = row.as_array()?;
                (values.len() >= 3).then_some((
                    i32::try_from(values[0].as_i64()?).ok()?,
                    i32::try_from(values[1].as_i64()?).ok()?,
                    i32::try_from(values[2].as_i64()?).ok()?,
                ))
            })
            .filter(|(goods_type, config_id, num)| *goods_type > 0 && *config_id > 0 && *num > 0)
            .collect::<Vec<_>>();
        let drop_id = json_i32(&value, "drop_id").unwrap_or_default();
        if item_id > 0 && (!options.is_empty() || drop_id > 0) {
            catalog
                .selected_treasure_by_item
                .insert(item_id, SelectedTreasure { drop_id, options });
        }
    }
    catalog
}

pub(super) fn load_build_formula_catalog(
    client_path: Option<&PathBuf>,
) -> BuildFormulaCatalogRuntime {
    let Some(client_path) = client_path else {
        return BuildFormulaCatalogRuntime::default();
    };
    let dir = config_dir(client_path);
    let mut configured_rows = Vec::new();
    for (_id, value) in read_config_rows(&dir.join("config_build_ship.db")) {
        let range = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
                .unwrap_or_default()
        };
        let ships = value
            .get("ship_list")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_i64)
                    .filter_map(|v| i32::try_from(v).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if ships.is_empty() {
            continue;
        }
        configured_rows.push((range("res1"), range("res2"), range("res3"), ships));
    }
    // C# construction gives each open handbook ship one deterministic exact
    // formula. Put these exact rows first so broad package ranges cannot shadow
    // a formula shown by buildnotes/discuss.
    let ship_main_ids = read_config_rows(&dir.join("config_ship_main.db"))
        .into_iter()
        .map(|(id, _)| id)
        .collect::<std::collections::HashSet<_>>();
    let mut handbook_ids = read_config_rows(&dir.join("config_ship_handbook.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            let show_tag = json_i32(&value, "show_tag").unwrap_or_default();
            let show_state = json_i32(&value, "show_state").unwrap_or_default();
            (matches!(show_tag, 0 | 2 | 3)
                && show_state == 1
                && ship_main_ids.contains(&id.saturating_mul(10).saturating_add(1)))
            .then_some(id)
        })
        .collect::<Vec<_>>();
    handbook_ids.sort_unstable();
    handbook_ids.dedup();
    let generated_rows = handbook_ids
        .into_iter()
        .enumerate()
        .map(|(index, ship_info_id)| {
            let span = 999_i64 - 30 + 1;
            let gold = 30 + (index as i64 % span);
            let steel = 30 + ((index as i64 * 173 + 7) % span);
            let aluminium = 30 + ((index as i64 * 337 + 13) % span);
            (
                vec![gold, gold],
                vec![steel, steel],
                vec![aluminium, aluminium],
                vec![ship_info_id.saturating_mul(10).saturating_add(1)],
            )
        })
        .collect::<Vec<_>>();
    let mut rows = generated_rows;
    rows.extend(configured_rows);
    BuildFormulaCatalogRuntime(rows)
}

pub(super) fn value_i64_any_opt(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

pub(super) fn mix_build_draw_roll(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

pub(super) fn draw_build_ship_reward_with_roll(
    catalog: &BuildShipCatalog,
    pool_id: i32,
    roll: u64,
) -> Option<(i32, i32, i32)> {
    let extract = *catalog.extract_to_drop.get(&pool_id)?;
    draw_build_drop_reward_with_roll(catalog, extract, roll)
}

pub(super) fn draw_build_drop_reward_with_roll(
    catalog: &BuildShipCatalog,
    drop_id: i32,
    mut roll: u64,
) -> Option<(i32, i32, i32)> {
    let mut current = drop_id;
    for _ in 0..8 {
        let entries = catalog.pools.get(&current)?;
        let total: i64 = entries.iter().map(|e| i64::from(e.4.max(0))).sum();
        if total <= 0 {
            return None;
        }
        let mut offset = (roll % total as u64) as i64;
        let mut picked = entries.last()?;
        for entry in entries {
            offset -= i64::from(entry.4.max(0));
            if offset < 0 {
                picked = entry;
                break;
            }
        }
        if picked.0 == 4 {
            current = picked.1;
            roll = mix_build_draw_roll(roll ^ current as u64);
            continue;
        }
        return Some((picked.0, picked.1, picked.2.max(1)));
    }
    None
}

pub(super) fn draw_build_ship_reward(pool_id: i32) -> Option<(i32, i32, i32)> {
    let catalog = BUILD_SHIP_CATALOG.get()?;
    // A ten-pull can execute within one millisecond. Blend a process-wide nonce
    // with wall-clock time so each draw gets a separate weighted roll.
    let sequence = BUILD_DRAW_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let roll = mix_build_draw_roll(u64::from(current_unix_millis()) ^ sequence);
    draw_build_ship_reward_with_roll(catalog, pool_id, roll)
}

pub(super) fn draw_sr_build_reward_with_roll(
    catalog: &BuildShipCatalog,
    drop_id: i32,
    roll: u64,
) -> Option<(i32, i32, i32)> {
    fn collect(
        catalog: &BuildShipCatalog,
        drop_id: i32,
        inherited_probability: f64,
        depth: u8,
        candidates: &mut Vec<(i32, i32, i32, f64)>,
    ) {
        if depth > 8 || inherited_probability <= 0.0 {
            return;
        }
        let Some(entries) = catalog.pools.get(&drop_id) else {
            return;
        };
        let total_weight: i64 = entries.iter().map(|entry| i64::from(entry.4.max(0))).sum();
        if total_weight <= 0 {
            return;
        }
        for &(goods_type, item_id, num, _, weight) in entries {
            let weight = weight.max(0);
            if weight == 0 {
                continue;
            }
            let probability = inherited_probability * f64::from(weight) / total_weight as f64;
            if goods_type == 4 {
                collect(catalog, item_id, probability, depth + 1, candidates);
            } else if goods_type == 3
                && catalog
                    .ship_quality
                    .get(&item_id)
                    .copied()
                    .unwrap_or_default()
                    >= 3
            {
                candidates.push((goods_type, item_id, num.max(1), probability));
            }
        }
    }

    let mut candidates = Vec::new();
    collect(catalog, drop_id, 1.0, 0, &mut candidates);
    let total = candidates.iter().fold(0.0_f64, |sum, entry| sum + entry.3);
    if total <= 0.0 {
        return None;
    }
    // Keep 53 random bits: f64 can represent every integer in this range and
    // the upper bound remains strictly below one.
    let mut offset = ((roll >> 11) as f64 / (1_u64 << 53) as f64) * total;
    for (goods_type, item_id, num, weight) in candidates {
        offset -= weight;
        if offset < 0.0 {
            return Some((goods_type, item_id, num));
        }
    }
    None
}

pub(super) fn build_drop_exists(catalog: &BuildShipCatalog, pool_id: i32) -> bool {
    let Some(extract) = catalog.extract_to_drop.get(&pool_id).copied() else {
        return false;
    };
    !expand_build_drop(catalog, extract).is_empty()
}

pub(super) fn load_battle_catalog(client_path: Option<&PathBuf>) -> BattleCatalog {
    let Some(client_path) = client_path else {
        return BattleCatalog::default();
    };
    let dir = config_dir(client_path);
    let mut catalog = BattleCatalog::default();
    // This file is server-owned balance data, independent of client assets.
    let quantities_path = dir
        .parent()
        .unwrap_or(&dir)
        .join("battle-drop-quantities.json");
    let bundled_quantities = include_str!("../../../catalog/battle-drop-quantities.json");
    let quantities = match std::fs::read_to_string(&quantities_path) {
        Ok(value) => value,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!(
                    "Cannot read {}: {error}; using bundled drop quantities",
                    quantities_path.display()
                );
            }
            bundled_quantities.to_owned()
        }
    };
    catalog.drop_quantities = serde_json::from_str(&quantities).unwrap_or_else(|error| {
        eprintln!(
            "Invalid {}: {error}; using bundled drop quantities",
            quantities_path.display()
        );
        serde_json::from_str(bundled_quantities).unwrap_or_default()
    });
    let copy_types = read_config_rows(&dir.join("config_chapter.db"))
        .into_iter()
        .flat_map(|(_, value)| {
            let copy_type = if value_i64_any(&value, &["chapter_plot_type"]) > 0
                || value_i64_any(&value, &["memory_id"]) > 0
            {
                1
            } else {
                match value_i64_any(&value, &["class_type"]) {
                    2 | 9 | 10 | 24 | 32 | 33 | 34 | 69 | 71 => {
                        value_i64_any(&value, &["class_type"]) as i32
                    }
                    _ => 0,
                }
            };
            (copy_type > 0).then(|| {
                json_i32_array(&value, "level_list")
                    .into_iter()
                    .map(move |copy_id| (copy_id, copy_type))
                    .collect::<Vec<_>>()
            })
        })
        .flatten()
        .collect::<std::collections::HashMap<_, _>>();
    for (_, value) in read_config_rows(&dir.join("config_chapter.db")) {
        if value_i64_any(&value, &["class_type"]) != 9 {
            continue;
        }
        let group_id = json_i32(&value, "dailygroup_id").unwrap_or_default();
        if group_id <= 0 {
            continue;
        }
        for copy_id in json_i32_array(&value, "level_list") {
            catalog.daily_group_by_copy.insert(copy_id, group_id);
        }
        for copy_id in json_i32_array(&value, "treaty_copy") {
            catalog.daily_group_by_copy.insert(copy_id, group_id);
        }
    }
    let mut candidates = std::collections::HashMap::<i32, (bool, BattleCopy)>::new();
    for (row_id, value) in read_config_rows(&dir.join("config_copy.db")) {
        let Some(copy_id) = json_i32(&value, "copy_id") else {
            continue;
        };
        if copy_id <= 0 {
            continue;
        }
        let fleet_ids = json_i32_array(&value, "fleet_id");
        if fleet_ids.is_empty() {
            continue;
        }
        let is_default = value_i64_any(&value, &["blood_range_lower"]) == -1
            && value_i64_any(&value, &["random_weight"]) == 1000;
        let copy = BattleCopy {
            config_id: row_id,
            copy_type: copy_types
                .get(&copy_id)
                .copied()
                .or_else(|| json_i32(&value, "copy_type"))
                .unwrap_or(1),
            fleet_ids: fleet_ids.clone(),
        };
        let replace = candidates
            .get(&copy_id)
            .is_none_or(|(current_default, _)| is_default && !current_default);
        if replace {
            candidates.insert(copy_id, (is_default, copy.clone()));
        }
        if is_default {
            catalog.copies.insert(copy_id, copy);
        }
    }
    for (copy_id, (_, copy)) in candidates {
        catalog.copies.entry(copy_id).or_insert(copy);
    }
    for (fleet_id, value) in read_config_rows(&dir.join("config_fleet.db")) {
        let attached_ids = value
            .get("copy_attacheds")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|row| row.as_array()?.first()?.as_i64())
            .filter_map(|id| i32::try_from(id).ok())
            .filter(|id| *id > 0)
            .collect::<Vec<_>>();
        if !attached_ids.is_empty() {
            catalog.attached_fleet_ids.insert(fleet_id, attached_ids);
        }
        let enemies = json_i32_array(&value, "copy_enemys");
        if !enemies.is_empty() {
            catalog.fleet_enemies.insert(fleet_id, enemies);
        }
        catalog
            .fleet_is_last
            .insert(fleet_id, value_i64_any(&value, &["is_last_fleet"]) == 1);
        let ship_exp = json_i32(&value, "ship_exp").unwrap_or_default().max(0);
        let commander_exp = json_i32(&value, "player_exp").unwrap_or_default().max(0);
        if ship_exp > 0 || commander_exp > 0 {
            catalog.fleet_rewards.insert(
                fleet_id,
                BattleFleetReward {
                    ship_exp,
                    commander_exp,
                },
            );
        }
        let mut drop_ids = Vec::new();
        if let Some(drop_id) = json_i32(&value, "drop_id").filter(|id| *id > 0) {
            drop_ids.push(drop_id);
        }
        if !drop_ids.is_empty() {
            catalog.fleet_drop_ids.insert(fleet_id, drop_ids);
        }
        let mut other_drop_ids = Vec::new();
        if let Some(drop_id) = json_i32(&value, "other_drop_id").filter(|id| *id > 0) {
            other_drop_ids.push(drop_id);
        }
        other_drop_ids.extend(
            json_i32_array(&value, "other_drop_ids")
                .into_iter()
                .filter(|id| *id > 0),
        );
        if !other_drop_ids.is_empty() {
            catalog
                .fleet_other_drop_ids
                .insert(fleet_id, other_drop_ids);
        }
        let mut settle_drop_ids = Vec::new();
        if let Some(drop_id) = json_i32(&value, "settle_drop_id").filter(|id| *id > 0) {
            settle_drop_ids.push(drop_id);
        }
        settle_drop_ids.extend(
            json_i32_array(&value, "settle_drop_ids")
                .into_iter()
                .filter(|id| *id > 0),
        );
        if !settle_drop_ids.is_empty() {
            catalog
                .fleet_settle_drop_ids
                .insert(fleet_id, settle_drop_ids);
        }
    }
    for (enemy_id, value) in read_config_rows(&dir.join("config_ship_enemy.db")) {
        let hp = json_i32(&value, "hp").unwrap_or_default();
        if hp <= 0 {
            continue;
        }
        catalog.enemies.insert(
            enemy_id,
            BattleEnemy {
                hp,
                ship_info_id: json_i32(&value, "ship_info_id").unwrap_or_default(),
                attack: json_i32(&value, "attack").unwrap_or_default(),
                defense: json_i32(&value, "defense").unwrap_or_default(),
                hit: json_i32(&value, "hit").unwrap_or(100),
                dodge: json_i32(&value, "dodge").unwrap_or_default(),
                crit: json_i32(&value, "crit").unwrap_or_default(),
                anti_crit: json_i32(&value, "anti_crit").unwrap_or_default(),
                torpedo: json_i32(&value, "torpedo").unwrap_or_default(),
                torpedo_defense: json_i32(&value, "torpedo_defense").unwrap_or_default(),
            },
        );
    }
    // Large-activity sea copies 20243-20246 keep copy-specific enemy IDs in
    // config_fleet, while config_ship_enemy stores six shared 15002024x
    // templates. Preserve wire IDs for the client, but attach canonical stats
    // so battle payloads and server-side validation do not fall back to 1000 HP.
    let challenge_enemy_aliases = battle_challenge_enemy_aliases(&catalog.enemies);
    for (fleet_id, enemy_ids) in &catalog.fleet_enemies {
        if !is_large_activity_sea_fleet(*fleet_id) {
            continue;
        }
        for enemy_id in enemy_ids {
            if catalog.enemies.contains_key(enemy_id) {
                continue;
            }
            if let Some(canonical_id) = challenge_enemy_aliases.get(enemy_id) {
                if let Some(stats) = catalog.enemies.get(canonical_id).cloned() {
                    catalog.enemies.insert(*enemy_id, stats);
                }
            }
        }
    }
    let factor_groups = read_config_rows(&dir.join("config_random_factor_group.db"))
        .into_iter()
        .map(|(id, value)| (id, value_array_i64(&value, &["factor"])))
        .collect::<std::collections::HashMap<_, _>>();
    let factor_sets = read_config_rows(&dir.join("config_random_factor_set.db"))
        .into_iter()
        .map(|(id, value)| (id, json_i32_array(&value, "factor_groups")))
        .collect::<std::collections::HashMap<_, _>>();
    let reward_rows = read_config_rows(&dir.join("config_rewards.db"))
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();
    for (rank_drop_id, value) in read_config_rows(&dir.join("config_copy_rank_drop.db")) {
        let rewards = value
            .get("drop_group")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|row| {
                let row = row.as_array()?;
                let grade = i32::try_from(row.first()?.as_i64()?).ok()?;
                let goods_type = i32::try_from(row.get(1)?.as_i64()?).ok()?;
                let item_id = i32::try_from(row.get(2)?.as_i64()?).ok()?;
                (grade > 0 && goods_type > 0 && item_id > 0).then_some((grade, goods_type, item_id))
            })
            .collect::<Vec<_>>();
        if !rewards.is_empty() {
            catalog.rank_drop_rewards.insert(rank_drop_id, rewards);
        }
    }
    for (copy_id, value) in read_config_rows(&dir.join("config_copy_display.db")) {
        if value_i64_any(&value, &["search_3d", "search3d"]) > 0 {
            catalog.search_3d.insert(copy_id);
        }
        let set_ids = json_i32_array(&value, "random_factor_sets");
        let entries = set_ids
            .into_iter()
            .filter_map(|set_id| {
                let group_id = *factor_sets.get(&set_id)?.first()?;
                let factors = factor_groups
                    .get(&group_id)
                    .into_iter()
                    .flatten()
                    .filter_map(|factor| i32::try_from(*factor).ok())
                    .collect::<Vec<_>>();
                (!factors.is_empty()).then_some(RandomFactorEntry {
                    set_id,
                    group_id,
                    factors,
                })
            })
            .collect::<Vec<_>>();
        if !entries.is_empty() {
            catalog.random_factors.insert(copy_id, entries);
        }
    }
    for (copy_id, value) in read_config_rows(&dir.join("config_copy_display.db")) {
        let drop_ids = value_array_i64(&value, &["drop_info_id", "dropInfoId"])
            .into_iter()
            .filter_map(|id| i32::try_from(id).ok())
            .filter(|id| *id > 0)
            .collect::<Vec<_>>();
        if !drop_ids.is_empty() {
            catalog.copy_drop_ids.insert(copy_id, drop_ids);
        }
        catalog.supply_cost_by_copy.insert(
            copy_id,
            (
                json_i64(&value, "supply_basic_cost")
                    .unwrap_or_default()
                    .max(0),
                json_i64(&value, "supple_cost_argu")
                    .unwrap_or_default()
                    .max(0),
            ),
        );
        let first = value_array_i64(&value, &["first_reward", "firstReward"])
            .into_iter()
            .filter_map(|id| i32::try_from(id).ok())
            .filter(|id| *id > 0)
            .collect::<Vec<_>>();
        if !first.is_empty() {
            catalog.copy_first_rewards.insert(
                copy_id,
                first
                    .into_iter()
                    .flat_map(|id| {
                        reward_rows
                            .get(&id)
                            .and_then(|reward| {
                                reward
                                    .get("reward")
                                    .or_else(|| reward.get("rewards"))
                                    .and_then(Value::as_array)
                                    .map(|rows| {
                                        rows.iter()
                                            .filter_map(|row| {
                                                let row = row.as_array()?;
                                                Some((
                                                    i32::try_from(row.first()?.as_i64()?).ok()?,
                                                    i32::try_from(row.get(1)?.as_i64()?).ok()?,
                                                    i32::try_from(row.get(2)?.as_i64()?).ok()?,
                                                ))
                                            })
                                            .collect::<Vec<_>>()
                                    })
                            })
                            .into_iter()
                            .flatten()
                    })
                    .collect(),
            );
        }
        let must_drop_reward = json_i32(&value, "copy_must_drop_reward").unwrap_or_default();
        let must_drop_num = json_i32(&value, "copy_must_drop_num")
            .unwrap_or_default()
            .max(0);
        if must_drop_reward > 0 && must_drop_num > 0 {
            if let Some(rewards) = reward_rows.get(&must_drop_reward).map(|reward| {
                reward
                    .get("rewards")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|row| {
                        let row = row.as_array()?;
                        Some((
                            i32::try_from(row.first()?.as_i64()?).ok()?,
                            i32::try_from(row.get(1)?.as_i64()?).ok()?,
                            i32::try_from(row.get(2)?.as_i64()?).ok()?,
                        ))
                    })
                    .filter(|(goods_type, item_id, num)| {
                        *goods_type > 0 && *item_id > 0 && *num > 0
                    })
                    .collect::<Vec<_>>()
            }) {
                if !rewards.is_empty() {
                    catalog
                        .copy_must_drop_rewards
                        .insert(copy_id, (must_drop_num, rewards));
                }
            }
        }
        if let Some(rank_drop_id) = json_i32(&value, "rank_drop").filter(|id| *id > 0) {
            catalog.copy_rank_drop_ids.insert(copy_id, rank_drop_id);
        }
    }
    for (ship_id, value) in read_config_rows(&dir.join("config_ship_main.db")) {
        if let Some(cost) = json_i64(&value, "supple_cost") {
            catalog.ship_supply_cost.insert(ship_id, cost.max(0));
        }
    }
    for (drop_id, value) in read_config_rows(&dir.join("config_drop_item.db")) {
        let mut entries = Vec::new();
        for key in ["drop", "drop_alone"] {
            if let Some(rows) = value.get(key).and_then(Value::as_array) {
                entries.extend(rows.iter().filter_map(|row| {
                    let row = row.as_array()?;
                    if row.len() < 5 {
                        return None;
                    }
                    Some((
                        i32::try_from(row[0].as_i64()?).ok()?,
                        i32::try_from(row[1].as_i64()?).ok()?,
                        i32::try_from(row[2].as_i64()?).ok()?,
                        i32::try_from(row[3].as_i64()?).ok()?,
                        i32::try_from(row[4].as_i64()?).ok()?,
                    ))
                }));
            }
        }
        if !entries.is_empty() {
            catalog.drop_pools.insert(drop_id, entries);
        }
    }
    // Battle copy display rows reference config_drop_info, whose item_info entries are the
    // authoritative visible rewards. Keep config_drop_item pools above for nested pools used by
    // those entries, then overlay direct drop-info IDs so mop-up can settle real rewards.
    for (drop_id, value) in read_config_rows(&dir.join("config_drop_info.db")) {
        let Some(rows) = value.get("item_info").and_then(Value::as_array) else {
            continue;
        };
        let entries = rows
            .iter()
            .filter_map(|row| {
                let values = row.as_array()?;
                let goods_type = i32::try_from(values.first()?.as_i64()?).ok()?;
                let item_id = i32::try_from(values.get(1)?.as_i64()?).ok()?;
                let num = values
                    .get(2)
                    .and_then(Value::as_i64)
                    .and_then(|v| i32::try_from(v).ok())
                    .unwrap_or(1)
                    .max(1);
                let weight = values
                    .get(4)
                    .and_then(Value::as_i64)
                    .and_then(|v| i32::try_from(v).ok())
                    .unwrap_or(1)
                    .max(1);
                (goods_type > 0 && item_id > 0).then_some((goods_type, item_id, num, 1, weight))
            })
            .collect::<Vec<_>>();
        if !entries.is_empty() {
            catalog.drop_pools.insert(drop_id, entries);
        }
    }
    for (id, value) in read_config_rows(&dir.join("config_main_line_reward_arg.db")) {
        let grade = json_i32(&value, "id").unwrap_or(id);
        if grade <= 0 {
            continue;
        }
        catalog.evaluation_by_grade.insert(
            grade,
            BattleEvaluationRule {
                exp_ratio: json_i32(&value, "exp_ratio").unwrap_or(10_000).max(0),
                settle_drop_ratio: json_i32(&value, "settle_drop_ratio")
                    .unwrap_or(10_000)
                    .max(0),
                other_drop_ratio: json_i32(&value, "other_drop_ratio")
                    .unwrap_or(10_000)
                    .max(0),
            },
        );
    }
    let affection_changes = read_config_rows(&dir.join("config_affection_change.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            let sort = json_i32(&value, "affection_change_sort").unwrap_or(id);
            (sort > 0).then_some((
                sort,
                BattleSettlementRule {
                    affection_add: json_i32(&value, "affection_add").unwrap_or_default(),
                    affection_flagship_add: json_i32(&value, "affection_flagship_add")
                        .unwrap_or_default(),
                    affection_mvp_add: json_i32(&value, "affection_mvp_add").unwrap_or_default(),
                    affection_reduce: json_i32(&value, "affection_reduce").unwrap_or_default(),
                    mood_reduce: json_i32(&value, "mood_reduce").unwrap_or_default(),
                    mood_shipwrecks_reduce: json_i32(&value, "mood_shipwrecks_reduce")
                        .unwrap_or_default(),
                },
            ))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let chapter_rules = read_config_rows(&dir.join("config_chapter_type.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            let start_chapter = json_i32(&value, "start_chapter").unwrap_or_default();
            let change_sort = json_i32(&value, "affection_change_sort").unwrap_or(id);
            let mut rule = affection_changes
                .get(&change_sort)
                .copied()
                .unwrap_or_default();
            for (target, field) in [
                (&mut rule.affection_add, "affection_add"),
                (&mut rule.affection_flagship_add, "affection_flagship_add"),
                (&mut rule.affection_mvp_add, "affection_mvp_add"),
                (&mut rule.affection_reduce, "affection_reduce"),
                (&mut rule.mood_reduce, "mood_reduce"),
                (&mut rule.mood_shipwrecks_reduce, "mood_shipwrecks_reduce"),
            ] {
                if let Some(value) = json_i32(&value, field).filter(|value| *value > 0) {
                    *target = value;
                }
            }
            (id > 0).then_some((id, start_chapter, rule))
        })
        .collect::<Vec<_>>();
    let task_disabled_types = read_config_rows(&dir.join("config_chapter_type.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            (json_i32(&value, "not_effect_task").unwrap_or_default() > 0).then_some(id)
        })
        .collect::<std::collections::HashSet<_>>();
    for (chapter_id, value) in read_config_rows(&dir.join("config_chapter.db")) {
        let explicit_type = json_i32(&value, "chapter_type").filter(|value| *value > 0);
        let selected = explicit_type
            .and_then(|id| chapter_rules.iter().find(|(rule_id, _, _)| *rule_id == id))
            .or_else(|| {
                chapter_rules
                    .iter()
                    .filter(|(_, start, _)| *start == chapter_id && *start > 0)
                    .min_by_key(|(id, _, _)| *id)
            })
            .or_else(|| {
                chapter_rules
                    .iter()
                    .filter(|(_, start, _)| *start > 0 && *start <= chapter_id)
                    .max_by_key(|(_, start, _)| *start)
            });
        let Some((selected_type, _, rule)) = selected else {
            continue;
        };
        for copy_id in json_i32_array(&value, "level_list") {
            catalog.settlement_by_copy.insert(copy_id, *rule);
            if task_disabled_types.contains(selected_type) {
                catalog.task_disabled_copies.insert(copy_id);
            }
        }
    }
    catalog
}

fn is_large_activity_sea_fleet(fleet_id: i32) -> bool {
    (202_431..=202_464).contains(&fleet_id)
}

fn battle_challenge_enemy_aliases(
    enemies: &std::collections::HashMap<i32, BattleEnemy>,
) -> std::collections::HashMap<i32, i32> {
    let mut aliases = std::collections::HashMap::new();
    for fleet_id in 202_431_i32..=202_464_i32 {
        for slot in 1_i32..=6_i32 {
            let alias_id = fleet_id.saturating_mul(10).saturating_add(slot);
            let canonical_id = 150_020_240_i32.saturating_add(slot);
            if !enemies.contains_key(&alias_id) && enemies.contains_key(&canonical_id) {
                aliases.insert(alias_id, canonical_id);
            }
        }
    }
    aliases
}
