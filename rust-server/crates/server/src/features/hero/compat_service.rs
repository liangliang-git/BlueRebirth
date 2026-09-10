//! Transitional feature boundary for low-traffic protocol methods.
//!
//! New routes must not be added here. Existing methods move into typed feature
//! modules as their request and state models are completed.

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;

const OATH_RING_TEMPLATE: i32 = 10_180;

fn consume_typed_item(
    account: &mut blueoath_domain::AccountState,
    item_id: i32,
    amount: u64,
) -> bool {
    let Some(template_id) = blueoath_domain::TemplateId::new(item_id.max(0) as u64).ok() else {
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

fn cache_data_payload() -> Vec<u8> {
    let mut payload = Vec::new();
    append_bytes_field(&mut payload, 1, b"local");
    payload
}

fn head_buy_count_payload() -> Vec<u8> {
    vec![0x08, 0x00, 0x10, 0x00]
}

fn encode_treasure_response(rewards: &[ShopReward], treasure_id: i32) -> Vec<u8> {
    let mut output = Vec::new();
    for reward in rewards {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, reward.goods_type.max(0) as u64);
        append_varint_field(&mut item, 2, reward.item_id.max(0) as u64);
        append_varint_field(&mut item, 3, reward.num.max(0) as u64);
        if reward.instance_id > 0 {
            append_varint_field(&mut item, 4, reward.instance_id as u64);
        }
        append_message_field(&mut output, 1, &item);
    }
    if treasure_id > 0 {
        append_varint_field(&mut output, 2, treasure_id as u64);
    }
    output
}

pub(crate) fn handles_typed(method: &str) -> bool {
    matches!(
        method,
        "cachedata.CacheData"
            | "user.GetHeadBuyCount"
            | "hero.Marry"
            | "hero.AddAffection"
            | "hero.HeroCombine"
            | "hero.HeroCombineUpLv"
            | "hero.HeroCombineQuickLevelUp"
            | "hero.HeroCombineBreak"
            | "bag.GetNormalTreasureInfo"
            | "bag.GetSelectTreasureInfo"
            | "illustrate.VowHero"
            | "illustrate.VowDecTime"
            | "illustrate.IllustrateNew"
            | "illustrate.AddBehaviour"
            | "illustrate.EquipNew"
            | "illustrate.ModiVowHeroList"
            | "repair.RepairHero"
    )
}

pub(crate) fn handle_typed(
    state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    affection_catalog: Option<&AffectionCatalog>,
    combination_catalog: Option<&CombinationCatalog>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    if method == "cachedata.CacheData" {
        return HandlerResult::Reply(Response::raw(method, cache_data_payload()));
    }
    if method == "user.GetHeadBuyCount" {
        return HandlerResult::Reply(Response::raw(method, head_buy_count_payload()));
    }
    if method == "illustrate.VowDecTime" {
        let Ok(request) = ItemCountListRequest::decode(request_args) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "wish cooldown item list is invalid",
            ));
        };
        let items = request
            .items
            .into_iter()
            .filter_map(|item| Some((i32::try_from(item.item_id).ok()?, item.count)))
            .filter(|(item_id, count)| *item_id > 0 && *count > 0)
            .collect::<Vec<_>>();
        if items.is_empty() {
            return HandlerResult::Error(GameError::InvalidRequest(
                "wish cooldown item list is empty",
            ));
        }
        for (item_id, count) in &items {
            let Some(template_id) = blueoath_domain::TemplateId::new(*item_id as u64).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "wish cooldown item id is invalid",
                ));
            };
            if account
                .inventory
                .items
                .get(&template_id)
                .copied()
                .unwrap_or_default()
                < *count
            {
                return HandlerResult::Error(GameError::InvalidState(
                    "not enough wish cooldown items",
                ));
            }
        }
        for (item_id, count) in items {
            if !consume_typed_item(account, item_id, count) {
                return HandlerResult::Error(GameError::InvalidState(
                    "wish cooldown item cannot be consumed",
                ));
            }
        }
        effects.push_pre(Response::raw(
            "bag.UpdateBagData",
            BagInfoCodec::encode(&bag_info_from_typed_account(account)),
        ));
        let mut output = Vec::new();
        append_varint_field(&mut output, 1, 0);
        append_varint_field(&mut output, 2, 0);
        return HandlerResult::Reply(Response::raw(method, output));
    }
    if method == "illustrate.VowHero" {
        let Ok(request) = PositiveIdListRequest::decode(request_args) else {
            return HandlerResult::Error(GameError::InvalidRequest("wish hero is invalid"));
        };
        let ship_info_id = request.ids.into_iter().find(|id| *id > 0);
        let Some(ship_info_id) = ship_info_id else {
            return HandlerResult::Error(GameError::InvalidRequest("wish hero is invalid"));
        };
        let Some(template_id) = ship_info_id
            .checked_mul(10)
            .and_then(|id| id.checked_add(1))
        else {
            return HandlerResult::Error(GameError::InvalidRequest("wish hero id is invalid"));
        };
        let Ok(template_id) = i32::try_from(template_id) else {
            return HandlerResult::Error(GameError::InvalidRequest("wish hero id is invalid"));
        };
        let mut reward = ShopReward {
            goods_type: 3,
            item_id: template_id,
            num: 1,
            instance_id: 0,
        };
        if !grant_typed_treasure_reward(account, &mut reward) {
            return HandlerResult::Error(GameError::InvalidState("wish hero cannot be granted"));
        }
        effects.push_pre(Response::raw(
            "hero.UpdateHeroBagData",
            HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
        ));
        effects.push_pre(Response::raw(
            "illustrate.IllustrateInfo",
            illustrate_info_payload_for_templates(&[template_id], None),
        ));
        let mut output = Vec::new();
        append_varint_field(&mut output, 1, reward.goods_type as u64);
        append_varint_field(&mut output, 2, reward.item_id as u64);
        append_varint_field(&mut output, 3, reward.num as u64);
        append_varint_field(&mut output, 4, reward.instance_id as u64);
        return HandlerResult::Reply(Response::raw(method, output));
    }
    if method == "illustrate.IllustrateNew" {
        let Ok(request) = PositiveIdListRequest::decode(request_args) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "illustrate id list is invalid",
            ));
        };
        let ids = request
            .ids
            .into_iter()
            .filter(|id| *id > 0)
            .filter_map(|id| i32::try_from(id).ok())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return HandlerResult::Error(GameError::InvalidRequest("illustrate id list is empty"));
        }
        let mut response = Vec::new();
        let mut entries = Vec::new();
        for id in ids {
            account
                .activities
                .progress
                .entry(format!("compat:illustrate:{id}:seen"))
                .or_insert(1);
            entries.push((id, Vec::new()));
        }
        response.extend(illustrate_info_payload_for_entries(&entries));
        return HandlerResult::Reply(Response::raw(method, response));
    }
    if method == "illustrate.AddBehaviour" {
        let Ok(request) = IllustrateBehaviourRequest::decode(request_args) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "illustrate behaviour request is invalid",
            ));
        };
        if request.entries.is_empty() {
            return HandlerResult::Error(GameError::InvalidRequest(
                "illustrate behaviour request is invalid",
            ));
        }
        let mut updated = Vec::new();
        for item in request.entries {
            let Some(illustrate_id) = i32::try_from(item.illustrate_id).ok() else {
                continue;
            };
            if illustrate_id <= 0 {
                continue;
            }
            let behaviours = item
                .behaviours
                .into_iter()
                .filter(|id| *id > 0)
                .filter_map(|id| i32::try_from(id).ok())
                .collect::<std::collections::BTreeSet<_>>();
            for behaviour in &behaviours {
                account.activities.progress.insert(
                    format!("compat:illustrate:{illustrate_id}:behaviour:{behaviour}"),
                    1,
                );
            }
            updated.push((illustrate_id, behaviours.into_iter().collect::<Vec<_>>()));
        }
        if updated.is_empty() {
            return HandlerResult::Error(GameError::InvalidRequest(
                "illustrate behaviour request is invalid",
            ));
        }
        effects.push_pre(Response::raw(
            "illustrate.IllustrateInfo",
            illustrate_info_payload_for_entries(&updated),
        ));
        return HandlerResult::PushOnly;
    }
    if method == "illustrate.EquipNew" {
        let Ok(request) = PositiveIdListRequest::decode(request_args) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "illustrate equipment id list is invalid",
            ));
        };
        let ids = request
            .ids
            .into_iter()
            .filter(|id| *id > 0)
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return HandlerResult::Error(GameError::InvalidRequest(
                "illustrate equipment id list is empty",
            ));
        }
        let now = u64::from(current_unix_seconds());
        let mut output = Vec::new();
        for id in ids {
            account
                .activities
                .progress
                .insert(format!("compat:illustrateEquip:{id}:getTime"), now);
            account
                .activities
                .progress
                .insert(format!("compat:illustrateEquip:{id}:new"), 1);
            let mut item = Vec::new();
            append_varint_field(&mut item, 1, id);
            append_varint_field(&mut item, 2, now);
            append_varint_field(&mut item, 3, 1);
            append_message_field(&mut output, 1, &item);
        }
        append_message_field(&mut output, 9, &[]);
        return HandlerResult::Reply(Response::raw(method, output));
    }
    if method == "illustrate.ModiVowHeroList" {
        let Ok(request) = PositiveIdListRequest::decode(request_args) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "illustrate vow hero list is invalid",
            ));
        };
        let hero_ids = request
            .ids
            .into_iter()
            .filter(|id| *id > 0)
            .collect::<std::collections::BTreeSet<u64>>();
        let template_ids = account
            .dock
            .heroes
            .values()
            .filter(|hero| hero_ids.contains(&hero.id.get()))
            .filter_map(|hero| i32::try_from(hero.template_id.get()).ok())
            .collect::<Vec<_>>();
        for key in account
            .activities
            .progress
            .keys()
            .filter(|key| key.starts_with("compat:illustrate:vow:"))
            .cloned()
            .collect::<Vec<_>>()
        {
            account.activities.progress.remove(&key);
        }
        for hero_id in &hero_ids {
            account
                .activities
                .progress
                .insert(format!("compat:illustrate:vow:{hero_id}"), 1);
        }
        if !template_ids.is_empty() {
            effects.push_pre(Response::raw(
                "illustrate.IllustrateInfo",
                illustrate_info_payload_for_templates(&template_ids, None),
            ));
        }
        return HandlerResult::PushOnly;
    }
    if matches!(
        method,
        "hero.HeroCombine"
            | "hero.HeroCombineUpLv"
            | "hero.HeroCombineQuickLevelUp"
            | "hero.HeroCombineBreak"
    ) {
        return handle_typed_combination(
            state,
            account,
            method,
            request_args,
            combination_catalog,
            effects,
        );
    }
    if matches!(
        method,
        "bag.GetNormalTreasureInfo" | "bag.GetSelectTreasureInfo"
    ) {
        return handle_typed_treasure(account, method, request_args, effects);
    }
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

fn typed_currency_kind(item_id: i32) -> Option<blueoath_domain::CurrencyKind> {
    Some(match item_id {
        1 => blueoath_domain::CurrencyKind::Gold,
        2 => blueoath_domain::CurrencyKind::Diamond,
        5 => blueoath_domain::CurrencyKind::Supply,
        30 => blueoath_domain::CurrencyKind::PvePoint,
        _ => return None,
    })
}

fn handle_typed_combination(
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

fn typed_treasure_reward_supported(reward: &ShopReward) -> bool {
    // Goods type 4 is a nested drop pool, resolved before this check. Other
    // positive types use the same inventory/currency semantics as task/shop
    // rewards, including material types 11/15/24.
    reward.num > 0 && reward.goods_type > 0 && reward.goods_type != 4 && reward.item_id > 0
}

fn typed_next_hero_id(account: &blueoath_domain::AccountState) -> Option<blueoath_domain::HeroId> {
    blueoath_domain::HeroId::new(
        account
            .dock
            .heroes
            .keys()
            .map(|id| id.get())
            .max()
            .unwrap_or_default()
            .checked_add(1)?,
    )
    .ok()
}

fn typed_next_equip_id(
    account: &blueoath_domain::AccountState,
) -> Option<blueoath_domain::EquipId> {
    blueoath_domain::EquipId::new(
        account
            .dock
            .equipments
            .keys()
            .map(|id| id.get())
            .max()
            .unwrap_or_default()
            .checked_add(1)?,
    )
    .ok()
}

fn grant_typed_treasure_reward(
    account: &mut blueoath_domain::AccountState,
    reward: &mut ShopReward,
) -> bool {
    if reward.goods_type == 5 {
        return can_grant_typed_task_reward(account, reward)
            && grant_typed_task_reward(account, reward);
    }
    if matches!(reward.goods_type, 1 | 6) {
        return can_grant_typed_task_reward(account, reward)
            && grant_typed_task_reward(account, reward);
    }
    if reward.goods_type == 18 {
        let Ok(fashion_tid) = blueoath_domain::TemplateId::new(reward.item_id as u64) else {
            return false;
        };
        account
            .fashion
            .entries
            .entry(reward.item_id as u64)
            .or_default()
            .insert(fashion_tid);
        return true;
    }
    if reward.goods_type == 3 {
        let count = usize::try_from(reward.num).unwrap_or_default();
        let mut last_id = None;
        for _ in 0..count {
            let Some(id) = typed_next_hero_id(account) else {
                return false;
            };
            let Ok(template_id) = blueoath_domain::TemplateId::new(reward.item_id as u64) else {
                return false;
            };
            account.dock.heroes.insert(
                id,
                blueoath_domain::HeroState {
                    id,
                    template_id,
                    fashioning: u32::try_from(template_id.get().saturating_sub(1) / 10)
                        .unwrap_or(u32::MAX),
                    name: String::new(),
                    change_name_time: 0,
                    level: 1,
                    exp: 0,
                    mood: 100,
                    affection: 500_000,
                    hp: ship_initial_hp_for_template(template_id.get()),
                    locked: false,
                    created_utc: String::new(),
                    equip_slots: vec![None; 6],
                    pskills: std::collections::BTreeMap::new(),
                },
            );
            last_id = Some(id.get());
        }
        reward.instance_id = i32::try_from(last_id.unwrap_or_default()).unwrap_or(i32::MAX);
        return true;
    }
    if reward.goods_type == 2 {
        let count = usize::try_from(reward.num).unwrap_or_default();
        let Ok(template_id) = blueoath_domain::TemplateId::new(reward.item_id as u64) else {
            return false;
        };
        let mut last_id = None;
        for _ in 0..count {
            let Some(id) = typed_next_equip_id(account) else {
                return false;
            };
            account.dock.equipments.insert(
                id,
                blueoath_domain::EquipmentState {
                    id,
                    template_id,
                    enhance_level: 0,
                    star: 0,
                    enhance_exp: 0,
                    hero_id: None,
                },
            );
            last_id = Some(id.get());
        }
        reward.instance_id = i32::try_from(last_id.unwrap_or_default()).unwrap_or(i32::MAX);
        return true;
    }
    can_grant_typed_task_reward(account, reward) && grant_typed_task_reward(account, reward)
}

fn handle_typed_treasure(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let Ok(request) = TreasureOpenRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest("treasure request is invalid"));
    };
    let treasure_id = request.treasure_id;
    let (open_num, selected_option, drop_id) = if method == "bag.GetNormalTreasureInfo" {
        let catalog = BUILD_SHIP_CATALOG.get_or_init(BuildShipCatalog::default);
        (
            request.count,
            None,
            catalog.treasure_drop_by_item.get(&treasure_id).copied(),
        )
    } else {
        let position = request.position;
        let open_num = request.count.max(1);
        let catalog = BUILD_SHIP_CATALOG.get_or_init(BuildShipCatalog::default);
        let Some(selected) = catalog.selected_treasure_by_item.get(&treasure_id) else {
            return HandlerResult::Error(GameError::CatalogUnavailable);
        };
        let selected_option = if selected.options.is_empty() {
            None
        } else if position <= 0
            || usize::try_from(position)
                .ok()
                .is_none_or(|p| p > selected.options.len())
        {
            return HandlerResult::Error(GameError::InvalidRequest(
                "select treasure position is out of range",
            ));
        } else {
            Some(selected.options[usize::try_from(position - 1).unwrap_or_default()])
        };
        (
            open_num,
            selected_option,
            (selected.drop_id > 0).then_some(selected.drop_id),
        )
    };
    if treasure_id <= 0 || !(1..=99).contains(&open_num) {
        return HandlerResult::Error(GameError::InvalidRequest("treasure id or count is invalid"));
    }
    let drop_id = if selected_option.is_some() {
        0
    } else {
        let Some(drop_id) = drop_id else {
            return HandlerResult::Error(GameError::CatalogUnavailable);
        };
        drop_id
    };
    let Some(treasure_template) = blueoath_domain::TemplateId::new(treasure_id as u64).ok() else {
        return HandlerResult::Error(GameError::InvalidRequest("treasure id is invalid"));
    };
    let consumed_all = account
        .inventory
        .items
        .get(&treasure_template)
        .copied()
        .unwrap_or_default()
        == open_num as u64;
    if account
        .inventory
        .items
        .get(&treasure_template)
        .copied()
        .unwrap_or_default()
        < open_num as u64
    {
        return HandlerResult::Error(GameError::InvalidState("treasure count is insufficient"));
    }
    let catalog = BUILD_SHIP_CATALOG.get_or_init(BuildShipCatalog::default);
    let mut pending = Vec::new();
    for index in 0..open_num {
        let roll = mix_build_draw_roll(
            u64::from(current_unix_millis())
                ^ BUILD_DRAW_SEQUENCE.fetch_add(1, Ordering::Relaxed)
                ^ u64::try_from(index).unwrap_or_default(),
        );
        if let Some((goods_type, item_id, num)) = selected_option {
            let reward = ShopReward {
                goods_type,
                item_id,
                num,
                instance_id: 0,
            };
            if !typed_treasure_reward_supported(&reward) {
                return HandlerResult::Error(GameError::InvalidState(
                    "treasure reward is unsupported",
                ));
            }
            pending.push(reward);
        } else if method == "bag.GetSelectTreasureInfo" {
            let Some((goods_type, item_id, num)) =
                draw_treasure_random_leaf(catalog, drop_id, roll, 0)
            else {
                return HandlerResult::Error(GameError::InvalidState(
                    "selected treasure drop pool is invalid",
                ));
            };
            let reward = ShopReward {
                goods_type,
                item_id,
                num,
                instance_id: 0,
            };
            if !typed_treasure_reward_supported(&reward) {
                return HandlerResult::Error(GameError::InvalidState(
                    "treasure reward is unsupported",
                ));
            }
            pending.push(reward);
        } else {
            let Some(rewards) = draw_treasure_rewards_with_roll(catalog, drop_id, roll) else {
                return HandlerResult::Error(GameError::InvalidState(
                    "treasure drop pool is invalid",
                ));
            };
            for (goods_type, item_id, num) in rewards {
                let reward = ShopReward {
                    goods_type,
                    item_id,
                    num,
                    instance_id: 0,
                };
                if !typed_treasure_reward_supported(&reward) {
                    return HandlerResult::Error(GameError::InvalidState(
                        "treasure reward is unsupported",
                    ));
                }
                pending.push(reward);
            }
        }
    }
    let snapshot = account.clone();
    if !consume_typed_item(account, treasure_id, open_num as u64) {
        return HandlerResult::Error(GameError::InvalidState("treasure count cannot be consumed"));
    }
    for reward in &mut pending {
        if !grant_typed_treasure_reward(account, reward) {
            *account = snapshot;
            return HandlerResult::Error(GameError::InvalidState(
                "treasure reward cannot be granted",
            ));
        }
    }
    let removed_treasure_ids = if consumed_all {
        vec![treasure_id]
    } else {
        Vec::new()
    };
    effects.push_pre(Response::raw(
        "hero.UpdateHeroBagData",
        HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
    ));
    // Treasure page can restore its local bag cache while handling the
    // treasure callback. Send final bag snapshot after callback so granted
    // materials win over that stale client state.
    effects.push_post(Response::raw(
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_typed_account_with_tombstones(
            account,
            &removed_treasure_ids,
        )),
    ));
    effects.push_pre(Response::raw(
        "equip.UpdateEquipBagData",
        EquipListCodec::encode(&equip_list_from_typed_account(account)),
    ));
    HandlerResult::Reply(Response::raw(
        method,
        encode_treasure_response(&pending, treasure_id),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_hero_relationships_use_domain_state() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("compat-typed").unwrap(),
            "Captain",
        );
        account.inventory.items.insert(
            blueoath_domain::TemplateId::new(OATH_RING_TEMPLATE as u64).unwrap(),
            1,
        );
        let state = ServerState::new("compat-typed", "Captain", "test");
        let mut effects = ResponseEffects::default();
        let mut marry = Vec::new();
        append_varint_field(&mut marry, 1, 1);
        append_varint_field(&mut marry, 2, 1);
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "hero.Marry",
                &marry,
                None,
                None,
                &mut effects,
            ),
            HandlerResult::PushOnly
        ));
        assert_eq!(
            account.activities.progress.get("compat:hero:1:marryType"),
            Some(&1)
        );
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "hero.Marry",
                &marry,
                None,
                None,
                &mut effects,
            ),
            HandlerResult::Error(_)
        ));

        let item_id: i32 = 99_999;
        account
            .inventory
            .items
            .insert(blueoath_domain::TemplateId::new(item_id as u64).unwrap(), 2);
        let affection_catalog = AffectionCatalog {
            exp_by_item: [(item_id, 100)].into_iter().collect(),
        };
        let mut gift = Vec::new();
        append_varint_field(&mut gift, 1, 1);
        append_varint_field(&mut gift, 2, item_id as u64);
        append_varint_field(&mut gift, 3, 2);
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "hero.AddAffection",
                &gift,
                Some(&affection_catalog),
                None,
                &mut effects,
            ),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account.dock.heroes.values().next().unwrap().affection,
            500_200
        );
        assert!(!account
            .inventory
            .items
            .contains_key(&blueoath_domain::TemplateId::new(item_id as u64).unwrap()));

        let cooldown_id: i32 = 12_345;
        account.inventory.items.insert(
            blueoath_domain::TemplateId::new(cooldown_id as u64).unwrap(),
            2,
        );
        let mut cooldown_item = Vec::new();
        append_varint_field(&mut cooldown_item, 1, cooldown_id as u64);
        append_varint_field(&mut cooldown_item, 2, 2);
        let mut cooldown = Vec::new();
        append_message_field(&mut cooldown, 1, &cooldown_item);
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "illustrate.VowDecTime",
                &cooldown,
                None,
                None,
                &mut effects,
            ),
            HandlerResult::Reply(_)
        ));
        assert!(!account
            .inventory
            .items
            .contains_key(&blueoath_domain::TemplateId::new(cooldown_id as u64).unwrap()));

        let mut illustrate = Vec::new();
        append_varint_field(&mut illustrate, 1, 7);
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "illustrate.IllustrateNew",
                &illustrate,
                None,
                None,
                &mut effects,
            ),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account.activities.progress.get("compat:illustrate:7:seen"),
            Some(&1)
        );

        let mut behaviour_item = Vec::new();
        append_varint_field(&mut behaviour_item, 1, 7);
        append_varint_field(&mut behaviour_item, 2, 101);
        append_varint_field(&mut behaviour_item, 2, 102);
        let mut behaviour = Vec::new();
        append_message_field(&mut behaviour, 1, &behaviour_item);
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "illustrate.AddBehaviour",
                &behaviour,
                None,
                None,
                &mut effects,
            ),
            HandlerResult::PushOnly
        ));
        assert_eq!(
            account
                .activities
                .progress
                .get("compat:illustrate:7:behaviour:101"),
            Some(&1)
        );

        let mut equip_new = Vec::new();
        append_varint_field(&mut equip_new, 1, 30_000_001);
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "illustrate.EquipNew",
                &equip_new,
                None,
                None,
                &mut effects,
            ),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account
                .activities
                .progress
                .get("compat:illustrateEquip:30000001:new"),
            Some(&1)
        );

        let mut vow_list = Vec::new();
        append_varint_field(&mut vow_list, 1, 1);
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "illustrate.ModiVowHeroList",
                &vow_list,
                None,
                None,
                &mut effects,
            ),
            HandlerResult::PushOnly
        ));
        assert_eq!(
            account.activities.progress.get("compat:illustrate:vow:1"),
            Some(&1)
        );

        let sf_id = i32::try_from(10_210_511_u64 / 10).unwrap();
        let rule_id = sf_id.saturating_mul(100);
        let combination_catalog = CombinationCatalog {
            rules_by_id: [(
                rule_id,
                CombinationRule {
                    level_end: 1,
                    next_id: rule_id,
                    star: 1,
                    ..CombinationRule::default()
                },
            )]
            .into_iter()
            .collect(),
            ..CombinationCatalog::default()
        };
        let mut relation = Vec::new();
        append_varint_field(&mut relation, 1, 1);
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "hero.HeroCombine",
                &relation,
                None,
                Some(&combination_catalog),
                &mut effects,
            ),
            HandlerResult::PushOnly
        ));
        let mut level_up = Vec::new();
        append_varint_field(&mut level_up, 1, 1);
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "hero.HeroCombineUpLv",
                &level_up,
                None,
                Some(&combination_catalog),
                &mut effects,
            ),
            HandlerResult::PushOnly
        ));
        assert_eq!(
            account
                .activities
                .progress
                .get("compat:hero:1:combination:level"),
            Some(&1)
        );

        let mut ship_reward = ShopReward {
            goods_type: 3,
            item_id: 20_000_001,
            num: 1,
            instance_id: 0,
        };
        assert!(grant_typed_treasure_reward(&mut account, &mut ship_reward));
        assert_eq!(account.dock.heroes.len(), 2);
        assert!(ship_reward.instance_id > 0);

        let mut equip_reward = ShopReward {
            goods_type: 2,
            item_id: 30_000_001,
            num: 1,
            instance_id: 0,
        };
        assert!(grant_typed_treasure_reward(&mut account, &mut equip_reward));
        assert_eq!(account.dock.equipments.len(), 3);

        let mut fashion_reward = ShopReward {
            goods_type: 18,
            item_id: 40_000_001,
            num: 1,
            instance_id: 0,
        };
        assert!(grant_typed_treasure_reward(
            &mut account,
            &mut fashion_reward
        ));
        assert!(account.fashion.entries.contains_key(&40_000_001));

        let mut vow = Vec::new();
        append_varint_field(&mut vow, 1, 1_234);
        assert!(matches!(
            handle_typed(
                &state,
                &mut account,
                "illustrate.VowHero",
                &vow,
                None,
                None,
                &mut effects,
            ),
            HandlerResult::Reply(_)
        ));
        assert_eq!(account.dock.heroes.len(), 3);
        let (pre, _, _) = effects.into_parts();
        assert!(pre
            .iter()
            .any(|response| response.method == "illustrate.IllustrateInfo"));
    }
}
