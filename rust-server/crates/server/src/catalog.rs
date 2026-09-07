#![allow(dead_code)]

use serde_json::Value;

pub(super) static SUPPORT_CATALOG: std::sync::OnceLock<SupportCatalog> = std::sync::OnceLock::new();

#[derive(Debug, Clone, Default)]
pub(super) struct EquipCatalog {
    pub(super) skills_by_template: std::collections::BTreeMap<i32, Vec<(i32, i32)>>,
    pub(super) quality_by_template: std::collections::BTreeMap<i32, i32>,
    pub(super) type_by_template: std::collections::BTreeMap<i32, i32>,
    pub(super) enhance_max_by_template: std::collections::BTreeMap<i32, i32>,
    pub(super) star_max_by_template: std::collections::BTreeMap<i32, i32>,
    pub(super) enhance_materials: std::collections::BTreeMap<i32, (i32, Option<(i32, i32)>)>,
    pub(super) enhance_level_exp: std::collections::BTreeMap<i32, i32>,
    pub(super) enhance_level_ur: std::collections::BTreeMap<i32, Vec<(i32, i32, i32)>>,
    pub(super) levelbreak_rules: std::collections::BTreeMap<i32, EquipLevelbreakRule>,
    pub(super) renovate_rules: std::collections::BTreeMap<i32, EquipRenovateRule>,
    pub(super) dismantle_rewards_by_template: std::collections::BTreeMap<i32, Vec<(i32, i32, i32)>>,
    pub(super) activity_equip_by_template: std::collections::BTreeSet<i32>,
    pub(super) activity_reward_by_template: std::collections::BTreeMap<i32, i32>,
    pub(super) no_resolve_templates: std::collections::BTreeSet<i32>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct EquipLevelbreakRule {
    pub(super) level_rank: Option<(i32, i32)>,
    pub(super) costs: Vec<(i32, i32, i32)>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct EquipRenovateRule {
    pub(super) costs: Vec<(i32, i32, i32)>,
    pub(super) self_count: usize,
    pub(super) need_level: i32,
}

#[derive(Debug, Clone, Default)]
pub(super) struct HeroBreakdownCatalog {
    pub(super) rewards_by_template: std::collections::BTreeMap<i32, Vec<(i32, i32, i32)>>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct CombinationRule {
    pub(super) level_end: i32,
    pub(super) next_id: i32,
    pub(super) star: i32,
    pub(super) levelup_costs: Vec<(i32, i32, i32)>,
    pub(super) break_costs: Vec<(i32, i32, i32)>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct CombinationCatalog {
    pub(super) open_sf_ids: std::collections::BTreeSet<i32>,
    pub(super) rules_by_id: std::collections::BTreeMap<i32, CombinationRule>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct BuildingCatalog {
    pub(super) capacities: std::collections::BTreeMap<i32, usize>,
    #[cfg(test)]
    pub(super) building_configs: std::collections::BTreeMap<i32, Value>,
    #[cfg(test)]
    pub(super) recipe_configs: std::collections::BTreeMap<i32, Value>,
    pub(super) typed_building_configs: std::collections::BTreeMap<i32, BuildingConfig>,
    pub(super) typed_recipe_configs: std::collections::BTreeMap<i32, RecipeConfig>,
    pub(super) resource_time_seconds: std::collections::BTreeMap<i32, i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct BuildingConfig {
    pub(super) building_type: i32,
    pub(super) product_max: i32,
    pub(super) product_id: Option<i32>,
    pub(super) productivity: i32,
    pub(super) produce_speed: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct RecipeConfig {
    pub(super) time_seconds: i32,
    pub(super) goods_type: i32,
    pub(super) item_id: i32,
    pub(super) item_amount: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ChapterCatalog {
    pub(super) plot: Vec<i32>,
    pub(super) sea: Vec<i32>,
    pub(super) sea_initial: i32,
    pub(super) mubar: Vec<i32>,
    pub(super) daily: Vec<i32>,
    pub(super) goods_copy: Vec<i32>,
    pub(super) tower: Vec<i32>,
    pub(super) equip_new_test: Vec<i32>,
    pub(super) tower_chapter_id: i32,
    pub(super) daily_chapters: Vec<(i32, i32)>,
    pub(super) daily_groups: Vec<i32>,
    pub(super) memories: Vec<(i32, i32)>,
    pub(super) star_rewards_by_chapter: std::collections::BTreeMap<i32, ChapterStarRewards>,
    pub(super) mini_game_ids: std::collections::BTreeSet<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ChapterStarRewards {
    pub(super) level_ids: Vec<i32>,
    pub(super) star_conditions: Vec<i32>,
    pub(super) reward_ids: Vec<i32>,
}

impl ChapterCatalog {
    pub(super) fn validate(&self) -> Result<(), String> {
        for (name, values) in [
            ("plot", &self.plot),
            ("sea", &self.sea),
            ("mubar", &self.mubar),
            ("daily", &self.daily),
            ("goods_copy", &self.goods_copy),
            ("tower", &self.tower),
            ("equip_new_test", &self.equip_new_test),
        ] {
            if values.iter().any(|id| *id <= 0) {
                return Err(format!("chapter catalog {name} contains non-positive id"));
            }
        }
        if self.sea_initial < 0 || self.tower_chapter_id < 0 {
            return Err("chapter catalog contains invalid default id".to_owned());
        }
        if self
            .daily_chapters
            .iter()
            .any(|(chapter_id, group_id)| *chapter_id <= 0 || *group_id <= 0)
        {
            return Err("chapter catalog contains invalid daily group reference".to_owned());
        }
        for (chapter_id, rewards) in &self.star_rewards_by_chapter {
            if *chapter_id <= 0
                || rewards.level_ids.is_empty()
                || rewards.star_conditions.len() != rewards.reward_ids.len()
                || rewards.star_conditions.iter().any(|stars| *stars < 0)
                || rewards.reward_ids.iter().any(|reward_id| *reward_id <= 0)
            {
                return Err(format!(
                    "chapter catalog star rewards invalid for {chapter_id}"
                ));
            }
        }
        Ok(())
    }

    pub(super) fn validate_references(&self, gameplay: &GameplayCatalog) -> Result<(), String> {
        for (chapter_id, rewards) in &self.star_rewards_by_chapter {
            for reward_id in &rewards.reward_ids {
                if !gameplay.rewards_by_id.contains_key(reward_id) {
                    return Err(format!(
                        "chapter {chapter_id} references missing reward {reward_id}"
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn with_mini_game_rows(
        mut self,
        rows: impl IntoIterator<Item = (i32, Value)>,
    ) -> Self {
        self.mini_game_ids = rows
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| *id > 0)
            .collect();
        self
    }

    pub(super) fn from_rows(rows: impl IntoIterator<Item = (i32, Value)>) -> Self {
        let mut catalog = Self::default();
        let mut sea_chapters = Vec::<(i32, Vec<i32>)>::new();
        for (chapter_id, value) in rows {
            let levels = json_i32_array(&value, "level_list");
            let star_conditions = json_i32_array(&value, "star_cond");
            let reward_ids = json_i32_array(&value, "star_reward");
            if !levels.is_empty()
                && !star_conditions.is_empty()
                && star_conditions.len() == reward_ids.len()
            {
                catalog.star_rewards_by_chapter.insert(
                    chapter_id,
                    ChapterStarRewards {
                        level_ids: levels.clone(),
                        star_conditions,
                        reward_ids,
                    },
                );
            }
            if levels.is_empty() {
                continue;
            }
            let class_type = json_i32(&value, "class_type").unwrap_or_default();
            let is_plot = json_i32(&value, "chapter_plot_type").unwrap_or_default() > 0
                || json_i32(&value, "memory_id").unwrap_or_default() > 0;
            if json_i32(&value, "memory_id").unwrap_or_default() > 0 {
                catalog.memories.push((chapter_id, levels.len() as i32));
            }
            if is_plot {
                catalog.plot.extend(levels);
            } else {
                match class_type {
                    2 | 32 | 69 | 71 => {
                        sea_chapters.push((chapter_id, levels));
                    }
                    33 => catalog.mubar.extend(levels),
                    10 => catalog.goods_copy.extend(levels),
                    24 => {
                        if chapter_id == 30_001 || catalog.tower_chapter_id == 0 {
                            catalog.tower_chapter_id = chapter_id;
                        }
                        catalog.tower.extend(levels);
                    }
                    34 => catalog.equip_new_test.extend(levels),
                    9 => {
                        catalog.daily.extend(levels);
                        catalog.daily.extend(json_i32_array(&value, "treaty_copy"));
                        let group_id = json_i32(&value, "dailygroup_id").unwrap_or_default();
                        catalog.daily_chapters.push((chapter_id, group_id));
                        if group_id > 0 {
                            catalog.daily_groups.push(group_id);
                        }
                    }
                    _ => {}
                }
            }
        }
        catalog.plot.sort_unstable();
        catalog.plot.dedup();
        sea_chapters.sort_unstable_by_key(|(chapter_id, _)| *chapter_id);
        catalog.sea = sea_chapters
            .into_iter()
            .flat_map(|(_, levels)| levels)
            .collect();
        let mut seen_sea = std::collections::HashSet::new();
        catalog.sea.retain(|level| seen_sea.insert(*level));
        catalog.sea_initial = catalog.sea.first().copied().unwrap_or_default();
        catalog.mubar.sort_unstable();
        catalog.mubar.dedup();
        catalog.daily.sort_unstable();
        catalog.daily.dedup();
        catalog.goods_copy.sort_unstable();
        catalog.goods_copy.dedup();
        catalog.tower.sort_unstable();
        catalog.tower.dedup();
        catalog.equip_new_test.sort_unstable();
        catalog.equip_new_test.dedup();
        catalog
            .daily_chapters
            .sort_unstable_by_key(|(chapter_id, _)| *chapter_id);
        catalog.daily_chapters.dedup();
        catalog.daily_groups.sort_unstable();
        catalog.daily_groups.dedup();
        catalog
            .memories
            .sort_unstable_by_key(|(chapter_id, _)| *chapter_id);
        catalog.memories.dedup();
        catalog
    }

    pub(super) fn fallback() -> Self {
        Self {
            plot: vec![
                1, 2, 3, 4, 6, 7, 9, 10, 11, 12, 13, 101, 102, 103, 104, 105, 106, 107, 108,
            ],
            sea: vec![1600100],
            sea_initial: 1600100,
            mubar: vec![932113],
            daily: vec![1],
            goods_copy: vec![60000],
            tower: vec![703001],
            equip_new_test: Vec::new(),
            tower_chapter_id: 30001,
            daily_chapters: vec![(1, 1)],
            daily_groups: vec![1],
            memories: Vec::new(),
            star_rewards_by_chapter: Default::default(),
            mini_game_ids: Default::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct EquipNewTestCatalog {
    pub(super) activity_id: i32,
    pub(super) copy_ids: Vec<i32>,
    pub(super) damage_thresholds: Vec<Vec<i32>>,
    pub(super) reward_ids: Vec<Vec<i32>>,
}

type EquipNewTestCandidate = (i32, Vec<i32>, Vec<Vec<i32>>, Vec<Vec<i32>>);

impl EquipNewTestCatalog {
    pub(super) fn from_rows(rows: impl IntoIterator<Item = (i32, Value)>) -> Self {
        let mut selected: Option<EquipNewTestCandidate> = None;
        for (activity_id, value) in rows {
            let is_new_test = json_i32(&value, "type") == Some(70)
                || value
                    .get("banner_gotopage_activity")
                    .and_then(Value::as_str)
                    == Some("EquipNewTestPage");
            if !is_new_test || json_i32(&value, "is_open").unwrap_or_default() <= 0 {
                continue;
            }
            let copy_ids = json_i32_array(&value, "p1");
            let damage_thresholds = value
                .get("p4")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .map(|row| {
                            row.as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_i64)
                                .filter_map(|value| i32::try_from(value).ok())
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let reward_ids = value
                .get("p5")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .map(|row| {
                            row.as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_i64)
                                .filter_map(|value| i32::try_from(value).ok())
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let valid = !copy_ids.is_empty()
                && copy_ids.len() == damage_thresholds.len()
                && copy_ids.len() == reward_ids.len()
                && damage_thresholds
                    .iter()
                    .zip(&reward_ids)
                    .all(|(thresholds, rewards)| {
                        !thresholds.is_empty()
                            && thresholds.len() == rewards.len()
                            && thresholds.iter().all(|value| *value > 0)
                            && rewards.iter().all(|value| *value > 0)
                    });
            if valid
                && selected
                    .as_ref()
                    .is_none_or(|current| activity_id > current.0)
            {
                selected = Some((activity_id, copy_ids, damage_thresholds, reward_ids));
            }
        }
        selected
            .map(
                |(activity_id, copy_ids, damage_thresholds, reward_ids)| Self {
                    activity_id,
                    copy_ids,
                    damage_thresholds,
                    reward_ids,
                },
            )
            .unwrap_or_default()
    }

    pub(super) fn reward_id(&self, copy_index: i32, damage_index: i32) -> Option<(i32, i32)> {
        let copy = usize::try_from(copy_index.saturating_sub(1)).ok()?;
        let damage = usize::try_from(damage_index.saturating_sub(1)).ok()?;
        Some((
            *self.damage_thresholds.get(copy)?.get(damage)?,
            *self.reward_ids.get(copy)?.get(damage)?,
        ))
    }
}

/// Read-only catalogs needed while dispatching one game-login request.
///
/// Keeping these dependencies together prevents route handlers from accepting a
/// long list of unrelated optional arguments and makes missing catalog data
/// explicit at the call site.
#[derive(Clone, Copy)]
pub(super) struct GameLoginCatalogs<'a> {
    pub(super) fashion: Option<&'a FashionList>,
    pub(super) equip: Option<&'a EquipCatalog>,
    pub(super) hero_level: Option<&'a HeroLevelCatalog>,
    pub(super) shop: Option<&'a ShopCatalog>,
    pub(super) mails: Option<&'a [MailTemplate]>,
    pub(super) handbook_behaviours: Option<&'a [i32]>,
    pub(super) hero_memories: Option<&'a [(i32, i32)]>,
    pub(super) chapters: Option<&'a ChapterCatalog>,
    pub(super) tasks: Option<&'a TaskCatalog>,
    pub(super) battle: Option<&'a BattleCatalog>,
    pub(super) hero_breakdown: Option<&'a HeroBreakdownCatalog>,
    pub(super) buildings: Option<&'a BuildingCatalog>,
    pub(super) equip_new_test: Option<&'a EquipNewTestCatalog>,
    pub(super) affection: Option<&'a AffectionCatalog>,
    pub(super) combination: Option<&'a CombinationCatalog>,
}

impl<'a> GameLoginCatalogs<'a> {
    pub(super) fn empty() -> Self {
        Self {
            fashion: None,
            equip: None,
            hero_level: None,
            shop: None,
            mails: None,
            handbook_behaviours: None,
            hero_memories: None,
            chapters: None,
            tasks: None,
            battle: None,
            hero_breakdown: None,
            buildings: None,
            equip_new_test: None,
            affection: None,
            combination: None,
        }
    }
}

#[derive(Clone)]
pub(super) struct GameCatalogs {
    pub(super) chapters: Arc<ChapterCatalog>,
    pub(super) fashion: Arc<FashionList>,
    pub(super) equip: Arc<EquipCatalog>,
    pub(super) shop: Arc<ShopCatalog>,
    pub(super) handbook_behaviours: Arc<Vec<i32>>,
    pub(super) hero_memories: Arc<Vec<(i32, i32)>>,
    pub(super) tasks: Arc<TaskCatalog>,
    pub(super) battle: Arc<BattleCatalog>,
    pub(super) mails: Arc<Vec<MailTemplate>>,
    pub(super) hero_level: Arc<HeroLevelCatalog>,
    pub(super) hero_breakdown: Arc<HeroBreakdownCatalog>,
    pub(super) buildings: Arc<BuildingCatalog>,
    pub(super) equip_new_test: Arc<EquipNewTestCatalog>,
    pub(super) affection: Arc<AffectionCatalog>,
    pub(super) combination: Arc<CombinationCatalog>,
}

impl GameCatalogs {
    pub(super) fn validate(&self) -> Result<(), String> {
        self.chapters.validate()?;
        self.battle.validate()?;
        if self
            .tasks
            .rewards_by_id
            .keys()
            .any(|reward_id| *reward_id <= 0)
        {
            return Err("task catalog contains non-positive reward id".to_owned());
        }
        let mut task_keys = std::collections::BTreeSet::new();
        for definition in &self.tasks.definitions {
            if definition.id <= 0
                || definition.task_type <= 0
                || definition.goal < 0
                || !task_keys.insert((definition.task_type, definition.id))
            {
                return Err(format!(
                    "task catalog contains invalid or duplicate task {}:{}",
                    definition.task_type, definition.id
                ));
            }
            if definition.reward_id < 0 || definition.medal_id < 0 || definition.point < 0 {
                return Err(format!(
                    "task catalog contains invalid reward fields for {}:{}",
                    definition.task_type, definition.id
                ));
            }
            if definition
                .inline_rewards
                .iter()
                .any(|(goods_type, item_id, amount)| {
                    *goods_type <= 0 || *item_id <= 0 || *amount <= 0
                })
            {
                return Err(format!(
                    "task catalog contains invalid inline reward for {}:{}",
                    definition.task_type, definition.id
                ));
            }
        }
        for (shop_id, good_ids) in &self.shop.goods_by_shop {
            if *shop_id <= 0
                || good_ids.iter().any(|good_id| {
                    *good_id <= 0
                        || self
                            .shop
                            .goods_by_id
                            .get(good_id)
                            .is_none_or(|good| good.shop_id != *shop_id)
                })
            {
                return Err(format!("shop catalog contains invalid shop {shop_id}"));
            }
        }
        for (good_id, good) in &self.shop.goods_by_id {
            if *good_id <= 0
                || good.shop_id <= 0
                || good.goods_type <= 0
                || good.item_id <= 0
                || good.num <= 0
                || good
                    .costs
                    .iter()
                    .any(|cost| cost.goods_type <= 0 || cost.item_id <= 0 || cost.amount <= 0)
            {
                return Err(format!("shop catalog contains invalid good {good_id}"));
            }
        }
        Ok(())
    }

    pub(super) fn validate_references(&self, gameplay: &GameplayCatalog) -> Result<(), String> {
        self.chapters.validate_references(gameplay)?;
        self.battle.validate_references()?;
        self.tasks.validate_references()?;
        gameplay.validate_references()?;
        Ok(())
    }

    pub(super) fn login_catalogs(&self) -> GameLoginCatalogs<'_> {
        GameLoginCatalogs {
            fashion: Some(&self.fashion),
            equip: Some(&self.equip),
            hero_level: Some(&self.hero_level),
            shop: Some(&self.shop),
            mails: Some(self.mails.as_slice()),
            handbook_behaviours: Some(self.handbook_behaviours.as_slice()),
            hero_memories: Some(self.hero_memories.as_slice()),
            chapters: Some(&self.chapters),
            tasks: Some(&self.tasks),
            battle: Some(&self.battle),
            hero_breakdown: Some(&self.hero_breakdown),
            buildings: Some(&self.buildings),
            equip_new_test: Some(&self.equip_new_test),
            affection: Some(&self.affection),
            combination: Some(&self.combination),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct TalentNode {
    pub(super) belong_talent: i32,
    pub(super) next_talent: i32,
    pub(super) precondition: Vec<i32>,
    pub(super) costs: Vec<(i32, i32, i64)>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct TalentCatalog {
    pub(super) roots: Vec<i32>,
    pub(super) nodes: std::collections::BTreeMap<i32, TalentNode>,
}

pub(super) static TALENT_CATALOG: std::sync::OnceLock<TalentCatalog> = std::sync::OnceLock::new();

pub(super) fn current_talent_catalog() -> TalentCatalog {
    TALENT_CATALOG.get().cloned().unwrap_or_default()
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShopCatalog {
    pub(super) goods_by_shop: std::collections::BTreeMap<i32, Vec<i32>>,
    pub(super) goods_by_id: std::collections::BTreeMap<i32, ShopGood>,
    pub(super) costs_by_good_id: std::collections::BTreeMap<i32, Vec<ShopCost>>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct SupportFleetItem {
    pub(super) duration_seconds: i64,
    pub(super) base_rewards: Vec<(i32, i32, i32)>,
    pub(super) big_success_base_rewards: Vec<(i32, i32, i32)>,
    pub(super) extra_drop_id: i32,
    pub(super) big_success_extra_drop_id: i32,
    pub(super) big_success_ratio: i32,
    pub(super) consumption: Option<(i32, i32, i32)>,
    pub(super) fast_consumption: Option<(i32, i32, i32)>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct SupportCatalog {
    pub(super) items: std::collections::BTreeMap<i32, SupportFleetItem>,
    pub(super) drop_rewards: std::collections::BTreeMap<i32, Vec<(i32, i32, i32)>>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShopGood {
    pub(super) shop_id: i32,
    pub(super) goods_type: i32,
    pub(super) item_id: i32,
    pub(super) num: i32,
    pub(super) costs: Vec<ShopCost>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShopCost {
    pub(super) goods_type: i32,
    pub(super) item_id: i32,
    pub(super) amount: i64,
}

#[derive(Clone, Debug, Default)]
pub(super) struct RechargeCatalog {
    pub(super) rewards_by_recharge_id: std::collections::BTreeMap<i32, Vec<ShopReward>>,
}

/// Config rows used by JP-only services that are not part of core fleet combat.
/// Keeping raw rows preserves version-specific fields while handlers validate the
/// fields they consume against the client protobuf descriptors.
#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub(super) struct BattlePassLevelConfig {
    pub(super) free_level_reward: i32,
    pub(super) pay_level_reward: i32,
}

#[derive(Clone, Debug, Default)]
pub(super) struct BattlePassTaskConfig {
    pub(super) experience: i32,
}

#[derive(Clone, Debug, Default)]
pub(super) struct BattlePassParamConfig {
    pub(super) buy_level_price: Option<(i32, i32)>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ExchangeConfig {
    pub(super) change_count: i32,
    pub(super) item_consume: Vec<(i32, i32, i32)>,
    pub(super) item_reward: Vec<(i32, i32, i32)>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct FoodRecipeConfig {
    pub(super) material: Vec<(i32, i32, i32)>,
    pub(super) reward_id: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct DropEntry {
    pub(super) goods_type: i32,
    pub(super) item_id: i32,
    pub(super) min: i32,
    pub(super) max: i32,
    pub(super) rate: i64,
}

#[derive(Clone, Debug, Default)]
pub(super) struct DropItemConfig {
    pub(super) entries: Vec<DropEntry>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct PaperCutFormulaConfig {
    pub(super) id: i32,
    pub(super) materials: Vec<i32>,
    pub(super) drop_id: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct AnniversaryVideoConfig {
    pub(super) reward_id: i32,
}

#[derive(Clone, Debug, Default)]
pub(super) struct MagazineInfoConfig {
    pub(super) rewards: Vec<i32>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct InteractionItemConfig {
    pub(super) reward_id: i32,
    pub(super) drop_id: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ParameterConfig {
    pub(super) value: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct InteractionFigureConfig {
    pub(super) is_drawable: i32,
    pub(super) figure_type: i32,
    pub(super) original_ship_required: i32,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ActivityExtractConfig {
    pub(super) cost: Option<(i32, i32, i32)>,
    pub(super) rewards: Vec<(i32, i32)>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ActivityConfig {
    pub(super) id: i32,
    pub(super) activity_type: i32,
    pub(super) is_open: i32,
    pub(super) p1: Vec<i32>,
    pub(super) p2: Option<(i32, i32)>,
    pub(super) p3: Option<(i32, i32)>,
    pub(super) p4: Vec<Vec<i32>>,
    pub(super) p5: Vec<(i32, i32)>,
    pub(super) p6: Vec<i32>,
    pub(super) p14: Option<(i32, i32, i32)>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct WorldEventConfig {
    pub(super) server_stage_rewards: Vec<(i32, i32)>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ValentineGiftConfig {
    pub(super) ship_fleet_id: i32,
    pub(super) attach_reward: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TestShipRewardConfig {
    pub(super) reward_id: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct GuildBoxScoreConfig {
    pub(super) reward_id: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct GuildWarRewardConfig {
    pub(super) base_id: i32,
    pub(super) stage: i32,
    pub(super) reward_id: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SportsMeetAwardConfig {
    pub(super) score: i32,
    pub(super) reward_id: i32,
}

#[derive(Clone, Debug, Default)]
pub(super) struct GameplayCatalog {
    pub(super) rewards_by_id: std::collections::BTreeMap<i32, Vec<ShopReward>>,
    pub(super) battlepass_levels: std::collections::BTreeMap<i32, BattlePassLevelConfig>,
    pub(super) battlepass_tasks: std::collections::BTreeMap<i32, BattlePassTaskConfig>,
    pub(super) battlepass_activity_levels: std::collections::BTreeMap<i32, BattlePassLevelConfig>,
    pub(super) battlepass_activity_tasks: std::collections::BTreeMap<i32, BattlePassTaskConfig>,
    pub(super) battlepass_param: Option<BattlePassParamConfig>,
    pub(super) battlepass_activity_param: Option<BattlePassParamConfig>,
    pub(super) activity: std::collections::BTreeMap<i32, ActivityConfig>,
    pub(super) parameters: std::collections::BTreeMap<i32, ParameterConfig>,
    pub(super) activity_extract: std::collections::BTreeMap<i32, ActivityExtractConfig>,
    pub(super) activity_extract_ur: std::collections::BTreeMap<i32, ActivityExtractConfig>,
    pub(super) anniversary_videos: std::collections::BTreeMap<i32, AnniversaryVideoConfig>,
    pub(super) paper_cut_formulas: std::collections::BTreeMap<i32, PaperCutFormulaConfig>,
    pub(super) drop_items: std::collections::BTreeMap<i32, DropItemConfig>,
    pub(super) exchanges: std::collections::BTreeMap<i32, ExchangeConfig>,
    pub(super) food_recipes: std::collections::BTreeMap<i32, FoodRecipeConfig>,
    pub(super) testship_rewards: std::collections::BTreeMap<i32, TestShipRewardConfig>,
    pub(super) world_events: std::collections::BTreeMap<i32, WorldEventConfig>,
    pub(super) guild_war_rewards: std::collections::BTreeMap<i32, GuildWarRewardConfig>,
    pub(super) magazine_info: std::collections::BTreeMap<i32, MagazineInfoConfig>,
    pub(super) interaction_items: std::collections::BTreeMap<i32, InteractionItemConfig>,
    pub(super) interaction_figures: std::collections::BTreeMap<i32, InteractionFigureConfig>,
    pub(super) guild_box_scores: std::collections::BTreeMap<i32, GuildBoxScoreConfig>,
    pub(super) valentine_gifts: std::collections::BTreeMap<i32, ValentineGiftConfig>,
    pub(super) sportsmeet_awards: std::collections::BTreeMap<i32, SportsMeetAwardConfig>,
}

impl GameplayCatalog {
    pub(super) fn validate(&self) -> Result<(), String> {
        for (reward_id, rewards) in &self.rewards_by_id {
            if *reward_id <= 0
                || rewards
                    .iter()
                    .any(|reward| reward.goods_type <= 0 || reward.item_id <= 0 || reward.num <= 0)
            {
                return Err(format!("gameplay reward config is invalid: {reward_id}"));
            }
        }
        for (drop_id, drop) in &self.drop_items {
            if *drop_id <= 0
                || drop.entries.iter().any(|entry| {
                    entry.goods_type <= 0
                        || entry.item_id <= 0
                        || entry.min <= 0
                        || entry.max < entry.min
                        || entry.rate <= 0
                })
            {
                return Err(format!("gameplay drop config is invalid: {drop_id}"));
            }
        }
        for (activity_id, activity) in &self.activity {
            if *activity_id <= 0 || activity.id <= 0 || activity.activity_type < 0 {
                return Err(format!("activity config is invalid: {activity_id}"));
            }
        }
        for (event_id, event) in &self.world_events {
            if *event_id <= 0
                || event
                    .server_stage_rewards
                    .iter()
                    .any(|(stage, reward)| *stage < 0 || *reward < 0)
            {
                return Err(format!("world event config is invalid: {event_id}"));
            }
        }
        for (recipe_id, recipe) in &self.food_recipes {
            if *recipe_id <= 0
                || recipe.reward_id < 0
                || recipe.material.iter().any(|(goods_type, item_id, amount)| {
                    *goods_type <= 0 || *item_id <= 0 || *amount <= 0
                })
            {
                return Err(format!("food recipe config is invalid: {recipe_id}"));
            }
        }
        Ok(())
    }

    pub(super) fn validate_references(&self) -> Result<(), String> {
        let reward_exists = |reward_id: i32, source: &str| {
            (reward_id <= 0 || self.rewards_by_id.contains_key(&reward_id))
                .then_some(())
                .ok_or_else(|| format!("{source} references missing reward {reward_id}"))
        };
        let drop_exists = |drop_id: i32, source: &str| {
            (drop_id <= 0 || self.drop_items.contains_key(&drop_id))
                .then_some(())
                .ok_or_else(|| format!("{source} references missing drop {drop_id}"))
        };

        for (level, config) in &self.battlepass_levels {
            reward_exists(
                config.free_level_reward,
                &format!("battlepass level {level}"),
            )?;
            reward_exists(
                config.pay_level_reward,
                &format!("battlepass level {level}"),
            )?;
        }
        for (level, config) in &self.battlepass_activity_levels {
            reward_exists(
                config.free_level_reward,
                &format!("activity battlepass level {level}"),
            )?;
            reward_exists(
                config.pay_level_reward,
                &format!("activity battlepass level {level}"),
            )?;
        }
        for (id, config) in &self.anniversary_videos {
            reward_exists(config.reward_id, &format!("anniversary video {id}"))?;
        }
        for (id, config) in &self.food_recipes {
            reward_exists(config.reward_id, &format!("food recipe {id}"))?;
        }
        for (id, config) in &self.testship_rewards {
            reward_exists(config.reward_id, &format!("test ship reward {id}"))?;
        }
        for (id, config) in &self.guild_war_rewards {
            reward_exists(config.reward_id, &format!("guild war reward {id}"))?;
        }
        for (id, config) in &self.guild_box_scores {
            reward_exists(config.reward_id, &format!("guild box score {id}"))?;
        }
        for (id, config) in &self.valentine_gifts {
            reward_exists(config.attach_reward, &format!("valentine gift {id}"))?;
        }
        for (id, config) in &self.sportsmeet_awards {
            reward_exists(config.reward_id, &format!("sports meet award {id}"))?;
        }
        for (id, config) in &self.magazine_info {
            for reward_id in &config.rewards {
                reward_exists(*reward_id, &format!("magazine info {id}"))?;
            }
        }
        for (id, config) in &self.interaction_items {
            reward_exists(config.reward_id, &format!("interaction item {id}"))?;
            drop_exists(config.drop_id, &format!("interaction item {id}"))?;
        }
        for (id, config) in &self.paper_cut_formulas {
            drop_exists(config.drop_id, &format!("paper cut formula {id}"))?;
        }
        for (id, event) in &self.world_events {
            for (_, reward_id) in &event.server_stage_rewards {
                reward_exists(*reward_id, &format!("world event {id}"))?;
            }
        }
        Ok(())
    }
}

pub(super) const MAX_SHOP_BUY_NUM: i32 = 99;

#[derive(Clone, Debug, Default)]
pub(super) struct MailTemplate {
    pub(super) mid: u64,
    pub(super) goods_type: i32,
    pub(super) config_id: i32,
    pub(super) num: i32,
    pub(super) subject: String,
    pub(super) content: String,
}

#[derive(Clone, Debug, Default)]
pub(super) struct HeroLevelCatalog {
    pub(super) exp_per_item: std::collections::BTreeMap<i32, i32>,
    pub(super) exp_needed: std::collections::BTreeMap<i32, i32>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShipIntensifyCatalog {
    pub(super) need_power_by_template: std::collections::BTreeMap<i32, (i32, Vec<(i32, i64)>)>,
    pub(super) provide_power_by_template: std::collections::BTreeMap<i32, Vec<(i32, i64)>>,
    pub(super) max_power_by_template: std::collections::BTreeMap<i32, Vec<(i32, i64)>>,
    pub(super) same_type_ratio: i64,
    pub(super) diamond_cost_per_hero: i64,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShipBreakCatalog {
    pub(super) by_template: std::collections::BTreeMap<i32, ShipBreakConfig>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShipBreakConfig {
    pub(super) min_level: i32,
    pub(super) break_to: i32,
    pub(super) break_item: Option<(Vec<i32>, usize)>,
    pub(super) break_item_optional_count: usize,
    pub(super) break_item_mub: Option<(i32, i32)>,
    pub(super) break_usableitem_mub: Vec<i32>,
    pub(super) currency_cost: Option<(i32, i32, i64)>,
}

impl ShipBreakCatalog {
    pub(super) fn validate(&self) -> Result<(), String> {
        for (template_id, config) in &self.by_template {
            if *template_id <= 0
                || config.min_level < 0
                || config.break_to < 0
                || config
                    .break_item
                    .as_ref()
                    .is_some_and(|(templates, count)| {
                        *count == 0 || templates.iter().any(|template| *template <= 0)
                    })
                || config
                    .break_item_mub
                    .is_some_and(|(item, count)| item <= 0 || count <= 0)
                || config.break_usableitem_mub.iter().any(|item| *item <= 0)
                || config
                    .currency_cost
                    .is_some_and(|(kind, item, cost)| kind != 5 || item <= 0 || cost < 0)
            {
                return Err(format!("ship break config is invalid: {template_id}"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShipAdvanceCatalog {
    /// 下一次 AdvLv -> config_ship_advance row。
    pub(super) by_level: std::collections::BTreeMap<i32, ShipAdvanceConfig>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ShipAdvanceConfig {
    pub(super) initial_level: i32,
    pub(super) max_level: i32,
}

impl ShipAdvanceCatalog {
    pub(super) fn validate(&self) -> Result<(), String> {
        for (level, config) in &self.by_level {
            if *level <= 0 || config.initial_level <= 0 || config.max_level <= config.initial_level
            {
                return Err(format!("ship advance config is invalid: {level}"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShipRemouldCatalog {
    /// sf_id -> config_ship_info row。
    pub(super) ship_info_by_sf_id: std::collections::BTreeMap<i32, ShipInfoRemouldConfig>,
    pub(super) templates: std::collections::BTreeMap<i32, ShipRemouldTemplateConfig>,
    pub(super) effects: std::collections::BTreeMap<i32, ShipRemouldEffectConfig>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShipInfoRemouldConfig {
    pub(super) remould_template: Vec<i32>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShipRemouldTemplateConfig {
    pub(super) remould_item_group: Vec<i32>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShipRemouldEffectConfig {
    pub(super) remould_prev: Vec<i32>,
    pub(super) limit_level: i32,
    pub(super) limit_star: i32,
    pub(super) costs: Vec<(i32, i32, i64)>,
    pub(super) remould_effect_type: Vec<Vec<i32>>,
}

impl ShipRemouldCatalog {
    pub(super) fn validate(&self) -> Result<(), String> {
        for (sf_id, config) in &self.ship_info_by_sf_id {
            if *sf_id <= 0 || config.remould_template.iter().any(|id| *id <= 0) {
                return Err(format!("ship remould info is invalid: {sf_id}"));
            }
        }
        for (template_id, config) in &self.templates {
            if *template_id <= 0 || config.remould_item_group.iter().any(|id| *id <= 0) {
                return Err(format!("ship remould template is invalid: {template_id}"));
            }
        }
        for (effect_id, config) in &self.effects {
            if *effect_id <= 0
                || config.remould_prev.iter().any(|id| *id <= 0)
                || config.limit_level < 0
                || config.limit_star < 0
                || config
                    .costs
                    .iter()
                    .any(|(kind, item, amount)| *kind <= 0 || *item <= 0 || *amount <= 0)
                || config
                    .remould_effect_type
                    .iter()
                    .any(|row| row.is_empty() || row.iter().any(|value| *value < 0))
            {
                return Err(format!("ship remould effect is invalid: {effect_id}"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct CommanderLevelCatalog {
    /// Commander level -> experience required to reach next level.
    pub(super) exp_needed: std::collections::BTreeMap<i32, i32>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct HeroSkillUpgradeCatalog {
    /// PSkill/group id -> cost rows for level 1->2, 2->3, ... .
    pub(super) costs_by_skill: std::collections::BTreeMap<i32, Vec<SkillUpgradeCosts>>,
}

pub(super) type SkillUpgradeCosts = Vec<(i32, i32, i32)>;

#[derive(Clone, Debug, Default)]
pub(super) struct BattleCopy {
    pub(super) config_id: i32,
    pub(super) copy_type: i32,
    pub(super) fleet_ids: Vec<i32>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct BattleEnemy {
    pub(super) hp: i32,
    pub(super) ship_info_id: i32,
    pub(super) attack: i32,
    pub(super) defense: i32,
    pub(super) hit: i32,
    pub(super) dodge: i32,
    pub(super) crit: i32,
    pub(super) anti_crit: i32,
    pub(super) torpedo: i32,
    pub(super) torpedo_defense: i32,
}

#[derive(Clone, Debug, Default)]
pub(super) struct BattleCatalog {
    pub(super) copies: std::collections::HashMap<i32, BattleCopy>,
    pub(super) daily_group_by_copy: std::collections::HashMap<i32, i32>,
    pub(super) search_3d: std::collections::HashSet<i32>,
    pub(super) fleet_enemies: std::collections::HashMap<i32, Vec<i32>>,
    /// config_fleet.is_last_fleet; required to keep non-final fleets on wire.
    pub(super) fleet_is_last: std::collections::HashMap<i32, bool>,
    /// Parent fleet -> config_fleet.copy_attacheds fleet IDs.
    pub(super) attached_fleet_ids: std::collections::HashMap<i32, Vec<i32>>,
    pub(super) enemies: std::collections::HashMap<i32, BattleEnemy>,
    pub(super) random_factors: std::collections::HashMap<i32, Vec<RandomFactorEntry>>,
    pub(super) copy_drop_ids: std::collections::HashMap<i32, Vec<i32>>,
    pub(super) fleet_drop_ids: std::collections::HashMap<i32, Vec<i32>>,
    pub(super) fleet_other_drop_ids: std::collections::HashMap<i32, Vec<i32>>,
    pub(super) fleet_settle_drop_ids: std::collections::HashMap<i32, Vec<i32>>,
    pub(super) copy_first_rewards: std::collections::HashMap<i32, Vec<(i32, i32, i32)>>,
    pub(super) copy_must_drop_rewards: std::collections::HashMap<i32, BattleMustDropReward>,
    pub(super) copy_rank_drop_ids: std::collections::HashMap<i32, i32>,
    pub(super) rank_drop_rewards: std::collections::HashMap<i32, Vec<BattleRewardRank>>,
    pub(super) drop_pools: std::collections::HashMap<i32, Vec<BuildDropEntry>>,
    pub(super) fleet_rewards: std::collections::HashMap<i32, BattleFleetReward>,
    pub(super) evaluation_by_grade: std::collections::HashMap<i32, BattleEvaluationRule>,
    pub(super) task_disabled_copies: std::collections::HashSet<i32>,
    pub(super) settlement_by_copy: std::collections::HashMap<i32, BattleSettlementRule>,
    pub(super) supply_cost_by_copy: std::collections::HashMap<i32, (i64, i64)>,
    pub(super) ship_supply_cost: std::collections::HashMap<i32, i64>,
    pub(super) drop_quantities: BattleDropQuantities,
}

impl BattleCatalog {
    pub(super) fn validate(&self) -> Result<(), String> {
        for (copy_id, copy) in &self.copies {
            if *copy_id <= 0 || copy.config_id <= 0 || copy.fleet_ids.is_empty() {
                return Err(format!("battle copy {copy_id} has invalid identity"));
            }
            if copy.fleet_ids.iter().any(|fleet_id| *fleet_id <= 0) {
                return Err(format!("battle copy {copy_id} has invalid fleet id"));
            }
        }
        if self
            .daily_group_by_copy
            .iter()
            .any(|(copy_id, group_id)| *copy_id <= 0 || *group_id <= 0)
        {
            return Err("battle daily copy reference is invalid".to_owned());
        }
        if self
            .fleet_enemies
            .iter()
            .any(|(fleet_id, enemies)| *fleet_id <= 0 || enemies.iter().any(|enemy| *enemy <= 0))
        {
            return Err("battle fleet enemy reference is invalid".to_owned());
        }
        if self.enemies.keys().any(|enemy_id| *enemy_id <= 0) {
            return Err("battle enemy catalog contains non-positive id".to_owned());
        }
        Ok(())
    }

    pub(super) fn validate_references(&self) -> Result<(), String> {
        for (copy_id, copy) in &self.copies {
            for fleet_id in &copy.fleet_ids {
                if !self.fleet_is_last.contains_key(fleet_id) {
                    return Err(format!(
                        "battle copy {copy_id} references missing fleet {fleet_id}"
                    ));
                }
            }
        }
        for (fleet_id, attached_ids) in &self.attached_fleet_ids {
            if !self.fleet_is_last.contains_key(fleet_id) {
                return Err(format!(
                    "battle attached fleet parent is missing: {fleet_id}"
                ));
            }
            if attached_ids
                .iter()
                .any(|attached_id| !self.fleet_is_last.contains_key(attached_id))
            {
                return Err(format!(
                    "battle fleet {fleet_id} references missing attached fleet"
                ));
            }
        }
        for (copy_id, rank_drop_id) in &self.copy_rank_drop_ids {
            if !self.rank_drop_rewards.contains_key(rank_drop_id) {
                return Err(format!(
                    "battle copy {copy_id} references missing rank drop {rank_drop_id}"
                ));
            }
        }
        Ok(())
    }
}

type BattleRewardTuple = (i32, i32, i32);
type BattleMustDropReward = (i32, Vec<BattleRewardTuple>);
type BattleRewardRank = (i32, i32, i32);

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BattleDropQuantities {
    pub(super) default_rewards: std::collections::HashMap<String, [i32; 2]>,
    #[serde(default)]
    pub(super) copies: std::collections::HashMap<i32, std::collections::HashMap<String, [i32; 2]>>,
}

impl BattleDropQuantities {
    pub(super) fn apply(&self, copy_id: i32, reward: &mut ShopReward, seed: u64) {
        // Only replace the singleton fallback. Explicit source quantities stay intact.
        if reward.num != 1 {
            return;
        }
        let key = format!("{}:{}", reward.goods_type, reward.item_id);
        let range = self
            .copies
            .get(&copy_id)
            .and_then(|rows| rows.get(&key))
            .or_else(|| self.default_rewards.get(&key));
        if let Some(&[min, max]) = range.filter(|range| range[0] > 0 && range[1] >= range[0]) {
            reward.num = min
                + (mix_build_draw_roll(seed) % (i64::from(max) - i64::from(min) + 1) as u64) as i32;
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct BattleFleetReward {
    pub(super) ship_exp: i32,
    pub(super) commander_exp: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct BattleEvaluationRule {
    pub(super) exp_ratio: i32,
    pub(super) settle_drop_ratio: i32,
    pub(super) other_drop_ratio: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct BattleSettlementRule {
    pub(super) affection_add: i32,
    pub(super) affection_flagship_add: i32,
    pub(super) affection_mvp_add: i32,
    pub(super) affection_reduce: i32,
    pub(super) mood_reduce: i32,
    pub(super) mood_shipwrecks_reduce: i32,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShipStat {
    pub(super) fixed_money: i64,
    pub(super) hp: i64,
    pub(super) hp_levelup: i64,
    pub(super) attack: i64,
    pub(super) attack_levelup: i64,
    pub(super) defense: i64,
    pub(super) defense_levelup: i64,
    pub(super) torpedo_attack: i64,
    pub(super) torpedo_attack_levelup: i64,
    pub(super) torpedo_defense: i64,
    pub(super) torpedo_defense_levelup: i64,
    pub(super) ship_bomb_attack: i64,
    pub(super) ship_bomb_attack_levelup: i64,
    pub(super) ship_torpedo_attack: i64,
    pub(super) ship_torpedo_attack_levelup: i64,
    pub(super) carry_plane_count: i64,
    pub(super) hit: i64,
    pub(super) dodge: i64,
    pub(super) crit: i64,
    pub(super) anti_crit: i64,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShipStatCatalog {
    pub(super) by_template: std::collections::BTreeMap<i32, ShipStat>,
}

pub(super) static SHIP_STAT_CATALOG: OnceLock<ShipStatCatalog> = OnceLock::new();

#[derive(Clone, Debug, Default)]
pub(super) struct RandomFactorEntry {
    pub(super) set_id: i32,
    pub(super) group_id: i32,
    pub(super) factors: Vec<i32>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct TaskDefinition {
    pub(super) task_type: i32,
    pub(super) id: i32,
    pub(super) event_type: i32,
    /// Optional condition from goal[1] (for example exact copy id for event 24/900).
    pub(super) event_param: Option<i32>,
    pub(super) goal: i32,
    pub(super) level_min: i32,
    pub(super) level_max: i32,
    pub(super) abandoned: i32,
    pub(super) next_task_id: i32,
    pub(super) previous_task_id: i32,
    pub(super) medal_id: i32,
    pub(super) point: i32,
    pub(super) reward_id: i32,
    pub(super) inline_rewards: Vec<(i32, i32, i32)>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct TaskCatalog {
    pub(super) definitions: Vec<TaskDefinition>,
    pub(super) rewards_by_id: std::collections::BTreeMap<i32, Vec<(i32, i32, i32)>>,
    pub(super) teaching_rewards_by_id: std::collections::BTreeMap<i32, i32>,
}

impl TaskCatalog {
    pub(super) fn validate_references(&self) -> Result<(), String> {
        let task_keys = self
            .definitions
            .iter()
            .map(|definition| (definition.task_type, definition.id))
            .collect::<std::collections::BTreeSet<_>>();
        for definition in &self.definitions {
            if definition.reward_id > 0 && !self.rewards_by_id.contains_key(&definition.reward_id) {
                return Err(format!(
                    "task {}:{} references missing reward {}",
                    definition.task_type, definition.id, definition.reward_id
                ));
            }
            if definition.next_task_id > 0
                && !task_keys.contains(&(definition.task_type, definition.next_task_id))
            {
                return Err(format!(
                    "task {}:{} references missing next task {}",
                    definition.task_type, definition.id, definition.next_task_id
                ));
            }
            if definition.previous_task_id > 0
                && !task_keys.contains(&(definition.task_type, definition.previous_task_id))
            {
                return Err(format!(
                    "task {}:{} references missing previous task {}",
                    definition.task_type, definition.id, definition.previous_task_id
                ));
            }
        }
        for (task_id, reward_id) in &self.teaching_rewards_by_id {
            if *task_id <= 0 || *reward_id <= 0 {
                return Err(format!(
                    "teaching task reward reference is invalid: {task_id}"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct AffectionCatalog {
    pub(super) exp_by_item: std::collections::BTreeMap<i32, i32>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct BuildShipCatalog {
    pub(super) pools: std::collections::HashMap<i32, Vec<BuildDropEntry>>,
    pub(super) extract_to_drop: std::collections::HashMap<i32, i32>,
    pub(super) extract_type_by_pool: std::collections::HashMap<i32, i32>,
    pub(super) box_drop_by_pool_count: std::collections::HashMap<(i32, i32), i32>,
    pub(super) reward_by_pool_count: std::collections::HashMap<(i32, i32), (i32, i32, i32)>,
    pub(super) expend_by_pool: std::collections::HashMap<i32, Vec<(i32, i32, i32)>>,
    pub(super) ten_expend_by_pool: std::collections::HashMap<i32, Vec<(i32, i32, i32)>>,
    pub(super) ship_defaults: std::collections::HashMap<i32, Vec<i32>>,
    pub(super) ship_build_time: std::collections::HashMap<i32, i32>,
    pub(super) ship_quality: std::collections::HashMap<i32, i32>,
    pub(super) treasure_drop_by_item: std::collections::HashMap<i32, i32>,
    pub(super) selected_treasure_by_item: std::collections::HashMap<i32, SelectedTreasure>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct SelectedTreasure {
    pub(super) drop_id: i32,
    pub(super) options: Vec<(i32, i32, i32)>,
}

pub(super) type BuildDropEntry = (i32, i32, i32, i32, i32);
pub(super) type BuildFormulaRow = (Vec<i64>, Vec<i64>, Vec<i64>, Vec<i32>);

impl BuildShipCatalog {
    pub(super) fn validate(&self) -> Result<(), String> {
        for (&pool_id, entries) in &self.pools {
            if pool_id <= 0 || entries.is_empty() {
                return Err(format!("build drop pool {pool_id} is invalid"));
            }
            for &(goods_type, item_id, amount, min_count, weight) in entries {
                if goods_type <= 0 || item_id <= 0 || amount < 0 || min_count < 0 || weight < 0 {
                    return Err(format!(
                        "build drop entry in pool {pool_id} is invalid: {goods_type}:{item_id}:{amount}:{min_count}:{weight}"
                    ));
                }
                if goods_type == 4 && !self.pools.contains_key(&item_id) {
                    return Err(format!(
                        "build drop pool {pool_id} references missing nested pool {item_id}"
                    ));
                }
            }
        }

        fn visit(
            catalog: &BuildShipCatalog,
            pool_id: i32,
            visiting: &mut std::collections::HashSet<i32>,
            visited: &mut std::collections::HashSet<i32>,
        ) -> Result<(), String> {
            if visited.contains(&pool_id) {
                return Ok(());
            }
            if !visiting.insert(pool_id) {
                return Err(format!("build drop pool cycle includes {pool_id}"));
            }
            if let Some(entries) = catalog.pools.get(&pool_id) {
                for &(goods_type, item_id, ..) in entries {
                    if goods_type == 4 {
                        visit(catalog, item_id, visiting, visited)?;
                    }
                }
            }
            visiting.remove(&pool_id);
            visited.insert(pool_id);
            Ok(())
        }

        let mut visiting = std::collections::HashSet::new();
        let mut visited = std::collections::HashSet::new();
        for &pool_id in self.pools.keys() {
            visit(self, pool_id, &mut visiting, &mut visited)?;
        }

        for (&pool_id, &drop_id) in &self.extract_to_drop {
            if pool_id <= 0 || drop_id <= 0 || !self.pools.contains_key(&drop_id) {
                return Err(format!(
                    "build extract pool {pool_id} references missing drop {drop_id}"
                ));
            }
        }
        for (&(pool_id, count), &drop_id) in &self.box_drop_by_pool_count {
            if pool_id <= 0 || count <= 0 || drop_id <= 0 || !self.pools.contains_key(&drop_id) {
                return Err(format!(
                    "build box reward ({pool_id}, {count}) references missing drop {drop_id}"
                ));
            }
        }
        for (&(pool_id, count), &(goods_type, item_id, amount)) in &self.reward_by_pool_count {
            if pool_id <= 0 || count <= 0 || goods_type <= 0 || item_id <= 0 || amount <= 0 {
                return Err(format!(
                    "build milestone reward ({pool_id}, {count}) is invalid"
                ));
            }
        }
        for (&pool_id, extract_type) in &self.extract_type_by_pool {
            if pool_id <= 0 || *extract_type <= 0 {
                return Err(format!("build extract type for pool {pool_id} is invalid"));
            }
        }
        for (&pool_id, costs) in self.expend_by_pool.iter().chain(&self.ten_expend_by_pool) {
            if pool_id <= 0 || costs.is_empty() {
                return Err(format!("build cost for pool {pool_id} is invalid"));
            }
            if costs.iter().any(|(goods_type, item_id, amount)| {
                *goods_type <= 0 || *item_id <= 0 || *amount <= 0
            }) {
                return Err(format!("build cost for pool {pool_id} is invalid"));
            }
        }
        for (&template_id, defaults) in &self.ship_defaults {
            if template_id <= 0 || defaults.is_empty() || defaults.iter().any(|id| *id <= 0) {
                return Err(format!("build ship defaults for {template_id} are invalid"));
            }
        }
        for (&template_id, &value) in self.ship_build_time.iter().chain(&self.ship_quality) {
            if template_id <= 0 || value <= 0 {
                return Err(format!(
                    "build ship catalog value for {template_id} is invalid"
                ));
            }
        }
        for (&item_id, &drop_id) in &self.treasure_drop_by_item {
            if item_id <= 0 || drop_id <= 0 {
                return Err(format!(
                    "treasure {item_id} references missing drop {drop_id}"
                ));
            }
        }
        for (&item_id, selected) in &self.selected_treasure_by_item {
            if item_id <= 0
                || (selected.drop_id <= 0 && selected.options.is_empty())
                || selected
                    .options
                    .iter()
                    .any(|(goods_type, config_id, amount)| {
                        *goods_type <= 0 || *config_id <= 0 || *amount <= 0
                    })
            {
                return Err(format!("selected treasure {item_id} is invalid"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct BuildFormulaCatalogRuntime(pub(super) Vec<BuildFormulaRow>);

pub(super) static BUILD_SHIP_CATALOG: OnceLock<BuildShipCatalog> = OnceLock::new();
pub(super) static BUILD_FORMULA_CATALOG: OnceLock<BuildFormulaCatalogRuntime> = OnceLock::new();
pub(super) static HERO_SKILL_UPGRADE_CATALOG: OnceLock<HeroSkillUpgradeCatalog> = OnceLock::new();
pub(super) static SHIP_INTENSIFY_CATALOG: OnceLock<ShipIntensifyCatalog> = OnceLock::new();
pub(super) static SHIP_BREAK_CATALOG: OnceLock<ShipBreakCatalog> = OnceLock::new();
pub(super) static SHIP_ADVANCE_CATALOG: OnceLock<ShipAdvanceCatalog> = OnceLock::new();
pub(super) static SHIP_REMOULD_CATALOG: OnceLock<ShipRemouldCatalog> = OnceLock::new();
pub(super) static RECHARGE_CATALOG: OnceLock<RechargeCatalog> = OnceLock::new();
pub(super) static GAMEPLAY_CATALOG: OnceLock<GameplayCatalog> = OnceLock::new();
pub(super) static COMMANDER_LEVEL_CATALOG: OnceLock<CommanderLevelCatalog> = OnceLock::new();
pub(super) static BUILD_DRAW_SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub(super) static BATTLE_DRAW_SEQUENCE: AtomicU64 = AtomicU64::new(0);

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, OnceLock};

use blueoath_protocol::FashionList;

use super::{json_i32, json_i32_array, mix_build_draw_roll, ShopReward};

#[cfg(test)]
mod validation_tests {
    use super::{
        BattleCatalog, BattleCopy, BuildShipCatalog, ChapterCatalog, ChapterStarRewards,
        GameplayCatalog, TaskCatalog, TaskDefinition,
    };

    #[test]
    fn fallback_catalog_passes_startup_validation() {
        assert!(ChapterCatalog::fallback().validate().is_ok());
        assert!(BattleCatalog::default().validate().is_ok());
    }

    #[test]
    fn malformed_catalog_is_rejected_before_runtime() {
        let mut catalog = ChapterCatalog::fallback();
        catalog.daily_chapters.push((8, 0));
        assert!(catalog.validate().is_err());
    }

    #[test]
    fn cross_catalog_reward_reference_is_required() {
        let mut chapters = ChapterCatalog::fallback();
        chapters.star_rewards_by_chapter.insert(
            1,
            ChapterStarRewards {
                level_ids: vec![1],
                star_conditions: vec![1],
                reward_ids: vec![99],
            },
        );
        assert!(chapters
            .validate_references(&GameplayCatalog::default())
            .is_err());
    }

    #[test]
    fn battle_cross_catalog_fleet_reference_is_required() {
        let mut battle = BattleCatalog::default();
        battle.copies.insert(
            1,
            BattleCopy {
                config_id: 1,
                copy_type: 1,
                fleet_ids: vec![7],
            },
        );
        assert!(battle.validate_references().is_err());
    }

    #[test]
    fn task_chain_reference_is_required() {
        let mut tasks = TaskCatalog::default();
        tasks.definitions.push(TaskDefinition {
            task_type: 1,
            id: 1,
            next_task_id: 2,
            ..TaskDefinition::default()
        });
        assert!(tasks.validate_references().is_err());
    }

    #[test]
    fn build_drop_cycle_is_rejected() {
        let mut catalog = BuildShipCatalog::default();
        catalog.pools.insert(10, vec![(4, 20, 1, 0, 1)]);
        catalog.pools.insert(20, vec![(4, 10, 1, 0, 1)]);
        assert!(catalog.validate().is_err());
    }

    #[test]
    fn build_extract_reference_is_required() {
        let mut catalog = BuildShipCatalog::default();
        catalog.extract_to_drop.insert(10, 20);
        assert!(catalog.validate().is_err());
    }
}
