pub(crate) fn load_hero_level_catalog(catalog_path: Option<&Path>) -> HeroLevelCatalog {
    let Some(catalog_path) = catalog_path else {
        return HeroLevelCatalog::default();
    };
    let dir = config_dir(catalog_path);
    let mut catalog = HeroLevelCatalog::default();
    for (item_id, value) in read_config_rows_or_empty(&dir.join("config_ship_exp_item.db")) {
        if let Some(exp) = json_i32(&value, "exp") {
            catalog.exp_per_item.insert(item_id, exp.max(0));
        }
    }
    for (row_level, value) in read_config_rows_or_empty(&dir.join("config_ship_levelup.db")) {
        let level = json_i32(&value, "level").unwrap_or(row_level);
        catalog.max_level = catalog.max_level.max(level.max(0));
        if let Some(exp) = json_i32(&value, "exp") {
            catalog.exp_needed.insert(level, exp.max(0));
        }
    }
    catalog
}

pub(crate) fn load_affection_catalog(catalog_path: Option<&Path>) -> AffectionCatalog {
    let Some(catalog_path) = catalog_path else {
        return AffectionCatalog::default();
    };
    let dir = config_dir(catalog_path);
    let exp_by_item = read_config_rows_or_empty(&dir.join("config_affection_item.db"))
        .into_iter()
        .filter_map(|(item_id, value)| {
            let exp = json_i32(&value, "affection_exp")
                .or_else(|| json_i32(&value, "affectionExp"))
                .or_else(|| json_i32(&value, "exp"))?;
            (item_id > 0 && exp > 0).then_some((item_id, exp))
        })
        .collect();
    let bathroom_item = read_config_rows_or_empty(&dir.join("config_bathroom_item.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            (id > 0).then_some(BathroomItemConfig {
                id,
                duration_seconds: json_i32(&value, "time").unwrap_or_default().max(0),
                once_exp: json_i32(&value, "once_exp").unwrap_or_default().max(0),
                price: json_i32(&value, "price").unwrap_or_default().max(0),
            })
        })
        .next();
    let gifts_by_id = read_config_rows_or_empty(&dir.join("config_gift.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            (id > 0).then_some((
                id,
                BathroomGiftConfig {
                    id,
                    gift_type: json_i32(&value, "gift_type").unwrap_or_default(),
                    quality: json_i32(&value, "quality")
                        .or_else(|| json_i32(&value, "quility"))
                        .unwrap_or_default(),
                    price: json_i32_array(&value, "price"),
                    value_id: json_i32(&value, "value_id").unwrap_or_default(),
                    match_groups: json_i32_array(&value, "match_value_group"),
                    not_match_groups: json_i32_array(&value, "not_match_value_group"),
                    match_power: json_i32_array(&value, "match_value_power"),
                    not_match_power: json_i32_array(&value, "not_match_value_power"),
                    match_rate: json_i32(&value, "match_valueup_rate").unwrap_or_default(),
                    not_match_rate: json_i32(&value, "notmatch_valueup_rate").unwrap_or_default(),
                },
            ))
        })
        .collect();
    let value_effect_time_by_id = read_config_rows_or_empty(&dir.join("config_value_effect.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            let time = json_i32(&value, "time")?;
            (id > 0 && time > 0).then_some((id, time))
        })
        .collect();
    let parameter_rows = read_config_rows_or_empty(&dir.join("config_parameter.db"));
    let parameter_value = |id: i32| {
        parameter_rows
            .iter()
            .find(|(row_id, _)| *row_id == id)
            .and_then(|(_, value)| json_i32(value, "value"))
    };
    let mood_bound = parameter_rows
        .iter()
        .find(|(id, _)| *id == 142)
        .map(|(_, value)| json_i32_array(value, "arrValue"))
        .unwrap_or_default();
    let mood_min = mood_bound.first().copied().unwrap_or(0);
    let mood_max = mood_bound.get(1).copied().unwrap_or(1_500_000);
    let mood_stages = read_config_rows_or_empty(&dir.join("config_affection_mood.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            let min = json_i32(&value, "mood_min")?;
            let max = json_i32(&value, "mood_max")?;
            let exp_up = json_i32(&value, "mood_exp_up")?;
            let affection_add = json_i32(&value, "mood_affection_add")?;
            (id > 0 && min >= mood_min && max >= min && exp_up > 0 && affection_add > 0)
                .then_some(MoodStage {
                    min,
                    max,
                    exp_up,
                    affection_add,
                })
        })
        .collect::<Vec<_>>();
    let bath_mood_value = parameter_value(212)
        .filter(|value| *value > 0)
        .unwrap_or(300_000);
    AffectionCatalog {
        exp_by_item,
        bathroom_item,
        gifts_by_id,
        value_effect_time_by_id,
        bath_mood_value,
        gift_mood_value: parameter_value(220)
            .filter(|value| *value > 0)
            .unwrap_or(bath_mood_value),
        bath_currency_id: 13,
        mood_min,
        mood_max,
        mood_initial: parameter_value(143)
            .filter(|value| *value >= mood_min)
            .unwrap_or(1_500_000),
        mood_natural_recovery: parameter_value(140).unwrap_or(100).max(0),
        mood_natural_limit: parameter_value(144).unwrap_or(1_190_000).max(mood_min),
        mood_bath_interval_seconds: parameter_value(139)
            .map(|minutes| i64::from(minutes.max(1)).saturating_mul(60))
            .unwrap_or(600),
        mood_bath_interval_recovery: parameter_value(141).unwrap_or(40_000).max(0),
        mood_married_recovery_bonus: parameter_value(145).unwrap_or(100).max(0),
        mood_affection_bonus_threshold: mood_stages
            .iter()
            .find(|stage| stage.exp_up > 10_000)
            .map(|stage| stage.min)
            .unwrap_or(1_200_000),
        mood_stages,
    }
}

pub(crate) fn json_i64_pairs(value: &Value, key: &str) -> Vec<(i32, i64)> {
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

pub(crate) fn load_ship_intensify_catalog(catalog_path: Option<&Path>) -> ShipIntensifyCatalog {
    let Some(catalog_path) = catalog_path else {
        return ShipIntensifyCatalog {
            same_type_ratio: 10_000,
            diamond_cost_per_hero: 5,
            ..ShipIntensifyCatalog::default()
        };
    };
    let dir = config_dir(catalog_path);
    let mut catalog = ShipIntensifyCatalog {
        same_type_ratio: 10_000,
        diamond_cost_per_hero: 5,
        ..ShipIntensifyCatalog::default()
    };
    for (template_id, value) in
        read_config_rows_or_empty(&dir.join("config_ship_need_power_exp.db"))
    {
        let rows = json_i64_pairs(&value, "need_power_exp");
        if !rows.is_empty() {
            catalog.need_power_by_template.insert(
                template_id,
                (json_i32(&value, "enhance_type").unwrap_or_default(), rows),
            );
        }
    }
    for (template_id, value) in
        read_config_rows_or_empty(&dir.join("config_ship_provide_power_exp.db"))
    {
        let rows = json_i64_pairs(&value, "provide_power_exp");
        if !rows.is_empty() {
            catalog.provide_power_by_template.insert(template_id, rows);
        }
    }
    for (template_id, value) in read_config_rows_or_empty(&dir.join("config_ship_max_power.db")) {
        let rows = json_i64_pairs(&value, "max_power_prop");
        if !rows.is_empty() {
            catalog.max_power_by_template.insert(template_id, rows);
        }
    }
    for (id, value) in read_config_rows_or_empty(&dir.join("config_parameter.db")) {
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

pub(crate) fn load_ship_break_catalog(catalog_path: Option<&Path>) -> ShipBreakCatalog {
    let Some(catalog_path) = catalog_path else {
        return ShipBreakCatalog::default();
    };
    let by_template =
        read_config_rows_or_empty(&config_dir(catalog_path).join("config_ship_break.db"))
            .into_iter()
            .filter_map(|(template_id, value)| {
                let break_to = value
                    .get("break_to")
                    .and_then(|value| value.as_i64())
                    .and_then(|value| i32::try_from(value).ok())
                    .or_else(|| {
                        value
                            .get("break_to")
                            .and_then(Value::as_str)
                            .and_then(|value| value.parse::<i32>().ok())
                    })
                    .unwrap_or_default();
                let break_item = value.get("break_item").and_then(|value| {
                    let values = value.as_array()?;
                    let templates = values.first()?.as_array()?.iter().filter_map(|value| {
                        value.as_i64().and_then(|value| i32::try_from(value).ok())
                    });
                    let templates = templates.collect::<Vec<_>>();
                    let count = usize::try_from(values.get(1)?.as_i64()?).ok()?;
                    Some((templates, count))
                });
                let break_item_mub = value.get("break_item_mub").and_then(|value| {
                    let values = value.as_array()?;
                    Some((
                        i32::try_from(values.first()?.as_i64()?).ok()?,
                        i32::try_from(values.get(1)?.as_i64()?).ok()?,
                    ))
                });
                let currency_cost = value.get("currency_cost").and_then(|value| {
                    let values = value.as_array()?;
                    Some((
                        i32::try_from(values.first()?.as_i64()?).ok()?,
                        i32::try_from(values.get(1)?.as_i64()?).ok()?,
                        values.get(2)?.as_i64()?,
                    ))
                });
                (template_id > 0).then_some((
                    template_id,
                    ShipBreakConfig {
                        min_level: json_i32(&value, "min_level").unwrap_or_default(),
                        break_to,
                        break_item,
                        break_item_optional_count: value
                            .get("break_item_optional")
                            .and_then(Value::as_array)
                            .map_or(0, Vec::len),
                        break_item_mub,
                        break_usableitem_mub: json_i32_array(&value, "break_usableitem_mub"),
                        currency_cost,
                        ship_break_effect_ids: json_i32_array(
                            &value,
                            "ship_break_effect_id_list",
                        ),
                        value_effect_ids: json_i32_array(&value, "value_effect_id_list"),
                        value_effect_powers: json_i32_array(&value, "value_effect_power_list"),
                    },
                ))
            })
            .collect();
    ShipBreakCatalog { by_template }
}

pub(crate) fn load_ship_advance_catalog(catalog_path: Option<&Path>) -> ShipAdvanceCatalog {
    let Some(catalog_path) = catalog_path else {
        return ShipAdvanceCatalog::default();
    };
    let by_level =
        read_config_rows_or_empty(&config_dir(catalog_path).join("config_ship_advance.db"))
            .into_iter()
            .filter_map(|(level, value)| {
                let initial_level = json_i32(&value, "initial_level")?;
                let max_level = json_i32(&value, "max_level")?;
                (level > 0).then_some((
                    level,
                    ShipAdvanceConfig {
                        initial_level,
                        max_level,
                    },
                ))
            })
            .collect();
    ShipAdvanceCatalog { by_level }
}

pub(crate) fn load_ship_remould_catalog(catalog_path: Option<&Path>) -> ShipRemouldCatalog {
    let Some(catalog_path) = catalog_path else {
        return ShipRemouldCatalog::default();
    };
    let dir = config_dir(catalog_path);
    let mut ship_info_by_sf_id = std::collections::BTreeMap::new();
    for (id, value) in read_config_rows_or_empty(&dir.join("config_ship_info.db")) {
        let sf_id = json_i32(&value, "sf_id").unwrap_or(id);
        if sf_id <= 0 {
            continue;
        }
        let config = ShipInfoRemouldConfig {
            remould_template: json_i32_array(&value, "remould_template"),
        };
        let has_remould = !config.remould_template.is_empty();
        let replace = ship_info_by_sf_id
            .get(&sf_id)
            .map(|current: &ShipInfoRemouldConfig| {
                has_remould && current.remould_template.is_empty()
            })
            .unwrap_or(true);
        if replace {
            ship_info_by_sf_id.insert(sf_id, config);
        }
    }
    let templates = read_config_rows_or_empty(&dir.join("config_ship_remould_template.db"))
        .into_iter()
        .filter(|(id, _)| *id > 0)
        .map(|(id, value)| {
            (
                id,
                ShipRemouldTemplateConfig {
                    remould_item_group: json_i32_array(&value, "remould_item_group"),
                },
            )
        })
        .collect();
    let effects = read_config_rows_or_empty(&dir.join("config_ship_remould_effect.db"))
        .into_iter()
        .filter(|(id, _)| *id > 0)
        .map(|(id, value)| {
            let costs = value
                .get("cost")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|row| {
                    let values = row.as_array()?;
                    Some((
                        i32::try_from(values.first()?.as_i64()?).ok()?,
                        i32::try_from(values.get(1)?.as_i64()?).ok()?,
                        values.get(2)?.as_i64()?,
                    ))
                })
                .collect();
            (
                id,
                ShipRemouldEffectConfig {
                    remould_prev: json_i32_array(&value, "remould_prev"),
                    limit_level: json_i32(&value, "limit_level").unwrap_or_default(),
                    limit_star: json_i32(&value, "limit_star").unwrap_or_default(),
                    costs,
                    remould_effect_type: config_i32_nested_array(&value, "remould_effect_type"),
                },
            )
        })
        .collect();
    ShipRemouldCatalog {
        ship_info_by_sf_id,
        templates,
        effects,
    }
}

pub(crate) fn load_commander_level_catalog(catalog_path: Option<&Path>) -> CommanderLevelCatalog {
    let Some(catalog_path) = catalog_path else {
        return CommanderLevelCatalog::default();
    };
    let mut catalog = CommanderLevelCatalog::default();
    for (level, value) in
        read_config_rows_or_empty(&config_dir(catalog_path).join("config_player_levelup.db"))
    {
        let level = json_i32(&value, "level").unwrap_or(level);
        catalog.max_level = catalog.max_level.max(level.max(0));
        let exp = json_i32(&value, "exp").unwrap_or_default();
        if level > 0 && exp > 0 {
            catalog.exp_needed.insert(level, exp);
        }
    }
    catalog
}

pub(crate) fn load_hero_breakdown_catalog(catalog_path: Option<&Path>) -> HeroBreakdownCatalog {
    let Some(catalog_path) = catalog_path else {
        return HeroBreakdownCatalog::default();
    };
    let mut catalog = HeroBreakdownCatalog::default();
    for (template_id, value) in
        read_config_rows_or_empty(&config_dir(catalog_path).join("config_ship_main.db"))
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
