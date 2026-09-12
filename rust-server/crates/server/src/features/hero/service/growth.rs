//! Hero skill, intensification, advancement, and remould handlers.

use super::super::*;
use super::response::{push_hero_changes, push_hero_delta_changes};
use super::{
    consume_costs, costs_available, hero_is_in_use, hero_progress, set_hero_progress,
    typed_currency_kind, typed_item_count,
};
use crate::common::error::GameError;
use crate::common::response::{HandlerResult, Response, ResponseEffects};
/// 处理英雄技能升级、消耗校验与状态更新。
pub(super) fn handle_skill_upgrade(
    account: &mut blueoath_domain::AccountState,
    _method: &str,
    request_args: &[u8],
    catalog: Option<&HeroSkillUpgradeCatalog>,
    state: Option<&ServerState>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let Some(catalog) = catalog else {
        return HandlerResult::Error(GameError::CatalogUnavailable);
    };
    let Ok(request) = HeroStudySkillRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest("study skill request is invalid"));
    };
    let Ok(hero_id) = blueoath_domain::HeroId::new(request.hero_id) else {
        return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
    };
    let Ok(skill_id) = i32::try_from(request.skill_id) else {
        return HandlerResult::Error(GameError::InvalidRequest("skill id is invalid"));
    };
    let Some(hero) = account.dock.heroes.get(&hero_id) else {
        return HandlerResult::Error(GameError::NotFound("hero"));
    };
    let current = hero.pskills.get(&request.skill_id).copied().unwrap_or(1);
    let Some(costs) = current
        .checked_sub(1)
        .and_then(|level| usize::try_from(level).ok())
        .and_then(|level| catalog.costs_by_skill.get(&skill_id)?.get(level))
    else {
        return HandlerResult::Error(GameError::InvalidState(
            "skill is already maxed or not upgradeable",
        ));
    };
    if !consume_costs(
        account,
        &costs
            .iter()
            .map(|(goods_type, item_id, amount)| (*goods_type, *item_id, i64::from(*amount)))
            .collect::<Vec<_>>(),
    ) {
        return HandlerResult::Error(GameError::InvalidState(
            "skill upgrade cost is insufficient",
        ));
    }
    if let Some(hero) = account.dock.heroes.get_mut(&hero_id) {
        hero.pskills
            .insert(request.skill_id, current.saturating_add(1));
    }
    push_hero_changes(account, state, effects, false);
    HandlerResult::PushOnly
}

/// 处理英雄强化请求及其资源消耗。
pub(super) fn handle_intensify(
    account: &mut blueoath_domain::AccountState,
    _method: &str,
    request_args: &[u8],
    catalog: Option<&ShipIntensifyCatalog>,
    state: Option<&ServerState>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let Some(catalog) = catalog else {
        return HandlerResult::Error(GameError::CatalogUnavailable);
    };
    let Ok(request) = HeroIntensifyRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest("intensify request is invalid"));
    };
    let Ok(target_id) = blueoath_domain::HeroId::new(request.hero_id) else {
        return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
    };
    if request.consumed_hero_ids.is_empty()
        || request.consumed_hero_ids.len() > 12
        || request
            .consumed_hero_ids
            .iter()
            .any(|id| *id == 0 || *id == request.hero_id)
        || request
            .consumed_hero_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != request.consumed_hero_ids.len()
    {
        return HandlerResult::Error(GameError::InvalidRequest("intensify materials are invalid"));
    }
    let Some(target) = account.dock.heroes.get(&target_id).cloned() else {
        return HandlerResult::Error(GameError::NotFound("hero"));
    };
    let template_id = i32::try_from(target.template_id.get()).unwrap_or_default();
    let Some((target_type, need_rows)) = catalog.need_power_by_template.get(&template_id) else {
        return HandlerResult::Error(GameError::CatalogUnavailable);
    };
    let Some(max_rows) = catalog.max_power_by_template.get(&template_id) else {
        return HandlerResult::Error(GameError::CatalogUnavailable);
    };
    let diamond_cost = if request.super_intensify {
        catalog
            .diamond_cost_per_hero
            .checked_mul(i64::try_from(request.consumed_hero_ids.len()).unwrap_or(i64::MAX))
            .unwrap_or(i64::MAX)
    } else {
        0
    };
    if account
        .resources
        .amount(blueoath_domain::CurrencyKind::Diamond)
        .get()
        < u64::try_from(diamond_cost.max(0)).unwrap_or(u64::MAX)
    {
        return HandlerResult::Error(GameError::InsufficientResource(
            blueoath_domain::CurrencyKind::Diamond,
        ));
    }
    let mut added = std::collections::BTreeMap::<i32, i64>::new();
    for raw_id in &request.consumed_hero_ids {
        let Ok(hero_id) = blueoath_domain::HeroId::new(*raw_id) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "intensify material id is invalid",
            ));
        };
        let Some(material) = account.dock.heroes.get(&hero_id) else {
            return HandlerResult::Error(GameError::NotFound("intensify material"));
        };
        if material.locked
            || material.level != 1
            || hero_progress(account, *raw_id, "advance") > 1
            || account.activities.progress.keys().any(|key| {
                key.starts_with(&format!("compat:hero:{}:intensify:", raw_id))
                    && key.ends_with(":level")
                    && account
                        .activities
                        .progress
                        .get(key)
                        .copied()
                        .unwrap_or_default()
                        > 0
            })
            || hero_is_in_use(account, hero_id)
        {
            return HandlerResult::Error(GameError::InvalidState(
                "intensify material is unavailable",
            ));
        }
        let material_template = i32::try_from(material.template_id.get()).unwrap_or_default();
        let Some((material_type, _)) = catalog.need_power_by_template.get(&material_template)
        else {
            return HandlerResult::Error(GameError::CatalogUnavailable);
        };
        let Some(provides) = catalog.provide_power_by_template.get(&material_template) else {
            return HandlerResult::Error(GameError::CatalogUnavailable);
        };
        let ratio = if material_type == target_type {
            catalog.same_type_ratio
        } else {
            10_000
        };
        for (attr_type, amount) in provides {
            let mut amount = amount.saturating_mul(ratio) / 10_000;
            if request.super_intensify {
                amount = amount.saturating_mul(2);
            }
            *added.entry(*attr_type).or_default() = added
                .get(attr_type)
                .copied()
                .unwrap_or_default()
                .saturating_add(amount);
        }
    }
    let max_by_attr = max_rows
        .iter()
        .copied()
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut updates = Vec::new();
    let mut gained = false;
    for (attr_type, need_exp) in need_rows {
        if *need_exp <= 0 {
            continue;
        }
        let Some(max_level) = max_by_attr.get(attr_type).copied() else {
            continue;
        };
        let Some(add) = added.get(attr_type).copied() else {
            continue;
        };
        let level = hero_progress(
            account,
            request.hero_id,
            &format!("intensify:{attr_type}:level"),
        ) as i64;
        let cur_exp = hero_progress(
            account,
            request.hero_id,
            &format!("intensify:{attr_type}:exp"),
        ) as i64;
        let current = level.saturating_mul(*need_exp).saturating_add(cur_exp);
        let cap = max_level.saturating_mul(*need_exp);
        if max_level <= 0 || add <= 0 || current >= cap {
            continue;
        }
        let next = current.saturating_add(add).min(cap);
        updates.push((*attr_type, next / *need_exp, next % *need_exp));
        gained |= next > current;
    }
    if !gained {
        return HandlerResult::Error(GameError::InvalidState(
            "intensify attributes are already capped",
        ));
    }
    for (attr_type, level, exp) in updates {
        set_hero_progress(
            account,
            request.hero_id,
            format!("intensify:{attr_type}:level"),
            level as u64,
        );
        set_hero_progress(
            account,
            request.hero_id,
            format!("intensify:{attr_type}:exp"),
            exp as u64,
        );
    }
    let consumed = request
        .consumed_hero_ids
        .iter()
        .filter_map(|id| blueoath_domain::HeroId::new(*id).ok())
        .collect::<std::collections::BTreeSet<_>>();
    account.dock.heroes.retain(|id, _| !consumed.contains(id));
    for equipment in account.dock.equipments.values_mut() {
        if equipment.hero_id.is_some_and(|id| consumed.contains(&id)) {
            equipment.hero_id = None;
        }
    }
    if diamond_cost > 0 {
        let _ = account.resources.debit(
            blueoath_domain::CurrencyKind::Diamond,
            u64::try_from(diamond_cost).unwrap_or(u64::MAX),
        );
    }
    push_hero_delta_changes(
        account,
        request.hero_id,
        &request.consumed_hero_ids,
        state,
        effects,
    );
    HandlerResult::PushOnly
}

/// 处理英雄进阶请求及进阶材料消耗。
pub(super) fn handle_advance(
    account: &mut blueoath_domain::AccountState,
    _method: &str,
    request_args: &[u8],
    catalog: Option<&ShipBreakCatalog>,
    effects: &mut ResponseEffects,
    state: Option<&ServerState>,
) -> HandlerResult {
    let Some(catalog) = catalog else {
        return HandlerResult::Error(GameError::CatalogUnavailable);
    };
    let Ok(request) = HeroAdvanceRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest("advance request is invalid"));
    };
    let Some(target_id) = blueoath_domain::HeroId::new(request.hero_id).ok() else {
        return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
    };
    let Some(target) = account.dock.heroes.get(&target_id).cloned() else {
        return HandlerResult::Error(GameError::NotFound("advance target"));
    };
    let template_id = i32::try_from(target.template_id.get()).unwrap_or_default();
    let Some(config) = catalog.by_template.get(&template_id) else {
        return HandlerResult::Error(GameError::CatalogUnavailable);
    };
    if i32::try_from(target.level).unwrap_or(i32::MAX) < config.min_level || config.break_to <= 0 {
        return HandlerResult::Error(GameError::InvalidState("hero level is too low for advance"));
    }
    let allowed = config
        .break_item
        .as_ref()
        .map(|(ids, _)| {
            ids.iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();
    let required = config
        .break_item
        .as_ref()
        .map(|(_, count)| *count)
        .unwrap_or(usize::from(config.break_item_optional_count > 0));
    if request.consumed_hero_ids.len() != required
        || request
            .consumed_hero_ids
            .iter()
            .any(|id| *id == 0 || *id == request.hero_id)
        || request
            .consumed_hero_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != request.consumed_hero_ids.len()
    {
        return HandlerResult::Error(GameError::InvalidRequest("advance materials are invalid"));
    }
    let consumed = request
        .consumed_hero_ids
        .iter()
        .filter_map(|id| blueoath_domain::HeroId::new(*id).ok())
        .collect::<Vec<_>>();
    if consumed.len() != request.consumed_hero_ids.len() {
        return HandlerResult::Error(GameError::InvalidRequest("advance material id is invalid"));
    }
    for hero_id in &consumed {
        let Some(material) = account.dock.heroes.get(hero_id) else {
            return HandlerResult::Error(GameError::NotFound("advance material"));
        };
        let material_template = i32::try_from(material.template_id.get()).unwrap_or_default();
        if material.locked
            || hero_is_in_use(account, *hero_id)
            || (!allowed.is_empty() && !allowed.contains(&material_template))
        {
            return HandlerResult::Error(GameError::InvalidState("advance material is invalid"));
        }
    }
    if let Some((item_id, count)) = config.break_item_mub {
        if request.consume_item_ids.len() != usize::try_from(count).unwrap_or_default()
            || request.consume_item_ids.iter().any(|id| *id != item_id)
        {
            return HandlerResult::Error(GameError::InvalidRequest(
                "advance item count is invalid",
            ));
        }
        if request
            .consume_item_ids
            .iter()
            .any(|id| typed_item_count(account, *id) == 0)
        {
            return HandlerResult::Error(GameError::InvalidState("advance item is unavailable"));
        }
    } else if !request.consume_item_ids.is_empty() {
        return HandlerResult::Error(GameError::InvalidRequest("advance does not accept items"));
    }
    let Some((goods_type, currency_id, currency_cost)) = config.currency_cost else {
        return HandlerResult::Error(GameError::CatalogUnavailable);
    };
    if goods_type != 5 || typed_currency_kind(currency_id).is_none() || currency_cost < 0 {
        return HandlerResult::Error(GameError::InvalidState(
            "advance currency config is invalid",
        ));
    }
    let mut costs = vec![(5, currency_id, currency_cost)];
    for item_id in &request.consume_item_ids {
        costs.push((1, *item_id, 1));
    }
    if !costs_available(account, &costs) {
        return HandlerResult::Error(GameError::InvalidState("advance cost is insufficient"));
    }
    if !consume_costs(account, &costs) {
        return HandlerResult::Error(GameError::InvalidState("advance cost cannot be consumed"));
    }
    let consumed_set = consumed
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    account
        .dock
        .heroes
        .retain(|id, _| !consumed_set.contains(id));
    for equipment in account.dock.equipments.values_mut() {
        if equipment
            .hero_id
            .is_some_and(|id| consumed_set.contains(&id))
        {
            equipment.hero_id = None;
        }
    }
    if let Some(hero) = account.dock.heroes.get_mut(&target_id) {
        hero.template_id = blueoath_domain::TemplateId::new(config.break_to as u64).unwrap();
    }
    let advance = hero_progress(account, request.hero_id, "advance").saturating_add(1);
    set_hero_progress(account, request.hero_id, "advance", advance);
    push_hero_delta_changes(
        account,
        request.hero_id,
        &request.consumed_hero_ids,
        state,
        effects,
    );
    HandlerResult::PushOnly
}

/// 处理英雄一键进阶至当前可达上限的请求。
pub(super) fn handle_advance_max_level(
    account: &mut blueoath_domain::AccountState,
    _method: &str,
    request_args: &[u8],
    catalog: Option<&ShipAdvanceCatalog>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let Some(catalog) = catalog else {
        return HandlerResult::Error(GameError::CatalogUnavailable);
    };
    let Ok(request) = HeroAdvanceMaxLevelRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "max-level advance request is invalid",
        ));
    };
    let Some(hero_id) = blueoath_domain::HeroId::new(request.hero_id).ok() else {
        return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
    };
    let Some(hero) = account.dock.heroes.get(&hero_id) else {
        return HandlerResult::Error(GameError::NotFound("hero"));
    };
    let next = hero_progress(account, request.hero_id, "advLv").saturating_add(1);
    if let Some(config) = catalog
        .by_level
        .get(&(i32::try_from(next).unwrap_or(i32::MAX)))
    {
        if hero.level < u32::try_from(config.initial_level.max(0)).unwrap_or(u32::MAX) {
            return HandlerResult::Error(GameError::InvalidState(
                "hero level is too low for max-level advance",
            ));
        }
    }
    set_hero_progress(account, request.hero_id, "advLv", next);
    effects.push_pre(Response::raw(
        "hero.UpdateHeroBagData",
        HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
    ));
    HandlerResult::PushOnly
}

/// 处理英雄突破请求及突破状态更新。
pub(super) fn handle_advance_mub(
    account: &mut blueoath_domain::AccountState,
    _method: &str,
    request_args: &[u8],
    catalog: Option<&ShipBreakCatalog>,
    state: Option<&ServerState>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let Some(catalog) = catalog else {
        return HandlerResult::Error(GameError::CatalogUnavailable);
    };
    let Ok(request) = HeroAdvanceMubRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest("mub advance request is invalid"));
    };
    let Some(hero_id) = blueoath_domain::HeroId::new(request.hero_id).ok() else {
        return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
    };
    let Some(hero) = account.dock.heroes.get(&hero_id).cloned() else {
        return HandlerResult::Error(GameError::NotFound("hero"));
    };
    let template_id = i32::try_from(hero.template_id.get()).unwrap_or_default();
    let Some(config) = catalog.by_template.get(&template_id) else {
        return HandlerResult::Error(GameError::CatalogUnavailable);
    };
    if hero.level < u32::try_from(config.min_level.max(0)).unwrap_or(u32::MAX) {
        return HandlerResult::Error(GameError::InvalidState("hero level is too low for advance"));
    }
    if config.break_to <= 0 {
        return HandlerResult::Error(GameError::InvalidState(
            "advance target has no next template",
        ));
    }
    let Some((fragment_id, required)) = config.break_item_mub else {
        return HandlerResult::Error(GameError::InvalidState(
            "advance fragment config is missing",
        ));
    };
    let counts = if request.item_counts.is_empty() {
        vec![1; request.item_ids.len()]
    } else {
        request.item_counts.clone()
    };
    if request.item_ids.len() != counts.len() {
        return HandlerResult::Error(GameError::InvalidRequest(
            "advance fragment count is invalid",
        ));
    }
    let allowed = config
        .break_usableitem_mub
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let mut effective = 0i64;
    for (item_id, count) in request.item_ids.iter().zip(&counts) {
        if *count <= 0
            || (*item_id != fragment_id && !allowed.contains(item_id))
            || typed_item_count(account, *item_id) < u64::try_from(*count).unwrap_or(u64::MAX)
        {
            return HandlerResult::Error(GameError::InvalidState("advance fragment is invalid"));
        }
        let ratio = match *item_id {
            18051 => 1,
            17512 => 30,
            _ => 1,
        };
        effective = effective.saturating_add(i64::from(*count).saturating_mul(ratio));
    }
    if effective != i64::from(required) {
        return HandlerResult::Error(GameError::InvalidRequest(
            "advance fragment count is invalid",
        ));
    }
    let Some((goods_type, currency_id, currency_cost)) = config.currency_cost else {
        return HandlerResult::Error(GameError::CatalogUnavailable);
    };
    if goods_type != 5 {
        return HandlerResult::Error(GameError::InvalidState(
            "advance currency config is invalid",
        ));
    }
    let mut costs = vec![(5, currency_id, currency_cost)];
    for (item_id, count) in request.item_ids.iter().zip(&counts) {
        costs.push((1, *item_id, i64::from(*count)));
    }
    if !consume_costs(account, &costs) {
        return HandlerResult::Error(GameError::InvalidState("advance cost is insufficient"));
    }
    if let Some(hero) = account.dock.heroes.get_mut(&hero_id) {
        hero.template_id = blueoath_domain::TemplateId::new(config.break_to as u64).unwrap();
    }
    set_hero_progress(
        account,
        request.hero_id,
        "advance",
        hero_progress(account, request.hero_id, "advance").saturating_add(1),
    );
    push_hero_changes(account, state, effects, true);
    HandlerResult::PushOnly
}

/// 处理英雄重铸请求及属性重置结果。
pub(super) fn handle_remould(
    account: &mut blueoath_domain::AccountState,
    _method: &str,
    request_args: &[u8],
    catalog: Option<&ShipRemouldCatalog>,
    state: Option<&ServerState>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let Some(catalog) = catalog else {
        return HandlerResult::Error(GameError::CatalogUnavailable);
    };
    let Ok(request) = HeroRemouldRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest("remould request is invalid"));
    };
    let Some(hero_id) = blueoath_domain::HeroId::new(request.hero_id).ok() else {
        return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
    };
    let Some(hero) = account.dock.heroes.get(&hero_id).cloned() else {
        return HandlerResult::Error(GameError::NotFound("hero"));
    };
    let template_id = i32::try_from(hero.template_id.get()).unwrap_or_default();
    let sf_id = template_id.saturating_sub(1) / 10;
    let Some(info) = catalog.ship_info_by_sf_id.get(&sf_id) else {
        return HandlerResult::Error(GameError::InvalidState("hero cannot be remoulded"));
    };
    let Some(effect) = catalog.effects.get(&request.effect_id) else {
        return HandlerResult::Error(GameError::NotFound("remould effect"));
    };
    let Some(effect_stage) =
        info.remould_template
            .iter()
            .enumerate()
            .find_map(|(index, stage_id)| {
                catalog
                    .templates
                    .get(stage_id)
                    .filter(|stage| stage.remould_item_group.contains(&request.effect_id))
                    .map(|_| index)
            })
    else {
        return HandlerResult::Error(GameError::InvalidState("remould effect is invalid"));
    };
    let mut completed = account
        .activities
        .progress
        .iter()
        .filter_map(|(key, value)| {
            key.strip_prefix(&format!("compat:hero:{}:remould:effect:", request.hero_id))
                .and_then(|id| id.parse::<i32>().ok())
                .filter(|_| *value > 0)
        })
        .collect::<std::collections::BTreeSet<_>>();
    if !completed.insert(request.effect_id) {
        return HandlerResult::Error(GameError::InvalidState("remould effect is already active"));
    }
    let mut before = completed.clone();
    before.remove(&request.effect_id);
    let expected_stage = info
        .remould_template
        .iter()
        .take_while(|stage_id| {
            catalog.templates.get(stage_id).is_some_and(|stage| {
                stage
                    .remould_item_group
                    .iter()
                    .all(|id| before.contains(id))
            })
        })
        .count();
    if effect_stage != expected_stage
        || (effect.remould_prev.iter().any(|id| *id > 0)
            && !effect.remould_prev.iter().any(|id| before.contains(id)))
        || hero.level < u32::try_from(effect.limit_level.max(0)).unwrap_or(u32::MAX)
        || hero_progress(account, request.hero_id, "advance")
            < u64::try_from(effect.limit_star.max(0)).unwrap_or(u64::MAX)
    {
        return HandlerResult::Error(GameError::InvalidState("remould requirement is not met"));
    }
    let mut costs = std::collections::BTreeMap::<(i32, i32), i64>::new();
    for (kind, item_id, amount) in &effect.costs {
        *costs.entry((*kind, *item_id)).or_default() = costs
            .get(&(*kind, *item_id))
            .copied()
            .unwrap_or_default()
            .saturating_add(*amount);
    }
    let costs = costs
        .into_iter()
        .map(|((kind, item), amount)| (kind, item, amount))
        .collect::<Vec<_>>();
    if !consume_costs(account, &costs) {
        return HandlerResult::Error(GameError::InvalidState("remould cost is insufficient"));
    }
    for effect_values in &effect.remould_effect_type {
        if effect_values.first() == Some(&4)
            && effect_values.get(1).copied().unwrap_or_default() > 0
        {
            let skill = effect_values[1] as u64;
            if let Some(hero) = account.dock.heroes.get_mut(&hero_id) {
                hero.pskills.entry(skill).or_insert(1);
            }
        } else if effect_values.first() == Some(&5) && effect_values.len() >= 3 {
            set_hero_progress(
                account,
                request.hero_id,
                format!("pskill:{}:replace", effect_values[1]),
                u64::try_from(effect_values[2].max(0)).unwrap_or_default(),
            );
        }
    }
    for effect_id in completed {
        set_hero_progress(
            account,
            request.hero_id,
            format!("remould:effect:{effect_id}"),
            1,
        );
    }
    let level = info
        .remould_template
        .iter()
        .take_while(|stage_id| {
            catalog.templates.get(stage_id).is_some_and(|stage| {
                stage.remould_item_group.iter().all(|id| {
                    account
                        .activities
                        .progress
                        .get(&format!(
                            "compat:hero:{}:remould:effect:{id}",
                            request.hero_id
                        ))
                        .copied()
                        .unwrap_or_default()
                        > 0
                })
            })
        })
        .count();
    set_hero_progress(account, request.hero_id, "remould:level", level as u64);
    push_hero_changes(account, state, effects, false);
    HandlerResult::PushOnly
}
