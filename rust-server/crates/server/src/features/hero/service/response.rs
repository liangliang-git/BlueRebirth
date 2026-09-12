//! Response synchronization for typed hero operations.

use super::super::*;
use crate::common::response::{Response, ResponseEffects};
/// 将英雄完整状态变化编码并追加到响应效果队列。
pub(super) fn push_hero_changes(
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

/// 将英雄局部字段变化编码并追加到响应效果队列。
pub(super) fn push_hero_delta_changes(
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
