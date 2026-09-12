//! Compatibility handlers for illustration and vow protocols.

use super::super::*;
use super::consume_typed_item;
use super::treasure::grant_typed_treasure_reward;
use crate::common::error::GameError;
use crate::common::response::{HandlerResult, Response, ResponseEffects};

/// 处理英雄图鉴查询、激活和奖励领取请求。
pub(super) fn handle(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
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
    HandlerResult::Empty
}
