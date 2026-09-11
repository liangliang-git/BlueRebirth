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
        "dailycopy.GetData" | "dailycopy.SelectEx" => {
            let mut daily_copy_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                let result = daily_copy_handler::handle_typed(
                    typed,
                    chapter_catalog,
                    request.method.as_str(),
                    request_args,
                    &mut daily_copy_effects,
                );
                if matches!(result, HandlerResult::PushOnly | HandlerResult::Error(_)) {
                    result
                } else {
                    HandlerResult::Error(GameError::InvalidRequest(
                        "daily copy request is not supported",
                    ))
                }
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "daily copy request requires typed account",
                ))
            };
            apply_response_effects(
                daily_copy_effects,
                pre_pushes,
                post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        "dailycopy.UpdateDailyCopyData" => typed_account.as_ref().map(|typed| {
            Response::battle_bytes(
                request.method.as_str(),
                daily_copy_snapshot_payload_from_typed_account(
                    typed,
                    chapter_catalog,
                    current_unix_seconds(),
                ),
            )
        }),
        _ => {
            *handler_error_slot = handler_error;
            return (false, None);
        }
    };
    *handler_error_slot = handler_error;
    (true, result)
}
