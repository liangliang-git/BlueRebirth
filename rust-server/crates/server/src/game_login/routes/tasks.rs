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
        affection: affection_catalog,
        ..
    } = *catalogs;
    let mut handler_error = handler_error_slot.take();
    let result = match request.method.as_str() {
        "task.TaskInfo" => {
            let result = typed_account
                .as_ref()
                .map(|typed| task_info_payload_from_typed_account(typed, task_catalog));
            match result {
                Some(payload) => response_payload(request.method.as_str(), payload),
                None => {
                    handler_error = Some(GameError::InvalidRequest(
                        "task info requires typed account",
                    ));
                    None
                }
            }
        }
        _ if method.is_family(MethodFamily::Study)
            || method.is_family(MethodFamily::Task)
            || method.is_family(MethodFamily::Bathroom) =>
        {
            let mut feature_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                if method.is_family(MethodFamily::Bathroom) {
                    progression_handler::handle_bathroom_typed(
                        typed,
                        request.method.as_str(),
                        request_args,
                        current_unix_seconds(),
                        state.mood_recovery_multiplier,
                        affection_catalog,
                        &mut feature_effects,
                    )
                } else if method.is_family(MethodFamily::Study) {
                    progression_handler::handle_study_typed(
                        typed,
                        request.method.as_str(),
                        request_args,
                        current_unix_seconds(),
                        &mut feature_effects,
                    )
                } else {
                    task_handler::handle_typed(
                        typed,
                        state,
                        request.method.as_str(),
                        request_args,
                        task_catalog,
                        fashion_catalog,
                        &mut feature_effects,
                    )
                }
            } else {
                if method.is_family(MethodFamily::Bathroom) {
                    let profile_id = blueoath_domain::ProfileId::new(state.profile_id.clone())
                        .unwrap_or_else(|_| {
                            blueoath_domain::ProfileId::new("anonymous").expect("static id")
                        });
                    let mut transient =
                        blueoath_domain::NewAccountFactory::create(profile_id, &state.name);
                    progression_handler::handle_bathroom_typed(
                        &mut transient,
                        request.method.as_str(),
                        request_args,
                        current_unix_seconds(),
                        state.mood_recovery_multiplier,
                        affection_catalog,
                        &mut feature_effects,
                    )
                } else {
                    HandlerResult::Error(GameError::InvalidRequest(
                        "progression request requires typed account",
                    ))
                }
            };
            apply_response_effects(feature_effects, pre_pushes, post_pushes, &mut handler_error);
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ => {
            *handler_error_slot = handler_error;
            return (false, None);
        }
    };
    *handler_error_slot = handler_error;
    (true, result)
}
