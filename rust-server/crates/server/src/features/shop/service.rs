use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;

#[derive(Clone, Copy)]
pub(crate) struct CommerceTypedCatalogs<'a> {
    pub(crate) shop: Option<&'a ShopCatalog>,
    pub(crate) fashion: Option<&'a FashionList>,
}

pub(crate) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    state: &ServerState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
    catalogs: CommerceTypedCatalogs<'_>,
) -> HandlerResult {
    match method {
        // Local builds do not have a payment provider.  The client still sends
        // this callback when opening/confirming a recharge item; acknowledge it
        // without mutating the account or granting paid goods.
        "recharge.DirectBuyItem" => HandlerResult::Reply(Response::raw(method, Vec::new())),
        "shop.GetShopsInfo" | "shop.UpdateShopInfo" => {
            HandlerResult::Reply(Response::raw(method, shop_info_payload(catalogs.shop)))
        }
        "shop.RefreshShop" => {
            let Ok(request) = ShopRefreshRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "shop refresh request is invalid",
                ));
            };
            match shop_refresh_payload(catalogs.shop, request.shop_id) {
                Ok(payload) => HandlerResult::Reply(Response::raw(method, payload)),
                Err(error) => HandlerResult::Error(GameError::InvalidRequest(error)),
            }
        }
        "shop.BuyGoods" => {
            let Ok(request) = ShopBuyRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "shop buy request is invalid",
                ));
            };
            if request.buy_num > MAX_SHOP_BUY_NUM {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "buy quantity exceeds limit",
                ));
            }
            let Some(good) = catalogs
                .shop
                .and_then(|catalog| catalog.goods_by_id.get(&request.good_id))
                .filter(|good| good.shop_id == request.shop_id)
            else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "shop goods were not found",
                ));
            };
            let Some(reward) =
                apply_typed_shop_good(account, good, request.buy_num, catalogs.fashion)
            else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "shop goods could not be granted",
                ));
            };
            append_typed_shop_pushes(effects, state, account, reward, catalogs.fashion);
            HandlerResult::Reply(Response::raw(
                method,
                return_shop_buy_response(request.good_id, request.buy_num, Some(reward)),
            ))
        }
        "shop.QualityBuyGoods" => {
            let Ok(request) = ShopQualityBuyRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "quality shop buy request is invalid",
                ));
            };
            let mut rewards = Vec::new();
            for good_id in &request.good_ids {
                let Some(good) = catalogs
                    .shop
                    .and_then(|catalog| catalog.goods_by_id.get(good_id))
                    .filter(|good| good.shop_id == request.shop_id)
                else {
                    continue;
                };
                if let Some(reward) = apply_typed_shop_good(account, good, 1, catalogs.fashion) {
                    append_typed_shop_pushes(effects, state, account, reward, catalogs.fashion);
                    rewards.push(reward);
                }
            }
            if rewards.is_empty() {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "shop goods were not found",
                ));
            }
            HandlerResult::Reply(Response::raw(
                method,
                encode_quality_buy_goods_response(&rewards, &request.good_ids),
            ))
        }
        "bag.GetBagInfo" => HandlerResult::Reply(Response::raw(
            method,
            BagInfoCodec::encode(&bag_info_from_typed_account(account)),
        )),
        "bag.SaleBagItem" => {
            let Ok(request) = BagSaleRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "bag sale request is invalid",
                ));
            };
            let mut total = 0_u64;
            for item in &request.items {
                let template_id = blueoath_domain::TemplateId::new(
                    u64::try_from(item.template_id).unwrap_or_default(),
                )
                .ok();
                if template_id
                    .and_then(|id| account.inventory.items.get(&id).copied())
                    .unwrap_or_default()
                    < u64::try_from(item.amount).unwrap_or_default()
                {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "not enough sale items",
                    ));
                }
            }
            for item in &request.items {
                if consume_typed_inventory(
                    account,
                    u64::try_from(item.template_id).unwrap_or_default(),
                    u64::try_from(item.amount).unwrap_or_default(),
                ) {
                    total = total.saturating_add(u64::try_from(item.amount).unwrap_or_default());
                }
            }
            let Ok(total_i64) = i64::try_from(total) else {
                return HandlerResult::Error(GameError::InvalidRequest("sale amount is too large"));
            };
            if account
                .resources
                .credit(
                    blueoath_domain::CurrencyKind::Gold,
                    u64::try_from(total_i64).unwrap_or_default(),
                )
                .is_err()
            {
                return HandlerResult::Error(GameError::InvalidState("gold balance overflow"));
            }
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
            ));
            HandlerResult::Reply(Response::raw(
                method,
                encode_rewards_list(&[ShopReward {
                    goods_type: 5,
                    item_id: 1,
                    num: i32::try_from(total).unwrap_or(i32::MAX),
                    instance_id: 0,
                }]),
            ))
        }
        "bag.CompositeItem" => {
            let Ok(request) = BagCompositeRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "bag composite request is invalid",
                ));
            };
            if !consume_typed_inventory(
                account,
                u64::try_from(request.template_id).unwrap_or_default(),
                u64::try_from(request.amount).unwrap_or_default(),
            ) {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "not enough composite items",
                ));
            }
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            HandlerResult::PushOnly
        }
        _ => HandlerResult::Empty,
    }
}

fn typed_currency_kind(item_id: i32) -> Option<blueoath_domain::CurrencyKind> {
    match item_id {
        1 => Some(blueoath_domain::CurrencyKind::Gold),
        2 => Some(blueoath_domain::CurrencyKind::Diamond),
        5 => Some(blueoath_domain::CurrencyKind::Supply),
        30 => Some(blueoath_domain::CurrencyKind::PvePoint),
        _ => None,
    }
}

fn typed_compat_currency(item_id: i32) -> bool {
    matches!(
        item_id,
        8..=15 | 18 | 22..=29 | 31..=33
    )
}

fn typed_currency_amount(account: &blueoath_domain::AccountState, item_id: i32) -> Option<u64> {
    if let Some(kind) = typed_currency_kind(item_id) {
        return Some(account.resources.amount(kind).get());
    }
    typed_compat_currency(item_id).then(|| {
        account
            .activities
            .progress
            .get(&format!("compat:currency:{item_id}"))
            .copied()
            .unwrap_or_default()
    })
}

fn credit_typed_currency(
    account: &mut blueoath_domain::AccountState,
    item_id: i32,
    amount: u64,
) -> bool {
    if let Some(kind) = typed_currency_kind(item_id) {
        return account.resources.credit(kind, amount).is_ok();
    }
    if !typed_compat_currency(item_id) {
        return false;
    }
    let key = format!("compat:currency:{item_id}");
    let current = account
        .activities
        .progress
        .get(&key)
        .copied()
        .unwrap_or_default();
    let Some(next) = current.checked_add(amount) else {
        return false;
    };
    account.activities.progress.insert(key, next);
    true
}

fn debit_typed_currency(
    account: &mut blueoath_domain::AccountState,
    item_id: i32,
    amount: u64,
) -> bool {
    if let Some(kind) = typed_currency_kind(item_id) {
        return account.resources.debit(kind, amount).is_ok();
    }
    if !typed_compat_currency(item_id) {
        return false;
    }
    let key = format!("compat:currency:{item_id}");
    let current = account
        .activities
        .progress
        .get(&key)
        .copied()
        .unwrap_or_default();
    let Some(next) = current.checked_sub(amount) else {
        return false;
    };
    account.activities.progress.insert(key, next);
    true
}

fn apply_typed_shop_good(
    account: &mut blueoath_domain::AccountState,
    good: &ShopGood,
    buy_num: i32,
    fashion_catalog: Option<&FashionList>,
) -> Option<ShopReward> {
    let buy_num = buy_num.max(1);
    let fashion = if good.goods_type == 18 {
        let fashion_tid =
            blueoath_domain::TemplateId::new(u64::try_from(good.item_id).unwrap_or_default())
                .ok()?;
        let sf_id = fashion_catalog?
            .items
            .iter()
            .find(|item| item.fashion_tids.contains(&good.item_id))
            .map(|item| u64::try_from(item.sf_id).ok())??;
        Some((sf_id, fashion_tid))
    } else {
        None
    };
    if !deduct_typed_shop_costs(account, &good.costs, buy_num) {
        return None;
    }
    let total = good.num.max(1).checked_mul(buy_num)?;
    match good.goods_type {
        2 => {
            let mut last_id = 0;
            for _ in 0..total {
                let id = account
                    .dock
                    .equipments
                    .keys()
                    .map(|value| value.get())
                    .max()
                    .unwrap_or_default()
                    .saturating_add(1);
                let id = blueoath_domain::EquipId::new(id).ok()?;
                let template_id = blueoath_domain::TemplateId::new(
                    u64::try_from(good.item_id).unwrap_or_default(),
                )
                .ok()?;
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
                last_id = i32::try_from(id.get()).ok()?;
            }
            Some(ShopReward {
                goods_type: good.goods_type,
                item_id: good.item_id,
                num: total,
                instance_id: last_id,
            })
        }
        3 => {
            let mut last_id = 0;
            for _ in 0..total {
                let id = account
                    .dock
                    .heroes
                    .keys()
                    .map(|value| value.get())
                    .max()
                    .unwrap_or_default()
                    .saturating_add(1);
                let id = blueoath_domain::HeroId::new(id).ok()?;
                let template_id = blueoath_domain::TemplateId::new(
                    u64::try_from(good.item_id).unwrap_or_default(),
                )
                .ok()?;
                let mut hero = blueoath_domain::HeroState {
                    id,
                    template_id,
                    fashioning: u32::try_from(template_id.get().saturating_sub(1) / 10)
                        .unwrap_or(u32::MAX),
                    name: String::new(),
                    change_name_time: 0,
                    level: 1,
                    exp: 0,
                    mood: blueoath_domain::HERO_MOOD_INITIAL,
                    affection: 500_000,
                    hp: ship_initial_hp_for_template(template_id.get()),
                    locked: false,
                    created_utc: String::new(),
                    equip_slots: vec![None; 6],
                    pskills: std::collections::BTreeMap::new(),
                };
                initialize_typed_hero_loadout_from_catalog(account, &mut hero);
                account.dock.heroes.insert(id, hero);
                last_id = i32::try_from(id.get()).ok()?;
            }
            Some(ShopReward {
                goods_type: good.goods_type,
                item_id: good.item_id,
                num: total,
                instance_id: last_id,
            })
        }
        5 => {
            if !credit_typed_currency(account, good.item_id, u64::try_from(total).ok()?) {
                return None;
            }
            Some(ShopReward {
                goods_type: good.goods_type,
                item_id: good.item_id,
                num: total,
                instance_id: 0,
            })
        }
        18 => {
            let (sf_id, fashion_tid) = fashion?;
            account
                .fashion
                .entries
                .entry(sf_id)
                .or_default()
                .insert(fashion_tid);
            Some(ShopReward {
                goods_type: good.goods_type,
                item_id: good.item_id,
                num: total,
                instance_id: 0,
            })
        }
        _ => {
            let template_id =
                blueoath_domain::TemplateId::new(u64::try_from(good.item_id).unwrap_or_default())
                    .ok()?;
            let entry = account.inventory.items.entry(template_id).or_default();
            *entry = entry.checked_add(u64::try_from(total).ok()?)?;
            Some(ShopReward {
                goods_type: good.goods_type,
                item_id: good.item_id,
                num: total,
                instance_id: 0,
            })
        }
    }
}

fn deduct_typed_shop_costs(
    account: &mut blueoath_domain::AccountState,
    costs: &[ShopCost],
    buy_num: i32,
) -> bool {
    let mut totals = std::collections::BTreeMap::<(i32, i32), u64>::new();
    for cost in costs {
        let amount = u64::try_from(cost.amount)
            .ok()
            .and_then(|value| value.checked_mul(u64::try_from(buy_num.max(1)).ok()?));
        let Some(amount) = amount.filter(|value| *value > 0) else {
            return false;
        };
        let entry = totals.entry((cost.goods_type, cost.item_id)).or_default();
        *entry = entry.checked_add(amount).unwrap_or(u64::MAX);
    }
    for (&(goods_type, item_id), &amount) in &totals {
        if goods_type == 5 {
            let Some(balance) = typed_currency_amount(account, item_id) else {
                return false;
            };
            if balance < amount {
                return false;
            }
        } else if typed_inventory_count(account, u64::try_from(item_id).unwrap_or_default())
            < amount
        {
            return false;
        }
    }
    for (&(goods_type, item_id), &amount) in &totals {
        if goods_type == 5 {
            if !debit_typed_currency(account, item_id, amount) {
                return false;
            }
        } else if !consume_typed_inventory(
            account,
            u64::try_from(item_id).unwrap_or_default(),
            amount,
        ) {
            return false;
        }
    }
    true
}

fn typed_inventory_count(account: &blueoath_domain::AccountState, template_id: u64) -> u64 {
    blueoath_domain::TemplateId::new(template_id)
        .ok()
        .and_then(|id| account.inventory.items.get(&id).copied())
        .unwrap_or_default()
}

fn consume_typed_inventory(
    account: &mut blueoath_domain::AccountState,
    template_id: u64,
    amount: u64,
) -> bool {
    let Some(template_id) = blueoath_domain::TemplateId::new(template_id).ok() else {
        return false;
    };
    let Some(value) = account.inventory.items.get_mut(&template_id) else {
        return false;
    };
    if *value < amount {
        return false;
    }
    *value -= amount;
    true
}

fn append_typed_shop_pushes(
    effects: &mut ResponseEffects,
    state: &ServerState,
    account: &blueoath_domain::AccountState,
    reward: ShopReward,
    fashion_catalog: Option<&FashionList>,
) {
    effects.push_pre(Response::raw(
        "user.UpdateUserInfo",
        UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
    ));
    effects.push_pre(Response::raw(
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_typed_account(account)),
    ));
    if reward.goods_type == 2
        || (reward.goods_type == 14
            && reward.item_id
                == i32::try_from(blueoath_domain::EQUIPMENT_DOCK_EXPANSION_ITEM_ID).unwrap())
    {
        effects.push_pre(Response::raw(
            "equip.UpdateEquipBagData",
            EquipListCodec::encode(&equip_list_from_typed_account(account)),
        ));
    }
    if reward.goods_type == 3
        || (reward.goods_type == 14
            && reward.item_id
                == i32::try_from(blueoath_domain::SHIP_DOCK_EXPANSION_ITEM_ID).unwrap())
    {
        effects.push_pre(Response::raw(
            "hero.UpdateHeroBagData",
            HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
        ));
    }
    if reward.goods_type == 18 {
        effects.push_pre(Response::raw(
            "fashion.updateData",
            FashionListCodec::encode(&fashion_list_from_typed_account(account, fashion_catalog)),
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::common::response::HandlerResult;

    use super::*;

    #[test]
    fn local_recharge_purchase_is_acknowledged_without_payment_side_effects() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("typed-recharge").unwrap(),
            "Captain",
        );
        let before = account.clone();
        let result = handle_typed(
            &mut account,
            &ServerState::new("typed-recharge", "Captain", "1.0.0"),
            "recharge.DirectBuyItem",
            &[0x08, 0x01, 0x10, 0x01],
            &mut ResponseEffects::default(),
            CommerceTypedCatalogs {
                shop: None,
                fashion: None,
            },
        );
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(account, before);
    }

    #[test]
    fn typed_shop_buy_updates_resources_and_inventory() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("typed-shop").unwrap(),
            "Captain",
        );
        account
            .resources
            .credit(blueoath_domain::CurrencyKind::Gold, 100)
            .unwrap();
        let mut catalog = ShopCatalog::default();
        catalog.goods_by_id.insert(
            7,
            ShopGood {
                shop_id: 1,
                goods_type: 1,
                item_id: 30_001,
                num: 2,
                costs: vec![ShopCost {
                    goods_type: 5,
                    item_id: 1,
                    amount: 10,
                }],
            },
        );
        let state = ServerState::new("typed-shop", "Captain", "1.0.0");
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_varint_field(&mut args, 2, 7);
        append_varint_field(&mut args, 3, 3);
        let mut effects = ResponseEffects::default();
        let result = handle_typed(
            &mut account,
            &state,
            "shop.BuyGoods",
            &args,
            &mut effects,
            CommerceTypedCatalogs {
                shop: Some(&catalog),
                fashion: None,
            },
        );
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(
            account
                .resources
                .amount(blueoath_domain::CurrencyKind::Gold)
                .get(),
            99_999_999 + 100 - 30
        );
        assert_eq!(
            account.inventory.items[&blueoath_domain::TemplateId::new(30_001).unwrap()],
            6
        );
        let (pushes, _, error) = effects.into_parts();
        assert_eq!(pushes.len(), 2);
        assert!(error.is_none());
    }

    #[test]
    fn typed_shop_buy_expands_ship_dock_and_pushes_new_capacity() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("typed-shop-dock-expansion").unwrap(),
            "Captain",
        );
        let mut shop = ShopCatalog::default();
        shop.goods_by_id.insert(
            7,
            ShopGood {
                shop_id: 1,
                goods_type: 14,
                item_id: i32::try_from(blueoath_domain::SHIP_DOCK_EXPANSION_ITEM_ID).unwrap(),
                num: 1,
                costs: vec![ShopCost {
                    goods_type: 5,
                    item_id: 2,
                    amount: 1,
                }],
            },
        );
        let state = ServerState::new("typed-shop-dock-expansion", "Captain", "1.0.0");
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_varint_field(&mut args, 2, 7);
        append_varint_field(&mut args, 3, 1);
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            &state,
            "shop.BuyGoods",
            &args,
            &mut effects,
            CommerceTypedCatalogs {
                shop: Some(&shop),
                fashion: None,
            },
        );

        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(account.ship_dock_capacity(), 210);
        let (pushes, _, error) = effects.into_parts();
        assert!(error.is_none());
        assert!(pushes
            .iter()
            .any(|push| push.method == "hero.UpdateHeroBagData"));
    }

    #[test]
    fn typed_shop_buy_unlocks_fashion_and_pushes_owned_list() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("typed-shop-fashion").unwrap(),
            "Captain",
        );
        account
            .resources
            .credit(blueoath_domain::CurrencyKind::Gold, 100)
            .unwrap();
        let mut shop = ShopCatalog::default();
        shop.goods_by_id.insert(
            7,
            ShopGood {
                shop_id: 23,
                goods_type: 18,
                item_id: 1001,
                num: 1,
                costs: vec![ShopCost {
                    goods_type: 5,
                    item_id: 1,
                    amount: 10,
                }],
            },
        );
        let fashion = FashionList {
            items: vec![FashionInfo {
                sf_id: 10,
                fashion_tids: vec![1001],
            }],
        };
        let state = ServerState::new("typed-shop-fashion", "Captain", "1.0.0");
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 23);
        append_varint_field(&mut args, 2, 7);
        append_varint_field(&mut args, 3, 1);
        let mut effects = ResponseEffects::default();
        let result = handle_typed(
            &mut account,
            &state,
            "shop.BuyGoods",
            &args,
            &mut effects,
            CommerceTypedCatalogs {
                shop: Some(&shop),
                fashion: Some(&fashion),
            },
        );

        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(
            account.fashion.entries[&10],
            [blueoath_domain::TemplateId::new(1001).unwrap()]
                .into_iter()
                .collect()
        );
        assert_eq!(
            account
                .resources
                .amount(blueoath_domain::CurrencyKind::Gold)
                .get(),
            99_999_999 + 100 - 10
        );
        assert_eq!(effects.into_parts().0.len(), 3);
    }

    #[test]
    fn typed_shop_buy_uses_compat_currency_and_grants_lucky_bag() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("typed-shop-lucky-bag").unwrap(),
            "Captain",
        );
        account
            .activities
            .progress
            .insert("compat:currency:25".to_owned(), 1_000);
        let good = ShopGood {
            shop_id: 9,
            goods_type: 1,
            item_id: 10_156,
            num: 1,
            costs: vec![ShopCost {
                goods_type: 5,
                item_id: 25,
                amount: 150,
            }],
        };

        let reward = apply_typed_shop_good(&mut account, &good, 1, None)
            .expect("lucky bag purchase should be granted");

        assert_eq!(reward.item_id, 10_156);
        assert_eq!(
            account.inventory.items[&blueoath_domain::TemplateId::new(10_156).unwrap()],
            1
        );
        assert_eq!(
            account.activities.progress.get("compat:currency:25"),
            Some(&850)
        );
    }

    #[test]
    fn typed_shop_buy_grants_compat_currency_reward() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("typed-shop-compat-reward").unwrap(),
            "Captain",
        );
        let good = ShopGood {
            shop_id: 9,
            goods_type: 5,
            item_id: 23,
            num: 12,
            costs: vec![],
        };

        apply_typed_shop_good(&mut account, &good, 1, None)
            .expect("compat currency reward should be granted");

        assert_eq!(
            account.activities.progress.get("compat:currency:23"),
            Some(&12)
        );
    }

    #[test]
    fn typed_bag_sale_consumes_inventory_and_credits_gold() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("typed-sale").unwrap(),
            "Captain",
        );
        account
            .inventory
            .items
            .insert(blueoath_domain::TemplateId::new(30_002).unwrap(), 5);
        let state = ServerState::new("typed-sale", "Captain", "1.0.0");
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, 30_002);
        append_varint_field(&mut item, 2, 3);
        let mut args = Vec::new();
        append_message_field(&mut args, 1, &item);
        let result = handle_typed(
            &mut account,
            &state,
            "bag.SaleBagItem",
            &args,
            &mut ResponseEffects::default(),
            CommerceTypedCatalogs {
                shop: None,
                fashion: None,
            },
        );
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(
            account.inventory.items[&blueoath_domain::TemplateId::new(30_002).unwrap()],
            2
        );
        assert_eq!(
            account
                .resources
                .amount(blueoath_domain::CurrencyKind::Gold)
                .get(),
            99_999_999 + 3
        );
    }
}
