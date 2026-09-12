//! Compatibility handlers for treasure opening and reward delivery.

use super::super::*;
use super::consume_typed_item;
use crate::common::error::GameError;
use crate::common::response::{HandlerResult, Response, ResponseEffects};

/// 将藏宝图奖励编码为兼容客户端响应。
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

/// 判断藏宝图奖励是否能映射到强类型账户状态。
fn typed_treasure_reward_supported(reward: &ShopReward) -> bool {
    // Goods type 4 is a nested drop pool, resolved before this check. Other
    // positive types use the same inventory/currency semantics as task/shop
    // rewards, including material types 11/15/24.
    reward.num > 0 && reward.goods_type > 0 && reward.goods_type != 4 && reward.item_id > 0
}

/// 计算新获得英雄所需的下一个可用标识。
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

/// 计算新获得装备所需的下一个可用标识。
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

/// 将藏宝图奖励写入强类型账户状态并生成响应效果。
pub(crate) fn grant_typed_treasure_reward(
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

/// 处理藏宝图开启请求及其奖励发放。
pub(super) fn handle_typed_treasure(
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
