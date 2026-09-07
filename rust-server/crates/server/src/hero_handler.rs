use serde_json::{json, Value};

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
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
    pre_pushes: &mut Vec<Vec<u8>>,
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
            append_method_push(
                pre_pushes,
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            );
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
            append_method_push(
                pre_pushes,
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            );
            HandlerResult::PushOnly
        }
        "hero.RetireHero" => {
            let Some(hero_breakdown_catalog) = hero_breakdown_catalog else {
                return HandlerResult::Empty;
            };
            let raw_ids = decode_repeated_i32_field(request_args, 1);
            let is_dis_equip = decode_varint_u64_field(request_args, 2) != 0;
            if raw_ids.is_empty() || raw_ids.len() > 99 || raw_ids.iter().any(|id| *id <= 0) {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "hero retire request is invalid",
                ));
            }
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
            append_method_push(
                pre_pushes,
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&HeroBag {
                    heroes: deleted,
                    bag_size: 200,
                }),
            );
            append_method_push(
                pre_pushes,
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            );
            append_method_push(
                pre_pushes,
                "equip.UpdateEquipBagData",
                EquipListCodec::encode(&equip_list_from_typed_account(account)),
            );
            append_method_push(
                pre_pushes,
                "task.TaskInfo",
                task_info_payload_from_typed_account(account, task_catalog),
            );
            HandlerResult::Reply(Response::raw(method, encode_retire_hero_response(&rewards)))
        }
        "hero.AddExp" => {
            let Some(hero_level_catalog) = hero_level_catalog else {
                return HandlerResult::Empty;
            };
            let (hero_id, items) = decode_hero_add_exp_request(request_args);
            if hero_id == 0 || items.is_empty() || items.len() > 99 {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "hero experience request is invalid",
                ));
            }
            let Some(hero_id) = blueoath_domain::HeroId::new(hero_id).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
            };
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
            append_method_push(
                pre_pushes,
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            );
            append_method_push(
                pre_pushes,
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            );
            append_method_push(
                pre_pushes,
                "task.TaskInfo",
                task_info_payload_from_typed_account(account, task_catalog),
            );
            HandlerResult::Reply(Response::raw(
                method,
                encode_hero_add_exp_response(hero_id.get(), &items),
            ))
        }
        _ => HandlerResult::Empty,
    }
}

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let payload = handle_legacy(context, method, request_args);
    if let Some(error) = context.handler_error.clone() {
        HandlerResult::Error(error)
    } else {
        match payload {
            Some(payload) => HandlerResult::Reply(Response::raw(method, payload)),
            None => HandlerResult::Empty,
        }
    }
}

fn handle_legacy<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let state = context.state;
    let account = &mut *context.account;
    let catalogs = context.catalogs;
    let GameLoginCatalogs {
        equip: equip_catalog,
        hero_level: hero_level_catalog,
        tasks: task_catalog,
        hero_breakdown: hero_breakdown_catalog,
        ..
    } = catalogs;
    let account_view = account.as_deref();
    let pre_pushes = &mut *context.pre_pushes;
    let post_pushes = &mut *context.post_pushes;
    let handler_error = &mut *context.handler_error;

    match method {
        "hero.GetHeroInfo" | "hero.GetHeroInfoByHeroIdArray" => Some(HeroBagCodec::encode(
            &hero_bag_from_account(account_view.unwrap_or(&Value::Null)),
        )),
        "hero.RetireHero" => {
            let raw_hero_ids = decode_repeated_i32_field(request_args, 1);
            let invalid_id = raw_hero_ids.iter().any(|id| *id <= 0);
            let hero_ids = raw_hero_ids
                .into_iter()
                .map(|id| id as u64)
                .collect::<Vec<_>>();
            let is_dis_equip = decode_varint_u64_field(request_args, 2) != 0;
            if invalid_id {
                *handler_error = Some(GameError::Internal("hero id is invalid".to_owned()));
                Some(Vec::new())
            } else if let Some(account) = account.as_deref_mut() {
                let requested_set = hero_ids
                    .iter()
                    .copied()
                    .collect::<std::collections::HashSet<_>>();
                let retired_templates = account
                    .get("dock")
                    .and_then(|dock| dock.get("heroes"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|hero| {
                        let id = json_u64(hero, "heroId")?;
                        let template = json_i32(hero, "templateId")?;
                        requested_set.contains(&id).then_some(template)
                    })
                    .collect::<Vec<_>>();
                let removed_equip_ids = if is_dis_equip {
                    account
                        .get("equip")
                        .and_then(|equip| equip.get("items"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|item| {
                            let hero_id = json_u64(item, "heroId")?;
                            let equip_id = json_u64(item, "equipId")?;
                            requested_set.contains(&hero_id).then_some(equip_id)
                        })
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                let retired = retire_heroes_state(account, &hero_ids, is_dis_equip);
                let mut response_payload = Vec::new();
                if !retired.is_empty() {
                    advance_task_event(account, task_catalog, 11, 1, current_unix_seconds());
                    let rewards = apply_hero_breakdown_rewards(
                        account,
                        &retired_templates,
                        hero_breakdown_catalog,
                    );
                    append_method_push(
                        pre_pushes,
                        "hero.UpdateHeroBagData",
                        encode_hero_delete_payload(account, &retired),
                    );
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
                        "task.TaskInfo",
                        task_info_payload(account, task_catalog),
                    );
                    let mut equip_push = equip_list_from_account(account, equip_catalog);
                    if is_dis_equip {
                        equip_push
                            .items
                            .extend(removed_equip_ids.into_iter().filter_map(|id| {
                                u32::try_from(id).ok().map(|equip_id| EquipInfo {
                                    equip_id,
                                    template_id: 0,
                                    ..EquipInfo::default()
                                })
                            }));
                    }
                    append_method_push(
                        pre_pushes,
                        "equip.UpdateEquipBagData",
                        EquipListCodec::encode(&equip_push),
                    );
                    response_payload = encode_retire_hero_response(&rewards);
                }
                Some(response_payload)
            } else {
                Some(Vec::new())
            }
        }
        "hero.ChangeName" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let name = decode_string_field(request_args, 2).unwrap_or_default();
            if let Some(account) = account.as_deref_mut() {
                if let Some(hero) = find_hero_mut(account, hero_id) {
                    hero.insert("name".to_owned(), Value::String(name));
                    hero.insert("changeNameTime".to_owned(), json!(current_unix_seconds()));
                    pre_pushes.push(encode_hero_bag_push(account));
                }
            }
            Some(Vec::new())
        }
        "hero.LockHero" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let locked = decode_varint_u64_field(request_args, 2) != 0;
            if let Some(account) = account.as_deref_mut() {
                if let Some(hero) = find_hero_mut(account, hero_id) {
                    hero.insert("lock".to_owned(), json!(locked));
                    pre_pushes.push(encode_hero_bag_push(account));
                }
            }
            Some(Vec::new())
        }
        "hero.AddExp" => {
            let (hero_id, items) = decode_hero_add_exp_request(request_args);
            const MAX_EXP_ITEMS: usize = 99;
            if hero_id == 0 || items.is_empty() || items.len() > MAX_EXP_ITEMS {
                *handler_error = Some(GameError::Internal(
                    "hero experience request is invalid".to_owned(),
                ));
                Some(Vec::new())
            } else if let Some(account) = account.as_deref_mut() {
                let hero_exists = account
                    .get("dock")
                    .and_then(|dock| dock.get("heroes"))
                    .and_then(Value::as_array)
                    .is_some_and(|heroes| {
                        heroes
                            .iter()
                            .any(|hero| json_u64(hero, "heroId") == Some(hero_id))
                    });
                if !hero_exists {
                    *handler_error = Some(GameError::Internal("hero was not found".to_owned()));
                    Some(Vec::new())
                } else {
                    let mut total_exp = 0i64;
                    for (item_id, requested_num) in &items {
                        let per_item = hero_level_catalog
                            .and_then(|catalog| catalog.exp_per_item.get(item_id))
                            .copied()
                            .unwrap_or_default()
                            .max(0);
                        if per_item == 0 {
                            continue;
                        }
                        let available = consume_bag_item(
                            account,
                            *item_id,
                            (*requested_num).clamp(0, 1_000_000),
                        );
                        total_exp = total_exp.saturating_add(
                            i64::from(available).saturating_mul(i64::from(per_item)),
                        );
                    }
                    if total_exp <= 0 {
                        *handler_error = Some(GameError::Internal(
                            "experience items were not found".to_owned(),
                        ));
                        Some(Vec::new())
                    } else {
                        if let Some(hero) = find_hero_mut(account, hero_id) {
                            let mut level = hero
                                .get("level")
                                .and_then(Value::as_i64)
                                .and_then(|value| i32::try_from(value).ok())
                                .unwrap_or(1)
                                .max(1);
                            let mut exp = hero
                                .get("exp")
                                .and_then(Value::as_i64)
                                .and_then(|value| i32::try_from(value).ok())
                                .unwrap_or_default()
                                .max(0);
                            let boosted = scale_reward(total_exp, state.ship_exp_multiplier);
                            let mut remaining = boosted.min(i64::from(i32::MAX));
                            while level < 200 {
                                let need = hero_level_catalog
                                    .and_then(|catalog| catalog.exp_needed.get(&level))
                                    .copied()
                                    .unwrap_or(500)
                                    .max(1);
                                if remaining + i64::from(exp) < i64::from(need) {
                                    exp = exp.saturating_add(remaining as i32);
                                    remaining = 0;
                                    break;
                                }
                                let available_exp = i64::from(exp).saturating_add(remaining);
                                remaining = available_exp.saturating_sub(i64::from(need));
                                exp = 0;
                                level += 1;
                            }
                            if remaining > 0 {
                                exp = exp.saturating_add(remaining.min(i64::from(i32::MAX)) as i32);
                            }
                            hero.insert("level".to_owned(), json!(level));
                            hero.insert("exp".to_owned(), json!(exp));
                        }
                        advance_task_event(account, task_catalog, 10, 1, current_unix_seconds());
                        pre_pushes.push(encode_hero_bag_push(account));
                        append_method_push(
                            pre_pushes,
                            "bag.UpdateBagData",
                            BagInfoCodec::encode(&bag_info_from_account(account)),
                        );
                        append_method_push(
                            post_pushes,
                            "task.TaskInfo",
                            task_info_payload(account, task_catalog),
                        );
                        Some(encode_hero_add_exp_response(hero_id, &items))
                    }
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "hero.HeroIntensify" => {
            let (hero_id, consumed_ids, super_intensify) =
                decode_hero_intensify_request(request_args);
            let catalog = SHIP_INTENSIFY_CATALOG.get().cloned().unwrap_or_default();
            if let Some(account) = account.as_deref_mut() {
                match hero_intensify_state(
                    account,
                    &catalog,
                    hero_id,
                    &consumed_ids,
                    super_intensify,
                ) {
                    Ok(_) => {
                        advance_task_event(account, task_catalog, 10, 1, current_unix_seconds());
                        append_method_push(
                            pre_pushes,
                            "hero.UpdateHeroBagData",
                            encode_hero_intensify_payload(account, hero_id, &consumed_ids),
                        );
                        append_method_push(
                            pre_pushes,
                            "equip.UpdateEquipBagData",
                            EquipListCodec::encode(&equip_list_from_account(
                                account,
                                equip_catalog,
                            )),
                        );
                        append_method_push(
                            pre_pushes,
                            "user.UpdateUserInfo",
                            UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                        );
                        Some(Vec::new())
                    }
                    Err(error) => {
                        *handler_error = Some(GameError::Internal(error.to_owned()));
                        Some(Vec::new())
                    }
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "hero.HeroAdvance" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let consumed_ids = decode_repeated_varint_field(request_args, 2)
                .into_iter()
                .filter_map(|id| u64::try_from(id).ok())
                .collect::<Vec<_>>();
            let consume_item_ids = decode_repeated_varint_field(request_args, 3);
            let catalog = SHIP_BREAK_CATALOG.get().cloned().unwrap_or_default();
            if let Some(account) = account.as_deref_mut() {
                match hero_advance_state(
                    account,
                    &catalog,
                    hero_id,
                    &consumed_ids,
                    &consume_item_ids,
                ) {
                    Ok((target_id, removed_ids)) => {
                        advance_task_event(account, task_catalog, 2, 1, current_unix_seconds());
                        append_method_push(
                            pre_pushes,
                            "hero.UpdateHeroBagData",
                            encode_hero_intensify_payload(account, target_id, &removed_ids),
                        );
                        append_method_push(
                            pre_pushes,
                            "equip.UpdateEquipBagData",
                            EquipListCodec::encode(&equip_list_from_account(
                                account,
                                equip_catalog,
                            )),
                        );
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
                        Some(Vec::new())
                    }
                    Err(error) => {
                        *handler_error = Some(GameError::Internal(error.to_owned()));
                        Some(Vec::new())
                    }
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "hero.HeroAdvanceMUB" => {
            let (hero_id, consume_items) = decode_hero_advance_mub_request(request_args);
            let catalog = SHIP_BREAK_CATALOG.get().cloned().unwrap_or_default();
            if let Some(account) = account.as_deref_mut() {
                match hero_advance_mub_state(account, &catalog, hero_id, &consume_items) {
                    Ok(()) => {
                        append_method_push(
                            pre_pushes,
                            "hero.UpdateHeroBagData",
                            encode_hero_intensify_payload(account, hero_id, &[]),
                        );
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
                        Some(Vec::new())
                    }
                    Err(error) => {
                        *handler_error = Some(GameError::Internal(error.to_owned()));
                        Some(Vec::new())
                    }
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "hero.HeroAdvMaxLv" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let catalog = SHIP_ADVANCE_CATALOG.get().cloned().unwrap_or_default();
            if let Some(account) = account.as_deref_mut() {
                match hero_advance_max_level_state(account, &catalog, hero_id) {
                    Ok(()) => {
                        append_method_push(
                            pre_pushes,
                            "hero.UpdateHeroBagData",
                            encode_hero_intensify_payload(account, hero_id, &[]),
                        );
                        Some(Vec::new())
                    }
                    Err(error) => {
                        *handler_error = Some(GameError::Internal(error.to_owned()));
                        Some(Vec::new())
                    }
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "hero.HeroRemould" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let effect_id = decode_varint_field(request_args, 2);
            let catalog = SHIP_REMOULD_CATALOG.get().cloned().unwrap_or_default();
            if let Some(account) = account.as_deref_mut() {
                match hero_remould_state(account, &catalog, hero_id, effect_id) {
                    Ok(()) => {
                        append_method_push(
                            pre_pushes,
                            "hero.UpdateHeroBagData",
                            encode_hero_intensify_payload(account, hero_id, &[]),
                        );
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
                        Some(Vec::new())
                    }
                    Err(error) => {
                        *handler_error = Some(GameError::Internal(error.to_owned()));
                        Some(Vec::new())
                    }
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "hero.StudySkill" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let skill_id = decode_varint_field(request_args, 2);
            if let Some(account) = account.as_deref_mut() {
                let before_level = account
                    .get("dock")
                    .and_then(|dock| dock.get("heroes"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
                    .and_then(|hero| hero.get("pSkills"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .find(|skill| {
                        json_i32(skill, "pSkillId").or_else(|| json_i32(skill, "pskillId"))
                            == Some(skill_id)
                    })
                    .and_then(|skill| json_i32(skill, "level"))
                    .unwrap_or(0);
                // Do not consume materials for malformed hero/skill requests. Legacy
                // snapshots may omit a skill and are repaired by login normalization;
                // preserve old insertion behavior for those rows.
                let materials_ok = if before_level <= 0 {
                    true
                } else {
                    HERO_SKILL_UPGRADE_CATALOG
                        .get()
                        .map(|catalog| {
                            consume_hero_skill_upgrade_materials(
                                account,
                                catalog,
                                skill_id,
                                before_level,
                            )
                        })
                        .unwrap_or(true)
                };
                if materials_ok && study_skill_state(account, hero_id, skill_id) {
                    if std::env::var_os("BLUEOATH_TRACE_METHODS").is_some() {
                        let after_level = account
                            .get("dock")
                            .and_then(|dock| dock.get("heroes"))
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
                            .and_then(|hero| hero.get("pSkills"))
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .find(|skill| {
                                json_i32(skill, "pSkillId").or_else(|| json_i32(skill, "pskillId"))
                                    == Some(skill_id)
                            })
                            .and_then(|skill| json_i32(skill, "level"))
                            .unwrap_or(0);
                        eprintln!(
                            "hero.StudySkill state hero={} skill={} before={} after={}",
                            hero_id, skill_id, before_level, after_level
                        );
                    }
                    advance_task_event(account, task_catalog, 9, 1, current_unix_seconds());
                    pre_pushes.push(encode_hero_bag_push(account));
                    append_method_push(
                        pre_pushes,
                        "bag.UpdateBagData",
                        BagInfoCodec::encode(&bag_info_from_account(account)),
                    );
                    append_method_push(
                        pre_pushes,
                        "task.TaskInfo",
                        task_info_payload(account, task_catalog),
                    );
                    // Some client builds apply the hero cache only after the request
                    // callback. Repeat the authoritative full bag after the response too;
                    // this prevents the skill page from reverting to its cached level 1.
                    append_method_push(
                        post_pushes,
                        "hero.UpdateHeroBagData",
                        HeroBagCodec::encode(&hero_bag_from_account(account)),
                    );
                    append_method_push(
                        post_pushes,
                        "bag.UpdateBagData",
                        BagInfoCodec::encode(&bag_info_from_account(account)),
                    );
                    Some(encode_study_skill_response(hero_id, skill_id))
                } else {
                    if std::env::var_os("BLUEOATH_TRACE_METHODS").is_some() {
                        eprintln!(
                            "hero.StudySkill rejected hero={} skill={}",
                            hero_id, skill_id
                        );
                    }
                    *handler_error = Some(GameError::Internal(if materials_ok {
                        "hero or skill was invalid".to_owned()
                    } else {
                        "not enough skill upgrade materials".to_owned()
                    }));
                    Some(Vec::new())
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "hero.AutoEquip" => {
            let equip_type = decode_varint_u64_field(request_args, 2);
            let units = decode_auto_equip_units(request_args);
            if let Some(account) = account.as_deref_mut() {
                match hero_auto_equip_state(account, equip_type, &units) {
                    Ok(()) => {
                        pre_pushes.push(encode_hero_bag_push(account));
                        append_method_push(
                            pre_pushes,
                            "equip.UpdateEquipBagData",
                            EquipListCodec::encode(&equip_list_from_account(
                                account,
                                equip_catalog,
                            )),
                        );
                    }
                    Err(error) => {
                        *handler_error = Some(GameError::Internal(error.to_owned()));
                    }
                }
            }
            Some(Vec::new())
        }
        "hero.AutoUnEquip" => {
            let equip_type = decode_varint_u64_field(request_args, 2);
            let hero_ids = decode_repeated_varint_field(request_args, 1)
                .into_iter()
                .filter_map(|id| u64::try_from(id).ok())
                .collect::<Vec<_>>();
            if let Some(account) = account.as_deref_mut() {
                match hero_auto_unequip_state(account, equip_type, &hero_ids) {
                    Ok(()) => {
                        pre_pushes.push(encode_hero_bag_push(account));
                        append_method_push(
                            pre_pushes,
                            "equip.UpdateEquipBagData",
                            EquipListCodec::encode(&equip_list_from_account(
                                account,
                                equip_catalog,
                            )),
                        );
                    }
                    Err(error) => {
                        *handler_error = Some(GameError::Internal(error.to_owned()));
                    }
                }
            }
            Some(Vec::new())
        }
        "hero.HeroEquipEffect" => {
            let (hero_id, effects) = decode_equip_effect_request(request_args);
            if let Some(account) = account.as_deref_mut() {
                if let Err(error) = hero_equip_effect_state(account, hero_id, &effects) {
                    *handler_error = Some(GameError::Internal(error.to_owned()));
                } else {
                    pre_pushes.push(encode_hero_bag_push(account));
                }
            }
            Some(Vec::new())
        }
        "hero.EquipBinding" => {
            let (hero_id, equip_id, equip_type) = decode_equip_binding_request(request_args);
            if let Some(account) = account.as_deref_mut() {
                if let Err(error) = hero_equip_binding_state(account, hero_id, equip_id, equip_type)
                {
                    *handler_error = Some(GameError::Internal(error.to_owned()));
                } else {
                    pre_pushes.push(encode_hero_bag_push(account));
                }
            }
            Some(Vec::new())
        }
        "hero.EquipUnBinding" => {
            let (hero_id, equip_id, equip_type) = decode_equip_binding_request(request_args);
            if let Some(account) = account.as_deref_mut() {
                match hero_equip_unbinding_state(account, hero_id, equip_id, equip_type) {
                    Ok(rewards) => {
                        pre_pushes.push(encode_hero_bag_push(account));
                        Some(encode_retire_hero_response(&rewards))
                    }
                    Err(error) => {
                        *handler_error = Some(GameError::Internal(error.to_owned()));
                        Some(Vec::new())
                    }
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "hero.EquipLockTransplant" => {
            let equip_type = decode_varint_u64_field(request_args, 2);
            let hero_ids = decode_repeated_varint_field(request_args, 1)
                .into_iter()
                .filter_map(|id| u64::try_from(id).ok())
                .collect::<Vec<_>>();
            if let Some(account) = account.as_deref_mut() {
                if let Err(error) = hero_equip_lock_transplant_state(account, &hero_ids, equip_type)
                {
                    *handler_error = Some(GameError::Internal(error.to_owned()));
                } else {
                    pre_pushes.push(encode_hero_bag_push(account));
                    append_method_push(
                        pre_pushes,
                        "equip.UpdateEquipBagData",
                        EquipListCodec::encode(&equip_list_from_account(account, equip_catalog)),
                    );
                }
            }
            Some(Vec::new())
        }
        "hero.ChangeEquip" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let slot = decode_varint_u64_field(request_args, 2);
            let equip_id = decode_varint_u64_field(request_args, 3);
            let equip_type = decode_varint_u64_field(request_args, 4).max(1);
            if let Some(account) = account.as_deref_mut() {
                let change = if equip_type == 1 {
                    hero_change_equip_state(account, hero_id, slot, equip_id)
                } else {
                    hero_change_equip_state_for_type(account, hero_id, slot, equip_id, equip_type)
                };
                match change {
                    Ok(()) => {
                        pre_pushes.push(encode_hero_bag_push(account));
                        append_method_push(
                            pre_pushes,
                            "equip.UpdateEquipBagData",
                            EquipListCodec::encode(&equip_list_from_account(
                                account,
                                equip_catalog,
                            )),
                        );
                    }
                    Err(error) => {
                        *handler_error = Some(GameError::Internal(error.to_owned()));
                    }
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
            }
            Some(Vec::new())
        }
        _ => None,
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
            },
        );
        let result = handle_typed(
            &mut account,
            "hero.GetHeroInfo",
            &[],
            &mut Vec::new(),
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
        let mut pushes = Vec::new();

        let result = handle_typed(
            &mut account,
            "hero.LockHero",
            &args,
            &mut pushes,
            HeroTypedCatalogs::empty(),
        );

        assert!(matches!(result, HandlerResult::PushOnly));
        assert!(!account.dock.heroes.get(&hero_id).unwrap().locked);
        assert_eq!(pushes.len(), 1);
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
        let mut pushes = Vec::new();

        let result = handle_typed(
            &mut account,
            "hero.ChangeName",
            &args,
            &mut pushes,
            HeroTypedCatalogs::empty(),
        );

        assert!(matches!(result, HandlerResult::PushOnly));
        assert_eq!(account.dock.heroes.values().next().unwrap().name, "Aegis");
        assert_eq!(pushes.len(), 1);
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
        let mut pushes = Vec::new();

        let result = handle_typed(
            &mut account,
            "hero.AddExp",
            &args,
            &mut pushes,
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
        assert_eq!(pushes.len(), 3);
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
        let mut pushes = Vec::new();

        let result = handle_typed(
            &mut account,
            "hero.RetireHero",
            &args,
            &mut pushes,
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
        assert_eq!(pushes.len(), 4);
    }
}
