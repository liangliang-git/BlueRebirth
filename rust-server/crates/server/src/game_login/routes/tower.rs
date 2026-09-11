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
        _ if method.is_family(MethodFamily::TalentTree) => {
            let mut talent_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                talent_handler::handle_typed(
                    typed,
                    request.method.as_str(),
                    request_args,
                    &mut talent_effects,
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "talent tree requires typed account",
                ))
            };
            apply_response_effects(talent_effects, pre_pushes, post_pushes, &mut handler_error);
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::Tower) => {
            let result = if let Some(typed) = typed_account.as_mut() {
                tower_handler::handle_typed(
                    typed,
                    request.method.as_str(),
                    request_args,
                    chapter_catalog,
                    current_unix_seconds(),
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest("tower requires typed account"))
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::ActivityTower) => {
            let result = if let Some(typed) = typed_account.as_mut() {
                tower_handler::handle_activity_typed(
                    typed,
                    request.method.as_str(),
                    request_args,
                    current_unix_seconds(),
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "activity tower requires typed account",
                ))
            };
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
