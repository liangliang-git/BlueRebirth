use serde_json::Value;

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(super) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    if method != "copy.AttackBase" {
        return HandlerResult::Empty;
    }
    let Ok(request) = CopyAttackRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest("copy attack request is invalid"));
    };
    let Some(active) = account.battle.active.as_mut() else {
        return HandlerResult::Error(GameError::InvalidState("battle session is not active"));
    };
    if active.copy_id.get() != request.copy_id
        || request
            .hero_ids
            .iter()
            .any(|hero_id| !active.hero_ids.iter().any(|id| id.get() == *hero_id))
    {
        return HandlerResult::Error(GameError::InvalidRequest(
            "battle attack does not match active session",
        ));
    }
    active.attack_count = active.attack_count.saturating_add(1);
    active.revision = active.revision.saturating_add(1);
    HandlerResult::Reply(Response::raw(
        method,
        battle_attack_payload_with_damage(request_args, 0),
    ))
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
        fashion: fashion_catalog,
        equip: equip_catalog,
        hero_level: hero_level_catalog,
        chapters: chapter_catalog,
        battle: battle_catalog,
        equip_new_test: equip_new_test_catalog,
        ..
    } = catalogs;
    let pre_pushes = &mut *context.pre_pushes;
    let post_pushes = &mut *context.post_pushes;
    let handler_error = &mut *context.handler_error;
    let pass_details = &mut *context.pass_details;
    let pass_rewards = &mut *context.pass_rewards;
    let pass_hero_ids = &mut *context.pass_hero_ids;
    let pass_mvp_hero_id = &mut *context.pass_mvp_hero_id;
    let pass_shipwrecked_ids = &mut *context.pass_shipwrecked_ids;

    match method {
        "copy.GetRecord" => {
            let request = match CopyRecordRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => {
                    *handler_error = Some(GameError::Internal(
                        "copy record request is invalid".to_owned(),
                    ));
                    return Some(Vec::new());
                }
            };
            let copy_id = request.copy_id;
            if copy_id <= 0 {
                *handler_error = Some(GameError::Internal("copy id is invalid".to_owned()));
                Some(Vec::new())
            } else {
                Some(CopyRecordListCodec::encode(&copy_record_list_from_account(
                    account.as_deref().unwrap_or(&Value::Null),
                    copy_id,
                )))
            }
        }
        "copy.DeleteRecord" => {
            let request = match CopyRecordRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => {
                    *handler_error = Some(GameError::Internal(
                        "copy record request is invalid".to_owned(),
                    ));
                    return Some(Vec::new());
                }
            };
            let copy_id = request.copy_id;
            let index = request.index;
            if copy_id <= 0 || index < 0 {
                *handler_error = Some(GameError::Internal("copy record is invalid".to_owned()));
                Some(Vec::new())
            } else if let Some(account) = account.as_deref_mut() {
                if !delete_copy_record(account, copy_id, index) {
                    *handler_error =
                        Some(GameError::Internal("copy record is not found".to_owned()));
                    Some(Vec::new())
                } else {
                    Some(CopyRecordListCodec::encode(&copy_record_list_from_account(
                        account, copy_id,
                    )))
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "copy.TacticOn" => {
            let request = match CopyRecordRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => {
                    *handler_error = Some(GameError::Internal(
                        "copy record request is invalid".to_owned(),
                    ));
                    return Some(Vec::new());
                }
            };
            let copy_id = request.copy_id;
            let index = request.index;
            if copy_id <= 0 || index < 0 {
                *handler_error = Some(GameError::Internal("copy record is invalid".to_owned()));
                Some(Vec::new())
            } else if let Some(account) = account.as_deref_mut() {
                if let Some(fleet) = apply_copy_record_to_fleet(account, copy_id, index) {
                    Some(FleetInfoCodec::encode(&fleet))
                } else {
                    *handler_error =
                        Some(GameError::Internal("copy record is not found".to_owned()));
                    Some(Vec::new())
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "copyinfo.GetCopyInfo" => {
            let copy_id = decode_varint_field(request_args, 1);
            Some(CopyInfoCodec::encode_record_response(
                &copy_info_response_from_account(
                    account.as_deref().unwrap_or(&Value::Null),
                    copy_id,
                ),
            ))
        }
        "dailycopy.CopyEnter" => {
            let request = match DailyCopyEnterRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => {
                    *handler_error = Some(GameError::Internal(
                        "daily copy enter request is invalid".to_owned(),
                    ));
                    return Some(Vec::new());
                }
            };
            let chapter_id = request.chapter_id;
            let copy_id = request.copy_id;
            // Client names field 3 TacticId, even though the decompiled local
            // variable is fleetId. It selects player's tactic used for sortie.
            let tactic_id = request.tactic_id;
            let daily_group = chapter_catalog.and_then(|catalog| {
                catalog
                    .daily_chapters
                    .iter()
                    .find(|(id, _)| *id == chapter_id)
                    .map(|(_, group_id)| *group_id)
            });
            let known_copy = match (daily_group, battle_catalog) {
                (Some(group_id), Some(catalog)) if group_id > 0 => catalog
                    .daily_group_by_copy
                    .get(&copy_id)
                    .is_some_and(|copy_group| *copy_group == group_id),
                (Some(_), Some(_)) => false,
                (Some(_), None) => {
                    chapter_catalog.is_some_and(|catalog| catalog.daily.contains(&copy_id))
                }
                (None, _) => chapter_catalog.is_none() && chapter_id == 1,
            };
            if chapter_id <= 0 || copy_id <= 0 || tactic_id <= 0 || !known_copy {
                *handler_error = Some(GameError::Internal(
                    "daily copy enter request is invalid".to_owned(),
                ));
                Some(Vec::new())
            } else if let Some(account) = account.as_deref_mut() {
                let tactic_hero_ids = fleet_hero_ids(account, tactic_id as u64)
                    .into_iter()
                    .filter_map(|id| i32::try_from(id).ok())
                    .collect::<Vec<_>>();
                let available_hero_ids = account
                    .get("dock")
                    .and_then(|dock| dock.get("heroes"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|hero| json_i32(hero, "heroId"))
                    .filter(|hero_id| *hero_id > 0)
                    .take(6)
                    .collect::<Vec<_>>();
                let hero_ids = if tactic_hero_ids.is_empty() {
                    available_hero_ids
                } else {
                    tactic_hero_ids
                };
                if hero_ids.is_empty()
                    || !consume_battle_supply(
                        account,
                        battle_catalog,
                        copy_id,
                        &hero_ids.iter().map(|id| *id as u64).collect::<Vec<_>>(),
                        1,
                    )
                {
                    *handler_error = Some(GameError::Internal(
                        "insufficient supply or missing daily tactic".to_owned(),
                    ));
                    return Some(Vec::new());
                }
                normalize_daily_copy_state(account, chapter_catalog, current_unix_seconds());
                let daily = account
                    .as_object_mut()
                    .expect("account must be an object")
                    .entry("dailyCopy".to_owned())
                    .or_insert_with(|| json!({}));
                let chapters = daily
                    .as_object_mut()
                    .expect("daily copy state must be an object")
                    .entry("chapters".to_owned())
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .expect("daily copy chapters must be an array");
                if let Some(chapter) = chapters
                    .iter_mut()
                    .find(|value| json_i32(value, "chapterId") == Some(chapter_id))
                {
                    let times = json_i32(chapter, "challengeTimes").unwrap_or_default();
                    chapter["challengeTimes"] = json!(times.saturating_add(1));
                    chapter["lastCopyId"] = json!(copy_id);
                    chapter["lastTacticId"] = json!(tactic_id);
                } else {
                    chapters.push(json!({
                        "chapterId": chapter_id,
                        "challengeTimes": 1,
                        "passCopy": [],
                        "selectEx": false,
                        "exStar": 0,
                        "lastCopyId": copy_id,
                        "lastTacticId": tactic_id,
                    }));
                }
                let daily_payload =
                    daily_copy_snapshot_payload(account, chapter_catalog, current_unix_seconds());
                append_method_push(post_pushes, "dailycopy.UpdateDailyCopyData", daily_payload);
                account["battleSession"] = json!({
                    "copyId": copy_id,
                    "startedAt": current_unix_seconds(),
                    "attackCount": 0,
                    "remainingFleetIds": battle_session_fleet_ids(copy_id, battle_catalog),
                    "fleetAliases": battle_fleet_aliases(copy_id, battle_catalog)
                        .into_iter()
                        .map(|(wire_id, real_id)| json!({
                            "wireId": wire_id,
                            "realId": real_id,
                        }))
                        .collect::<Vec<_>>(),
                    "remainingEnemyIds": battle_enemy_ids(copy_id, battle_catalog),
                    "remainingEnemyHp": battle_enemy_hps(copy_id, battle_catalog),
                    "heroIds": hero_ids.clone(),
                    "heroGroups": [hero_ids.clone()],
                    "exBuff": [],
                    "isPvePtMode": false,
                });
                append_method_push(
                    post_pushes,
                    "user.UpdateUserInfo",
                    UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                );
                let start_payload = battle_start_payload_with_fleet_groups_with_stats(
                    account,
                    copy_id,
                    &[hero_ids],
                    battle_catalog,
                    SHIP_STAT_CATALOG.get(),
                    state.ship_stat_multiplier,
                    BattleStartOptions::default(),
                );
                Some(daily_copy_enter_payload(&start_payload))
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "dailycopy.GetData" | "dailycopy.SelectEx" => {
            if let Some(account) = account.as_deref_mut() {
                normalize_daily_copy_state(account, chapter_catalog, current_unix_seconds());
            }
            if method == "dailycopy.SelectEx" {
                let chapter_id = decode_varint_field(request_args, 1);
                let select_ex = decode_varint_u64_field(request_args, 2) != 0;
                let known_chapter = chapter_catalog
                    .map(|catalog| {
                        catalog
                            .daily_chapters
                            .iter()
                            .any(|(id, _)| *id == chapter_id)
                    })
                    .unwrap_or_else(|| chapter_id == 1);
                if known_chapter {
                    if let Some(account) = account.as_deref_mut() {
                        let daily = account.as_object_mut().and_then(|root| {
                            root.entry("dailyCopy".to_owned())
                                .or_insert_with(|| json!({}))
                                .as_object_mut()
                        });
                        if let Some(daily) = daily {
                            let chapters = daily
                                .entry("chapters".to_owned())
                                .or_insert_with(|| json!([]))
                                .as_array_mut();
                            if let Some(chapters) = chapters {
                                if let Some(chapter) = chapters
                                    .iter_mut()
                                    .find(|value| json_i32(value, "chapterId") == Some(chapter_id))
                                {
                                    chapter["selectEx"] = json!(select_ex);
                                } else {
                                    chapters.push(json!({
                                        "chapterId": chapter_id,
                                        "challengeTimes": 0,
                                        "passCopy": [],
                                        "selectEx": select_ex,
                                        "exStar": 0,
                                    }));
                                }
                            }
                        }
                    }
                }
            }
            let payload = daily_copy_snapshot_payload(
                account.as_deref().unwrap_or(&Value::Null),
                chapter_catalog,
                current_unix_seconds(),
            );
            append_method_push(post_pushes, "dailycopy.UpdateDailyCopyData", payload);
            Some(Vec::new())
        }
        "mopUp.GetMopUpData" => {
            let mut response_payload = None;
            if let Some(account) = account.as_deref_mut() {
                let copy_id = completed_sweep_copy_id(account, current_unix_seconds());
                let rewards = settle_mop_up_with_gameplay_config(
                    account,
                    battle_catalog,
                    fashion_catalog,
                    current_unix_seconds(),
                    state.drop_multiplier,
                    state.commander_exp_multiplier,
                    state.ship_exp_multiplier,
                    state.affection_multiplier,
                    COMMANDER_LEVEL_CATALOG.get(),
                    hero_level_catalog,
                );
                if !rewards.is_empty() {
                    append_shop_update_pushes(
                        pre_pushes,
                        state,
                        account,
                        rewards[0],
                        fashion_catalog,
                        equip_catalog,
                    );
                }
                append_method_push(
                    post_pushes,
                    "user.UpdateUserInfo",
                    UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                );
                // Sweep rewards are settled before the callback. Repeat both inventory
                // snapshots after callback so goods_type=6 enhancement materials are not
                // overwritten by the client's pre-sweep cache.
                append_method_push(
                    post_pushes,
                    "bag.UpdateBagData",
                    BagInfoCodec::encode(&bag_info_from_account(account)),
                );
                append_method_push(
                    post_pushes,
                    "equip.UpdateEquipBagData",
                    EquipListCodec::encode(&equip_list_from_account(account, equip_catalog)),
                );
                if copy_id > 0 {
                    append_method_push(
                        post_pushes,
                        "hero.UpdateHeroBagData",
                        HeroBagCodec::encode(&hero_bag_from_account(account)),
                    );
                }
                let pass_rets = mop_up_pass_rets(copy_id, &rewards);
                let payload =
                    mop_up_payload_with_pass_rets(account, current_unix_seconds(), &pass_rets);
                response_payload = Some(payload);
            }
            Some(response_payload.unwrap_or_else(|| {
                mop_up_payload(
                    account.as_deref().unwrap_or(&Value::Null),
                    current_unix_seconds(),
                )
            }))
        }
        "copy.StartBase" | "copy.PvpStartBase" => {
            let start_request = match CopyStartRequest::decode(request_args) {
                Ok(request) => request,
                Err(_) => {
                    *handler_error = Some(GameError::Internal(
                        "copy start request is invalid".to_owned(),
                    ));
                    return Some(Vec::new());
                }
            };
            let copy_id = start_request.copy_id;
            let requested_hero_groups = decode_start_hero_groups(request_args);
            let known_copy = battle_catalog
                .map(|catalog| catalog.copies.contains_key(&copy_id))
                .unwrap_or(copy_id > 0);
            let available_hero_ids = account
                .as_deref()
                .and_then(|account| account.get("dock"))
                .and_then(|dock| dock.get("heroes"))
                .and_then(Value::as_array)
                .map(|heroes| {
                    heroes
                        .iter()
                        .filter_map(|hero| json_i32(hero, "heroId"))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let hero_groups = if requested_hero_groups.is_empty() {
                vec![available_hero_ids
                    .iter()
                    .copied()
                    .take(6)
                    .collect::<Vec<_>>()]
            } else {
                let groups = requested_hero_groups
                    .into_iter()
                    .map(|requested| {
                        requested
                            .into_iter()
                            .filter(|hero_id| available_hero_ids.contains(hero_id))
                            .fold(Vec::new(), |mut selected, hero_id| {
                                if !selected.contains(&hero_id) && selected.len() < 6 {
                                    selected.push(hero_id);
                                }
                                selected
                            })
                    })
                    .filter(|group| !group.is_empty())
                    .collect::<Vec<_>>();
                if groups.is_empty() {
                    vec![available_hero_ids
                        .iter()
                        .copied()
                        .take(6)
                        .collect::<Vec<_>>()]
                } else {
                    groups
                }
            };
            let hero_ids = hero_groups.iter().flatten().copied().collect::<Vec<_>>();
            let ex_buffs = start_request.ex_buffs.clone();
            let is_pve_pt_mode = start_request.is_pve_pt_mode;
            let start_options = BattleStartOptions {
                is_running_fight: start_request.is_running_fight,
                battle_mode: start_request.battle_mode,
                anim_mode: start_request.anim_mode,
                match_type: start_request.match_type,
            };
            if !known_copy || hero_ids.is_empty() {
                *handler_error = Some(GameError::Internal(
                    "battle copy or fleet is invalid".to_owned(),
                ));
                Some(Vec::new())
            } else if !account.as_deref_mut().is_some_and(|account| {
                consume_battle_supply(
                    account,
                    battle_catalog,
                    copy_id,
                    &hero_ids.iter().map(|id| *id as u64).collect::<Vec<_>>(),
                    1,
                )
            }) {
                *handler_error = Some(GameError::Internal(
                    "insufficient supply or missing supply configuration".to_owned(),
                ));
                Some(Vec::new())
            } else {
                if let Some(account) = account.as_deref_mut() {
                    append_method_push(
                        post_pushes,
                        "user.UpdateUserInfo",
                        UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                    );
                    account["battleSession"] = json!({
                        "copyId": copy_id,
                        "startedAt": current_unix_seconds(),
                        "attackCount": 0,
                        "remainingFleetIds": battle_session_fleet_ids(copy_id, battle_catalog),
                        "fleetAliases": battle_fleet_aliases(copy_id, battle_catalog)
                            .into_iter()
                            .map(|(wire_id, real_id)| json!({
                                "wireId": wire_id,
                                "realId": real_id,
                            }))
                            .collect::<Vec<_>>(),
                        "remainingEnemyIds": battle_enemy_ids(copy_id, battle_catalog),
                        "remainingEnemyHp": battle_enemy_hps(copy_id, battle_catalog),
                        "heroIds": hero_ids.clone(),
                        "heroGroups": hero_groups.clone(),
                        "exBuff": ex_buffs,
                        "isPvePtMode": is_pve_pt_mode,
                    });
                }
                Some(battle_start_payload_with_fleet_groups_with_stats(
                    account.as_deref().unwrap_or(&Value::Null),
                    copy_id,
                    &hero_groups,
                    battle_catalog,
                    SHIP_STAT_CATALOG.get(),
                    state.ship_stat_multiplier,
                    start_options,
                ))
            }
        }
        "copy.AttackBase" => {
            if let Some(account) = account.as_deref_mut() {
                let enemy_id = account
                    .get("battleSession")
                    .and_then(Value::as_object)
                    .and_then(|session| validate_battle_attack(session, request_args));
                if let Some(enemy_id) = enemy_id {
                    let attacks = account["battleSession"]["attackCount"]
                        .as_i64()
                        .unwrap_or_default()
                        .saturating_add(1);
                    account["battleSession"]["attackCount"] = json!(attacks);
                    // Client owns combat simulation. Server only records that a
                    // valid attack happened; PassBase is the client result.
                    let _ = enemy_id;
                    Some(battle_attack_payload_with_damage(request_args, 0))
                } else {
                    *handler_error = Some(GameError::Internal(
                        "battle session is not active".to_owned(),
                    ));
                    Some(Vec::new())
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "copy.PassBase" => {
            // JP client sends BaseId=1 from its generic settlement helper and
            // runs combat locally, so AttackBase is not guaranteed to reach
            // the server. The active server session is the authoritative
            // sortie context for settlement.
            let requested_copy_id = decode_varint_field(request_args, 1);
            let active_copy_id = account.as_deref().and_then(|account| {
                account
                    .get("battleSession")
                    .and_then(Value::as_object)
                    .and_then(|session| session.get("copyId"))
                    .and_then(Value::as_i64)
                    .and_then(|copy_id| i32::try_from(copy_id).ok())
            });
            let copy_id = active_copy_id.unwrap_or(requested_copy_id);
            let now = current_unix_seconds();
            let known_copy = battle_catalog
                .map(|catalog| catalog.copies.contains_key(&copy_id))
                .unwrap_or(copy_id > 0);
            let valid_session = account.as_deref().is_some_and(|account| {
                account
                    .get("battleSession")
                    .and_then(Value::as_object)
                    .is_some_and(|session| {
                        session
                            .get("copyId")
                            .and_then(Value::as_i64)
                            .is_some_and(|active_id| active_id == i64::from(copy_id))
                            && session
                                .get("startedAt")
                                .and_then(Value::as_u64)
                                .is_some_and(|started_at| {
                                    started_at <= u64::from(now)
                                        && u64::from(now).saturating_sub(started_at) <= 1800
                                })
                    })
            });
            if !known_copy || !valid_session {
                *handler_error = Some(GameError::Internal(
                    "battle session is invalid or not won".to_owned(),
                ));
                Some(Vec::new())
            } else if let Some(account) = account.as_deref_mut() {
                let server_battle_time = now.saturating_sub(
                    account["battleSession"]["startedAt"]
                        .as_u64()
                        .unwrap_or(u64::from(now)) as u32,
                );
                let server_battle_time =
                    i32::try_from(server_battle_time.max(1)).unwrap_or(i32::MAX);
                *pass_hero_ids = account["battleSession"]["heroIds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64)
                    .collect();
                let battle_result = decode_battle_pass_result(request_args);
                let battle_time = if battle_result.battle_time > 0 {
                    battle_result.battle_time
                } else {
                    server_battle_time
                };
                save_battle_hero_hp(account, &battle_result.heroes, pass_hero_ids);
                *pass_mvp_hero_id = battle_result.mvp_hero_id;
                *pass_shipwrecked_ids = battle_result.shipwrecked_ids;
                let grade = if battle_result.grade > 0 {
                    battle_result.grade
                } else {
                    3
                };
                if grade >= 9 {
                    account["battleSession"] = Value::Null;
                    return Some(battle_pass_payload_with_rewards(
                        copy_id,
                        false,
                        grade,
                        battle_time,
                        &[],
                    ));
                }
                // TPassBaseArg.EnemyFleets is field 20. Field 17 is retained
                // for older client settlement payloads.
                let has_fleet_result = !decode_repeated_message_field(request_args, 20).is_empty()
                    || !decode_repeated_message_field(request_args, 17).is_empty();
                if has_fleet_result
                    && !validate_battle_fleet_pass(
                        account.get("battleSession").unwrap_or(&Value::Null),
                        request_args,
                    )
                {
                    *handler_error = Some(GameError::Internal(
                        "battle fleet result is invalid".to_owned(),
                    ));
                    return Some(Vec::new());
                }
                let final_fleet = if has_fleet_result {
                    mark_battle_fleet_passed(&mut account["battleSession"], request_args)
                } else {
                    // JP Lua sends one placeholder TPassBaseArg per encounter
                    // without FleetInfo/EnemyFleets. Advance one configured
                    // fleet only; later encounters must remain playable.
                    mark_first_battle_fleet_passed(&mut account["battleSession"])
                };
                if !final_fleet {
                    // copy.PassBase is emitted once per enemy fleet. Keep the
                    // sortie session alive until last fleet is settled; client
                    // needs successful response to advance to next fleet.
                    Some(battle_pass_payload_with_rewards(
                        copy_id,
                        false,
                        grade,
                        battle_time,
                        &[],
                    ))
                } else {
                    let ex_buffs = account["battleSession"]["exBuff"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_i64)
                        .filter_map(|value| i32::try_from(value).ok())
                        .collect();
                    account["battleSession"] = Value::Null;
                    let first_pass = !battle_copy_passed(account, copy_id);
                    if first_pass {
                        *pass_rewards = battle_catalog
                            .and_then(|catalog| catalog.copy_first_rewards.get(&copy_id))
                            .into_iter()
                            .flatten()
                            .map(|(goods_type, item_id, num)| ShopReward {
                                goods_type: *goods_type,
                                item_id: *item_id,
                                num: *num,
                                instance_id: 0,
                            })
                            .map(|reward| grant_reward(account, reward, now, fashion_catalog))
                            .collect();
                    }
                    pass_rewards.extend(draw_battle_drop_rewards_for_grade(
                        account,
                        battle_catalog,
                        copy_id,
                        state.drop_multiplier,
                        grade,
                        now,
                        fashion_catalog,
                    ));
                    if let Some(reward) = battle_rank_drop_reward(battle_catalog, copy_id, grade) {
                        pass_rewards.push(grant_reward(account, reward, now, fashion_catalog));
                    }
                    let (_, ship_base_exp) = battle_copy_experience(battle_catalog, copy_id);
                    let (evaluation_exp_multiplier, _) =
                        battle_evaluation_multipliers(battle_catalog, grade);
                    let ship_exp = scale_reward(
                        i64::from(ship_base_exp),
                        state.ship_exp_multiplier * evaluation_exp_multiplier,
                    )
                    .clamp(0, i64::from(i32::MAX)) as i32;
                    let exp_rewards = pass_hero_ids
                        .iter()
                        .copied()
                        .filter(|hero_id| *hero_id > 0)
                        .map(|hero_id| (hero_id, ship_exp))
                        .collect::<Vec<_>>();
                    *pass_details = Some((
                        copy_id,
                        grade,
                        battle_time,
                        first_pass,
                        ex_buffs,
                        exp_rewards.clone(),
                    ));
                    let copy_type = battle_catalog
                        .and_then(|catalog| catalog.copies.get(&copy_id))
                        .map(|copy| copy.copy_type)
                        .unwrap_or_default();
                    let payload = if copy_type == 34 && battle_result.damage > 0 {
                        let max_damage = equip_new_test_catalog
                            .and_then(|catalog| {
                                super::equip_handler::update_new_test_max_damage(
                                    account,
                                    catalog,
                                    copy_id,
                                    battle_result.damage,
                                )
                            })
                            .unwrap_or(battle_result.damage);
                        append_method_push(
                            post_pushes,
                            "equipnewtestcopy.UpdateEquipNewData",
                            super::equip_handler::equip_new_test_copy_payload(account),
                        );
                        battle_pass_payload_with_damage_and_experience(
                            BattlePassDamagePayload {
                                copy_id,
                                first_pass,
                                grade,
                                battle_time,
                                rewards: pass_rewards,
                                current_damage: battle_result.damage,
                                max_damage,
                            },
                            &exp_rewards,
                        )
                    } else {
                        battle_pass_payload_with_experience(
                            copy_id,
                            first_pass,
                            grade,
                            battle_time,
                            pass_rewards,
                            &exp_rewards,
                        )
                    };
                    Some(payload)
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "copy.QuitBase" => {
            if let Some(account) = account.as_deref_mut() {
                account["battleSession"] = Value::Null;
            }
            Some(request_args.to_vec())
        }
        // Mop-up availability check expects TMopUpRet{SweepFleetsNum}; an
        // empty protobuf causes the client callback to dereference nil.
        "mopUp.CheckSweep" => {
            let active = account
                .as_deref()
                .map(|value| mop_up_active_count(value, current_unix_seconds()))
                .unwrap_or_default();
            Some(vec![0x08, (active == 0) as u8])
        }
        "mopUp.StartSweep" | "mopUp.StopSweep" => {
            let mut response_payload = Vec::new();
            if let Some(account) = account.as_deref_mut() {
                let now = current_unix_seconds();
                let (fleet_id, copy_id, sweep_count) = decode_mop_up_arg(request_args);
                let valid_start = method == "mopUp.StopSweep"
                    || (sweep_count > 0
                        && sweep_count <= 99
                        && battle_catalog.is_some_and(|catalog| {
                            i32::try_from(copy_id).ok().is_some_and(|copy_id| {
                                catalog.copies.contains_key(&copy_id)
                                    && battle_copy_passed(account, copy_id)
                            })
                        })
                        && account_has_fleet(account, fleet_id));
                if !valid_start {
                    *handler_error = Some(GameError::Internal(
                        "sweep fleet or copy is not available".to_owned(),
                    ));
                } else if method == "mopUp.StartSweep"
                    && !consume_battle_supply(
                        account,
                        battle_catalog,
                        copy_id as i32,
                        &fleet_hero_ids(account, fleet_id),
                        sweep_count as i32,
                    )
                {
                    *handler_error = Some(GameError::Internal(
                        "insufficient supply or missing supply configuration".to_owned(),
                    ));
                } else {
                    update_mop_up_state(account, method, request_args, now);
                    // Local server sweeps resolve immediately. Keep helper's delayed mode for
                    // direct state tests, but settle this user action before refreshing the UI.
                    let rewards = if method == "mopUp.StartSweep" {
                        settle_mop_up_with_gameplay_config(
                            account,
                            battle_catalog,
                            fashion_catalog,
                            now.saturating_add(1),
                            state.drop_multiplier,
                            state.commander_exp_multiplier,
                            state.ship_exp_multiplier,
                            state.affection_multiplier,
                            COMMANDER_LEVEL_CATALOG.get(),
                            hero_level_catalog,
                        )
                    } else {
                        Vec::new()
                    };
                    if !rewards.is_empty() {
                        append_shop_update_pushes(
                            pre_pushes,
                            state,
                            account,
                            rewards[0],
                            fashion_catalog,
                            equip_catalog,
                        );
                    }
                    append_method_push(
                        post_pushes,
                        "user.UpdateUserInfo",
                        UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                    );
                    if method == "mopUp.StartSweep" {
                        append_method_push(
                            post_pushes,
                            "hero.UpdateHeroBagData",
                            HeroBagCodec::encode(&hero_bag_from_account(account)),
                        );
                    }
                    let pass_rets = mop_up_pass_rets(copy_id as i32, &rewards);
                    let payload = mop_up_payload_with_pass_rets(account, now, &pass_rets);
                    // StartSweep callback consumes TMopUpRet directly to render the reward
                    // dialog. Keep identical payload in push for clients that refresh via
                    // mopUp.GetMopUpData after callback.
                    response_payload = payload.clone();
                    append_method_push(post_pushes, "mopUp.GetMopUpData", payload);
                }
            }
            Some(response_payload)
        }
        "copy.GetRandomFactors" => Some(encode_random_factor_payload(
            decode_varint_field(request_args, 1),
            battle_catalog,
        )),
        _ => None,
    }
}

fn daily_copy_enter_payload(start_base_ret: &[u8]) -> Vec<u8> {
    let mut payload = Vec::new();
    // TDailyCopyEnterRet.StartBaseRet = 1. The nested value is the same
    // TStartBaseRet returned by copy.StartBase.
    append_message_field(&mut payload, 1, start_base_ret);
    payload
}

#[cfg(test)]
mod tests {
    use crate::common::response::HandlerResult;
    use blueoath_domain::{ChapterId, CopyId, FleetId, NewAccountFactory, ProfileId};

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
    fn typed_attack_updates_active_battle_session() {
        let mut account = NewAccountFactory::create(ProfileId::new("battle").unwrap(), "Battle");
        let chapter_id = ChapterId::new(1).unwrap();
        let copy_id = CopyId::new(9).unwrap();
        let fleet_id = FleetId::new(1).unwrap();
        let hero_id = account.dock.heroes.keys().next().copied().unwrap();
        account
            .fleet
            .fleets
            .entry(fleet_id)
            .or_default()
            .members
            .push(hero_id);
        blueoath_game::BattleService::start(&mut account, chapter_id, copy_id, fleet_id, 10)
            .unwrap();
        account.battle.active.as_mut().unwrap().hero_ids = vec![hero_id];

        let mut request = Vec::new();
        append_varint_field(&mut request, 1, 1);
        append_varint_field(&mut request, 2, copy_id.get());
        append_varint_field(&mut request, 3, hero_id.get());
        append_varint_field(&mut request, 4, 7);

        assert!(matches!(
            handle_typed(&mut account, "copy.AttackBase", &request),
            HandlerResult::Reply(_)
        ));
        let active = account.battle.active.as_ref().unwrap();
        assert_eq!(active.attack_count, 1);
        assert_eq!(active.revision, 1);
    }
}
