use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;

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

fn hero_progress(account: &blueoath_domain::AccountState, hero_id: u64, field: &str) -> u64 {
    account
        .activities
        .progress
        .get(&format!("compat:hero:{hero_id}:{field}"))
        .copied()
        .unwrap_or_default()
}

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

fn typed_currency_kind(item_id: i32) -> Option<blueoath_domain::CurrencyKind> {
    Some(match item_id {
        1 => blueoath_domain::CurrencyKind::Gold,
        2 => blueoath_domain::CurrencyKind::Diamond,
        5 => blueoath_domain::CurrencyKind::Supply,
        30 => blueoath_domain::CurrencyKind::PvePoint,
        _ => return None,
    })
}

fn typed_item_count(account: &blueoath_domain::AccountState, item_id: i32) -> u64 {
    blueoath_domain::TemplateId::new(item_id.max(0) as u64)
        .ok()
        .and_then(|id| account.inventory.items.get(&id).copied())
        .unwrap_or_default()
}

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

fn push_hero_changes(
    account: &blueoath_domain::AccountState,
    state: Option<&ServerState>,
    effects: &mut ResponseEffects,
    include_equip: bool,
) {
    effects.push_pre(Response::raw(
        "hero.UpdateHeroBagData",
        HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
    ));
    effects.push_pre(Response::raw(
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_typed_account(account)),
    ));
    if include_equip {
        effects.push_pre(Response::raw(
            "equip.UpdateEquipBagData",
            EquipListCodec::encode(&equip_list_from_typed_account(account)),
        ));
    }
    if let Some(state) = state {
        effects.push_pre(Response::raw(
            "user.UpdateUserInfo",
            UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
        ));
    }
}

fn push_hero_delta_changes(
    account: &blueoath_domain::AccountState,
    target_id: u64,
    consumed_ids: &[u64],
    state: Option<&ServerState>,
    effects: &mut ResponseEffects,
) {
    let full = hero_bag_from_typed_account(account);
    let mut heroes = full
        .heroes
        .into_iter()
        .filter(|hero| u64::from(hero.hero_id) == target_id)
        .collect::<Vec<_>>();
    heroes.extend(consumed_ids.iter().filter_map(|hero_id| {
        u32::try_from(*hero_id).ok().map(|hero_id| HeroGrid {
            hero_id,
            template_id: 0,
            ..HeroGrid::default()
        })
    }));
    effects.push_pre(Response::raw(
        "hero.UpdateHeroBagData",
        HeroBagCodec::encode(&HeroBag {
            heroes,
            bag_size: full.bag_size,
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
    if let Some(state) = state {
        effects.push_pre(Response::raw(
            "user.UpdateUserInfo",
            UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
        ));
    }
}

fn handle_fashion_equip(
    account: &mut blueoath_domain::AccountState,
    request_args: &[u8],
    catalog: Option<&FashionList>,
    state: Option<&ServerState>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let Ok(request) = FashionEquipRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "fashion equip request is invalid",
        ));
    };
    let Ok(hero_id) = blueoath_domain::HeroId::new(request.hero_id) else {
        return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
    };
    let Some(hero) = account.dock.heroes.get(&hero_id) else {
        return HandlerResult::Error(GameError::NotFound("hero"));
    };
    let sf_id = hero.template_id.get().saturating_sub(1) / 10;
    let selected = if request.fashion_tid > 0 {
        u64::try_from(request.fashion_tid).unwrap_or_default()
    } else if request.equip_status == 0 {
        sf_id
    } else {
        return HandlerResult::Error(GameError::InvalidRequest("fashion is invalid"));
    };
    let Some(selected) = blueoath_domain::TemplateId::new(selected).ok() else {
        return HandlerResult::Error(GameError::InvalidRequest("fashion is invalid"));
    };
    if let Some(catalog) = catalog {
        let Ok(sf_id) = i32::try_from(sf_id) else {
            return HandlerResult::Error(GameError::InvalidState("fashion ship id is invalid"));
        };
        let belongs = catalog
            .items
            .iter()
            .find(|item| item.sf_id == sf_id)
            .is_some_and(|item| {
                item.fashion_tids.contains(&(selected.get() as i32))
                    || selected.get() == sf_id as u64
            });
        if !belongs {
            return HandlerResult::Error(GameError::InvalidState("fashion is not for this hero"));
        }
        let owned = selected.get() == sf_id as u64
            || account
                .fashion
                .entries
                .get(&(sf_id as u64))
                .is_some_and(|items| items.contains(&selected));
        if !owned {
            return HandlerResult::Error(GameError::InvalidState("fashion is not owned by hero"));
        }
    }
    let Some(hero) = account.dock.heroes.get_mut(&hero_id) else {
        return HandlerResult::Error(GameError::NotFound("hero"));
    };
    hero.fashioning = u32::try_from(selected.get()).unwrap_or_default();
    push_hero_changes(account, state, effects, false);
    HandlerResult::PushOnly
}

fn handle_skill_upgrade(
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

fn handle_intensify(
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

fn handle_advance(
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

fn handle_advance_max_level(
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

fn handle_advance_mub(
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

fn handle_remould(
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
    match method {
        "fashion.Equip" => {
            handle_fashion_equip(account, request_args, fashion_catalog, state, effects)
        }
        "hero.GetHeroInfo" | "hero.GetHeroInfoByHeroIdArray" => {
            HandlerResult::Reply(Response::raw(
                method,
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            ))
        }
        "hero.LockHero" => {
            let Ok(request) = HeroLockRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "hero lock request is invalid",
                ));
            };
            let hero_id = request.hero_id;
            let locked = request.locked;
            let Some(hero_id) = blueoath_domain::HeroId::new(hero_id).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
            };
            let Some(hero) = account.dock.heroes.get_mut(&hero_id) else {
                return HandlerResult::Error(GameError::InvalidRequest("hero was not found"));
            };
            hero.locked = locked;
            effects.push_pre(Response::raw(
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            ));
            HandlerResult::PushOnly
        }
        "hero.ChangeName" => {
            let Ok(request) = HeroChangeNameRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "hero name request is invalid",
                ));
            };
            let hero_id = request.hero_id;
            let Some(hero_id) = blueoath_domain::HeroId::new(hero_id).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
            };
            let Some(hero) = account.dock.heroes.get_mut(&hero_id) else {
                return HandlerResult::Error(GameError::InvalidRequest("hero was not found"));
            };
            hero.name = request.name;
            hero.change_name_time = u64::from(current_unix_seconds());
            effects.push_pre(Response::raw(
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            ));
            HandlerResult::PushOnly
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
                    bag_size: 200,
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
        "hero.ChangeEquip" => {
            let Ok(request) = HeroChangeEquipRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "hero equip request is invalid",
                ));
            };
            if request.equip_type != 1 {
                return HandlerResult::Empty;
            }
            let hero_id = request.hero_id;
            let slot = request.slot;
            let equip_id = request.equip_id;
            let Some(hero_id) = blueoath_domain::HeroId::new(hero_id).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
            };
            let Some(hero) = account.dock.heroes.get(&hero_id) else {
                return HandlerResult::Error(GameError::InvalidRequest("hero was not found"));
            };
            let Some(slot_index) = usize::try_from(slot - 1).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "equipment slot is invalid",
                ));
            };
            let old_equip_id = hero.equip_slots.get(slot_index).copied().flatten();
            let new_equip_id = if equip_id == 0 {
                None
            } else {
                let Some(equip_id) = blueoath_domain::EquipId::new(equip_id).ok() else {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "equipment id is invalid",
                    ));
                };
                Some(equip_id)
            };
            if new_equip_id == old_equip_id {
                return HandlerResult::PushOnly;
            }
            if let Some(new_equip_id) = new_equip_id {
                let Some(equipment) = account.dock.equipments.get(&new_equip_id) else {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "equipment was not found",
                    ));
                };
                if equipment.hero_id.is_some_and(|owner| owner != hero_id)
                    || account.dock.heroes.values().any(|candidate| {
                        candidate.id != hero_id
                            && candidate
                                .equip_slots
                                .iter()
                                .flatten()
                                .any(|id| *id == new_equip_id)
                    })
                {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "equipment belongs to another hero",
                    ));
                }
            }
            if let Some(old_equip_id) = old_equip_id {
                if let Some(equipment) = account.dock.equipments.get_mut(&old_equip_id) {
                    equipment.hero_id = None;
                }
            }
            if let Some(new_equip_id) = new_equip_id {
                if let Some(equipment) = account.dock.equipments.get_mut(&new_equip_id) {
                    equipment.hero_id = Some(hero_id);
                }
                if let Some(hero) = account.dock.heroes.get_mut(&hero_id) {
                    for equipped in &mut hero.equip_slots {
                        if *equipped == Some(new_equip_id) {
                            *equipped = None;
                        }
                    }
                }
            }
            if let Some(hero) = account.dock.heroes.get_mut(&hero_id) {
                if hero.equip_slots.len() <= slot_index {
                    hero.equip_slots.resize(slot_index + 1, None);
                }
                hero.equip_slots[slot_index] = new_equip_id;
            }
            effects.push_pre(Response::raw(
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "equip.UpdateEquipBagData",
                EquipListCodec::encode(&equip_list_from_typed_account(account)),
            ));
            HandlerResult::PushOnly
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
            while level < 200 {
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
            exp = exp
                .saturating_add(remaining)
                .min(u64::from(i32::MAX as u32));
            if let Some(hero) = account.dock.heroes.get_mut(&hero_id) {
                hero.level = level;
                hero.exp = exp;
            }
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
        "hero.StudySkill" => handle_skill_upgrade(
            account,
            method,
            request_args,
            hero_skill_upgrade_catalog,
            state,
            effects,
        ),
        "hero.HeroIntensify" => handle_intensify(
            account,
            method,
            request_args,
            ship_intensify_catalog,
            state,
            effects,
        ),
        "hero.HeroAdvance" => handle_advance(
            account,
            method,
            request_args,
            ship_break_catalog,
            effects,
            state,
        ),
        "hero.HeroAdvMaxLv" => {
            handle_advance_max_level(account, method, request_args, ship_advance_catalog, effects)
        }
        "hero.HeroAdvanceMUB" => handle_advance_mub(
            account,
            method,
            request_args,
            ship_break_catalog,
            state,
            effects,
        ),
        "hero.HeroRemould" => handle_remould(
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
        assert_eq!(effects.into_parts().0.len(), 3);
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
    fn typed_fashion_equip_updates_owned_hero_fashion() {
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
}
