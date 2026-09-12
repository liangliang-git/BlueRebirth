//! Hero equipment association and slot mutation handlers.

use super::super::*;
use crate::common::error::GameError;
use crate::common::response::{HandlerResult, Response, ResponseEffects};
/// 在强类型英雄状态中应用单件装备并维护装备归属。
pub(super) fn apply_typed_hero_equip(
    account: &mut blueoath_domain::AccountState,
    hero_id: u64,
    slot: u64,
    equip_id: u64,
    equip_type: u64,
) -> Result<bool, &'static str> {
    if equip_type != 1 || hero_id == 0 || !(1..=6).contains(&slot) {
        return Err("hero equipment request is invalid");
    }
    let hero_id = blueoath_domain::HeroId::new(hero_id).map_err(|_| "hero id is invalid")?;
    let slot_index = usize::try_from(slot - 1).map_err(|_| "equipment slot is invalid")?;
    let Some(hero) = account.dock.heroes.get(&hero_id) else {
        return Err("hero was not found");
    };
    let old_equip_id = hero.equip_slots.get(slot_index).copied().flatten();
    let new_equip_id = if equip_id == 0 {
        None
    } else {
        Some(blueoath_domain::EquipId::new(equip_id).map_err(|_| "equipment id is invalid")?)
    };
    if new_equip_id == old_equip_id {
        return Ok(false);
    }
    if let Some(new_equip_id) = new_equip_id {
        let Some(equipment) = account.dock.equipments.get(&new_equip_id) else {
            return Err("equipment was not found");
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
            return Err("equipment belongs to another hero");
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
    Ok(true)
}

/// 处理自动装备、自动卸装和换装请求。
pub(super) fn handle(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
    match method {
        "hero.AutoEquip" => {
            let Ok(request) = HeroAutoEquipRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "auto equipment request is invalid",
                ));
            };
            let mut candidate = account.clone();
            for unit in &request.units {
                for equip in &unit.equips {
                    if let Err(error) = apply_typed_hero_equip(
                        &mut candidate,
                        unit.hero_id,
                        equip.index.saturating_add(1),
                        equip.equip_id,
                        request.equip_type,
                    ) {
                        return HandlerResult::Error(GameError::InvalidRequest(error));
                    }
                }
            }
            *account = candidate;
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
        "hero.AutoUnEquip" => {
            let Ok(request) = HeroAutoUnEquipRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "auto unequipment request is invalid",
                ));
            };
            let mut candidate = account.clone();
            for hero_id in &request.hero_ids {
                for slot in 1..=6 {
                    if let Err(error) = apply_typed_hero_equip(
                        &mut candidate,
                        *hero_id,
                        slot,
                        0,
                        request.equip_type,
                    ) {
                        return HandlerResult::Error(GameError::InvalidRequest(error));
                    }
                }
            }
            *account = candidate;
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
        _ => HandlerResult::Empty,
    }
}
