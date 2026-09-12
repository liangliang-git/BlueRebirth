//! Compatibility handlers for marriage, affection, and repair protocols.

use super::super::*;
use super::consume_typed_item;
use crate::common::error::GameError;
use crate::common::response::{HandlerResult, Response, ResponseEffects};

const OATH_RING_TEMPLATE: i32 = 10_180;

/// 处理英雄结婚、好感度增加和修复请求。
pub(super) fn handle(
    state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    affection_catalog: Option<&AffectionCatalog>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    if method == "hero.Marry" {
        let Ok(request) = HeroMarryRequest::decode(request_args) else {
            return HandlerResult::Error(GameError::InvalidRequest("marriage request is invalid"));
        };
        let hero_id = request.hero_id;
        let marry_type = request.marry_type;
        if hero_id == 0 || !(1..=2).contains(&marry_type) {
            return HandlerResult::Error(GameError::InvalidRequest("marriage request is invalid"));
        }
        if !account
            .dock
            .heroes
            .values()
            .any(|hero| hero.id.get() == hero_id)
        {
            return HandlerResult::Error(GameError::NotFound("hero"));
        }
        let marry_time_key = format!("compat:hero:{hero_id}:marryTime");
        if account
            .activities
            .progress
            .get(&marry_time_key)
            .copied()
            .unwrap_or_default()
            > 0
        {
            return HandlerResult::Error(GameError::InvalidState("hero is already married"));
        }
        let Ok(ring) = blueoath_domain::TemplateId::new(OATH_RING_TEMPLATE as u64) else {
            return HandlerResult::Error(GameError::InvalidState("oath ring id is invalid"));
        };
        if account
            .inventory
            .items
            .get(&ring)
            .copied()
            .unwrap_or_default()
            < 1
        {
            return HandlerResult::Error(GameError::InvalidState("oath ring is missing"));
        }
        let now = current_unix_seconds();
        account
            .inventory
            .items
            .entry(ring)
            .and_modify(|count| *count -= 1);
        if account.inventory.items.get(&ring).copied() == Some(0) {
            account.inventory.items.remove(&ring);
        }
        account
            .activities
            .progress
            .insert(marry_time_key, u64::from(now));
        account.activities.progress.insert(
            format!("compat:hero:{hero_id}:marryType"),
            marry_type as u64,
        );
        let married_count_key = "compat:character:marriedNum".to_owned();
        let married_count = account
            .activities
            .progress
            .get(&married_count_key)
            .copied()
            .unwrap_or_default();
        account
            .activities
            .progress
            .insert(married_count_key, married_count.saturating_add(1));
        effects.push_pre(Response::raw(
            "hero.UpdateHeroBagData",
            HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
        ));
        effects.push_pre(Response::raw(
            "user.UpdateUserInfo",
            UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
        ));
        return HandlerResult::PushOnly;
    }
    if method == "hero.AddAffection" {
        let Ok(request) = HeroAffectionRequest::decode(request_args) else {
            return HandlerResult::Error(GameError::InvalidRequest("affection request is invalid"));
        };
        let hero_id = request.hero_id;
        let item_id = request.item_id;
        let requested = request.count.clamp(1, 99) as u64;
        let Some(exp_per_item) = affection_catalog
            .and_then(|catalog| catalog.exp_by_item.get(&item_id))
            .copied()
            .filter(|exp| *exp > 0)
        else {
            return HandlerResult::Error(GameError::InvalidState(
                "affection item configuration was not found",
            ));
        };
        let Some(hero) = account
            .dock
            .heroes
            .values()
            .find(|hero| hero.id.get() == hero_id)
        else {
            return HandlerResult::Error(GameError::NotFound("hero"));
        };
        let marry_time = account
            .activities
            .progress
            .get(&format!("compat:hero:{hero_id}:marryTime"))
            .copied()
            .unwrap_or_default();
        let max_affection = if marry_time > 0 { 2_000_000 } else { 1_000_000 };
        let current = hero.affection.min(max_affection);
        let room = max_affection.saturating_sub(current);
        let available = blueoath_domain::TemplateId::new(item_id.max(0) as u64)
            .ok()
            .and_then(|template_id| account.inventory.items.get(&template_id).copied())
            .unwrap_or_default();
        let count = requested
            .min(available)
            .min(room.div_ceil(u64::from(exp_per_item as u32)));
        if count == 0 {
            return HandlerResult::Error(GameError::InvalidState("affection gift cannot be used"));
        }
        let count_i32 = i32::try_from(count).unwrap_or(i32::MAX);
        if !consume_typed_item(
            account,
            item_id,
            u64::try_from(count_i32).unwrap_or_default(),
        ) {
            return HandlerResult::Error(GameError::InvalidState(
                "affection gift cannot be consumed",
            ));
        }
        let gained = count
            .saturating_mul(u64::from(exp_per_item as u32))
            .min(room);
        if let Some(hero) = account
            .dock
            .heroes
            .values_mut()
            .find(|hero| hero.id.get() == hero_id)
        {
            hero.affection = current.saturating_add(gained);
        }
        effects.push_pre(Response::raw(
            "hero.UpdateHeroBagData",
            HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
        ));
        effects.push_pre(Response::raw(
            "bag.UpdateBagData",
            BagInfoCodec::encode(&bag_info_from_typed_account(account)),
        ));
        let mut output = Vec::new();
        append_varint_field(&mut output, 1, 0);
        append_varint_field(&mut output, 2, hero_id);
        append_varint_field(&mut output, 3, current.saturating_add(gained));
        return HandlerResult::Reply(Response::raw(method, output));
    }
    if method != "repair.RepairHero" {
        return HandlerResult::Error(GameError::InvalidRequest(
            "compat feature method is unsupported",
        ));
    }
    let Ok(request) = PositiveIdListRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest("repair hero list is invalid"));
    };
    let hero_ids = request
        .ids
        .into_iter()
        .filter(|id| *id > 0)
        .collect::<std::collections::BTreeSet<_>>();
    if hero_ids.is_empty() {
        return HandlerResult::Error(GameError::InvalidRequest("repair hero list is empty"));
    }
    let mut total_cost = 0_u64;
    for hero_id in &hero_ids {
        let Some(hero) = account
            .dock
            .heroes
            .values()
            .find(|hero| hero.id.get() == *hero_id)
        else {
            return HandlerResult::Error(GameError::NotFound("hero"));
        };
        let max_hp = ship_max_hp_for_typed_hero(
            hero,
            &account.activities.progress,
            &account.dock.equipments,
            SHIP_STAT_CATALOG.get(),
            EQUIP_CATALOG.get(),
            SHIP_REMOULD_CATALOG.get(),
            state.ship_stat_multiplier,
        );
        if hero.hp >= max_hp {
            continue;
        }
        let template_id = i32::try_from(hero.template_id.get()).unwrap_or_default();
        let fixed_money = SHIP_STAT_CATALOG
            .get()
            .and_then(|catalog| catalog.by_template.get(&template_id))
            .map(|stats| stats.fixed_money.max(0) as u64)
            .unwrap_or_default();
        total_cost = total_cost.saturating_add(fixed_money);
    }
    if account
        .resources
        .amount(blueoath_domain::CurrencyKind::Gold)
        .get()
        < total_cost
    {
        return HandlerResult::Error(GameError::InsufficientResource(
            blueoath_domain::CurrencyKind::Gold,
        ));
    }
    let changed = hero_ids.iter().any(|hero_id| {
        account
            .dock
            .heroes
            .values()
            .find(|hero| hero.id.get() == *hero_id)
            .is_some_and(|hero| {
                hero.hp
                    < ship_max_hp_for_typed_hero(
                        hero,
                        &account.activities.progress,
                        &account.dock.equipments,
                        SHIP_STAT_CATALOG.get(),
                        EQUIP_CATALOG.get(),
                        SHIP_REMOULD_CATALOG.get(),
                        state.ship_stat_multiplier,
                    )
            })
    });
    if total_cost > 0
        && account
            .resources
            .debit(blueoath_domain::CurrencyKind::Gold, total_cost)
            .is_err()
    {
        return HandlerResult::Error(GameError::InsufficientResource(
            blueoath_domain::CurrencyKind::Gold,
        ));
    }
    if changed {
        for hero_id in hero_ids {
            if let Some(hero) = account
                .dock
                .heroes
                .values_mut()
                .find(|hero| hero.id.get() == hero_id)
            {
                hero.hp = ship_max_hp_for_typed_hero(
                    hero,
                    &account.activities.progress,
                    &account.dock.equipments,
                    SHIP_STAT_CATALOG.get(),
                    EQUIP_CATALOG.get(),
                    SHIP_REMOULD_CATALOG.get(),
                    state.ship_stat_multiplier,
                );
            }
        }
        effects.push_pre(Response::raw(
            "hero.UpdateHeroBagData",
            HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
        ));
        effects.push_pre(Response::raw(
            "user.UpdateUserInfo",
            UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
        ));
    }
    HandlerResult::PushOnly
}
