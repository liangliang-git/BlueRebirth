pub(crate) fn load_battle_catalog(catalog_path: Option<&Path>) -> BattleCatalog {
    let Some(catalog_path) = catalog_path else {
        return BattleCatalog::default();
    };
    let dir = config_dir(catalog_path);
    let mut catalog = BattleCatalog::default();
    let database_path = config_db::config_db_path(&dir);
    catalog.drop_quantities = if database_path.is_file() {
        config_db::load_drop_quantities(&database_path).unwrap_or_else(|error| {
            tracing::error!(
                path = %database_path.display(),
                %error,
                "cannot load battle drop quantities"
            );
            BattleDropQuantities::default()
        })
    } else {
        BattleDropQuantities::default()
    };
    let copy_types = read_config_rows_or_empty(&dir.join("config_chapter.db"))
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
    let mut daily_drop_index_by_copy = std::collections::HashMap::<i32, usize>::new();
    for (_, value) in read_config_rows_or_empty(&dir.join("config_chapter.db")) {
        if value_i64_any(&value, &["class_type"]) != 9 {
            continue;
        }
        let group_id = json_i32(&value, "dailygroup_id").unwrap_or_default();
        if group_id <= 0 {
            continue;
        }
        for (index, copy_id) in json_i32_array(&value, "level_list").into_iter().enumerate() {
            catalog.daily_group_by_copy.insert(copy_id, group_id);
            daily_drop_index_by_copy.insert(copy_id, index);
        }
        for copy_id in json_i32_array(&value, "treaty_copy") {
            catalog.daily_group_by_copy.insert(copy_id, group_id);
        }
    }
    let daily_group_drop_ids = read_config_rows_or_empty(&dir.join("config_daily_group.db"))
        .into_iter()
        .map(|(group_id, value)| {
            let parse_nested_ids = |key: &str| {
                value
                    .get(key)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|row| {
                        row.as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_i64)
                            .filter_map(|id| i32::try_from(id).ok())
                            .filter(|id| *id > 0)
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            };
            (
                group_id,
                (
                    parse_nested_ids("basic_drop"),
                    parse_nested_ids("extra_drop"),
                ),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    for (copy_id, group_id) in &catalog.daily_group_by_copy {
        let Some(index) = daily_drop_index_by_copy.get(copy_id).copied() else {
            continue;
        };
        let Some((basic_drop, extra_drop)) = daily_group_drop_ids.get(group_id) else {
            continue;
        };
        if let Some(drop_ids) = basic_drop.get(index).filter(|drop_ids| !drop_ids.is_empty()) {
            catalog
                .daily_basic_drop_ids_by_copy
                .insert(*copy_id, drop_ids.clone());
        }
        if let Some(drop_ids) = extra_drop.get(index).filter(|drop_ids| !drop_ids.is_empty()) {
            catalog
                .daily_extra_drop_ids_by_copy
                .insert(*copy_id, drop_ids.clone());
        }
    }
    let mut candidates = std::collections::HashMap::<i32, (bool, BattleCopy)>::new();
    for (row_id, value) in read_config_rows_or_empty(&dir.join("config_copy.db")) {
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
    for (fleet_id, value) in read_config_rows_or_empty(&dir.join("config_fleet.db")) {
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
    for (enemy_id, value) in read_config_rows_or_empty(&dir.join("config_ship_enemy.db")) {
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
    let factor_groups = read_config_rows_or_empty(&dir.join("config_random_factor_group.db"))
        .into_iter()
        .map(|(id, value)| (id, value_array_i64(&value, &["factor"])))
        .collect::<std::collections::HashMap<_, _>>();
    let factor_sets = read_config_rows_or_empty(&dir.join("config_random_factor_set.db"))
        .into_iter()
        .map(|(id, value)| (id, json_i32_array(&value, "factor_groups")))
        .collect::<std::collections::HashMap<_, _>>();
    let reward_rows = read_config_rows_or_empty(&dir.join("config_rewards.db"))
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();
    for (rank_drop_id, value) in read_config_rows_or_empty(&dir.join("config_copy_rank_drop.db")) {
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
    for (copy_id, value) in read_config_rows_or_empty(&dir.join("config_copy_display.db")) {
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
    for (copy_id, value) in read_config_rows_or_empty(&dir.join("config_copy_display.db")) {
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
    for (ship_id, value) in read_config_rows_or_empty(&dir.join("config_ship_main.db")) {
        if let Some(cost) = json_i64(&value, "supple_cost") {
            catalog.ship_supply_cost.insert(ship_id, cost.max(0));
        }
    }
    for (drop_id, value) in read_config_rows_or_empty(&dir.join("config_drop_item.db")) {
        let parse_entries = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|row| {
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
                })
                .collect::<Vec<_>>()
        };
        let pool = BattleDropPool {
            random_entries: parse_entries("drop"),
            random_count: json_i32(&value, "drop_count").unwrap_or_default().max(0),
            separate_entries: parse_entries("drop_alone"),
            separate_count: json_i32(&value, "drop_alone_count")
                .unwrap_or_default()
                .max(0),
        };
        if !pool.random_entries.is_empty() || !pool.separate_entries.is_empty() {
            catalog.drop_pools.insert(drop_id, pool);
        }
    }
    // Battle copy display rows reference config_drop_info. Keep explicit guaranteed rows as a
    // complete reward set and draw configured count from other categories. Do not overlay these
    // rows onto config_drop_item: IDs can collide with treasure pools, and tables have different
    // namespaces/semantics.
    for (drop_id, value) in read_config_rows_or_empty(&dir.join("config_drop_info.db")) {
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
            let mut pool = BattleCopyDropPool::default();
            let drop_type = json_i32(&value, "type").unwrap_or_default();
            let drop_rate = value
                .get("drop_rate")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if drop_type == 3 && matches!(drop_rate, "必ず" | "報酬") {
                pool.first_clear_entries = entries;
            } else if drop_type == 2 {
                pool.guaranteed_entries = entries;
            } else {
                pool.random_entries = entries;
                pool.random_count = json_i32(&value, "show_num").unwrap_or(1).max(1);
            }
            catalog.copy_drop_pools.insert(drop_id, pool);
        }
    }
    for (id, value) in read_config_rows_or_empty(&dir.join("config_main_line_reward_arg.db")) {
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
    let affection_changes = read_config_rows_or_empty(&dir.join("config_affection_change.db"))
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
    let chapter_rules = read_config_rows_or_empty(&dir.join("config_chapter_type.db"))
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
    let task_disabled_types = read_config_rows_or_empty(&dir.join("config_chapter_type.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            (json_i32(&value, "not_effect_task").unwrap_or_default() > 0).then_some(id)
        })
        .collect::<std::collections::HashSet<_>>();
    for (chapter_id, value) in read_config_rows_or_empty(&dir.join("config_chapter.db")) {
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
