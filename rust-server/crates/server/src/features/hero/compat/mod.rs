//! Transitional feature boundary for low-traffic protocol methods.
//!
//! New routes must not be added here. Existing methods move into typed feature
//! modules as their request and state models are completed.

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;

mod combination;
mod illustration;
mod relationship;
mod treasure;

/// 扣除兼容协议使用的模板道具并维护库存状态。
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

/// 生成客户端缓存刷新所需的兼容响应 payload。
fn cache_data_payload() -> Vec<u8> {
    let mut payload = Vec::new();
    append_bytes_field(&mut payload, 1, b"local");
    payload
}

/// 生成头像购买次数响应 payload。
fn head_buy_count_payload() -> Vec<u8> {
    vec![0x08, 0x00, 0x10, 0x00]
}

/// 判断兼容协议请求是否属于当前英雄领域。
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

/// 分发图鉴、关系、组合和藏宝图等兼容英雄请求。
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
    if matches!(
        method,
        "illustrate.VowHero"
            | "illustrate.VowDecTime"
            | "illustrate.IllustrateNew"
            | "illustrate.AddBehaviour"
            | "illustrate.EquipNew"
            | "illustrate.ModiVowHeroList"
    ) {
        return illustration::handle(account, method, request_args, effects);
    }
    if matches!(
        method,
        "hero.HeroCombine"
            | "hero.HeroCombineUpLv"
            | "hero.HeroCombineQuickLevelUp"
            | "hero.HeroCombineBreak"
    ) {
        return combination::handle_typed_combination(
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
        return treasure::handle_typed_treasure(account, method, request_args, effects);
    }
    if matches!(
        method,
        "hero.Marry" | "hero.AddAffection" | "repair.RepairHero"
    ) {
        return relationship::handle(
            state,
            account,
            method,
            request_args,
            affection_catalog,
            effects,
        );
    }
    HandlerResult::Error(GameError::InvalidRequest(
        "compat feature method is unsupported",
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
        account
            .inventory
            .items
            .insert(blueoath_domain::TemplateId::new(10_180_u64).unwrap(), 1);
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
            ..AffectionCatalog::default()
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
        assert!(treasure::grant_typed_treasure_reward(
            &mut account,
            &mut ship_reward
        ));
        assert_eq!(account.dock.heroes.len(), 2);
        assert!(ship_reward.instance_id > 0);

        let mut equip_reward = ShopReward {
            goods_type: 2,
            item_id: 30_000_001,
            num: 1,
            instance_id: 0,
        };
        assert!(treasure::grant_typed_treasure_reward(
            &mut account,
            &mut equip_reward
        ));
        assert_eq!(account.dock.equipments.len(), 3);

        let mut fashion_reward = ShopReward {
            goods_type: 18,
            item_id: 40_000_001,
            num: 1,
            instance_id: 0,
        };
        assert!(treasure::grant_typed_treasure_reward(
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
