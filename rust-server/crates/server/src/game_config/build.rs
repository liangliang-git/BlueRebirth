pub(crate) fn load_build_ship_catalog(catalog_path: Option<&Path>) -> BuildShipCatalog {
    let Some(catalog_path) = catalog_path else {
        return BuildShipCatalog::default();
    };
    let dir = config_dir(catalog_path);
    let mut catalog = BuildShipCatalog::default();
    for (pool_id, value) in read_config_rows_or_empty(&dir.join("config_extract_ship.db")) {
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
    for (ship_info_id, value) in read_config_rows_or_empty(&dir.join("config_ship_info.db")) {
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
    for (drop_id, value) in read_config_rows_or_empty(&dir.join("config_drop_item.db")) {
        let mut entries = Vec::new();
        let parse_entries = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|row| {
                    let a = row.as_array()?;
                    if a.len() < 5 {
                        return None;
                    }
                    let vals = a
                        .iter()
                        .take(5)
                        .filter_map(Value::as_i64)
                        .collect::<Vec<_>>();
                    (vals.len() == 5).then_some((
                        vals[0] as i32,
                        vals[1] as i32,
                        vals[2] as i32,
                        vals[3] as i32,
                        vals[4] as i32,
                    ))
                })
                .collect::<Vec<_>>()
        };
        let random_entries = parse_entries("drop");
        let guaranteed_entries = parse_entries("drop_alone");
        entries.extend(random_entries.iter().copied());
        entries.extend(guaranteed_entries.iter().copied());
        if !entries.is_empty() {
            catalog.pools.insert(drop_id, entries);
            catalog.treasure_drop_pools.insert(
                drop_id,
                TreasureDropPool {
                    random_entries,
                    guaranteed_entries,
                    random_count: json_i32(&value, "drop_count").unwrap_or_default(),
                    guaranteed_count: json_i32(&value, "drop_alone_count").unwrap_or_default(),
                },
            );
        }
    }
    for (item_id, value) in read_config_rows_or_empty(&dir.join("config_item_info.db")) {
        if let Some(drop_id) = json_i32(&value, "drop_id").filter(|id| *id > 0) {
            catalog.treasure_drop_by_item.insert(item_id, drop_id);
        }
    }
    for (item_id, value) in read_config_rows_or_empty(&dir.join("config_item_selected.db")) {
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

pub(crate) fn load_build_formula_catalog(
    catalog_path: Option<&Path>,
) -> BuildFormulaCatalogRuntime {
    let Some(catalog_path) = catalog_path else {
        return BuildFormulaCatalogRuntime::default();
    };
    let dir = config_dir(catalog_path);
    let mut configured_rows = Vec::new();
    for (_id, value) in read_config_rows_or_empty(&dir.join("config_build_ship.db")) {
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
    let ship_main_ids = read_config_rows_or_empty(&dir.join("config_ship_main.db"))
        .into_iter()
        .map(|(id, _)| id)
        .collect::<std::collections::HashSet<_>>();
    let mut handbook_ids = read_config_rows_or_empty(&dir.join("config_ship_handbook.db"))
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

pub(crate) fn value_i64_any_opt(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

pub(crate) fn mix_build_draw_roll(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

pub(crate) fn draw_build_ship_reward_with_roll(
    catalog: &BuildShipCatalog,
    pool_id: i32,
    roll: u64,
) -> Option<(i32, i32, i32)> {
    let extract = *catalog.extract_to_drop.get(&pool_id)?;
    draw_build_drop_reward_with_roll(catalog, extract, roll)
}

pub(crate) fn draw_build_drop_reward_with_roll(
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

fn weighted_treasure_entry(entries: &[BuildDropEntry], roll: u64) -> Option<BuildDropEntry> {
    let total: i64 = entries.iter().map(|entry| i64::from(entry.4.max(0))).sum();
    if total <= 0 {
        return None;
    }
    let mut offset = (roll % total as u64) as i64;
    let mut picked = *entries.last()?;
    for entry in entries {
        offset -= i64::from(entry.4.max(0));
        if offset < 0 {
            picked = *entry;
            break;
        }
    }
    Some(picked)
}

pub(crate) fn draw_treasure_random_leaf(
    catalog: &BuildShipCatalog,
    drop_id: i32,
    roll: u64,
    depth: u8,
) -> Option<(i32, i32, i32)> {
    if depth > 8 {
        return None;
    }
    let pool = catalog.treasure_drop_pools.get(&drop_id)?;
    // Nested pools normally use `drop`. Falling back to `drop_alone` keeps
    // malformed/legacy nested boxes usable without changing top-level semantics.
    let entries = if !pool.random_entries.is_empty() {
        &pool.random_entries
    } else if !pool.guaranteed_entries.is_empty() {
        &pool.guaranteed_entries
    } else {
        return None;
    };
    let picked = weighted_treasure_entry(entries, roll)?;
    if picked.0 == 4 {
        return draw_treasure_random_leaf(
            catalog,
            picked.1,
            mix_build_draw_roll(roll ^ u64::try_from(picked.1).unwrap_or_default()),
            depth + 1,
        );
    }
    Some((picked.0, picked.1, picked.2.max(1)))
}

fn append_treasure_guaranteed(
    catalog: &BuildShipCatalog,
    drop_id: i32,
    roll: u64,
    depth: u8,
    rewards: &mut Vec<(i32, i32, i32)>,
) -> bool {
    if depth > 8 {
        return false;
    }
    let Some(pool) = catalog.treasure_drop_pools.get(&drop_id) else {
        return false;
    };
    if pool.guaranteed_entries.is_empty() {
        return true;
    }
    let repeat = pool.guaranteed_count.max(1);
    for repeat_index in 0..repeat {
        for (entry_index, entry) in pool.guaranteed_entries.iter().enumerate() {
            if entry.0 == 4 {
                let nested_has_guaranteed = catalog
                    .treasure_drop_pools
                    .get(&entry.1)
                    .is_some_and(|nested| !nested.guaranteed_entries.is_empty());
                if nested_has_guaranteed {
                    if !append_treasure_guaranteed(
                        catalog,
                        entry.1,
                        mix_build_draw_roll(roll ^ u64::try_from(repeat_index).unwrap_or_default()),
                        depth + 1,
                        rewards,
                    ) {
                        return false;
                    }
                } else if let Some(reward) = draw_treasure_random_leaf(
                    catalog,
                    entry.1,
                    mix_build_draw_roll(roll ^ u64::try_from(entry_index).unwrap_or_default()),
                    depth + 1,
                ) {
                    rewards.push(reward);
                } else {
                    return false;
                }
            } else {
                rewards.push((entry.0, entry.1, entry.2.max(1)));
            }
        }
    }
    true
}

/// Open normal treasure according to client config semantics:
/// guaranteed `drop_alone` rewards plus `drop_count` random `drop` rolls.
pub(crate) fn draw_treasure_rewards_with_roll(
    catalog: &BuildShipCatalog,
    drop_id: i32,
    roll: u64,
) -> Option<Vec<(i32, i32, i32)>> {
    let pool = catalog.treasure_drop_pools.get(&drop_id)?;
    let mut rewards = Vec::new();
    if !append_treasure_guaranteed(catalog, drop_id, roll, 0, &mut rewards) {
        return None;
    }
    let random_count = if pool.random_entries.is_empty() {
        0
    } else {
        pool.random_count.max(1)
    };
    for index in 0..random_count {
        let draw_roll = mix_build_draw_roll(roll ^ u64::try_from(index).unwrap_or_default());
        rewards.push(draw_treasure_random_leaf(catalog, drop_id, draw_roll, 0)?);
    }
    (!rewards.is_empty()).then_some(rewards)
}

pub(crate) fn draw_build_ship_reward(pool_id: i32) -> Option<(i32, i32, i32)> {
    let catalog = BUILD_SHIP_CATALOG.get()?;
    // A ten-pull can execute within one millisecond. Blend a process-wide nonce
    // with wall-clock time so each draw gets a separate weighted roll.
    let sequence = BUILD_DRAW_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let roll = mix_build_draw_roll(u64::from(current_unix_millis()) ^ sequence);
    draw_build_ship_reward_with_roll(catalog, pool_id, roll)
}

pub(crate) fn draw_sr_build_reward_with_roll(
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

pub(crate) fn build_drop_exists(catalog: &BuildShipCatalog, pool_id: i32) -> bool {
    let Some(extract) = catalog.extract_to_drop.get(&pool_id).copied() else {
        return false;
    };
    !expand_build_drop(catalog, extract).is_empty()
}
