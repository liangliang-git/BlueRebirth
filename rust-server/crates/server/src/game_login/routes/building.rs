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
        _ if method.is_family(MethodFamily::Building)
            || method.is_family(MethodFamily::Build)
            || method.is_family(MethodFamily::BuildNotes)
            || method.is_family(MethodFamily::Discuss) =>
        {
            let mut building_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                let result = building_handler::handle_typed_with_multipliers(
                    Some(state),
                    typed,
                    request.method.as_str(),
                    request_args,
                    current_unix_seconds(),
                    &mut building_effects,
                    building_handler::BuildingTypedCatalogs {
                        building: catalogs.buildings,
                        oil_multiplier: state.building_oil_multiplier,
                        gold_multiplier: state.building_gold_multiplier,
                    },
                );
                if matches!(result, HandlerResult::Empty) {
                    HandlerResult::Error(GameError::InvalidRequest(
                        "building request is not supported",
                    ))
                } else {
                    result
                }
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "building request requires typed account",
                ))
            };
            apply_response_effects(
                building_effects,
                pre_pushes,
                post_pushes,
                &mut handler_error,
            );
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
