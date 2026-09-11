pub(crate) fn load_hero_skill_catalog(
    catalog_path: Option<&Path>,
) -> std::collections::BTreeMap<i32, Vec<i32>> {
    let Some(catalog_path) = catalog_path else {
        return std::collections::BTreeMap::new();
    };
    read_config_rows_or_empty(&config_dir(catalog_path).join("config_ship_main.db"))
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

pub(crate) fn load_hero_skill_upgrade_catalog(
    catalog_path: Option<&Path>,
) -> HeroSkillUpgradeCatalog {
    let Some(catalog_path) = catalog_path else {
        return HeroSkillUpgradeCatalog::default();
    };
    let mut catalog = HeroSkillUpgradeCatalog::default();
    for (group_id, value) in
        read_config_rows_or_empty(&config_dir(catalog_path).join("config_pskill_dict_group.db"))
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

pub(crate) fn load_ship_stat_catalog(catalog_path: Option<&Path>) -> ShipStatCatalog {
    let Some(catalog_path) = catalog_path else {
        return ShipStatCatalog::default();
    };
    let dir = config_dir(catalog_path);
    let mut catalog = load_ship_stat_catalog_from_rows(read_config_rows_or_empty(
        &dir.join("config_ship_main.db"),
    ));
    catalog.level_attribute_by_level = load_level_attribute_rows(
        read_config_rows_or_empty(&dir.join("config_ship_levelup.db")),
    );
    catalog.mubar_level_attribute_by_level = load_level_attribute_rows(
        read_config_rows_or_empty(&dir.join("config_ship_levelup_mub.db")),
    );
    catalog.value_effects_by_id = read_config_rows_or_empty(&dir.join("config_value_effect.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            let values = value.get("values")?.as_str()?;
            let (target_attr, raw_value) = values.split_once(',')?;
            let target_attr = target_attr.trim().parse::<i32>().ok()?;
            let raw_value = raw_value.trim().parse::<f64>().ok()?;
            (id > 0 && target_attr > 0 && raw_value.is_finite()).then_some((
                id,
                ValueEffectConfig {
                    target_attr,
                    value: raw_value,
                },
            ))
        })
        .collect();
    // config_ship_main references config_ship_info through ship_info_id.
    // Client Mubar camp is ship_country=12.
    let mubar_info_ids = read_config_rows_or_empty(&dir.join("config_ship_info.db"))
        .into_iter()
        .filter(|(_, value)| json_i32(value, "ship_country") == Some(12))
        .map(|(id, _)| id)
        .collect::<std::collections::BTreeSet<_>>();
    for (template_id, value) in read_config_rows_or_empty(&dir.join("config_ship_main.db")) {
        if json_i32(&value, "ship_info_id").is_some_and(|id| mubar_info_ids.contains(&id)) {
            catalog.mubar_templates.insert(template_id);
        }
    }
    catalog
}

fn load_level_attribute_rows(rows: Vec<(i32, Value)>) -> BTreeMap<i32, i32> {
    rows.into_iter()
        .filter_map(|(row_id, value)| {
            let level = json_i32(&value, "level").unwrap_or(row_id);
            let attribute_level = json_i32(&value, "attribute_level")?;
            (level > 0 && attribute_level > 0).then_some((level, attribute_level))
        })
        .collect()
}

fn load_ship_stat_catalog_from_rows(rows: Vec<(i32, Value)>) -> ShipStatCatalog {
    let by_template = rows
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
                    to_air_attack: json_i64(&value, "to_air_attack").unwrap_or_default(),
                    to_air_attack_levelup: json_i64(&value, "to_air_attack_levelup")
                        .unwrap_or_default(),
                    to_torpedo_attack: json_i64(&value, "to_torpedo_attack").unwrap_or_default(),
                    to_torpedo_attack_levelup: json_i64(&value, "to_torpedo_attack_levelup")
                        .unwrap_or_default(),
                    ship_bomb_attack: json_i64(&value, "ship_bomb_attack").unwrap_or_default(),
                    ship_bomb_attack_levelup: json_i64(&value, "ship_bomb_attack_levelup")
                        .unwrap_or_default(),
                    ship_torpedo_attack: json_i64(&value, "ship_torpedo_attack")
                        .unwrap_or_default(),
                    ship_torpedo_attack_levelup: json_i64(&value, "ship_torpedo_attack_levelup")
                        .unwrap_or_default(),
                    ship_air_control: json_i64(&value, "ship_air_control").unwrap_or_default(),
                    ship_air_control_levelup: json_i64(&value, "ship_air_control_levelup")
                        .unwrap_or_default(),
                    carry_plane_count: json_i64(&value, "carry_plane_count").unwrap_or_default(),
                    hit: json_i64(&value, "hit").unwrap_or_default(),
                    dodge: json_i64(&value, "dodge").unwrap_or_default(),
                    crit: json_i64(&value, "crit").unwrap_or_default(),
                    anti_crit: json_i64(&value, "anti_crit").unwrap_or_default(),
                    favorite_gifts: json_i32_array(&value, "favorite_gift"),
                    view_range: json_i64(&value, "view_range").unwrap_or_default(),
                    main_gun_cd: json_i64(&value, "main_gun_cd").unwrap_or_default(),
                    main_gun_range: json_i64(&value, "main_gun_range").unwrap_or_default(),
                    fate: json_i64(&value, "fate").unwrap_or_default(),
                    torpedo_num: json_i64(&value, "torpedo_num").unwrap_or_default(),
                    torpedo_range: json_i64(&value, "torpedo_range").unwrap_or_default(),
                    speed: json_i64(&value, "speed").unwrap_or_default(),
                    plane_health: json_i64(&value, "plane_health").unwrap_or_default(),
                    plane_bomb: json_i64(&value, "plane_bomb").unwrap_or_default(),
                    plane_torpedo: json_i64(&value, "plane_torpedo").unwrap_or_default(),
                    plane_to_air: json_i64(&value, "plane_to_air").unwrap_or_default(),
                    backup_plane_count: json_i64(&value, "backup_plane_count")
                        .unwrap_or_default(),
                    submarine: json_i64(&value, "submarine").unwrap_or_default(),
                    submarine_levelup: json_i64(&value, "submarine_levelup").unwrap_or_default(),
                    antisubmarine: json_i64(&value, "antisubmarine").unwrap_or_default(),
                    antisubmarine_levelup: json_i64(&value, "antisubmarine_levelup")
                        .unwrap_or_default(),
                    level_value_effect: json_i32_array(&value, "level_value_effect"),
                },
            ))
        })
        .collect();
    ShipStatCatalog {
        by_template,
        ..ShipStatCatalog::default()
    }
}

pub(crate) fn scaled_ship_stat(value: i64, multiplier: f64) -> i64 {
    let multiplier = normalize_multiplier(multiplier);
    ((value.max(0) as f64) * multiplier)
        .round()
        .clamp(0.0, i64::MAX as f64) as i64
}

pub(crate) fn ship_attributes_for_template(
    template_id: i32,
    level: i64,
    catalog: Option<&ShipStatCatalog>,
    multiplier: f64,
) -> std::collections::BTreeMap<i32, i64> {
    ship_attributes_with_intensify(template_id, level, catalog, multiplier, |_| 0)
}

pub(crate) fn ship_attributes_for_typed_hero(
    hero: &blueoath_domain::HeroState,
    progress: &std::collections::BTreeMap<String, u64>,
    equipments: &std::collections::BTreeMap<
        blueoath_domain::EquipId,
        blueoath_domain::EquipmentState,
    >,
    catalog: Option<&ShipStatCatalog>,
    equip_catalog: Option<&EquipCatalog>,
    remould_catalog: Option<&ShipRemouldCatalog>,
    multiplier: f64,
) -> std::collections::BTreeMap<i32, i64> {
    ship_attributes_for_typed_hero_with_heroes(
        hero,
        progress,
        equipments,
        catalog,
        equip_catalog,
        remould_catalog,
        multiplier,
        None,
    )
}

pub(crate) fn ship_attributes_for_typed_hero_with_heroes(
    hero: &blueoath_domain::HeroState,
    progress: &std::collections::BTreeMap<String, u64>,
    equipments: &std::collections::BTreeMap<
        blueoath_domain::EquipId,
        blueoath_domain::EquipmentState,
    >,
    catalog: Option<&ShipStatCatalog>,
    equip_catalog: Option<&EquipCatalog>,
    remould_catalog: Option<&ShipRemouldCatalog>,
    multiplier: f64,
    heroes: Option<&std::collections::BTreeMap<
        blueoath_domain::HeroId,
        blueoath_domain::HeroState,
    >>,
) -> std::collections::BTreeMap<i32, i64> {
    let prefix = format!("compat:hero:{}:intensify:", hero.id.get());
    let mut attributes = ship_attributes_with_intensify(
        i32::try_from(hero.template_id.get()).unwrap_or_default(),
        i64::try_from(hero.level).unwrap_or(i64::MAX),
        catalog,
        multiplier,
        |attr_id| {
            progress
                .get(&format!("{prefix}{attr_id}:level"))
                .and_then(|value| i64::try_from(*value).ok())
                .unwrap_or_default()
                .max(0)
        },
    );
    add_typed_equipment_attributes(&mut attributes, hero, equipments, equip_catalog, multiplier);
    add_typed_remould_attributes(&mut attributes, hero, progress, remould_catalog, multiplier);
    add_typed_combination_attributes(&mut attributes, hero, progress, heroes, multiplier);
    add_typed_break_value_effects(&mut attributes, hero, catalog);
    attributes
}

fn add_typed_break_value_effects(
    attributes: &mut BTreeMap<i32, i64>,
    hero: &blueoath_domain::HeroState,
    ship_catalog: Option<&ShipStatCatalog>,
) {
    let (Some(ship_catalog), Some(break_catalog)) = (ship_catalog, SHIP_BREAK_CATALOG.get())
    else {
        return;
    };
    let template_id = i32::try_from(hero.template_id.get()).unwrap_or_default();
    let Some(config) = break_catalog.by_template.get(&template_id) else {
        return;
    };
    apply_value_effects(
        attributes,
        &config.value_effect_ids,
        &config.value_effect_powers,
        &ship_catalog.value_effects_by_id,
    );
}

fn add_typed_combination_attributes(
    attributes: &mut BTreeMap<i32, i64>,
    hero: &blueoath_domain::HeroState,
    progress: &BTreeMap<String, u64>,
    heroes: Option<&BTreeMap<blueoath_domain::HeroId, blueoath_domain::HeroState>>,
    multiplier: f64,
) {
    let Some(heroes) = heroes else {
        return;
    };
    let combine_id = progress
        .get(&format!("compat:hero:{}:combination:combine", hero.id.get()))
        .copied()
        .and_then(|id| blueoath_domain::HeroId::new(id).ok());
    let Some(combine_id) = combine_id else {
        return;
    };
    let Some(combine_hero) = heroes.get(&combine_id) else {
        return;
    };
    let Some(catalog) = COMBINATION_CATALOG.get() else {
        return;
    };
    let sf_id = i32::try_from(combine_hero.template_id.get() / 10).unwrap_or_default();
    if sf_id <= 0 || (!catalog.open_sf_ids.is_empty() && !catalog.open_sf_ids.contains(&sf_id)) {
        return;
    }
    let combine_level = progress
        .get(&format!("compat:hero:{}:combination:level", combine_id.get()))
        .copied()
        .unwrap_or_default()
        .clamp(1, 100) as i32;
    let Some((_, current)) = catalog.rules_by_id.iter().find(|(id, rule)| {
        *id / 100 == sf_id
            && rule.level_start <= combine_level
            && combine_level <= rule.level_end
    }) else {
        return;
    };
    for (_, rule) in catalog.rules_by_id.iter().filter(|(id, rule)| {
        *id / 100 == sf_id && rule.star < current.star
    }) {
        for &(attr_id, value) in &rule.break_prop_up {
            add_combination_attribute(attributes, attr_id, value, multiplier);
        }
        for &(attr_id, value) in &rule.prop_up {
            add_combination_attribute(
                attributes,
                attr_id,
                value.saturating_mul(10),
                multiplier,
            );
        }
        for &(effect_id, value) in &rule.break_prop_up_percent {
            add_combination_percent_attribute(attributes, effect_id, value);
        }
        for &(effect_id, value) in &rule.prop_up_percent {
            add_combination_percent_attribute(
                attributes,
                effect_id,
                value.saturating_mul(10),
            );
        }
    }
    let stage_level = if combine_level % 10 == 0 {
        10
    } else {
        combine_level % 10
    };
    for &(attr_id, value) in &current.prop_up {
        add_combination_attribute(
            attributes,
            attr_id,
            value.saturating_mul(i64::from(stage_level)),
            multiplier,
        );
    }
    for &(effect_id, value) in &current.prop_up_percent {
        add_combination_percent_attribute(
            attributes,
            effect_id,
            value.saturating_mul(i64::from(stage_level)),
        );
    }
    let combine_grade = progress
        .get(&format!("compat:hero:{}:combination:grade", combine_id.get()))
        .copied()
        .unwrap_or_default() as i32;
    if let Some(next) = catalog.rules_by_id.get(&current.next_id) {
        if combine_grade == next.star {
            for &(attr_id, value) in &next.break_prop_up {
                add_combination_attribute(attributes, attr_id, value, multiplier);
            }
            for &(effect_id, value) in &next.break_prop_up_percent {
                add_combination_percent_attribute(attributes, effect_id, value);
            }
        }
    }
}

fn add_combination_attribute(
    attributes: &mut BTreeMap<i32, i64>,
    attr_id: i32,
    value: i64,
    multiplier: f64,
) {
    if attr_id > 0 && value != 0 {
        let entry = attributes.entry(attr_id).or_default();
        *entry = entry.saturating_add(scaled_ship_stat(value, multiplier));
    }
}

fn add_combination_percent_attribute(
    attributes: &mut BTreeMap<i32, i64>,
    effect_id: i32,
    value: i64,
) {
    let Some(attr_id) = combination_percent_attribute(effect_id) else {
        return;
    };
    let entry = attributes.entry(attr_id).or_default();
    *entry = entry.saturating_add(value.max(0));
}

fn combination_percent_attribute(effect_id: i32) -> Option<i32> {
    Some(match effect_id {
        74 => 138, // Percentage_ShipHp
        75 => 139, // Percentage_Attack
        76 => 142, // Percentage_Defense
        77 => 140, // Percentage_Torpedo
        78 => 143, // Percentage_TorpedoDefense
        79 => 141, // Percentage_ToAirAttack
        80 => 144, // Percentage_ShipPlaneAttack
        81 => 145, // Percentage_ShipAirControl
        _ => return None,
    })
}

fn apply_value_effects(
    attributes: &mut BTreeMap<i32, i64>,
    effect_ids: &[i32],
    powers: &[i32],
    effects: &BTreeMap<i32, ValueEffectConfig>,
) {
    for (index, effect_id) in effect_ids.iter().enumerate() {
        let Some(effect) = effects.get(effect_id) else {
            continue;
        };
        let power = powers.get(index).copied().unwrap_or_default();
        let amount = if power != 0 {
            i64::from(power)
        } else if effect.value.fract() == 0.0 {
            effect.value as i64
        } else {
            // Value-effect definitions use decimal coefficients while HeroAttr
            // carries integer percentage points for these secondary properties.
            (effect.value * 100.0).round() as i64
        };
        if amount == 0 {
            continue;
        }
        if amount < 0 {
            // Negative HeroAttr values are not valid protobuf varints. Keep
            // effect id in battle break-effect list for client-side handling.
            continue;
        }
        let entry = attributes.entry(effect.target_attr).or_default();
        *entry = entry.saturating_add(amount);
    }
}

fn add_typed_equipment_attributes(
    attributes: &mut std::collections::BTreeMap<i32, i64>,
    hero: &blueoath_domain::HeroState,
    equipments: &std::collections::BTreeMap<
        blueoath_domain::EquipId,
        blueoath_domain::EquipmentState,
    >,
    equip_catalog: Option<&EquipCatalog>,
    multiplier: f64,
) {
    let Some(equip_catalog) = equip_catalog else {
        return;
    };
    for equip_id in hero.equip_slots.iter().flatten() {
        let Some(equipment) = equipments.get(equip_id) else {
            continue;
        };
        let template_id = i32::try_from(equipment.template_id.get()).unwrap_or_default();
        let mut raw_props = equip_catalog
            .prop_by_template
            .get(&template_id)
            .into_iter()
            .flatten()
            .copied()
            .collect::<BTreeMap<_, _>>();
        let enhance_props = equip_catalog
            .enhance_prop_by_template
            .get(&template_id)
            .into_iter()
            .flatten()
            .map(|(attr_id, value)| (*attr_id, *value))
            .collect::<std::collections::BTreeMap<_, _>>();
        for (attr_id, enhance_value) in enhance_props {
            let value = enhance_value.saturating_mul(i64::from(equipment.enhance_level));
            raw_props
                .entry(attr_id)
                .and_modify(|current| *current = current.saturating_add(value))
                .or_insert(value);
        }
        for (attr_id, raw_value) in raw_props {
            let current = attributes.entry(attr_id).or_default();
            *current = current.saturating_add(scaled_ship_stat(raw_value, multiplier));
        }
    }
}

fn add_typed_remould_attributes(
    attributes: &mut std::collections::BTreeMap<i32, i64>,
    hero: &blueoath_domain::HeroState,
    progress: &std::collections::BTreeMap<String, u64>,
    remould_catalog: Option<&ShipRemouldCatalog>,
    multiplier: f64,
) {
    let Some(remould_catalog) = remould_catalog else {
        return;
    };
    let prefix = format!("compat:hero:{}:remould:effect:", hero.id.get());
    for (effect_id, effect) in &remould_catalog.effects {
        if progress
            .get(&format!("{}{}", prefix, effect_id))
            .copied()
            .unwrap_or_default()
            == 0
        {
            continue;
        }
        for values in &effect.remould_effect_type {
            if values.first() != Some(&2) || values.len() < 3 {
                continue;
            }
            let attr_id = values[1];
            let value = values[2];
            let current = attributes.entry(attr_id).or_default();
            *current = current.saturating_add(scaled_ship_stat(i64::from(value), multiplier));
        }
    }
}

pub(crate) fn ship_max_hp_for_typed_hero(
    hero: &blueoath_domain::HeroState,
    progress: &std::collections::BTreeMap<String, u64>,
    equipments: &std::collections::BTreeMap<
        blueoath_domain::EquipId,
        blueoath_domain::EquipmentState,
    >,
    catalog: Option<&ShipStatCatalog>,
    equip_catalog: Option<&EquipCatalog>,
    remould_catalog: Option<&ShipRemouldCatalog>,
    multiplier: f64,
) -> u64 {
    ship_max_hp_for_typed_hero_with_heroes(
        hero,
        progress,
        equipments,
        catalog,
        equip_catalog,
        remould_catalog,
        multiplier,
        None,
    )
}

pub(crate) fn ship_max_hp_for_typed_hero_with_heroes(
    hero: &blueoath_domain::HeroState,
    progress: &std::collections::BTreeMap<String, u64>,
    equipments: &std::collections::BTreeMap<
        blueoath_domain::EquipId,
        blueoath_domain::EquipmentState,
    >,
    catalog: Option<&ShipStatCatalog>,
    equip_catalog: Option<&EquipCatalog>,
    remould_catalog: Option<&ShipRemouldCatalog>,
    multiplier: f64,
    heroes: Option<&std::collections::BTreeMap<
        blueoath_domain::HeroId,
        blueoath_domain::HeroState,
    >>,
) -> u64 {
    ship_attributes_for_typed_hero_with_heroes(
        hero,
        progress,
        equipments,
        catalog,
        equip_catalog,
        remould_catalog,
        multiplier,
        heroes,
    )
    .get(&1)
    .copied()
    .unwrap_or(1)
    .max(1) as u64
}

pub(crate) fn ship_initial_hp_for_template(template_id: u64) -> u64 {
    ship_attributes_for_template(
        i32::try_from(template_id).unwrap_or_default(),
        1,
        SHIP_STAT_CATALOG.get(),
        SHIP_STAT_MULTIPLIER.get().copied().unwrap_or(1.0),
    )
    .get(&1)
    .copied()
    .unwrap_or(1)
    .max(1) as u64
}

fn ship_attributes_with_intensify(
    template_id: i32,
    level: i64,
    catalog: Option<&ShipStatCatalog>,
    multiplier: f64,
    intensify: impl Fn(i32) -> i64,
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
    let attribute_level = catalog
        .and_then(|catalog| {
            let levels = if catalog.mubar_templates.contains(&template_id) {
                &catalog.mubar_level_attribute_by_level
            } else {
                &catalog.level_attribute_by_level
            };
            levels.get(&(level as i32)).copied()
        })
        .map(i64::from)
        .unwrap_or(level);
    let level_delta = attribute_level.saturating_sub(1).max(0);
    let value = |attr_id: i32, base: i64, growth: i64| {
        // Client stores level-up growth on a 100-level scale. Keep integer
        // truncation so template 20530211 at level 41 yields 4435 HP.
        let leveled = base.saturating_add(growth.saturating_mul(level_delta) / 100);
        scaled_ship_stat(leveled.saturating_add(intensify(attr_id)), multiplier)
    };
    let scout_num = if stats.carry_plane_count > 0 {
        stats.carry_plane_count
    } else {
        1
    };
    let mut attributes: BTreeMap<i32, i64> = [
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
            12,
            value(12, stats.to_air_attack, stats.to_air_attack_levelup),
        ),
        (
            13,
            value(13, stats.to_torpedo_attack, stats.to_torpedo_attack_levelup),
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
            16,
            value(16, stats.ship_air_control, stats.ship_air_control_levelup),
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
        (6, stats.backup_plane_count.max(0)),
        (21, stats.main_gun_range.max(0)),
        (22, stats.fate.max(0)),
        (23, stats.view_range.max(0)),
        (24, stats.main_gun_cd.max(0)),
        (25, stats.torpedo_num.max(0)),
        (27, stats.speed.max(0)),
        (39, stats.torpedo_range.max(0)),
        (42, stats.plane_health.max(0)),
        (43, stats.plane_bomb.max(0)),
        (44, stats.plane_torpedo.max(0)),
        (45, stats.plane_to_air.max(0)),
        (
            210,
            value(210, stats.submarine, stats.submarine_levelup),
        ),
        (
            211,
            value(211, stats.antisubmarine, stats.antisubmarine_levelup),
        ),
        (212, stats.submarine.max(0)),
    ]
    .into_iter()
    .collect();
    if let Some(catalog) = catalog {
        if let Some(stats) = catalog.by_template.get(&template_id) {
            apply_value_effects(
                &mut attributes,
                &stats.level_value_effect,
                &[],
                &catalog.value_effects_by_id,
            );
        }
    }
    attributes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ship_hp_uses_level_growth() {
        let mut catalog = ShipStatCatalog::default();
        catalog.by_template.insert(
            7,
            ShipStat {
                hp: 100,
                hp_levelup: 2_000,
                attack: 10,
                ..ShipStat::default()
            },
        );

        assert_eq!(
            ship_attributes_for_template(7, 1, Some(&catalog), 1.0)[&1],
            100
        );
        assert_eq!(
            ship_attributes_for_template(7, 3, Some(&catalog), 1.0)[&1],
            140
        );
    }

    #[test]
    fn ship_attributes_include_static_combat_fields_and_level_effects() {
        let mut catalog = ShipStatCatalog::default();
        catalog.by_template.insert(
            7,
            ShipStat {
                hp: 100,
                main_gun_cd: 7_000,
                main_gun_range: 2,
                view_range: 10,
                torpedo_num: 1,
                speed: 2_000,
                level_value_effect: vec![48, 65],
                ..ShipStat::default()
            },
        );
        catalog.value_effects_by_id.insert(
            48,
            ValueEffectConfig {
                target_attr: 3_200,
                value: 1.0,
            },
        );
        catalog.value_effects_by_id.insert(
            65,
            ValueEffectConfig {
                target_attr: 106,
                value: 0.01,
            },
        );

        let attributes = ship_attributes_for_template(7, 1, Some(&catalog), 1.0);
        assert_eq!(attributes[&24], 7_000);
        assert_eq!(attributes[&21], 2);
        assert_eq!(attributes[&23], 10);
        assert_eq!(attributes[&25], 1);
        assert_eq!(attributes[&27], 2_000);
        assert_eq!(attributes[&3_200], 1);
        assert_eq!(attributes[&106], 1);
    }

    #[test]
    fn combination_percentage_effects_use_client_property_ids() {
        let mut attributes = [(1, 1_000), (8, 100)].into_iter().collect();
        add_combination_percent_attribute(&mut attributes, 74, 100);
        add_combination_percent_attribute(&mut attributes, 75, 20);
        assert_eq!(attributes[&138], 100);
        assert_eq!(attributes[&139], 20);
        assert_eq!(attributes[&1], 1_000);
        assert_eq!(attributes[&8], 100);
    }

    #[test]
    fn typed_intensify_is_included_in_combat_attributes_and_max_hp() {
        let mut catalog = ShipStatCatalog::default();
        catalog.by_template.insert(
            7,
            ShipStat {
                hp: 100,
                hp_levelup: 20,
                attack: 10,
                ..ShipStat::default()
            },
        );
        let hero = blueoath_domain::HeroState {
            id: blueoath_domain::HeroId::new(1).unwrap(),
            template_id: blueoath_domain::TemplateId::new(7).unwrap(),
            fashioning: 0,
            name: String::new(),
            change_name_time: 0,
            level: 3,
            exp: 0,
            mood: 100,
            affection: 0,
            hp: 100,
            locked: false,
            created_utc: String::new(),
            equip_slots: vec![None; 6],
            pskills: std::collections::BTreeMap::new(),
        };
        let mut progress = std::collections::BTreeMap::new();
        progress.insert("compat:hero:1:intensify:1:level".to_owned(), 5);
        progress.insert("compat:hero:1:intensify:8:level".to_owned(), 3);

        let equipments = std::collections::BTreeMap::new();
        let attrs = ship_attributes_for_typed_hero(
            &hero,
            &progress,
            &equipments,
            Some(&catalog),
            None,
            None,
            1.0,
        );
        assert_eq!(attrs[&1], 105);
        assert_eq!(attrs[&8], 13);
        assert_eq!(
            ship_max_hp_for_typed_hero(
                &hero,
                &progress,
                &equipments,
                Some(&catalog),
                None,
                None,
                1.0,
            ),
            105
        );
    }

    #[test]
    fn equipped_stats_include_base_and_enhance_values() {
        let mut ship_catalog = ShipStatCatalog::default();
        ship_catalog.by_template.insert(
            7,
            ShipStat {
                hp: 100,
                attack: 10,
                ..ShipStat::default()
            },
        );
        let hero_id = blueoath_domain::HeroId::new(1).unwrap();
        let equip_id = blueoath_domain::EquipId::new(1).unwrap();
        let hero = blueoath_domain::HeroState {
            id: hero_id,
            template_id: blueoath_domain::TemplateId::new(7).unwrap(),
            fashioning: 0,
            name: String::new(),
            change_name_time: 0,
            level: 1,
            exp: 0,
            mood: 100,
            affection: 0,
            hp: 100,
            locked: false,
            created_utc: String::new(),
            equip_slots: vec![Some(equip_id)],
            pskills: std::collections::BTreeMap::new(),
        };
        let equipment = blueoath_domain::EquipmentState {
            id: equip_id,
            template_id: blueoath_domain::TemplateId::new(99).unwrap(),
            enhance_level: 2,
            star: 0,
            enhance_exp: 0,
            hero_id: Some(hero_id),
        };
        let equipments = [(equip_id, equipment)].into_iter().collect();
        let mut equip_catalog = EquipCatalog::default();
        equip_catalog
            .prop_by_template
            .insert(99, vec![(1, 10), (8, 5)]);
        equip_catalog
            .enhance_prop_by_template
            .insert(99, vec![(1, 7), (8, 3), (12, 4)]);

        let attrs = ship_attributes_for_typed_hero(
            &hero,
            &std::collections::BTreeMap::new(),
            &equipments,
            Some(&ship_catalog),
            Some(&equip_catalog),
            None,
            1.0,
        );
        assert_eq!(attrs[&1], 124);
        assert_eq!(attrs[&8], 21);
        assert_eq!(attrs[&12], 8);
    }
}

pub(crate) fn load_talent_catalog(catalog_path: Option<&Path>) -> TalentCatalog {
    let Some(catalog_path) = catalog_path else {
        return TalentCatalog::default();
    };
    let dir = config_dir(catalog_path);
    let mut nodes = std::collections::BTreeMap::new();
    for (id, value) in read_config_rows_or_empty(&dir.join("config_talent.db")) {
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
    for (_, value) in read_config_rows_or_empty(&dir.join("config_talentmain.db")) {
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

pub(crate) fn compute_talent_target(
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

pub(crate) fn talent_tree_payload_typed(
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

pub(crate) fn talent_data_payload_typed(
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

pub(crate) fn apply_talent_change_typed(
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

pub(crate) fn encode_talent_data(id: i32, precondition: &[i32], is_operate: i32) -> Vec<u8> {
    let mut out = Vec::new();
    append_varint_field(&mut out, 1, id.max(0) as u64);
    for pre in precondition {
        append_varint_field(&mut out, 2, (*pre).max(0) as u64);
    }
    append_varint_field(&mut out, 3, is_operate.max(0) as u64);
    out
}

pub(crate) fn talent_change_payload(target: (i32, Vec<i32>, i32)) -> Vec<u8> {
    let mut out = Vec::new();
    append_message_field(
        &mut out,
        1,
        &encode_talent_data(target.0, &target.1, target.2),
    );
    out
}
