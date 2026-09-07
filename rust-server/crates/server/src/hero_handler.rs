use serde_json::{json, Value};

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let payload = handle_legacy(context, method, request_args);
    if *context.response_err != 0 {
        HandlerResult::Error(GameError::Internal(context.response_err_msg.clone()))
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
    let response_err = &mut *context.response_err;
    let response_err_msg = &mut *context.response_err_msg;

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
                *response_err = 1;
                *response_err_msg = "hero id is invalid".to_owned();
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
                *response_err = 1;
                *response_err_msg = "hero experience request is invalid".to_owned();
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
                    *response_err = 1;
                    *response_err_msg = "hero was not found".to_owned();
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
                        *response_err = 1;
                        *response_err_msg = "experience items were not found".to_owned();
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
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
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
                        *response_err = 1;
                        *response_err_msg = error.to_owned();
                        Some(Vec::new())
                    }
                }
            } else {
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
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
                        *response_err = 1;
                        *response_err_msg = error.to_owned();
                        Some(Vec::new())
                    }
                }
            } else {
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
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
                        *response_err = 1;
                        *response_err_msg = error.to_owned();
                        Some(Vec::new())
                    }
                }
            } else {
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
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
                        *response_err = 1;
                        *response_err_msg = error.to_owned();
                        Some(Vec::new())
                    }
                }
            } else {
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
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
                        *response_err = 1;
                        *response_err_msg = error.to_owned();
                        Some(Vec::new())
                    }
                }
            } else {
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
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
                    *response_err = 1;
                    *response_err_msg = if materials_ok {
                        "hero or skill was invalid".to_owned()
                    } else {
                        "not enough skill upgrade materials".to_owned()
                    };
                    Some(Vec::new())
                }
            } else {
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
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
                        *response_err = 1;
                        *response_err_msg = error.to_owned();
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
                        *response_err = 1;
                        *response_err_msg = error.to_owned();
                    }
                }
            }
            Some(Vec::new())
        }
        "hero.HeroEquipEffect" => {
            let (hero_id, effects) = decode_equip_effect_request(request_args);
            if let Some(account) = account.as_deref_mut() {
                if let Err(error) = hero_equip_effect_state(account, hero_id, &effects) {
                    *response_err = 1;
                    *response_err_msg = error.to_owned();
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
                    *response_err = 1;
                    *response_err_msg = error.to_owned();
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
                        *response_err = 1;
                        *response_err_msg = error.to_owned();
                        Some(Vec::new())
                    }
                }
            } else {
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
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
                    *response_err = 1;
                    *response_err_msg = error.to_owned();
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
                        *response_err = 1;
                        *response_err_msg = error.to_owned();
                    }
                }
            } else {
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
            }
            Some(Vec::new())
        }
        _ => None,
    }
}
