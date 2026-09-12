use super::*;

#[allow(unused_variables)]
pub(super) fn handle(
    state: &ServerState,
    typed_account: &mut Option<&mut AccountState>,
    request: &RequestContext,
    request_args: &[u8],
    method: &GameMethod,
    known_method: Option<KnownMethod>,
    catalogs: &GameLoginCatalogs<'_>,
    pre_pushes: &mut LoginPushes,
    post_pushes: &mut LoginPushes,
    handler_error_slot: &mut Option<GameError>,
) -> (bool, Option<Response>) {
    let GameLoginCatalogs {
        fashion: fashion_catalog,
        equip: equip_catalog,
        hero_level: hero_level_catalog,
        hero_breakdown: hero_breakdown_catalog,
        shop: shop_catalog,
        mails: mail_catalog,
        handbook_behaviours,
        chapters: chapter_catalog,
        tasks: task_catalog,
        battle: battle_catalog,
        ..
    } = *catalogs;
    let mut handler_error = handler_error_slot.take();
    let result = match request.method.as_str() {
        _ if method.is_family(MethodFamily::MopUp) && typed_account.is_some() => {
            let mut mop_up_effects = ResponseEffects::default();
            let result = battle_handler::handle_typed_mop_up(
                state,
                typed_account.as_mut().expect("typed mop up account"),
                request.method.as_str(),
                request_args,
                battle_catalog,
                &mut mop_up_effects,
            );
            apply_response_effects(mop_up_effects, pre_pushes, post_pushes, &mut handler_error);
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::Copy)
            && !matches!(
                request.method.as_str(),
                "copy.ChooseSfLv" | "copy.GetCopy" | "copy.UnLockCopy"
            )
            || method.is_family(MethodFamily::MopUp)
            || method.is_family(MethodFamily::DailyCopy)
            || request.method == "copyinfo.GetCopyInfo" =>
        {
            let settled_copy_id = if matches!(
                request.method.as_str(),
                "copy.PassBase" | "copy.PassMiniGame"
            ) {
                typed_account.as_ref().and_then(|account| {
                    account
                        .battle
                        .active
                        .as_ref()
                        .and_then(|session| i32::try_from(session.copy_id.get()).ok())
                        .or_else(|| {
                            CopyMiniGamePassRequest::decode(request_args)
                                .ok()
                                .map(|request| request.copy_id)
                        })
                })
            } else {
                None
            };
            let mut typed_handled = false;
            let mut battle_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                let result = battle_handler::handle_typed_with_catalog(
                    typed,
                    request.method.as_str(),
                    request_args,
                    battle_handler::TypedBattleContext::new(
                        battle_catalog,
                        fashion_catalog,
                        hero_level_catalog,
                        state.drop_multiplier,
                        state.ship_stat_multiplier,
                        state.commander_exp_multiplier,
                        state.ship_exp_multiplier,
                        &mut battle_effects,
                    )
                    .with_server_state(state)
                    .with_affection_multiplier(state.affection_multiplier),
                );
                if !matches!(result, HandlerResult::Empty) {
                    typed_handled = true;
                    result
                } else {
                    HandlerResult::Error(GameError::InvalidRequest(
                        "battle request is not supported",
                    ))
                }
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "battle request requires typed account",
                ))
            };
            apply_response_effects(battle_effects, pre_pushes, post_pushes, &mut handler_error);
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            let successful_battle_result = matches!(&result, HandlerResult::Reply(_));
            let payload = handler_payload(result, request.method.as_str());
            if typed_handled && request.method == "dailycopy.CopyEnter" {
                if let Some(typed) = typed_account.as_deref() {
                    append_method_push(
                        post_pushes,
                        "dailycopy.UpdateDailyCopyData",
                        daily_copy_snapshot_payload_from_typed_account(
                            typed,
                            chapter_catalog,
                            current_unix_seconds(),
                        ),
                    );
                    append_method_push(
                        post_pushes,
                        "user.UpdateUserInfo",
                        UserInfoCodec::encode(&user_info_from_typed_account(state, typed)),
                    );
                }
            }
            if typed_handled && request.method == "copy.PassBase" {
                if let Some(typed) = typed_account.as_deref() {
                    append_method_push(
                        post_pushes,
                        "user.UpdateUserInfo",
                        UserInfoCodec::encode(&user_info_from_typed_account(state, typed)),
                    );
                }
            }
            if typed_handled
                && matches!(
                    request.method.as_str(),
                    "copy.PassBase" | "copy.PassMiniGame"
                )
                && successful_battle_result
            {
                if let (Some(copy_id), Some(catalog)) = (settled_copy_id, chapter_catalog) {
                    if let Some(typed) = typed_account.as_deref() {
                        if catalog
                            .daily_level_ids_by_chapter
                            .values()
                            .any(|copy_ids| copy_ids.contains(&copy_id))
                        {
                            append_method_push(
                                post_pushes,
                                "dailycopy.UpdateDailyCopyData",
                                daily_copy_snapshot_payload_from_typed_account(
                                    typed,
                                    Some(catalog),
                                    current_unix_seconds(),
                                ),
                            );
                        }
                        append_method_push(
                            post_pushes,
                            "copy.GetCopy",
                            copy_progress_payload_for_copy(catalog, copy_id, typed),
                        );
                    }
                }
            }
            payload
        }
        _ => {
            *handler_error_slot = handler_error;
            return (false, None);
        }
    };
    *handler_error_slot = handler_error;
    (true, result)
}
