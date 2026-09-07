use serde_json::Value;

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

#[derive(Clone, Copy)]
pub(super) struct CommerceTypedCatalogs<'a> {
    pub(super) shop: Option<&'a ShopCatalog>,
}

pub(super) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    state: &ServerState,
    method: &str,
    request_args: &[u8],
    pre_pushes: &mut Vec<Vec<u8>>,
    catalogs: CommerceTypedCatalogs<'_>,
) -> HandlerResult {
    match method {
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
                Some(payload) => HandlerResult::Reply(Response::raw(method, payload)),
                None => HandlerResult::Error(GameError::InvalidRequest("shop was not found")),
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
            let Some(reward) = apply_typed_shop_good(account, good, request.buy_num) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "shop goods could not be granted",
                ));
            };
            append_typed_shop_pushes(pre_pushes, state, account, reward);
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
                if let Some(reward) = apply_typed_shop_good(account, good, 1) {
                    append_typed_shop_pushes(pre_pushes, state, account, reward);
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
            append_method_push(
                pre_pushes,
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            );
            append_method_push(
                pre_pushes,
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
            );
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
            append_method_push(
                pre_pushes,
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            );
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

fn apply_typed_shop_good(
    account: &mut blueoath_domain::AccountState,
    good: &ShopGood,
    buy_num: i32,
) -> Option<ShopReward> {
    let buy_num = buy_num.max(1);
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
                account.dock.heroes.insert(
                    id,
                    blueoath_domain::HeroState {
                        id,
                        template_id,
                        name: String::new(),
                        change_name_time: 0,
                        level: 1,
                        exp: 0,
                        mood: 100,
                        affection: 500_000,
                        hp: 10_000_000_000,
                        locked: false,
                        equip_slots: vec![None; 6],
                        pskills: std::collections::BTreeMap::new(),
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
        5 => {
            let kind = typed_currency_kind(good.item_id)?;
            account
                .resources
                .credit(kind, u64::try_from(total).ok()?)
                .ok()?;
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
            let Some(kind) = typed_currency_kind(item_id) else {
                return false;
            };
            if account.resources.amount(kind).get() < amount {
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
            let Some(kind) = typed_currency_kind(item_id) else {
                return false;
            };
            if account.resources.debit(kind, amount).is_err() {
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
    pushes: &mut Vec<Vec<u8>>,
    state: &ServerState,
    account: &blueoath_domain::AccountState,
    reward: ShopReward,
) {
    append_method_push(
        pushes,
        "user.UpdateUserInfo",
        UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
    );
    append_method_push(
        pushes,
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_typed_account(account)),
    );
    if reward.goods_type == 2 {
        append_method_push(
            pushes,
            "equip.UpdateEquipBagData",
            EquipListCodec::encode(&equip_list_from_typed_account(account)),
        );
    } else if reward.goods_type == 3 {
        append_method_push(
            pushes,
            "hero.UpdateHeroBagData",
            HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
        );
    }
}

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let state = context.state;
    let account = &mut *context.account;
    let catalogs = context.catalogs;
    let GameLoginCatalogs {
        fashion: fashion_catalog,
        equip: equip_catalog,
        handbook_behaviours,
        shop: shop_catalog,
        tasks: task_catalog,
        ..
    } = catalogs;
    let account_view = account.as_deref();
    let pre_pushes = &mut *context.pre_pushes;

    match method {
        "fashion.Equip" => {
            let fashion_tid = decode_varint_field(request_args, 1);
            let equip_status = decode_varint_field(request_args, 2);
            let hero_id = decode_varint_u64_field(request_args, 3);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            {
                match fashion_equip_state(
                    account,
                    fashion_catalog,
                    hero_id,
                    fashion_tid,
                    equip_status,
                ) {
                    Ok(()) => {
                        append_method_push(
                            pre_pushes,
                            "hero.UpdateHeroBagData",
                            encode_hero_intensify_payload(account, hero_id, &[]),
                        );
                        HandlerResult::PushOnly
                    }
                    Err(error) => invalid(error),
                }
            }
        }
        "shop.BuyGoods" => {
            let shop_id = decode_varint_field(request_args, 1);
            let good_id = decode_varint_field(request_args, 2);
            let requested_buy_num = decode_varint_field(request_args, 3);
            let buy_num = requested_buy_num.max(1);
            if buy_num > MAX_SHOP_BUY_NUM {
                invalid("buy quantity exceeds limit")
            } else {
                let Some(good) = shop_catalog
                    .and_then(|catalog| catalog.goods_by_id.get(&good_id))
                    .filter(|good| good.shop_id == shop_id)
                else {
                    return invalid("shop goods were not found");
                };
                let Some(account) = account.as_deref_mut() else {
                    return HandlerResult::Error(GameError::AccountUnavailable);
                };
                if let Some(reward) = apply_shop_good(
                    account,
                    good,
                    buy_num,
                    current_unix_seconds(),
                    fashion_catalog,
                ) {
                    advance_task_event(account, task_catalog, 401, 1, current_unix_seconds());
                    append_method_push(
                        pre_pushes,
                        "task.TaskInfo",
                        task_info_payload(account, task_catalog),
                    );
                    append_shop_update_pushes(
                        pre_pushes,
                        state,
                        account,
                        reward,
                        fashion_catalog,
                        equip_catalog,
                    );
                    reply(
                        method,
                        return_shop_buy_response(good_id, buy_num, Some(reward)),
                    )
                } else {
                    invalid("shop goods could not be granted")
                }
            }
        }
        "shop.QualityBuyGoods" => {
            let shop_id = decode_varint_field(request_args, 1);
            let good_ids = decode_repeated_varint_field(request_args, 2);
            if good_ids.len() > MAX_SHOP_BATCH_SIZE {
                invalid("purchase list exceeds limit")
            } else {
                let Some(account) = account.as_deref_mut() else {
                    return HandlerResult::Error(GameError::AccountUnavailable);
                };
                let mut rewards = Vec::new();
                for good_id in &good_ids {
                    let Some(good) = shop_catalog
                        .and_then(|catalog| catalog.goods_by_id.get(good_id))
                        .filter(|good| good.shop_id == shop_id)
                    else {
                        continue;
                    };
                    if let Some(reward) =
                        apply_shop_good(account, good, 1, current_unix_seconds(), fashion_catalog)
                    {
                        advance_task_event(account, task_catalog, 401, 1, current_unix_seconds());
                        append_method_push(
                            pre_pushes,
                            "task.TaskInfo",
                            task_info_payload(account, task_catalog),
                        );
                        append_shop_update_pushes(
                            pre_pushes,
                            state,
                            account,
                            reward,
                            fashion_catalog,
                            equip_catalog,
                        );
                        rewards.push(reward);
                    }
                }
                if rewards.is_empty() {
                    invalid("shop goods were not found")
                } else {
                    reply(
                        method,
                        encode_quality_buy_goods_response(&rewards, &good_ids),
                    )
                }
            }
        }
        "shop.GetShopsInfo" => reply(method, shop_info_payload(shop_catalog)),
        "shop.RefreshShop" => {
            let shop_id = decode_varint_field(request_args, 1);
            if let Some(payload) = shop_refresh_payload(shop_catalog, shop_id) {
                reply(method, payload)
            } else {
                invalid("shop was not found")
            }
        }
        "shop.UpdateShopInfo" => reply(method, shop_info_payload(shop_catalog)),
        "bag.GetBagInfo" => reply(
            method,
            BagInfoCodec::encode(&bag_info_from_account(account_view.unwrap_or(&Value::Null))),
        ),
        "bag.SaleBagItem" => {
            let sale_items = decode_repeated_message_field(request_args, 1)
                .into_iter()
                .map(|item| (decode_varint_field(&item, 1), decode_varint_field(&item, 2)))
                .filter(|(template_id, count)| *template_id > 0 && *count > 0)
                .collect::<Vec<_>>();
            if sale_items.is_empty() {
                invalid("sale item list is empty")
            } else {
                let Some(account) = account.as_deref_mut() else {
                    return HandlerResult::Error(GameError::AccountUnavailable);
                };
                if sale_items.iter().any(|(template_id, count)| {
                    bag_item_count(account, *template_id) < i64::from(*count)
                }) {
                    invalid("not enough sale items")
                } else {
                    let total = sale_items.iter().fold(0i32, |total, (template_id, count)| {
                        consume_bag_item(account, *template_id, *count);
                        total.saturating_add(*count)
                    });
                    add_character_i64(account, "gold", total);
                    append_method_push(
                        pre_pushes,
                        "bag.UpdateBagData",
                        BagInfoCodec::encode(&bag_info_from_account(account)),
                    );
                    append_method_push(
                        pre_pushes,
                        "user.UpdateUserInfo",
                        UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                    );
                    reply(
                        method,
                        encode_rewards_list(&[ShopReward {
                            goods_type: 5,
                            item_id: 1,
                            num: total,
                            instance_id: 0,
                        }]),
                    )
                }
            }
        }
        "bag.CompositeItem" => {
            let template_id = decode_varint_field(request_args, 1);
            let requested = decode_varint_field(request_args, 2);
            if template_id <= 0 || requested <= 0 {
                invalid("composite item arguments are invalid")
            } else {
                let Some(account) = account.as_deref_mut() else {
                    return HandlerResult::Error(GameError::AccountUnavailable);
                };
                let consumed = consume_bag_item(account, template_id, requested);
                if consumed != requested {
                    invalid("not enough composite items")
                } else {
                    account["lastComposite"] = serde_json::json!({
                        "templateId": template_id,
                        "num": requested,
                        "time": current_unix_seconds(),
                    });
                    append_method_push(
                        pre_pushes,
                        "bag.UpdateBagData",
                        BagInfoCodec::encode(&bag_info_from_account(account)),
                    );
                    HandlerResult::PushOnly
                }
            }
        }
        "recharge.GetRechargeInfo" | "recharge.RechargeInfo" => reply(
            method,
            recharge_info_payload(account_view.unwrap_or(&Value::Null), RECHARGE_CATALOG.get()),
        ),
        "recharge.FreeReward" => {
            let recharge_id = decode_varint_field(request_args, 1);
            redeem_recharge(context, recharge_id, fashion_catalog, false)
        }
        "recharge.GetPaybackReward" => {
            if let Some(account) = account.as_deref_mut() {
                account["lastPaybackRequest"] = serde_json::json!({
                    "goldNum": decode_varint_field(request_args, 1),
                    "cardNum": decode_varint_field(request_args, 2),
                    "time": current_unix_seconds(),
                });
            }
            HandlerResult::PushOnly
        }
        "recharge.DirectBuyItem" => {
            let recharge_id = decode_varint_field(request_args, 1);
            redeem_recharge(context, recharge_id, fashion_catalog, true)
        }
        "recharge.DirectBuySelectItem" => {
            let recharge_id =
                decode_varint_field(request_args, 2).max(decode_varint_field(request_args, 1));
            redeem_recharge(context, recharge_id, fashion_catalog, true)
        }
        "recharge.Buy" | "recharge.BuyGoods" | "recharge.Purchase" | "shop.BuyRecharge" => {
            // Single-player mode has no payment provider. Treat recharge card click as a
            // local grant using client recharge/reward catalogs; this includes lucky-bag
            // currency and bundled bag items, with authoritative pushes before callback.
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            {
                let configured = RECHARGE_CATALOG.get().and_then(|catalog| {
                    // Client builds have used different protobuf field numbers for
                    // recharge product id. Resolve first catalog hit, preserving
                    // compatibility without trusting an unknown product id.
                    [1, 2, 3]
                        .into_iter()
                        .map(|field| decode_varint_field(request_args, field))
                        .find_map(|recharge_id| {
                            catalog.rewards_by_recharge_id.get(&recharge_id).cloned()
                        })
                });
                if let Some(configured) = configured {
                    let now = current_unix_seconds();
                    let rewards = configured
                        .into_iter()
                        .map(|reward| grant_reward(account, reward, now, fashion_catalog))
                        .collect::<Vec<_>>();
                    append_method_push(
                        pre_pushes,
                        "user.UpdateUserInfo",
                        UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                    );
                    append_method_push(
                        pre_pushes,
                        "bag.UpdateBagData",
                        BagInfoCodec::encode(&bag_info_from_account(account)),
                    );
                    append_method_push(
                        pre_pushes,
                        "hero.UpdateHeroBagData",
                        HeroBagCodec::encode(&hero_bag_from_account(account)),
                    );
                    if let Some(payload) =
                        illustrate_info_payload_for_rewards(&rewards, handbook_behaviours)
                    {
                        append_method_push(pre_pushes, "illustrate.IllustrateInfo", payload);
                    }
                    reply(method, encode_rewards_list(&rewards))
                } else {
                    invalid("recharge product was not found")
                }
            }
        }
        "fashion.updateData" => reply(
            method,
            FashionListCodec::encode(&fashion_list_from_account(
                account_view.unwrap_or(&Value::Null),
                fashion_catalog,
            )),
        ),
        _ => HandlerResult::Empty,
    }
}

fn recharge_info_payload(account: &Value, catalog: Option<&RechargeCatalog>) -> Vec<u8> {
    let mut output = Vec::new();
    let purchases = account.get("rechargePurchases");
    if let Some(catalog) = catalog {
        for recharge_id in catalog.rewards_by_recharge_id.keys() {
            let buy_times = purchases
                .and_then(|value| value.get(recharge_id.to_string()))
                .and_then(Value::as_i64)
                .unwrap_or_default();
            let mut item = Vec::new();
            append_varint_field(&mut item, 1, (*recharge_id).max(0) as u64);
            append_varint_field(&mut item, 2, buy_times.max(0) as u64);
            append_varint_field(&mut item, 3, 1);
            append_varint_field(&mut item, 4, 0);
            append_varint_field(&mut item, 5, 0);
            append_message_field(&mut output, 3, &item);
        }
    }
    append_varint_field(
        &mut output,
        4,
        account
            .get("rechargeTotal")
            .and_then(Value::as_i64)
            .unwrap_or_default()
            .max(0) as u64,
    );
    output
}

fn redeem_recharge<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    recharge_id: i32,
    fashion_catalog: Option<&FashionList>,
    count_purchase: bool,
) -> HandlerResult {
    let Some(configured) = RECHARGE_CATALOG
        .get()
        .and_then(|catalog| catalog.rewards_by_recharge_id.get(&recharge_id))
        .cloned()
    else {
        return invalid("recharge product was not found");
    };
    let Some(account) = context.account.as_deref_mut() else {
        return HandlerResult::Error(GameError::AccountUnavailable);
    };
    let now = current_unix_seconds();
    let rewards = configured
        .into_iter()
        .map(|reward| grant_reward(account, reward, now, fashion_catalog))
        .collect::<Vec<_>>();
    if count_purchase {
        let purchases = account
            .as_object_mut()
            .expect("account must be an object")
            .entry("rechargePurchases".to_owned())
            .or_insert_with(|| serde_json::json!({}));
        let key = recharge_id.to_string();
        let old = purchases
            .get(&key)
            .and_then(Value::as_i64)
            .unwrap_or_default();
        purchases[&key] = serde_json::json!(old.saturating_add(1));
        let total = account
            .get("rechargeTotal")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        account["rechargeTotal"] = serde_json::json!(total.saturating_add(1));
    }
    append_method_push(
        context.pre_pushes,
        "user.UpdateUserInfo",
        UserInfoCodec::encode(&user_info_from_account(context.state, Some(account))),
    );
    append_method_push(
        context.pre_pushes,
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_account(account)),
    );
    append_method_push(
        context.pre_pushes,
        "recharge.RechargeInfo",
        recharge_info_payload(account, RECHARGE_CATALOG.get()),
    );
    reply(
        if count_purchase {
            "recharge.DirectBuyItem"
        } else {
            "recharge.FreeReward"
        },
        encode_rewards_list(&rewards),
    )
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn invalid(message: &'static str) -> HandlerResult {
    HandlerResult::Error(GameError::InvalidRequest(message))
}

#[cfg(test)]
mod tests {
    use crate::common::response::HandlerResult;

    use super::*;

    #[test]
    fn handler_exposes_typed_result() {
        let _: for<'state, 'account, 'scratch> fn(
            &mut GameLoginRequestContext<'state, 'account, 'scratch>,
            &str,
            &[u8],
        ) -> HandlerResult = handle;
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
        let mut pushes = Vec::new();
        let result = handle_typed(
            &mut account,
            &state,
            "shop.BuyGoods",
            &args,
            &mut pushes,
            CommerceTypedCatalogs {
                shop: Some(&catalog),
            },
        );
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(
            account
                .resources
                .amount(blueoath_domain::CurrencyKind::Gold)
                .get(),
            70
        );
        assert_eq!(
            account.inventory.items[&blueoath_domain::TemplateId::new(30_001).unwrap()],
            6
        );
        assert_eq!(pushes.len(), 2);
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
            &mut Vec::new(),
            CommerceTypedCatalogs { shop: None },
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
            3
        );
    }
}
