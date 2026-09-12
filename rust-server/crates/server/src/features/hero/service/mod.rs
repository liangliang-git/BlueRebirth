//! Typed hero service routing and shared domain helpers.

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;

mod equipment;
mod experience;
mod fashion;
mod growth;
mod query;
mod response;

pub(crate) struct HeroTypedCatalogs<'a> {
    pub(crate) hero_level: Option<&'a HeroLevelCatalog>,
    pub(crate) tasks: Option<&'a TaskCatalog>,
    pub(crate) breakdown: Option<&'a HeroBreakdownCatalog>,
    pub(crate) fashion: Option<&'a FashionList>,
    pub(crate) ship_exp_multiplier: f64,
    pub(crate) hero_skill_upgrade: Option<&'a HeroSkillUpgradeCatalog>,
    pub(crate) ship_intensify: Option<&'a ShipIntensifyCatalog>,
    pub(crate) ship_break: Option<&'a ShipBreakCatalog>,
    pub(crate) ship_advance: Option<&'a ShipAdvanceCatalog>,
    pub(crate) ship_remould: Option<&'a ShipRemouldCatalog>,
    pub(crate) state: Option<&'a ServerState>,
}

#[cfg(test)]
impl HeroTypedCatalogs<'static> {
    /// 创建不依赖运行时目录数据的空目录集合，供测试和轻量调用使用。
    pub(crate) const fn empty() -> Self {
        Self {
            hero_level: None,
            tasks: None,
            breakdown: None,
            fashion: None,
            ship_exp_multiplier: 1.0,
            hero_skill_upgrade: None,
            ship_intensify: None,
            ship_break: None,
            ship_advance: None,
            ship_remould: None,
            state: None,
        }
    }
}

/// 检查英雄是否仍被秘书、舰队、建筑或战斗等系统引用。
fn hero_is_in_use(
    account: &blueoath_domain::AccountState,
    hero_id: blueoath_domain::HeroId,
) -> bool {
    account.character.secretary_id == Some(hero_id)
        || account
            .fleet
            .fleets
            .values()
            .any(|fleet| fleet.members.contains(&hero_id))
        || account.fleet.presets.iter().any(|fleet| {
            fleet
                .hero_ids
                .iter()
                .chain(&fleet.ex_hero_ids)
                .any(|id| *id == hero_id)
        })
        || account
            .buildings
            .hero_assignments
            .values()
            .any(|ids| ids.contains(&hero_id))
        || account
            .bathroom
            .heroes
            .iter()
            .any(|hero| hero.hero_id == hero_id.get())
        || account
            .support
            .entries
            .iter()
            .any(|entry| entry.hero_ids.contains(&hero_id))
        || account.supply.hero_ids.contains(&hero_id)
        || account.activity_tower.hero_ids.contains(&hero_id)
        || account.tower.hero_ids.contains(&hero_id)
        || account
            .battle
            .active
            .as_ref()
            .is_some_and(|battle| battle.hero_ids.contains(&hero_id))
}

/// 读取英雄领域共享的兼容进度字段。
fn hero_progress(account: &blueoath_domain::AccountState, hero_id: u64, field: &str) -> u64 {
    account
        .activities
        .progress
        .get(&format!("compat:hero:{hero_id}:{field}"))
        .copied()
        .unwrap_or_default()
}

/// 写入英雄领域共享的兼容进度字段。
fn set_hero_progress(
    account: &mut blueoath_domain::AccountState,
    hero_id: u64,
    field: impl Into<String>,
    value: u64,
) {
    account
        .activities
        .progress
        .insert(format!("compat:hero:{hero_id}:{}", field.into()), value);
}

/// 将协议中的货币物品编号转换为领域货币类型。
fn typed_currency_kind(item_id: i32) -> Option<blueoath_domain::CurrencyKind> {
    Some(match item_id {
        1 => blueoath_domain::CurrencyKind::Gold,
        2 => blueoath_domain::CurrencyKind::Diamond,
        5 => blueoath_domain::CurrencyKind::Supply,
        30 => blueoath_domain::CurrencyKind::PvePoint,
        _ => return None,
    })
}

/// 查询账户持有的指定模板道具数量。
fn typed_item_count(account: &blueoath_domain::AccountState, item_id: i32) -> u64 {
    blueoath_domain::TemplateId::new(item_id.max(0) as u64)
        .ok()
        .and_then(|id| account.inventory.items.get(&id).copied())
        .unwrap_or_default()
}

/// 原子扣除账户中的指定模板道具，数量不足时保持不变。
fn consume_typed_item(
    account: &mut blueoath_domain::AccountState,
    item_id: i32,
    amount: u64,
) -> bool {
    let Ok(template_id) = blueoath_domain::TemplateId::new(item_id.max(0) as u64) else {
        return false;
    };
    let Some(current) = account.inventory.items.get_mut(&template_id) else {
        return false;
    };
    if *current < amount {
        return false;
    }
    *current -= amount;
    if *current == 0 {
        account.inventory.items.remove(&template_id);
    }
    true
}

/// 校验一组成长或装备消耗是否可以完整支付。
fn costs_available(account: &blueoath_domain::AccountState, costs: &[(i32, i32, i64)]) -> bool {
    let mut totals = std::collections::BTreeMap::<(i32, i32), u64>::new();
    for (goods_type, item_id, amount) in costs {
        let Ok(amount) = u64::try_from(*amount) else {
            return false;
        };
        let Some(total) = totals
            .entry((*goods_type, *item_id))
            .or_default()
            .checked_add(amount)
        else {
            return false;
        };
        *totals.get_mut(&(*goods_type, *item_id)).unwrap() = total;
    }
    totals.into_iter().all(|((goods_type, item_id), amount)| {
        if goods_type == 5 {
            typed_currency_kind(item_id)
                .is_some_and(|kind| account.resources.amount(kind).get() >= amount)
        } else if matches!(goods_type, 1 | 6) {
            typed_item_count(account, item_id) >= amount
        } else {
            false
        }
    })
}

/// 扣除一组成长或装备消耗，并在中途失败时回滚账户。
fn consume_costs(account: &mut blueoath_domain::AccountState, costs: &[(i32, i32, i64)]) -> bool {
    let snapshot = account.clone();
    if !costs_available(account, costs) {
        return false;
    }
    for (goods_type, item_id, amount) in costs {
        let Ok(amount) = u64::try_from(*amount) else {
            return false;
        };
        if *goods_type == 5 {
            let Some(kind) = typed_currency_kind(*item_id) else {
                *account = snapshot;
                return false;
            };
            if account.resources.debit(kind, amount).is_err() {
                *account = snapshot;
                return false;
            }
        } else if !consume_typed_item(account, *item_id, amount) {
            *account = snapshot;
            return false;
        }
    }
    true
}

/// 按领域边界分发强类型英雄请求，并协调共享目录与响应副作用。
pub(crate) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
    catalogs: HeroTypedCatalogs<'_>,
) -> HandlerResult {
    let HeroTypedCatalogs {
        hero_level: hero_level_catalog,
        tasks: task_catalog,
        breakdown: hero_breakdown_catalog,
        fashion: fashion_catalog,
        ship_exp_multiplier,
        hero_skill_upgrade: hero_skill_upgrade_catalog,
        ship_intensify: ship_intensify_catalog,
        ship_break: ship_break_catalog,
        ship_advance: ship_advance_catalog,
        ship_remould: ship_remould_catalog,
        state,
    } = catalogs;
    if matches!(
        method,
        "hero.GetHeroInfo" | "hero.GetHeroInfoByHeroIdArray" | "hero.LockHero" | "hero.ChangeName"
    ) {
        return query::handle(account, method, request_args, effects);
    }
    if matches!(
        method,
        "hero.AutoEquip" | "hero.AutoUnEquip" | "hero.ChangeEquip"
    ) {
        return equipment::handle(account, method, request_args, effects);
    }
    match method {
        "fashion.Equip" => {
            fashion::handle_fashion_equip(account, request_args, fashion_catalog, state, effects)
        }
        "hero.RetireHero" => {
            let Some(hero_breakdown_catalog) = hero_breakdown_catalog else {
                return HandlerResult::Empty;
            };
            let Ok(request) = HeroRetireRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "hero retire request is invalid",
                ));
            };
            let raw_ids = request.hero_ids;
            let is_dis_equip = request.dismantle_equipment;
            let mut requested = std::collections::BTreeSet::new();
            for raw_id in raw_ids {
                let Some(hero_id) =
                    blueoath_domain::HeroId::new(u64::try_from(raw_id).unwrap_or_default()).ok()
                else {
                    return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
                };
                if !requested.insert(hero_id) {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "hero retire request has duplicate hero",
                    ));
                }
            }
            if requested.iter().any(|hero_id| {
                account.character.secretary_id == Some(*hero_id)
                    || account
                        .fleet
                        .fleets
                        .values()
                        .any(|fleet| fleet.members.contains(hero_id))
                    || account
                        .buildings
                        .hero_assignments
                        .values()
                        .any(|hero_ids| hero_ids.contains(hero_id))
            }) {
                return HandlerResult::Error(GameError::InvalidRequest("hero is in use"));
            }
            let retired = requested
                .iter()
                .filter_map(|hero_id| account.dock.heroes.get(hero_id).cloned())
                .collect::<Vec<_>>();
            if retired.is_empty() {
                return HandlerResult::Error(GameError::InvalidRequest("hero was not found"));
            }
            let retired_ids = retired
                .iter()
                .map(|hero| hero.id)
                .collect::<std::collections::BTreeSet<_>>();
            let retired_templates = retired
                .iter()
                .filter_map(|hero| i32::try_from(hero.template_id.get()).ok())
                .collect::<Vec<_>>();
            account
                .dock
                .heroes
                .retain(|hero_id, _| !retired_ids.contains(hero_id));
            if is_dis_equip {
                account.dock.equipments.retain(|_, equipment| {
                    !equipment
                        .hero_id
                        .is_some_and(|hero_id| retired_ids.contains(&hero_id))
                });
            } else {
                for equipment in account.dock.equipments.values_mut() {
                    if equipment
                        .hero_id
                        .is_some_and(|hero_id| retired_ids.contains(&hero_id))
                    {
                        equipment.hero_id = None;
                    }
                }
            }
            for fleet in account.fleet.fleets.values_mut() {
                fleet
                    .members
                    .retain(|hero_id| !retired_ids.contains(hero_id));
            }
            for hero_ids in account.buildings.hero_assignments.values_mut() {
                hero_ids.retain(|hero_id| !retired_ids.contains(hero_id));
            }
            account
                .buildings
                .hero_assignments
                .retain(|_, hero_ids| !hero_ids.is_empty());
            if account
                .character
                .secretary_id
                .is_some_and(|hero_id| retired_ids.contains(&hero_id))
            {
                account.character.secretary_id = account.dock.heroes.keys().next().copied();
            }
            let mut rewards = Vec::new();
            for template_id in retired_templates {
                for &(goods_type, item_id, amount) in hero_breakdown_catalog
                    .rewards_by_template
                    .get(&template_id)
                    .into_iter()
                    .flatten()
                {
                    if amount <= 0 {
                        continue;
                    }
                    if goods_type == 5 {
                        let kind = match item_id {
                            1 => Some(blueoath_domain::CurrencyKind::Gold),
                            2 => Some(blueoath_domain::CurrencyKind::Diamond),
                            5 => Some(blueoath_domain::CurrencyKind::Supply),
                            30 => Some(blueoath_domain::CurrencyKind::PvePoint),
                            _ => None,
                        };
                        if let Some(kind) = kind {
                            let _ = account
                                .resources
                                .credit(kind, u64::try_from(amount).unwrap_or_default());
                        }
                    } else if let Ok(template_id) =
                        blueoath_domain::TemplateId::new(u64::try_from(item_id).unwrap_or_default())
                    {
                        let entry = account.inventory.items.entry(template_id).or_default();
                        *entry = entry.saturating_add(u64::try_from(amount).unwrap_or_default());
                    }
                    rewards.push(ShopReward {
                        goods_type,
                        item_id,
                        num: amount,
                        instance_id: 0,
                    });
                }
            }
            advance_typed_task_event(account, task_catalog, 11, 1);
            let deleted = retired_ids
                .iter()
                .map(|hero_id| HeroGrid {
                    hero_id: u32::try_from(hero_id.get()).unwrap_or(u32::MAX),
                    ..HeroGrid::default()
                })
                .collect::<Vec<_>>();
            effects.push_pre(Response::raw(
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&HeroBag {
                    heroes: deleted,
                    bag_size: i32::try_from(account.ship_dock_capacity()).unwrap_or(i32::MAX),
                }),
            ));
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "equip.UpdateEquipBagData",
                EquipListCodec::encode(&equip_list_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "task.TaskInfo",
                task_info_payload_from_typed_account(account, task_catalog),
            ));
            HandlerResult::Reply(Response::raw(method, encode_retire_hero_response(&rewards)))
        }
        "hero.AddExp" => {
            let Some(hero_level_catalog) = hero_level_catalog else {
                return HandlerResult::Empty;
            };
            let Ok(request) = HeroAddExpRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "hero experience request is invalid",
                ));
            };
            let Some(hero_id) = blueoath_domain::HeroId::new(request.hero_id).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
            };
            let items = request
                .items
                .iter()
                .map(|item| (item.template_id, item.amount))
                .collect::<Vec<_>>();
            if !account.dock.heroes.contains_key(&hero_id) {
                return HandlerResult::Error(GameError::InvalidRequest("hero was not found"));
            }
            let plan = items
                .iter()
                .filter_map(|(item_id, requested)| {
                    let item_id =
                        blueoath_domain::TemplateId::new(u64::try_from(*item_id).ok()?).ok()?;
                    let raw_item_id = i32::try_from(item_id.get()).ok()?;
                    let per_item = *hero_level_catalog.exp_per_item.get(&raw_item_id)?;
                    let requested = u64::try_from((*requested).clamp(0, 1_000_000)).ok()?;
                    let available = account
                        .inventory
                        .items
                        .get(&item_id)
                        .copied()
                        .unwrap_or_default();
                    let amount = requested.min(available);
                    (per_item > 0 && amount > 0).then_some((item_id, amount, i64::from(per_item)))
                })
                .collect::<Vec<_>>();
            if plan.is_empty() {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "experience items were not found",
                ));
            }
            let total_exp = plan.iter().fold(0i64, |total, (_, amount, per_item)| {
                total.saturating_add(
                    i64::try_from(*amount)
                        .unwrap_or(i64::MAX)
                        .saturating_mul(*per_item),
                )
            });
            for (item_id, amount, _) in &plan {
                if let Some(available) = account.inventory.items.get_mut(item_id) {
                    *available = available.saturating_sub(*amount);
                }
            }
            let boosted = scale_reward(total_exp, ship_exp_multiplier);
            let (level, exp) = account
                .dock
                .heroes
                .get(&hero_id)
                .map(|hero| (hero.level.max(1), hero.exp))
                .unwrap_or((1, 0));
            let mut level = level;
            let mut exp = exp.min(u64::from(i32::MAX as u32));
            let mut remaining = u64::try_from(boosted).unwrap_or_default();
            let max_level = u32::try_from(hero_level_catalog.max_level()).unwrap_or(100);
            while level < max_level {
                let need = hero_level_catalog
                    .exp_needed
                    .get(&i32::try_from(level).unwrap_or(i32::MAX))
                    .copied()
                    .unwrap_or(500)
                    .max(1) as u64;
                if exp.saturating_add(remaining) < need {
                    exp = exp.saturating_add(remaining);
                    remaining = 0;
                    break;
                }
                remaining = exp.saturating_add(remaining).saturating_sub(need);
                exp = 0;
                level = level.saturating_add(1);
            }
            if level >= max_level {
                level = max_level;
                exp = 0;
            } else {
                exp = exp
                    .saturating_add(remaining)
                    .min(u64::from(i32::MAX as u32));
            }
            let old_level = account
                .dock
                .heroes
                .get(&hero_id)
                .map(|hero| hero.level)
                .unwrap_or(level);
            if let Some(hero) = account.dock.heroes.get_mut(&hero_id) {
                hero.level = level;
                hero.exp = exp;
            }
            experience::refresh_typed_hero_hp_after_level_up(account, hero_id, old_level, level);
            advance_typed_task_event(account, task_catalog, 10, 1);
            effects.push_pre(Response::raw(
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "task.TaskInfo",
                task_info_payload_from_typed_account(account, task_catalog),
            ));
            HandlerResult::Reply(Response::raw(
                method,
                encode_hero_add_exp_response(hero_id.get(), &items),
            ))
        }
        "hero.StudySkill" => growth::handle_skill_upgrade(
            account,
            method,
            request_args,
            hero_skill_upgrade_catalog,
            state,
            effects,
        ),
        "hero.HeroIntensify" => growth::handle_intensify(
            account,
            method,
            request_args,
            ship_intensify_catalog,
            state,
            effects,
        ),
        "hero.HeroAdvance" => growth::handle_advance(
            account,
            method,
            request_args,
            ship_break_catalog,
            effects,
            state,
        ),
        "hero.HeroAdvMaxLv" => growth::handle_advance_max_level(
            account,
            method,
            request_args,
            ship_advance_catalog,
            effects,
        ),
        "hero.HeroAdvanceMUB" => growth::handle_advance_mub(
            account,
            method,
            request_args,
            ship_break_catalog,
            state,
            effects,
        ),
        "hero.HeroRemould" => growth::handle_remould(
            account,
            method,
            request_args,
            ship_remould_catalog,
            state,
            effects,
        ),
        _ => HandlerResult::Empty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_hero_info_reads_normalized_hero_state() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("hero-typed").unwrap(),
            "Captain",
        );
        let hero_id = blueoath_domain::HeroId::new(7).unwrap();
        account.dock.heroes.insert(
            hero_id,
            blueoath_domain::HeroState {
                id: hero_id,
                template_id: blueoath_domain::TemplateId::new(70).unwrap(),
                fashioning: 6,
                name: String::new(),
                change_name_time: 0,
                level: 8,
                exp: 9,
                mood: 10,
                affection: 11,
                hp: 12,
                locked: true,
                created_utc: String::new(),
                equip_slots: Vec::new(),
                pskills: std::collections::BTreeMap::new(),
            },
        );
        let result = handle_typed(
            &mut account,
            "hero.GetHeroInfo",
            &[],
            &mut ResponseEffects::default(),
            HeroTypedCatalogs::empty(),
        );
        let HandlerResult::Reply(response) = result else {
            panic!("typed hero info must reply");
        };
        let hero_payload = response.payload.into_bytes();
        assert!(hero_payload.len() > 2);
        assert_eq!(decode_varint_u64_field(&hero_payload, 2), 200);
    }

    #[test]
    fn typed_hero_lock_mutation_updates_normalized_state() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("hero-lock-typed").unwrap(),
            "Captain",
        );
        let hero_id = blueoath_domain::HeroId::new(1).unwrap();
        assert!(account.dock.heroes.get(&hero_id).unwrap().locked);
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_varint_field(&mut args, 2, 0);
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "hero.LockHero",
            &args,
            &mut effects,
            HeroTypedCatalogs::empty(),
        );

        assert!(matches!(result, HandlerResult::PushOnly));
        assert!(!account.dock.heroes.get(&hero_id).unwrap().locked);
        assert_eq!(effects.into_parts().0.len(), 1);
    }

    #[test]
    fn typed_hero_name_mutation_updates_normalized_state() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("hero-name-typed").unwrap(),
            "Captain",
        );
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_bytes_field(&mut args, 2, b"Aegis");
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "hero.ChangeName",
            &args,
            &mut effects,
            HeroTypedCatalogs::empty(),
        );

        assert!(matches!(result, HandlerResult::PushOnly));
        assert_eq!(account.dock.heroes.values().next().unwrap().name, "Aegis");
        assert_eq!(effects.into_parts().0.len(), 1);
    }

    #[test]
    fn typed_hero_add_exp_consumes_inventory_and_updates_level() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("hero-exp-typed").unwrap(),
            "Captain",
        );
        let item_id = blueoath_domain::TemplateId::new(10_182).unwrap();
        let before = account
            .inventory
            .items
            .get(&item_id)
            .copied()
            .unwrap_or_default();
        let mut catalog = HeroLevelCatalog::default();
        catalog.exp_per_item.insert(10_182, 600);
        catalog.exp_needed.insert(1, 500);
        catalog.max_level = 2;
        let mut item = Vec::new();
        append_varint_field(&mut item, 2, 10_182);
        append_varint_field(&mut item, 3, 1);
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_bytes_field(&mut args, 2, &item);
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "hero.AddExp",
            &args,
            &mut effects,
            HeroTypedCatalogs {
                hero_level: Some(&catalog),
                tasks: None,
                breakdown: None,
                fashion: None,
                ship_exp_multiplier: 1.0,
                hero_skill_upgrade: None,
                ship_intensify: None,
                ship_break: None,
                ship_advance: None,
                ship_remould: None,
                state: None,
            },
        );

        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(account.inventory.items.get(&item_id), Some(&(before - 1)));
        assert_eq!(account.dock.heroes.values().next().unwrap().level, 2);
        assert_eq!(account.dock.heroes.values().next().unwrap().exp, 0);
        assert_eq!(effects.into_parts().0.len(), 3);
    }

    #[test]
    fn hp_after_level_up_refreshes_full_or_overcapped_hp_only() {
        assert_eq!(experience::hp_after_level_up(4_435, 4_435, 4_561), 4_561);
        assert_eq!(experience::hp_after_level_up(4_466, 4_435, 4_561), 4_561);
        assert_eq!(experience::hp_after_level_up(2_000, 4_435, 4_561), 2_000);
        assert_eq!(experience::hp_after_level_up(5_000, 4_435, 4_561), 4_561);
    }

    #[test]
    fn typed_hero_retire_removes_owned_hero_and_grants_breakdown_rewards() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("hero-retire-typed").unwrap(),
            "Captain",
        );
        account.character.secretary_id = None;
        let item_id = blueoath_domain::TemplateId::new(10_182).unwrap();
        let before_items = account
            .inventory
            .items
            .get(&item_id)
            .copied()
            .unwrap_or_default();
        let before_gold = account
            .resources
            .amount(blueoath_domain::CurrencyKind::Gold)
            .get();
        let mut catalog = HeroBreakdownCatalog::default();
        catalog
            .rewards_by_template
            .insert(10_210_511, vec![(1, 10_182, 2), (5, 1, 3)]);
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_varint_field(&mut args, 2, 1);
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "hero.RetireHero",
            &args,
            &mut effects,
            HeroTypedCatalogs {
                hero_level: None,
                tasks: None,
                breakdown: Some(&catalog),
                fashion: None,
                ship_exp_multiplier: 1.0,
                hero_skill_upgrade: None,
                ship_intensify: None,
                ship_break: None,
                ship_advance: None,
                ship_remould: None,
                state: None,
            },
        );

        assert!(matches!(result, HandlerResult::Reply(_)));
        assert!(!account
            .dock
            .heroes
            .contains_key(&blueoath_domain::HeroId::new(1).unwrap()));
        assert!(account.dock.equipments.is_empty());
        assert_eq!(
            account.inventory.items.get(&item_id),
            Some(&(before_items + 2))
        );
        assert_eq!(
            account
                .resources
                .amount(blueoath_domain::CurrencyKind::Gold)
                .get(),
            before_gold + 3
        );
        assert_eq!(effects.into_parts().0.len(), 4);
    }

    #[test]
    fn typed_study_skill_consumes_cost_and_levels_skill() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("skill-upgrade-typed").unwrap(),
            "Captain",
        );
        let hero_id = blueoath_domain::HeroId::new(1).unwrap();
        let item_id = blueoath_domain::TemplateId::new(9001).unwrap();
        account.inventory.items.insert(item_id, 1);
        let mut catalog = HeroSkillUpgradeCatalog::default();
        catalog.costs_by_skill.insert(41, vec![vec![(1, 9001, 1)]]);
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, hero_id.get());
        append_varint_field(&mut args, 2, 41);
        let result = handle_typed(
            &mut account,
            "hero.StudySkill",
            &args,
            &mut ResponseEffects::default(),
            HeroTypedCatalogs {
                hero_level: None,
                tasks: None,
                breakdown: None,
                fashion: None,
                ship_exp_multiplier: 1.0,
                hero_skill_upgrade: Some(&catalog),
                ship_intensify: None,
                ship_break: None,
                ship_advance: None,
                ship_remould: None,
                state: None,
            },
        );
        assert!(matches!(result, HandlerResult::PushOnly));
        assert_eq!(account.dock.heroes[&hero_id].pskills.get(&41), Some(&2));
        assert_eq!(account.inventory.items.get(&item_id), None);
    }

    #[test]
    fn typed_fashion_equip_accepts_owned_fashion() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("fashion-equip-typed").unwrap(),
            "Captain",
        );
        let hero_id = blueoath_domain::HeroId::new(1).unwrap();
        let sf_id = account.dock.heroes[&hero_id]
            .template_id
            .get()
            .saturating_sub(1)
            / 10;
        let fashion_tid = sf_id + 1;
        account
            .fashion
            .entries
            .entry(sf_id)
            .or_default()
            .insert(blueoath_domain::TemplateId::new(fashion_tid).unwrap());
        let catalog = FashionList {
            items: vec![FashionInfo {
                sf_id: i32::try_from(sf_id).unwrap(),
                fashion_tids: vec![i32::try_from(fashion_tid).unwrap()],
            }],
        };
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, fashion_tid);
        append_varint_field(&mut args, 2, 1);
        append_varint_field(&mut args, 3, hero_id.get());
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "fashion.Equip",
            &args,
            &mut effects,
            HeroTypedCatalogs {
                fashion: Some(&catalog),
                ..HeroTypedCatalogs::empty()
            },
        );

        assert!(matches!(result, HandlerResult::PushOnly));
        assert_eq!(account.dock.heroes[&hero_id].fashioning, fashion_tid as u32);
        assert_eq!(effects.into_parts().0.len(), 2);
    }

    #[test]
    fn typed_fashion_equip_rejects_unowned_catalog_fashion() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("fashion-unowned-typed").unwrap(),
            "Captain",
        );
        let hero_id = blueoath_domain::HeroId::new(1).unwrap();
        let sf_id = account.dock.heroes[&hero_id]
            .template_id
            .get()
            .saturating_sub(1)
            / 10;
        let fashion_tid = sf_id + 1;
        let catalog = FashionList {
            items: vec![FashionInfo {
                sf_id: i32::try_from(sf_id).unwrap(),
                fashion_tids: vec![i32::try_from(fashion_tid).unwrap()],
            }],
        };
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, fashion_tid);
        append_varint_field(&mut args, 2, 1);
        append_varint_field(&mut args, 3, hero_id.get());

        let result = handle_typed(
            &mut account,
            "fashion.Equip",
            &args,
            &mut ResponseEffects::default(),
            HeroTypedCatalogs {
                fashion: Some(&catalog),
                ..HeroTypedCatalogs::empty()
            },
        );

        assert!(matches!(
            result,
            HandlerResult::Error(GameError::InvalidState("fashion is not owned by hero"))
        ));
    }

    #[test]
    fn typed_intensify_consumes_material_and_updates_attributes() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("intensify-typed").unwrap(),
            "Captain",
        );
        let target_id = blueoath_domain::HeroId::new(1).unwrap();
        let material_id = blueoath_domain::HeroId::new(2).unwrap();
        account.dock.heroes.get_mut(&target_id).unwrap().template_id =
            blueoath_domain::TemplateId::new(100).unwrap();
        let mut material = account.dock.heroes[&target_id].clone();
        material.id = material_id;
        material.locked = false;
        account.dock.heroes.insert(material_id, material);
        let mut catalog = ShipIntensifyCatalog::default();
        catalog
            .need_power_by_template
            .insert(100, (1, vec![(1, 10)]));
        catalog.provide_power_by_template.insert(100, vec![(1, 10)]);
        catalog.max_power_by_template.insert(100, vec![(1, 10)]);
        catalog.same_type_ratio = 10_000;
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, target_id.get());
        append_varint_field(&mut args, 2, material_id.get());
        let result = handle_typed(
            &mut account,
            "hero.HeroIntensify",
            &args,
            &mut ResponseEffects::default(),
            HeroTypedCatalogs {
                hero_level: None,
                tasks: None,
                breakdown: None,
                fashion: None,
                ship_exp_multiplier: 1.0,
                hero_skill_upgrade: None,
                ship_intensify: Some(&catalog),
                ship_break: None,
                ship_advance: None,
                ship_remould: None,
                state: None,
            },
        );
        assert!(matches!(result, HandlerResult::PushOnly));
        assert!(!account.dock.heroes.contains_key(&material_id));
        assert_eq!(hero_progress(&account, 1, "intensify:1:level"), 1);
    }

    #[test]
    fn typed_advance_changes_template_and_consumes_duplicate() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("advance-typed").unwrap(),
            "Captain",
        );
        let target_id = blueoath_domain::HeroId::new(1).unwrap();
        let material_id = blueoath_domain::HeroId::new(2).unwrap();
        account.dock.heroes.get_mut(&target_id).unwrap().template_id =
            blueoath_domain::TemplateId::new(100).unwrap();
        let mut material = account.dock.heroes[&target_id].clone();
        material.id = material_id;
        material.locked = false;
        account.dock.heroes.insert(material_id, material);
        let _ = account
            .resources
            .credit(blueoath_domain::CurrencyKind::Gold, 10);
        let mut catalog = ShipBreakCatalog::default();
        catalog.by_template.insert(
            100,
            ShipBreakConfig {
                min_level: 1,
                break_to: 101,
                break_item: Some((vec![100], 1)),
                currency_cost: Some((5, 1, 10)),
                ..ShipBreakConfig::default()
            },
        );
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, target_id.get());
        append_varint_field(&mut args, 2, material_id.get());
        let result = handle_typed(
            &mut account,
            "hero.HeroAdvance",
            &args,
            &mut ResponseEffects::default(),
            HeroTypedCatalogs {
                hero_level: None,
                tasks: None,
                breakdown: None,
                fashion: None,
                ship_exp_multiplier: 1.0,
                hero_skill_upgrade: None,
                ship_intensify: None,
                ship_break: Some(&catalog),
                ship_advance: None,
                ship_remould: None,
                state: None,
            },
        );
        assert!(matches!(result, HandlerResult::PushOnly));
        assert_eq!(account.dock.heroes[&target_id].template_id.get(), 101);
        assert_eq!(hero_progress(&account, 1, "advance"), 1);
        assert!(!account.dock.heroes.contains_key(&material_id));
    }

    #[test]
    fn typed_remould_unlocks_resonance_skill_and_persists_stage() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("remould-typed").unwrap(),
            "Captain",
        );
        let hero_id = blueoath_domain::HeroId::new(1).unwrap();
        account.dock.heroes.get_mut(&hero_id).unwrap().template_id =
            blueoath_domain::TemplateId::new(100).unwrap();
        let mut catalog = ShipRemouldCatalog::default();
        catalog.ship_info_by_sf_id.insert(
            9,
            ShipInfoRemouldConfig {
                remould_template: vec![10],
            },
        );
        catalog.templates.insert(
            10,
            ShipRemouldTemplateConfig {
                remould_item_group: vec![200],
            },
        );
        catalog.effects.insert(
            200,
            ShipRemouldEffectConfig {
                remould_effect_type: vec![vec![4, 9001]],
                ..ShipRemouldEffectConfig::default()
            },
        );
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, hero_id.get());
        append_varint_field(&mut args, 2, 200);
        let result = handle_typed(
            &mut account,
            "hero.HeroRemould",
            &args,
            &mut ResponseEffects::default(),
            HeroTypedCatalogs {
                hero_level: None,
                tasks: None,
                breakdown: None,
                fashion: None,
                ship_exp_multiplier: 1.0,
                hero_skill_upgrade: None,
                ship_intensify: None,
                ship_break: None,
                ship_advance: None,
                ship_remould: Some(&catalog),
                state: None,
            },
        );
        assert!(matches!(result, HandlerResult::PushOnly));
        assert_eq!(account.dock.heroes[&hero_id].pskills.get(&9001), Some(&1));
        assert_eq!(hero_progress(&account, 1, "remould:level"), 1);
    }

    #[test]
    fn typed_hero_change_equip_moves_standard_slot_atomically() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("hero-equip-typed").unwrap(),
            "Captain",
        );
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_varint_field(&mut args, 2, 2);
        append_varint_field(&mut args, 3, 1);
        append_varint_field(&mut args, 4, 1);
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "hero.ChangeEquip",
            &args,
            &mut effects,
            HeroTypedCatalogs::empty(),
        );

        let hero = account
            .dock
            .heroes
            .get(&blueoath_domain::HeroId::new(1).unwrap())
            .unwrap();
        assert!(matches!(result, HandlerResult::PushOnly));
        assert_eq!(hero.equip_slots[0], None);
        assert_eq!(
            hero.equip_slots[1],
            Some(blueoath_domain::EquipId::new(1).unwrap())
        );
        assert_eq!(
            account
                .dock
                .equipments
                .get(&blueoath_domain::EquipId::new(1).unwrap())
                .unwrap()
                .hero_id,
            Some(blueoath_domain::HeroId::new(1).unwrap())
        );
        assert_eq!(effects.into_parts().0.len(), 2);
    }

    #[test]
    fn typed_auto_equip_applies_nested_slot_changes_and_pushes_state() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("hero-auto-equip-typed").unwrap(),
            "Captain",
        );
        let equip_id = blueoath_domain::EquipId::new(3).unwrap();
        account.dock.equipments.insert(
            equip_id,
            blueoath_domain::EquipmentState {
                id: equip_id,
                template_id: blueoath_domain::TemplateId::new(30_301).unwrap(),
                enhance_level: 0,
                star: 0,
                enhance_exp: 0,
                hero_id: None,
            },
        );
        let mut equip = Vec::new();
        append_varint_field(&mut equip, 1, 1);
        append_varint_field(&mut equip, 2, equip_id.get());
        let mut unit = Vec::new();
        append_varint_field(&mut unit, 1, 1);
        append_bytes_field(&mut unit, 2, &equip);
        let mut args = Vec::new();
        append_bytes_field(&mut args, 1, &unit);
        append_varint_field(&mut args, 2, 1);
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "hero.AutoEquip",
            &args,
            &mut effects,
            HeroTypedCatalogs::empty(),
        );

        assert!(matches!(result, HandlerResult::PushOnly));
        let hero = &account.dock.heroes[&blueoath_domain::HeroId::new(1).unwrap()];
        assert_eq!(hero.equip_slots[1], Some(equip_id));
        assert_eq!(account.dock.equipments[&equip_id].hero_id, Some(hero.id));
        assert_eq!(effects.into_parts().0.len(), 2);
    }

    #[test]
    fn typed_auto_unequip_clears_all_slots_and_equipment_owners() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("hero-auto-unequip-typed").unwrap(),
            "Captain",
        );
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_varint_field(&mut args, 2, 1);
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "hero.AutoUnEquip",
            &args,
            &mut effects,
            HeroTypedCatalogs::empty(),
        );

        assert!(matches!(result, HandlerResult::PushOnly));
        let hero = &account.dock.heroes[&blueoath_domain::HeroId::new(1).unwrap()];
        assert!(hero.equip_slots.iter().all(Option::is_none));
        assert!(account
            .dock
            .equipments
            .values()
            .all(|equipment| equipment.hero_id.is_none()));
        assert_eq!(effects.into_parts().0.len(), 2);
    }
}
