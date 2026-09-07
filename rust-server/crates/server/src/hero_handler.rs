use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;

pub(super) struct HeroTypedCatalogs<'a> {
    pub(super) hero_level: Option<&'a HeroLevelCatalog>,
    pub(super) tasks: Option<&'a TaskCatalog>,
    pub(super) breakdown: Option<&'a HeroBreakdownCatalog>,
    pub(super) ship_exp_multiplier: f64,
}

#[cfg(test)]
impl HeroTypedCatalogs<'static> {
    pub(super) const fn empty() -> Self {
        Self {
            hero_level: None,
            tasks: None,
            breakdown: None,
            ship_exp_multiplier: 1.0,
        }
    }
}

pub(super) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
    catalogs: HeroTypedCatalogs<'_>,
) -> HandlerResult {
    let HeroTypedCatalogs {
        hero_level: hero_level_catalog,
        tasks: task_catalog,
        breakdown: hero_breakdown_catalog,
        ship_exp_multiplier,
    } = catalogs;
    match method {
        "hero.GetHeroInfo" | "hero.GetHeroInfoByHeroIdArray" => {
            HandlerResult::Reply(Response::raw(
                method,
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            ))
        }
        "hero.LockHero" => {
            let Ok(request) = HeroLockRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "hero lock request is invalid",
                ));
            };
            let hero_id = request.hero_id;
            let locked = request.locked;
            let Some(hero_id) = blueoath_domain::HeroId::new(hero_id).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
            };
            let Some(hero) = account.dock.heroes.get_mut(&hero_id) else {
                return HandlerResult::Error(GameError::InvalidRequest("hero was not found"));
            };
            hero.locked = locked;
            effects.push_pre(Response::raw(
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            ));
            HandlerResult::PushOnly
        }
        "hero.ChangeName" => {
            let Ok(request) = HeroChangeNameRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "hero name request is invalid",
                ));
            };
            let hero_id = request.hero_id;
            let Some(hero_id) = blueoath_domain::HeroId::new(hero_id).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
            };
            let Some(hero) = account.dock.heroes.get_mut(&hero_id) else {
                return HandlerResult::Error(GameError::InvalidRequest("hero was not found"));
            };
            hero.name = request.name;
            hero.change_name_time = u64::from(current_unix_seconds());
            effects.push_pre(Response::raw(
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            ));
            HandlerResult::PushOnly
        }
        "hero.RetireHero" => {
            let Some(hero_breakdown_catalog) = hero_breakdown_catalog else {
                return HandlerResult::Empty;
            };
            let Ok(request) = HeroRetireRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "hero retire request is invalid",
                ));
            };
            let raw_ids = request.hero_ids;
            let is_dis_equip = request.dismantle_equipment;
            let mut requested = std::collections::BTreeSet::new();
            for raw_id in raw_ids {
                let Some(hero_id) =
                    blueoath_domain::HeroId::new(u64::try_from(raw_id).unwrap_or_default()).ok()
                else {
                    return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
                };
                if !requested.insert(hero_id) {
                    return HandlerResult::Error(GameError::InvalidRequest(
                        "hero retire request has duplicate hero",
                    ));
                }
            }
            if requested.iter().any(|hero_id| {
                account.character.secretary_id == Some(*hero_id)
                    || account
                        .fleet
                        .fleets
                        .values()
                        .any(|fleet| fleet.members.contains(hero_id))
                    || account
                        .buildings
                        .hero_assignments
                        .values()
                        .any(|hero_ids| hero_ids.contains(hero_id))
            }) {
                return HandlerResult::Error(GameError::InvalidRequest("hero is in use"));
            }
            let retired = requested
                .iter()
                .filter_map(|hero_id| account.dock.heroes.get(hero_id).cloned())
                .collect::<Vec<_>>();
            if retired.is_empty() {
                return HandlerResult::Error(GameError::InvalidRequest("hero was not found"));
            }
            let retired_ids = retired
                .iter()
                .map(|hero| hero.id)
                .collect::<std::collections::BTreeSet<_>>();
            let retired_templates = retired
                .iter()
                .filter_map(|hero| i32::try_from(hero.template_id.get()).ok())
                .collect::<Vec<_>>();
            account
                .dock
                .heroes
                .retain(|hero_id, _| !retired_ids.contains(hero_id));
            if is_dis_equip {
                account.dock.equipments.retain(|_, equipment| {
                    !equipment
                        .hero_id
                        .is_some_and(|hero_id| retired_ids.contains(&hero_id))
                });
            } else {
                for equipment in account.dock.equipments.values_mut() {
                    if equipment
                        .hero_id
                        .is_some_and(|hero_id| retired_ids.contains(&hero_id))
                    {
                        equipment.hero_id = None;
                    }
                }
            }
            for fleet in account.fleet.fleets.values_mut() {
                fleet
                    .members
                    .retain(|hero_id| !retired_ids.contains(hero_id));
            }
            for hero_ids in account.buildings.hero_assignments.values_mut() {
                hero_ids.retain(|hero_id| !retired_ids.contains(hero_id));
            }
            account
                .buildings
                .hero_assignments
                .retain(|_, hero_ids| !hero_ids.is_empty());
            if account
                .character
                .secretary_id
                .is_some_and(|hero_id| retired_ids.contains(&hero_id))
            {
                account.character.secretary_id = account.dock.heroes.keys().next().copied();
            }
            let mut rewards = Vec::new();
            for template_id in retired_templates {
                for &(goods_type, item_id, amount) in hero_breakdown_catalog
                    .rewards_by_template
                    .get(&template_id)
                    .into_iter()
                    .flatten()
                {
                    if amount <= 0 {
                        continue;
                    }
                    if goods_type == 5 {
                        let kind = match item_id {
                            1 => Some(blueoath_domain::CurrencyKind::Gold),
                            2 => Some(blueoath_domain::CurrencyKind::Diamond),
                            5 => Some(blueoath_domain::CurrencyKind::Supply),
                            30 => Some(blueoath_domain::CurrencyKind::PvePoint),
                            _ => None,
                        };
                        if let Some(kind) = kind {
                            let _ = account
                                .resources
                                .credit(kind, u64::try_from(amount).unwrap_or_default());
                        }
                    } else if let Ok(template_id) =
                        blueoath_domain::TemplateId::new(u64::try_from(item_id).unwrap_or_default())
                    {
                        let entry = account.inventory.items.entry(template_id).or_default();
                        *entry = entry.saturating_add(u64::try_from(amount).unwrap_or_default());
                    }
                    rewards.push(ShopReward {
                        goods_type,
                        item_id,
                        num: amount,
                        instance_id: 0,
                    });
                }
            }
            advance_typed_task_event(account, task_catalog, 11, 1);
            let deleted = retired_ids
                .iter()
                .map(|hero_id| HeroGrid {
                    hero_id: u32::try_from(hero_id.get()).unwrap_or(u32::MAX),
                    ..HeroGrid::default()
                })
                .collect::<Vec<_>>();
            effects.push_pre(Response::raw(
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&HeroBag {
                    heroes: deleted,
                    bag_size: 200,
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
            effects.push_pre(Response::raw(
                "task.TaskInfo",
                task_info_payload_from_typed_account(account, task_catalog),
            ));
            HandlerResult::Reply(Response::raw(method, encode_retire_hero_response(&rewards)))
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
        "hero.AddExp" => {
            let Some(hero_level_catalog) = hero_level_catalog else {
                return HandlerResult::Empty;
            };
            let Ok(request) = HeroAddExpRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "hero experience request is invalid",
                ));
            };
            let Some(hero_id) = blueoath_domain::HeroId::new(request.hero_id).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
            };
            let items = request
                .items
                .iter()
                .map(|item| (item.template_id, item.amount))
                .collect::<Vec<_>>();
            if !account.dock.heroes.contains_key(&hero_id) {
                return HandlerResult::Error(GameError::InvalidRequest("hero was not found"));
            }
            let plan = items
                .iter()
                .filter_map(|(item_id, requested)| {
                    let item_id =
                        blueoath_domain::TemplateId::new(u64::try_from(*item_id).ok()?).ok()?;
                    let raw_item_id = i32::try_from(item_id.get()).ok()?;
                    let per_item = *hero_level_catalog.exp_per_item.get(&raw_item_id)?;
                    let requested = u64::try_from((*requested).clamp(0, 1_000_000)).ok()?;
                    let available = account
                        .inventory
                        .items
                        .get(&item_id)
                        .copied()
                        .unwrap_or_default();
                    let amount = requested.min(available);
                    (per_item > 0 && amount > 0).then_some((item_id, amount, i64::from(per_item)))
                })
                .collect::<Vec<_>>();
            if plan.is_empty() {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "experience items were not found",
                ));
            }
            let total_exp = plan.iter().fold(0i64, |total, (_, amount, per_item)| {
                total.saturating_add(
                    i64::try_from(*amount)
                        .unwrap_or(i64::MAX)
                        .saturating_mul(*per_item),
                )
            });
            for (item_id, amount, _) in &plan {
                if let Some(available) = account.inventory.items.get_mut(item_id) {
                    *available = available.saturating_sub(*amount);
                }
            }
            let boosted = scale_reward(total_exp, ship_exp_multiplier);
            let (level, exp) = account
                .dock
                .heroes
                .get(&hero_id)
                .map(|hero| (hero.level.max(1), hero.exp))
                .unwrap_or((1, 0));
            let mut level = level;
            let mut exp = exp.min(u64::from(i32::MAX as u32));
            let mut remaining = u64::try_from(boosted).unwrap_or_default();
            while level < 200 {
                let need = hero_level_catalog
                    .exp_needed
                    .get(&i32::try_from(level).unwrap_or(i32::MAX))
                    .copied()
                    .unwrap_or(500)
                    .max(1) as u64;
                if exp.saturating_add(remaining) < need {
                    exp = exp.saturating_add(remaining);
                    remaining = 0;
                    break;
                }
                remaining = exp.saturating_add(remaining).saturating_sub(need);
                exp = 0;
                level = level.saturating_add(1);
            }
            exp = exp
                .saturating_add(remaining)
                .min(u64::from(i32::MAX as u32));
            if let Some(hero) = account.dock.heroes.get_mut(&hero_id) {
                hero.level = level;
                hero.exp = exp;
            }
            advance_typed_task_event(account, task_catalog, 10, 1);
            effects.push_pre(Response::raw(
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "task.TaskInfo",
                task_info_payload_from_typed_account(account, task_catalog),
            ));
            HandlerResult::Reply(Response::raw(
                method,
                encode_hero_add_exp_response(hero_id.get(), &items),
            ))
        }
        _ => HandlerResult::Empty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_hero_info_reads_normalized_hero_state() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("hero-typed").unwrap(),
            "Captain",
        );
        let hero_id = blueoath_domain::HeroId::new(7).unwrap();
        account.dock.heroes.insert(
            hero_id,
            blueoath_domain::HeroState {
                id: hero_id,
                template_id: blueoath_domain::TemplateId::new(70).unwrap(),
                name: String::new(),
                change_name_time: 0,
                level: 8,
                exp: 9,
                mood: 10,
                affection: 11,
                hp: 12,
                locked: true,
                equip_slots: Vec::new(),
                pskills: std::collections::BTreeMap::new(),
            },
        );
        let result = handle_typed(
            &mut account,
            "hero.GetHeroInfo",
            &[],
            &mut ResponseEffects::default(),
            HeroTypedCatalogs::empty(),
        );
        let HandlerResult::Reply(response) = result else {
            panic!("typed hero info must reply");
        };
        let hero_payload = response.payload;
        assert!(hero_payload.len() > 2);
        assert_eq!(decode_varint_u64_field(&hero_payload, 2), 200);
    }

    #[test]
    fn typed_hero_lock_mutation_updates_normalized_state() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("hero-lock-typed").unwrap(),
            "Captain",
        );
        let hero_id = blueoath_domain::HeroId::new(1).unwrap();
        assert!(account.dock.heroes.get(&hero_id).unwrap().locked);
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_varint_field(&mut args, 2, 0);
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "hero.LockHero",
            &args,
            &mut effects,
            HeroTypedCatalogs::empty(),
        );

        assert!(matches!(result, HandlerResult::PushOnly));
        assert!(!account.dock.heroes.get(&hero_id).unwrap().locked);
        assert_eq!(effects.into_parts().0.len(), 1);
    }

    #[test]
    fn typed_hero_name_mutation_updates_normalized_state() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("hero-name-typed").unwrap(),
            "Captain",
        );
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_bytes_field(&mut args, 2, b"Aegis");
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "hero.ChangeName",
            &args,
            &mut effects,
            HeroTypedCatalogs::empty(),
        );

        assert!(matches!(result, HandlerResult::PushOnly));
        assert_eq!(account.dock.heroes.values().next().unwrap().name, "Aegis");
        assert_eq!(effects.into_parts().0.len(), 1);
    }

    #[test]
    fn typed_hero_add_exp_consumes_inventory_and_updates_level() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("hero-exp-typed").unwrap(),
            "Captain",
        );
        let item_id = blueoath_domain::TemplateId::new(10_182).unwrap();
        let before = account
            .inventory
            .items
            .get(&item_id)
            .copied()
            .unwrap_or_default();
        let mut catalog = HeroLevelCatalog::default();
        catalog.exp_per_item.insert(10_182, 600);
        catalog.exp_needed.insert(1, 500);
        let mut item = Vec::new();
        append_varint_field(&mut item, 2, 10_182);
        append_varint_field(&mut item, 3, 1);
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_bytes_field(&mut args, 2, &item);
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "hero.AddExp",
            &args,
            &mut effects,
            HeroTypedCatalogs {
                hero_level: Some(&catalog),
                tasks: None,
                breakdown: None,
                ship_exp_multiplier: 1.0,
            },
        );

        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(account.inventory.items.get(&item_id), Some(&(before - 1)));
        assert_eq!(account.dock.heroes.values().next().unwrap().level, 2);
        assert_eq!(effects.into_parts().0.len(), 3);
    }

    #[test]
    fn typed_hero_retire_removes_owned_hero_and_grants_breakdown_rewards() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("hero-retire-typed").unwrap(),
            "Captain",
        );
        account.character.secretary_id = None;
        let item_id = blueoath_domain::TemplateId::new(10_182).unwrap();
        let before_items = account
            .inventory
            .items
            .get(&item_id)
            .copied()
            .unwrap_or_default();
        let before_gold = account
            .resources
            .amount(blueoath_domain::CurrencyKind::Gold)
            .get();
        let mut catalog = HeroBreakdownCatalog::default();
        catalog
            .rewards_by_template
            .insert(10_210_511, vec![(1, 10_182, 2), (5, 1, 3)]);
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_varint_field(&mut args, 2, 1);
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "hero.RetireHero",
            &args,
            &mut effects,
            HeroTypedCatalogs {
                hero_level: None,
                tasks: None,
                breakdown: Some(&catalog),
                ship_exp_multiplier: 1.0,
            },
        );

        assert!(matches!(result, HandlerResult::Reply(_)));
        assert!(!account
            .dock
            .heroes
            .contains_key(&blueoath_domain::HeroId::new(1).unwrap()));
        assert!(account.dock.equipments.is_empty());
        assert_eq!(
            account.inventory.items.get(&item_id),
            Some(&(before_items + 2))
        );
        assert_eq!(
            account
                .resources
                .amount(blueoath_domain::CurrencyKind::Gold)
                .get(),
            before_gold + 3
        );
        assert_eq!(effects.into_parts().0.len(), 4);
    }

    #[test]
    fn typed_hero_change_equip_moves_standard_slot_atomically() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("hero-equip-typed").unwrap(),
            "Captain",
        );
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_varint_field(&mut args, 2, 2);
        append_varint_field(&mut args, 3, 1);
        append_varint_field(&mut args, 4, 1);
        let mut effects = ResponseEffects::default();

        let result = handle_typed(
            &mut account,
            "hero.ChangeEquip",
            &args,
            &mut effects,
            HeroTypedCatalogs::empty(),
        );

        let hero = account
            .dock
            .heroes
            .get(&blueoath_domain::HeroId::new(1).unwrap())
            .unwrap();
        assert!(matches!(result, HandlerResult::PushOnly));
        assert_eq!(hero.equip_slots[0], None);
        assert_eq!(
            hero.equip_slots[1],
            Some(blueoath_domain::EquipId::new(1).unwrap())
        );
        assert_eq!(
            account
                .dock
                .equipments
                .get(&blueoath_domain::EquipId::new(1).unwrap())
                .unwrap()
                .hero_id,
            Some(blueoath_domain::HeroId::new(1).unwrap())
        );
        assert_eq!(effects.into_parts().0.len(), 2);
    }
}
