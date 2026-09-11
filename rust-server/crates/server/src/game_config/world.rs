pub(crate) fn load_building_catalog(catalog_path: Option<&Path>) -> BuildingCatalog {
    let Some(catalog_path) = catalog_path else {
        return BuildingCatalog::default();
    };
    let building_configs =
        read_config_rows_or_empty(&config_dir(catalog_path).join("config_buildinginfo.db"))
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
    let typed_building_configs = building_configs
        .iter()
        .filter_map(|(template_id, value)| {
            Some((
                *template_id,
                BuildingConfig {
                    building_type: json_i32(value, "type")?,
                    level: json_i32(value, "level").unwrap_or(1).max(1),
                    hero_capacity: json_i32(value, "heronumber")
                        .unwrap_or_default()
                        .max(0) as usize,
                    product_max: json_i32(value, "productmax").unwrap_or_default().max(0),
                    // productid is [GoodsType, ConfigId], for example [5,19]
                    // means currency type 5, resource id 19.
                    product_id: json_i32_array(value, "productid").get(1).copied(),
                    productivity: json_i32(value, "productivity").unwrap_or_default().max(0),
                    produce_speed: json_i32(value, "producespeed")
                        .or_else(|| json_i32(value, "produceSpeed"))
                        .unwrap_or_default()
                        .max(0),
                    add_mood: json_i32(value, "addmood").unwrap_or_default().max(0),
                    hero_addition: json_i32(value, "heroaddition").unwrap_or_default(),
                    add_worker_hp: json_i32(value, "addworkerhp").unwrap_or_default().max(0),
                    food_cost: json_i32(value, "foodcost").unwrap_or_default().max(0),
                    mood_cost: json_i32(value, "moodcost").unwrap_or_default().max(0),
                    oil_addition: json_i32(value, "oiladdition").unwrap_or_default(),
                    gold_addition: json_i32(value, "goldaddition").unwrap_or_default(),
                    power_cost: json_i32(value, "powercost").unwrap_or_default().max(0),
                    production_time_less: json_i32(value, "productiontimeless")
                        .unwrap_or_default()
                        .max(0),
                    reduce_cost: json_i32(value, "reducecost").unwrap_or_default().max(0),
                    recipe_ids: json_i32_array(value, "recipeid"),
                },
            ))
        })
        .collect();
    let upgrade_rules_by_template = read_config_rows_or_empty(
        &config_dir(catalog_path).join("config_buildinglevelup.db"),
    )
    .into_iter()
    .filter_map(|(template_id, value)| {
        let mut costs = Vec::new();
        for key in ["rawmaterial1", "rawmaterial2", "rawmaterial3"] {
            if let Some(row) = value.get(key).and_then(Value::as_array) {
                if row.len() >= 3 {
                    let goods_type = i32::try_from(row[0].as_i64()?).ok()?;
                    let item_id = i32::try_from(row[1].as_i64()?).ok()?;
                    let amount = row[2].as_i64()?;
                    if goods_type > 0 && item_id > 0 && amount > 0 {
                        costs.push((goods_type, item_id, amount));
                    }
                }
            }
        }
        Some((
            template_id,
            BuildingUpgradeRule {
                costs,
                cost_money: value
                    .get("costmoney")
                    .and_then(Value::as_i64)
                    .or_else(|| json_i64(&value, "cost_money"))
                    .unwrap_or_default()
                    .max(0),
                cost_work: json_i32(&value, "costwork").unwrap_or_default().max(0),
                office_level: json_i32(&value, "officelevel").unwrap_or_default().max(0),
                duration_seconds: json_i32(&value, "leveluptime")
                    .unwrap_or_default()
                    .max(0),
            },
        ))
    })
    .collect();
    let recipe_configs =
        read_config_rows_or_empty(&config_dir(catalog_path).join("config_recipe.db"))
            .into_iter()
            .filter(|(recipe_id, _)| *recipe_id > 0)
            .collect::<std::collections::BTreeMap<_, _>>();
    let typed_recipe_configs = recipe_configs
        .iter()
        .filter_map(|(recipe_id, value)| {
            let item = value.get("item")?.as_array()?;
            Some((
                *recipe_id,
                RecipeConfig {
                    time_seconds: json_i32(value, "time")?.max(1),
                    goods_type: i32::try_from(item.first()?.as_i64()?).ok()?,
                    item_id: i32::try_from(item.get(1)?.as_i64()?).ok()?,
                    item_amount: i32::try_from(item.get(2)?.as_i64()?).ok()?.max(1),
                },
            ))
        })
        .collect();
    let resource_time_seconds =
        read_config_rows_or_empty(&config_dir(catalog_path).join("config_parameter.db"))
            .into_iter()
            .filter_map(|(parameter_id, value)| {
                let seconds = json_i32(&value, "value")?;
                (parameter_id > 0 && seconds > 0).then_some((parameter_id, seconds))
            })
            .collect();
    let (worker_hp_max, worker_hp_level_up, worker_recover, worker_recover_interval_seconds) = read_config_rows_or_empty(
        &config_dir(catalog_path).join("config_worker.db"),
    )
    .into_iter()
    .find_map(|(_, value)| {
        Some((
            json_i32(&value, "workerhpmax")?.max(0),
            json_i32_array(&value, "workerhplevelup")
                .into_iter()
                .map(|value| value.max(0))
                .collect(),
            // chargehp is [goods type, item id, amount], not recovery rate.
            // The client wire default recovers 10 points every addworkerhptime seconds.
            10,
            json_i32(&value, "addworkerhptime")?.max(1),
        ))
    })
    .unwrap_or((0, Vec::new(), 10, 60));
    BuildingCatalog {
        capacities,
        #[cfg(test)]
        building_configs,
        #[cfg(test)]
        recipe_configs,
        typed_building_configs,
        upgrade_rules_by_template,
        typed_recipe_configs,
        resource_time_seconds,
        worker_hp_max,
        worker_hp_level_up,
        worker_recover,
        worker_recover_interval_seconds,
    }
}

pub(crate) fn load_support_catalog(catalog_path: Option<&Path>) -> SupportCatalog {
    let Some(catalog_path) = catalog_path else {
        return SupportCatalog::default();
    };
    let dir = config_dir(catalog_path);
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
    let items = read_config_rows_or_empty(&dir.join("config_support_fleet_item.db"))
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
    let drop_rewards = read_config_rows_or_empty(&dir.join("config_drop_item.db"))
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

pub(crate) fn load_handbook_behaviours(catalog_path: Option<&Path>) -> Vec<i32> {
    let Some(catalog_path) = catalog_path else {
        return Vec::new();
    };
    let mut ids = read_config_rows_or_empty(
        &config_dir(catalog_path).join("config_handbook_behaviour_index.db"),
    )
    .into_iter()
    .map(|(id, _)| id)
    .filter(|id| *id > 0)
    .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub(crate) fn load_hero_memories(catalog_path: Option<&Path>) -> Vec<(i32, i32)> {
    let Some(catalog_path) = catalog_path else {
        return Vec::new();
    };
    let mut memories = read_config_rows_or_empty(
        &config_dir(catalog_path).join("config_building_character_story.db"),
    )
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
