pub(crate) fn value_i64_any(value: &Value, keys: &[&str]) -> i64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_i64))
        .unwrap_or_default()
}

pub(crate) fn value_array_i64(value: &Value, keys: &[&str]) -> Vec<i64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_array))
        .map(|items| items.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default()
}

pub(crate) fn task_definition_from_value(
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

pub(crate) fn load_task_catalog(catalog_path: Option<&Path>) -> TaskCatalog {
    let Some(catalog_path) = catalog_path else {
        return TaskCatalog::default();
    };
    let dir = config_dir(catalog_path);
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
        for (id, value) in read_config_rows_or_empty(&dir.join(file)) {
            if let Some(definition) = task_definition_from_value(task_type, id, &value) {
                definitions.push(definition);
            }
        }
    }
    for (id, value) in read_config_rows_or_empty(&dir.join("config_achievement.db")) {
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
    let teaching = read_config_rows_or_empty(&dir.join("config_task_teaching.db"))
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();
    for (_group_id, group) in read_config_rows_or_empty(&dir.join("config_task_teaching_group.db"))
    {
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
    let rewards_by_id = read_config_rows_or_empty(&dir.join("config_rewards.db"))
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
    let teaching_rewards_by_id =
        read_config_rows_or_empty(&dir.join("config_teaching_achievement.db"))
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
