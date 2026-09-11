use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use super::db as config_db;
use super::parse::*;
use super::*;
use crate::common::json::*;
use crate::{
    append_message_field, append_varint_field, current_unix_millis, expand_build_drop,
    normalize_multiplier, FashionInfo,
};

/// Immutable source for all server-owned game configuration.
///
/// Runtime opens one source, validates/warm-loads every table used by loaders, then passes its
/// normalized root to individual catalog builders. `catalogPath` remains an external compatibility
/// name; internally this is a config source, not a client catalog directory.
#[derive(Clone, Debug)]
pub(crate) struct ConfigSource {
    root: PathBuf,
    database_path: PathBuf,
}

impl ConfigSource {
    pub(crate) fn open(catalog_path: Option<&Path>) -> Result<Self, String> {
        let catalog_path = catalog_path
            .ok_or_else(|| "server config database path is not configured".to_owned())?;
        let root = config_dir(catalog_path);
        let database_path = config_db::config_db_path(&root);
        config_db::validate_config_db(&database_path).map_err(|error| {
            format!(
                "server config database {} is invalid: {error}",
                database_path.display()
            )
        })?;
        Ok(Self {
            root,
            database_path,
        })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub(crate) fn rows(
        &self,
        config_name: &str,
    ) -> Result<std::sync::Arc<Vec<(i32, Value)>>, String> {
        let path = self.root.join(format!("{config_name}.db"));
        let Some(config_name) = path.file_stem().and_then(|value| value.to_str()) else {
            return Err(format!(
                "config path has no valid file stem: {}",
                path.display()
            ));
        };
        config_db::load_config_rows_shared(&self.database_path, config_name)
            .map_err(|error| format!("load config table `{config_name}`: {error}"))
    }

    /// Decode every table used by runtime before listeners start.
    ///
    /// This turns missing tables and malformed typed columns into startup errors instead of empty
    /// catalogs that fail later in a client request. It also warms the existing table cache once.
    pub(crate) fn warm_required_configs(&self) -> Result<usize, String> {
        let started = std::time::Instant::now();
        let mut total_rows = 0;
        for config_name in REQUIRED_CONFIG_NAMES {
            let rows = self
                .rows(config_name)
                .map_err(|error| format!("load config table `{config_name}`: {error}"))?;
            tracing::debug!(
                config = config_name,
                rows = rows.len(),
                "game config table loaded"
            );
            total_rows += rows.len();
        }
        tracing::info!(
            config_tables = REQUIRED_CONFIG_NAMES.len(),
            config_rows = total_rows,
            elapsed_ms = started.elapsed().as_millis() as u64,
            path = %self.database_path.display(),
            "game config loaded"
        );
        Ok(total_rows)
    }
}

/// Tables referenced by every runtime catalog loader. Keep this list explicit so a damaged or
/// partially exported `server_config.db` fails at startup with a useful table name.
const REQUIRED_CONFIG_NAMES: &[&str] = &[
    "config_achievement",
    "config_activity",
    "config_activity_extract",
    "config_activity_extract_ur",
    "config_affection_change",
    "config_affection_item",
    "config_bathroom_item",
    "config_anniversary_video",
    "config_battlepass_level",
    "config_battlepass_level_activity",
    "config_battlepass_param",
    "config_battlepass_param_activity",
    "config_battlepass_task",
    "config_battlepass_task_activity",
    "config_build_ship",
    "config_building_character_story",
    "config_buildinginfo",
    "config_buildinglevelup",
    "config_worker",
    "config_chapter",
    "config_chapter_type",
    "config_combination_ship",
    "config_copy",
    "config_copy_display",
    "config_copy_rank_drop",
    "config_drop_info",
    "config_drop_item",
    "config_equip",
    "config_equip_enhance_item",
    "config_equip_enhance_level_exp",
    "config_equip_enhance_level_ur",
    "config_equip_enhance_renovate",
    "config_equip_levelbreak_item",
    "config_extract_ship",
    "config_fashion",
    "config_fleet",
    "config_food_compose",
    "config_gift",
    "config_guildboxscore",
    "config_guildwar_reward",
    "config_handbook_behaviour_index",
    "config_interaction_figurte",
    "config_interaction_item",
    "config_interaction_paper_cut_fomula",
    "config_item_exchange",
    "config_item_info",
    "config_item_selected",
    "config_item_valentine_gift",
    "config_magazine_info",
    "config_main_line_reward_arg",
    "config_minigame_copy",
    "config_parameter",
    "config_player_levelup",
    "config_pskill_dict_group",
    "config_random_factor_group",
    "config_random_factor_set",
    "config_recharge",
    "config_recipe",
    "config_rewards",
    "config_ship_advance",
    "config_ship_break",
    "config_ship_break_effect",
    "config_ship_enemy",
    "config_ship_exp_item",
    "config_ship_fleet",
    "config_ship_handbook",
    "config_ship_info",
    "config_ship_levelup",
    "config_ship_levelup_mub",
    "config_ship_main",
    "config_ship_max_power",
    "config_ship_need_power_exp",
    "config_ship_provide_power_exp",
    "config_ship_remould_effect",
    "config_ship_remould_template",
    "config_shop",
    "config_shop_goods",
    "config_sportsmeet_award",
    "config_support_fleet_item",
    "config_talent",
    "config_talentmain",
    "config_task_activity",
    "config_task_daily",
    "config_task_grow",
    "config_task_main",
    "config_task_return",
    "config_task_teaching",
    "config_task_teaching_group",
    "config_task_treaty",
    "config_task_weekly",
    "config_teaching_achievement",
    "config_testship_reward",
    "config_world_event",
    "config_value_effect",
];

fn read_config_rows(path: &Path) -> Result<Vec<(i32, Value)>, String> {
    let Some(config_name) = path.file_stem().and_then(|value| value.to_str()) else {
        return Err(format!(
            "config path has no valid file stem: {}",
            path.display()
        ));
    };
    let config_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let database_path = config_db::config_db_path(config_dir);
    config_db::load_config_rows(&database_path, config_name).map_err(|error| {
        format!(
            "load config table `{config_name}` from {}: {error}",
            database_path.display()
        )
    })
}

/// Legacy catalog builders still return defaults for optional/test-only callers. Production calls
/// `ConfigSource::warm_required_configs` first, so required table and decode errors stop startup.
fn read_config_rows_or_empty(path: &Path) -> Vec<(i32, Value)> {
    match read_config_rows(path) {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!(%error, "cannot load server config table");
            Vec::new()
        }
    }
}

pub(crate) fn config_dir(catalog_path: &Path) -> PathBuf {
    if catalog_path
        .file_name()
        .is_some_and(|name| name == "server-config")
    {
        return catalog_path.to_path_buf();
    }
    let Some(parent) = catalog_path.parent() else {
        return catalog_path.to_path_buf();
    };
    let prepared = parent.join("server-config");
    if prepared.join("manifest.json").is_file() && config_db::config_db_path(&prepared).is_file() {
        prepared
    } else {
        catalog_path.to_path_buf()
    }
}

pub(crate) fn load_chapter_catalog(catalog_path: Option<&Path>) -> ChapterCatalog {
    let Some(catalog_path) = catalog_path else {
        return ChapterCatalog::fallback();
    };
    let dir = config_dir(catalog_path);
    let rows = read_config_rows_or_empty(&dir.join("config_chapter.db"));
    // The response ChapterId must identify a row in client config_chapter.
    // Parameter 203 (30001) is the related tower-definition id, not the
    // ChapterId accepted by TowerThemePage. The class_type=24 row carries the
    // client-facing chapter id and its tower copy list.
    let tower_first_chapter = rows
        .iter()
        .find_map(|(id, value)| {
            (json_i32(&value, "class_type") == Some(24) && *id > 0).then_some(*id)
        })
        .unwrap_or(30_001);
    if rows.is_empty() {
        ChapterCatalog::fallback()
            .with_mini_game_rows(read_config_rows_or_empty(
                &dir.join("config_minigame_copy.db"),
            ))
            .with_tower_chapter_id(tower_first_chapter)
    } else {
        ChapterCatalog::from_rows(rows)
            .with_mini_game_rows(read_config_rows_or_empty(
                &dir.join("config_minigame_copy.db"),
            ))
            .with_tower_chapter_id(tower_first_chapter)
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

pub(crate) fn load_combination_catalog(catalog_path: Option<&Path>) -> CombinationCatalog {
    let Some(catalog_path) = catalog_path else {
        return CombinationCatalog::default();
    };
    let dir = config_dir(catalog_path);
    let open_sf_ids = read_config_rows_or_empty(&dir.join("config_ship_fleet.db"))
        .into_iter()
        .filter(|(_, value)| json_i32(value, "combination_open") == Some(1))
        .map(|(id, _)| id)
        .collect();
    let rules_by_id = read_config_rows_or_empty(&dir.join("config_combination_ship.db"))
        .into_iter()
        .filter_map(|(id, value)| {
            let levels = json_i32_array(&value, "level");
            let sf_id = json_i32(&value, "sf_id")?;
            let level_start = *levels.first()?;
            let level_end = *levels.get(1).unwrap_or(&level_start);
            (id > 0 && sf_id > 0).then_some((
                id,
                CombinationRule {
                    level_start,
                    level_end,
                    next_id: json_i32(&value, "next_id").unwrap_or_default(),
                    star: json_i32(&value, "star").unwrap_or_default(),
                    levelup_costs: combination_costs(&value, "levelup_item"),
                    break_costs: combination_costs(&value, "break_item"),
                    prop_up: json_i64_pairs(&value, "prop_up"),
                    break_prop_up: json_i64_pairs(&value, "break_prop_up"),
                    prop_up_percent: json_i64_pairs(&value, "prop_up_percent"),
                    break_prop_up_percent: json_i64_pairs(&value, "break_prop_up_percent"),
                },
            ))
        })
        .collect();
    CombinationCatalog {
        open_sf_ids,
        rules_by_id,
    }
}

mod equip {
    use super::*;
    include!("equip.rs");
}
mod ship {
    use super::*;
    include!("ship.rs");
}
mod shop {
    use super::*;
    include!("shop.rs");
}
mod gameplay {
    use super::*;
    include!("gameplay.rs");
}
mod progression {
    use super::*;
    include!("progression.rs");
}
mod world {
    use super::*;
    include!("world.rs");
}
mod task {
    use super::*;
    include!("task.rs");
}
mod build {
    use super::*;
    include!("build.rs");
}
mod battle {
    use super::*;
    include!("battle.rs");
}

pub(crate) use battle::*;
pub(crate) use build::*;
pub(crate) use equip::*;
pub(crate) use gameplay::*;
pub(crate) use progression::*;
pub(crate) use ship::*;
pub(crate) use shop::*;
pub(crate) use task::*;
pub(crate) use world::*;
#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn config_dir_prefers_prepared_server_catalog_when_manifest_exists() {
        let root =
            std::env::temp_dir().join(format!("blueoath-catalog-dir-test-{}", std::process::id()));
        let source = root.join("config");
        let prepared = root.join("server-config");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&prepared).unwrap();
        std::fs::write(prepared.join("manifest.json"), b"{}").unwrap();
        std::fs::write(prepared.join("config_chapter.json"), b"{}").unwrap();
        std::fs::write(prepared.join("config_shop.json"), b"{}").unwrap();
        std::fs::write(root.join("server_config.db"), b"{}").unwrap();

        assert_eq!(config_dir(&source), prepared);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bundled_config_source_warms_all_runtime_tables() {
        let config_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../catalog/server-config");
        let source = ConfigSource::open(Some(&config_dir)).unwrap();
        let total_rows = source.warm_required_configs().unwrap();

        assert_eq!(source.root(), config_dir);
        assert!(total_rows > 0);
    }

    #[test]
    fn bundled_catalogs_pass_startup_reference_validation() {
        let config_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../catalog/server-config");
        assert!(
            config_dir.is_dir(),
            "missing bundled catalog: {}",
            config_dir.display()
        );

        let chapters = load_chapter_catalog(Some(&config_dir));
        assert_eq!(chapters.tower_chapter_id, 100_001);
        let battle = load_battle_catalog(Some(&config_dir));
        let tasks = load_task_catalog(Some(&config_dir));
        let gameplay = load_gameplay_catalog(Some(&config_dir));
        let build_ship = load_build_ship_catalog(Some(&config_dir));
        let build_formula = load_build_formula_catalog(Some(&config_dir));

        chapters.validate().unwrap();
        chapters.validate_references(&gameplay).unwrap();
        battle.validate().unwrap();
        battle.validate_references().unwrap();
        tasks.validate_references().unwrap();
        gameplay.validate().unwrap();
        gameplay.validate_references().unwrap();
        build_ship.validate().unwrap();
        build_formula.validate().unwrap();

        let weekly_supply = build_ship.treasure_drop_pools.get(&14_000).unwrap();
        assert!(weekly_supply.random_entries.is_empty());
        assert_eq!(weekly_supply.guaranteed_entries.len(), 2);

        let daily_equipment = battle.copy_drop_pools.get(&20_0412).unwrap();
        assert!(daily_equipment.first_clear_entries.is_empty());
        assert!(daily_equipment.guaranteed_entries.is_empty());
        assert_eq!(daily_equipment.random_entries.len(), 52);
        assert_eq!(daily_equipment.random_count, 1);

        let daily_box = battle.copy_drop_pools.get(&20_101).unwrap();
        assert_eq!(daily_box.first_clear_entries.len(), 4);
        assert!(daily_box.guaranteed_entries.is_empty());
        assert!(daily_box.random_entries.is_empty());
        assert_eq!(daily_box.random_count, 0);

        // Daily battle rewards come from config_daily_group -> config_drop_item,
        // not from config_copy_display's UI preview rows.
        assert_eq!(battle.daily_group_by_copy.get(&20_101), Some(&2));
        assert_eq!(
            battle.daily_basic_drop_ids_by_copy.get(&20_101),
            Some(&vec![33_011])
        );
        assert_eq!(
            battle.daily_extra_drop_ids_by_copy.get(&20_101),
            Some(&vec![33_012])
        );
        let daily_basic = battle.drop_pools.get(&33_011).unwrap();
        assert_eq!(daily_basic.separate_count, 1);
        assert!(daily_basic
            .separate_entries
            .iter()
            .any(|entry| entry.0 == 1 && entry.1 == 13_001 && entry.2 == 180));
    }

    #[test]
    fn treasure_drop_preserves_all_guaranteed_rewards_and_random_count() {
        let mut catalog = BuildShipCatalog::default();
        catalog.treasure_drop_pools.insert(
            14_000,
            TreasureDropPool {
                random_entries: vec![(1, 10_181, 1, 1, 10_000)],
                guaranteed_entries: vec![(5, 5, 3_000, 3_000, 10_000), (1, 10_182, 1, 1, 10_000)],
                random_count: 2,
                guaranteed_count: 1,
            },
        );

        let rewards = draw_treasure_rewards_with_roll(&catalog, 14_000, 123).unwrap();
        assert_eq!(rewards.len(), 4);
        assert!(rewards.contains(&(5, 5, 3_000)));
        assert!(rewards.contains(&(1, 10_182, 1)));
        assert_eq!(
            rewards
                .iter()
                .filter(|reward| **reward == (1, 10_181, 1))
                .count(),
            2
        );
    }
}
