//! Compatibility handlers for hero combination protocols.

use super::super::*;
use super::consume_typed_item;
use crate::common::error::GameError;
use crate::common::response::{HandlerResult, Response, ResponseEffects};

/// 读取英雄组合在领域进度中的当前值。
fn typed_combination_value(
    account: &blueoath_domain::AccountState,
    hero_id: u64,
    field: &str,
) -> u64 {
    account
        .activities
        .progress
        .get(&format!("compat:hero:{hero_id}:combination:{field}"))
        .copied()
        .unwrap_or_default()
}

/// 写入英雄组合在领域进度中的当前值。
fn set_typed_combination_value(
    account: &mut blueoath_domain::AccountState,
    hero_id: u64,
    field: &str,
    value: u64,
) {
    account
        .activities
        .progress
        .insert(format!("compat:hero:{hero_id}:combination:{field}"), value);
}

/// 从组合目录中查找指定组合规则。
fn typed_combination_rule<'a>(
    account: &blueoath_domain::AccountState,
    hero_id: u64,
    catalog: &'a CombinationCatalog,
    level: u64,
) -> Option<&'a CombinationRule> {
    let template_id = account
        .dock
        .heroes
        .values()
        .find(|hero| hero.id.get() == hero_id)
        .map(|hero| hero.template_id.get() / 10)?;
    let sf_id = i32::try_from(template_id).ok()?;
    if !catalog.open_sf_ids.is_empty() && !catalog.open_sf_ids.contains(&sf_id) {
        return None;
    }
    let level = level.clamp(1, 100);
    let key = sf_id.saturating_mul(100) + i32::try_from((level - 1) / 10).ok()?;
    catalog.rules_by_id.get(&key)
}

/// 校验英雄组合升级所需的材料与货币。
fn typed_combination_cost_available(
    account: &blueoath_domain::AccountState,
    costs: &[(i32, i32, i32)],
) -> bool {
    costs.iter().all(|(goods_type, item_id, amount)| {
        let Ok(amount) = u64::try_from(*amount) else {
            return false;
        };
        if *goods_type == 5 {
            let Some(currency) = typed_currency_kind(*item_id) else {
                return false;
            };
            account.resources.amount(currency).get() >= amount
        } else if matches!(*goods_type, 1 | 6) {
            blueoath_domain::TemplateId::new((*item_id).max(0) as u64)
                .ok()
                .is_some_and(|template_id| {
                    account
                        .inventory
                        .items
                        .get(&template_id)
                        .copied()
                        .unwrap_or_default()
                        >= amount
                })
        } else {
            false
        }
    })
}

/// 扣除英雄组合升级所需的材料与货币。
fn typed_combination_consume(
    account: &mut blueoath_domain::AccountState,
    costs: &[(i32, i32, i32)],
) -> bool {
    if !typed_combination_cost_available(account, costs) {
        return false;
    }
    for (goods_type, item_id, amount) in costs {
        let amount = u64::try_from(*amount).unwrap_or_default();
        if *goods_type == 5 {
            let Some(currency) = typed_currency_kind(*item_id) else {
                return false;
            };
            if account.resources.debit(currency, amount).is_err() {
                return false;
            }
        } else if !consume_typed_item(account, *item_id, amount) {
            return false;
        }
    }
    true
}

/// 将组合协议中的货币编号转换为领域货币类型。
fn typed_currency_kind(item_id: i32) -> Option<blueoath_domain::CurrencyKind> {
    Some(match item_id {
        1 => blueoath_domain::CurrencyKind::Gold,
        2 => blueoath_domain::CurrencyKind::Diamond,
        5 => blueoath_domain::CurrencyKind::Supply,
        30 => blueoath_domain::CurrencyKind::PvePoint,
        _ => return None,
    })
}

/// 处理英雄组合查询、升级和奖励领取请求。
pub(super) fn handle_typed_combination(
    _state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    combination_catalog: Option<&CombinationCatalog>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let Some(catalog) = combination_catalog else {
        return HandlerResult::Error(GameError::CatalogUnavailable);
    };
    if method == "hero.HeroCombine" {
        let Ok(request) = HeroCombineRequest::decode(request_args) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "hero combination relation is invalid",
            ));
        };
        let main_id = request.main_id;
        let deputy_id = request.deputy_id;
        if main_id == 0 || main_id == deputy_id {
            return HandlerResult::Error(GameError::InvalidRequest(
                "hero combination relation is invalid",
            ));
        }
        let has_hero = |id| account.dock.heroes.values().any(|hero| hero.id.get() == id);
        if !has_hero(main_id) || (deputy_id > 0 && !has_hero(deputy_id)) {
            return HandlerResult::Error(GameError::NotFound("hero"));
        }
        if deputy_id > 0 {
            let is_open = |id| {
                account
                    .dock
                    .heroes
                    .values()
                    .find(|hero| hero.id.get() == id)
                    .and_then(|hero| i32::try_from(hero.template_id.get() / 10).ok())
                    .is_some_and(|sf_id| {
                        catalog.open_sf_ids.is_empty() || catalog.open_sf_ids.contains(&sf_id)
                    })
            };
            if !is_open(main_id) || !is_open(deputy_id) {
                return HandlerResult::Error(GameError::InvalidState(
                    "hero combination is not open for this ship",
                ));
            }
        }
        let current_deputy = typed_combination_value(account, main_id, "combine");
        if deputy_id > 0
            && (typed_combination_value(account, deputy_id, "beCombined") > 0
                || (current_deputy > 0 && current_deputy != deputy_id))
        {
            return HandlerResult::Error(GameError::InvalidState(
                "hero is already in another combination",
            ));
        }
        if current_deputy > 0 {
            set_typed_combination_value(account, current_deputy, "beCombined", 0);
        }
        set_typed_combination_value(account, main_id, "combine", deputy_id);
        if deputy_id > 0 {
            set_typed_combination_value(account, deputy_id, "beCombined", main_id);
        }
        effects.push_pre(Response::raw(
            "hero.UpdateHeroBagData",
            HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
        ));
        return HandlerResult::PushOnly;
    }

    let Ok(request) = HeroCombineRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "hero combination request is invalid",
        ));
    };
    let hero_id = request.main_id;
    if hero_id == 0
        || !account
            .dock
            .heroes
            .values()
            .any(|hero| hero.id.get() == hero_id)
    {
        return HandlerResult::Error(GameError::NotFound("hero"));
    }
    if method == "hero.HeroCombineUpLv" || method == "hero.HeroCombineQuickLevelUp" {
        let mut level = typed_combination_value(account, hero_id, "level");
        if level >= 100 {
            return HandlerResult::Error(GameError::InvalidState(
                "hero combination level is already maxed",
            ));
        }
        let max_steps = if method == "hero.HeroCombineQuickLevelUp" {
            100
        } else {
            1
        };
        let mut changed = 0;
        while changed < max_steps && level < 100 {
            let Some(costs) = typed_combination_rule(account, hero_id, catalog, level + 1)
                .map(|rule| rule.levelup_costs.clone())
            else {
                break;
            };
            if !typed_combination_consume(account, &costs) {
                break;
            }
            level += 1;
            changed += 1;
        }
        if changed == 0 {
            return HandlerResult::Error(GameError::InvalidState(
                "hero combination level-up cost is insufficient",
            ));
        }
        set_typed_combination_value(account, hero_id, "level", level);
        effects.push_pre(Response::raw(
            "hero.UpdateHeroBagData",
            HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
        ));
        effects.push_pre(Response::raw(
            "bag.UpdateBagData",
            BagInfoCodec::encode(&bag_info_from_typed_account(account)),
        ));
        return HandlerResult::PushOnly;
    }

    let level = typed_combination_value(account, hero_id, "level");
    let grade = typed_combination_value(account, hero_id, "grade");
    let Some((break_costs, level_end, next_id)) =
        typed_combination_rule(account, hero_id, catalog, level)
            .map(|rule| (rule.break_costs.clone(), rule.level_end, rule.next_id))
    else {
        return HandlerResult::Error(GameError::InvalidState(
            "hero combination break configuration was not found",
        ));
    };
    let Some(next_star) = catalog.rules_by_id.get(&next_id).map(|rule| rule.star) else {
        return HandlerResult::Error(GameError::InvalidState(
            "hero combination is already at final stage",
        ));
    };
    if level < u64::try_from(level_end.max(0)).unwrap_or_default()
        || grade >= u64::try_from(next_star.max(0)).unwrap_or_default()
        || !typed_combination_consume(account, &break_costs)
    {
        return HandlerResult::Error(GameError::InvalidState(
            "hero combination break requirement is not met",
        ));
    }
    set_typed_combination_value(account, hero_id, "grade", next_star.max(0) as u64);
    effects.push_pre(Response::raw(
        "hero.UpdateHeroBagData",
        HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
    ));
    effects.push_pre(Response::raw(
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_typed_account(account)),
    ));
    HandlerResult::PushOnly
}
