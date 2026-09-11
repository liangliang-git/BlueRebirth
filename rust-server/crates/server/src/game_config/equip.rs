pub(crate) fn load_equip_new_test_catalog(catalog_path: Option<&Path>) -> EquipNewTestCatalog {
    let Some(catalog_path) = catalog_path else {
        return EquipNewTestCatalog::default();
    };
    EquipNewTestCatalog::from_rows(read_config_rows_or_empty(
        &config_dir(catalog_path).join("config_activity.db"),
    ))
}

pub(crate) fn load_fashion_catalog(catalog_path: Option<&Path>) -> FashionList {
    let Some(catalog_path) = catalog_path else {
        return FashionList::default();
    };
    let path = config_dir(catalog_path).join("config_fashion.db");
    let mut grouped = std::collections::BTreeMap::<i32, Vec<i32>>::new();
    for (fashion_tid, value) in read_config_rows_or_empty(&path) {
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

pub(crate) fn load_equip_catalog(catalog_path: Option<&Path>) -> EquipCatalog {
    let Some(catalog_path) = catalog_path else {
        return EquipCatalog::default();
    };
    let dir = config_dir(catalog_path);
    let equip_rows = read_config_rows_or_empty(&dir.join("config_equip.db"));
    let skill_rows =
        read_config_rows_or_empty(&config_dir(catalog_path).join("config_pskill_dict_group.db"));
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
    let mut prop_by_template = std::collections::BTreeMap::new();
    let mut enhance_prop_by_template = std::collections::BTreeMap::new();
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
        prop_by_template.insert(template_id, config_i64_pairs(&value, "equip_prop"));
        enhance_prop_by_template.insert(template_id, config_i64_pairs(&value, "enhance_prop"));
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
    let enhance_materials = read_config_rows_or_empty(&dir.join("config_equip_enhance_item.db"))
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
    let enhance_level_exp =
        read_config_rows_or_empty(&dir.join("config_equip_enhance_level_exp.db"))
            .into_iter()
            .filter_map(|(id, value)| Some((id, json_i32(&value, "exp")?.max(0))))
            .collect();
    let enhance_level_ur = read_config_rows_or_empty(&dir.join("config_equip_enhance_level_ur.db"))
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
    let levelbreak_rules = read_config_rows_or_empty(&dir.join("config_equip_levelbreak_item.db"))
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
    let renovate_rules = read_config_rows_or_empty(&dir.join("config_equip_enhance_renovate.db"))
        .into_iter()
        .filter_map(|(row_id, value)| {
            let level = json_i32(&value, "renovate_level").unwrap_or(row_id);
            if level <= 0 {
                return None;
            }
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
                level,
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
        prop_by_template,
        enhance_prop_by_template,
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
