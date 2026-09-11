pub(crate) fn load_recharge_catalog(catalog_path: Option<&Path>) -> RechargeCatalog {
    let Some(catalog_path) = catalog_path else {
        return RechargeCatalog::default();
    };
    let dir = config_dir(catalog_path);
    let rewards = load_reward_definitions(&dir);
    let mut catalog = RechargeCatalog::default();
    for (recharge_id, value) in read_config_rows_or_empty(&dir.join("config_recharge.db")) {
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

pub(crate) fn load_gameplay_catalog(catalog_path: Option<&Path>) -> GameplayCatalog {
    let Some(catalog_path) = catalog_path else {
        return GameplayCatalog::default();
    };
    let dir = config_dir(catalog_path);
    let rows = |name: &str| {
        read_config_rows_or_empty(&dir.join(name))
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let battlepass_levels = |name: &str| {
        rows(name)
            .into_iter()
            .map(|(id, value)| {
                (
                    id,
                    BattlePassLevelConfig {
                        free_level_reward: json_i32(&value, "free_level_reward")
                            .unwrap_or_default(),
                        pay_level_reward: json_i32(&value, "pay_level_reward").unwrap_or_default(),
                    },
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let battlepass_tasks = |name: &str| {
        rows(name)
            .into_iter()
            .map(|(id, value)| {
                (
                    id,
                    BattlePassTaskConfig {
                        experience: json_i32(&value, "battlepass_exp").unwrap_or_default(),
                    },
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let battlepass_param = |name: &str| {
        read_config_rows_or_empty(&dir.join(name))
            .into_iter()
            .next()
            .map(|(_, value)| BattlePassParamConfig {
                buy_level_price: value
                    .get("buy_level_price")
                    .and_then(Value::as_array)
                    .and_then(|values| {
                        Some((
                            i32::try_from(values.first()?.as_i64()?).ok()?,
                            i32::try_from(values.get(1)?.as_i64()?).ok()?,
                        ))
                    }),
            })
    };
    let exchanges = rows("config_item_exchange.db")
        .into_iter()
        .map(|(id, value)| {
            (
                id,
                ExchangeConfig {
                    change_count: json_i32(&value, "change_count").unwrap_or_default(),
                    item_consume: config_triplets(&value, "item_consume"),
                    item_reward: config_triplets(&value, "item_reward"),
                },
            )
        })
        .collect();
    let food_recipes = rows("config_food_compose.db")
        .into_iter()
        .map(|(id, value)| {
            let reward_id = value
                .get("reward")
                .and_then(Value::as_array)
                .and_then(|values| values.first())
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok())
                .unwrap_or_default();
            (
                id,
                FoodRecipeConfig {
                    material: config_triplets(&value, "material"),
                    reward_id,
                },
            )
        })
        .collect();
    let anniversary_videos = rows("config_anniversary_video.db")
        .into_iter()
        .map(|(id, value)| {
            (
                id,
                AnniversaryVideoConfig {
                    reward_id: json_i32(&value, "reward").unwrap_or_default(),
                },
            )
        })
        .collect();
    let paper_cut_formulas = rows("config_interaction_paper_cut_fomula.db")
        .into_iter()
        .map(|(id, value)| {
            (
                id,
                PaperCutFormulaConfig {
                    id: json_i32(&value, "id").unwrap_or(id),
                    materials: config_i32_array(&value, "formula"),
                    drop_id: json_i32(&value, "drop_id").unwrap_or_default(),
                },
            )
        })
        .collect();
    let drop_items = rows("config_drop_item.db")
        .into_iter()
        .map(|(id, value)| {
            (
                id,
                DropItemConfig {
                    entries: config_drop_entries(&value),
                },
            )
        })
        .collect();
    let magazine_info = rows("config_magazine_info.db")
        .into_iter()
        .map(|(id, value)| {
            (
                id,
                MagazineInfoConfig {
                    rewards: config_i32_array(&value, "rewards"),
                },
            )
        })
        .collect();
    let interaction_items = rows("config_interaction_item.db")
        .into_iter()
        .map(|(id, value)| {
            (
                id,
                InteractionItemConfig {
                    reward_id: json_i32(&value, "reward").unwrap_or_default(),
                    drop_id: json_i32(&value, "drop_id").unwrap_or_default(),
                },
            )
        })
        .collect();
    let parameters = rows("config_parameter.db")
        .into_iter()
        .map(|(id, value)| {
            (
                id,
                ParameterConfig {
                    value: json_i32(&value, "value").unwrap_or_default(),
                },
            )
        })
        .collect();
    let interaction_figures = rows("config_interaction_figurte.db")
        .into_iter()
        .map(|(id, value)| {
            (
                id,
                InteractionFigureConfig {
                    is_drawable: json_i32(&value, "is_drawable").unwrap_or_default(),
                    figure_type: json_i32(&value, "figure_type").unwrap_or_default(),
                    original_ship_required: json_i32(&value, "origional_ship_required")
                        .unwrap_or_default(),
                },
            )
        })
        .collect();
    let activity_extract = rows("config_activity_extract.db")
        .into_iter()
        .map(|(id, value)| (id, config_extract(&value)))
        .collect();
    let activity_extract_ur = rows("config_activity_extract_ur.db")
        .into_iter()
        .map(|(id, value)| (id, config_extract(&value)))
        .collect();
    let guild_war_rewards = rows("config_guildwar_reward.db")
        .into_iter()
        .map(|(id, value)| {
            (
                id,
                GuildWarRewardConfig {
                    base_id: json_i32(&value, "base_id").unwrap_or_default(),
                    stage: json_i32(&value, "stage").unwrap_or_default(),
                    reward_id: json_i32(&value, "guild_reward").unwrap_or_default(),
                },
            )
        })
        .collect();
    let guild_box_scores = rows("config_guildboxscore.db")
        .into_iter()
        .map(|(id, value)| {
            (
                id,
                GuildBoxScoreConfig {
                    reward_id: json_i32(&value, "reward").unwrap_or_default(),
                },
            )
        })
        .collect();
    let sportsmeet_awards = rows("config_sportsmeet_award.db")
        .into_iter()
        .map(|(id, value)| {
            (
                id,
                SportsMeetAwardConfig {
                    score: json_i32(&value, "score").unwrap_or_default(),
                    reward_id: json_i32(&value, "rewards")
                        .or_else(|| json_i32(&value, "reward"))
                        .unwrap_or_default(),
                },
            )
        })
        .collect();
    GameplayCatalog {
        rewards_by_id: load_reward_definitions(&dir),
        battlepass_levels: battlepass_levels("config_battlepass_level.db"),
        battlepass_tasks: battlepass_tasks("config_battlepass_task.db"),
        battlepass_activity_levels: battlepass_levels("config_battlepass_level_activity.db"),
        battlepass_activity_tasks: battlepass_tasks("config_battlepass_task_activity.db"),
        battlepass_param: battlepass_param("config_battlepass_param.db"),
        battlepass_activity_param: battlepass_param("config_battlepass_param_activity.db"),
        activity: rows("config_activity.db")
            .into_iter()
            .map(|(id, value)| (id, config_activity(id, &value)))
            .collect(),
        parameters,
        activity_extract,
        activity_extract_ur,
        anniversary_videos,
        paper_cut_formulas,
        drop_items,
        exchanges,
        food_recipes,
        testship_rewards: rows("config_testship_reward.db")
            .into_iter()
            .map(|(id, value)| (id, config_testship_reward(&value)))
            .collect(),
        world_events: rows("config_world_event.db")
            .into_iter()
            .map(|(id, value)| (id, config_world_event(&value)))
            .collect(),
        guild_war_rewards,
        magazine_info,
        interaction_items,
        interaction_figures,
        guild_box_scores,
        valentine_gifts: rows("config_item_valentine_gift.db")
            .into_iter()
            .map(|(id, value)| (id, config_valentine_gift(&value)))
            .collect(),
        sportsmeet_awards,
    }
}

pub(crate) fn load_server_mail_templates(
    database_path: Option<&Path>,
) -> Result<Vec<MailTemplate>, String> {
    let database_path = database_path.ok_or_else(|| {
        "server config database path is not configured for mail templates".to_owned()
    })?;
    let rows = config_db::load_server_mail_templates(database_path).map_err(|error| {
        format!(
            "load server mail templates from {}: {error}",
            database_path.display()
        )
    })?;
    Ok(rows
        .into_iter()
        .map(|mail| MailTemplate {
            mid: mail.mid,
            goods_type: mail.goods_type,
            config_id: mail.config_id,
            num: mail.num.max(1),
            subject: mail.subject,
            content: mail.content,
        })
        .collect())
}
