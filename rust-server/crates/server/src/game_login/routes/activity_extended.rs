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
        _ if method.is_family(MethodFamily::Exchange) && typed_account.is_some() => {
            let mut exchange_effects = ResponseEffects::default();
            let result = extended_handler::handle_typed_exchange(
                state,
                typed_account.as_mut().expect("typed exchange account"),
                request.method.as_str(),
                request_args,
                &mut exchange_effects,
            );
            apply_response_effects(
                exchange_effects,
                pre_pushes,
                post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::FoodCompose) && typed_account.is_some() => {
            let mut food_effects = ResponseEffects::default();
            let result = extended_handler::handle_typed_food_compose(
                state,
                typed_account.as_mut().expect("typed food compose account"),
                request.method.as_str(),
                request_args,
                &mut food_effects,
            );
            apply_response_effects(food_effects, pre_pushes, post_pushes, &mut handler_error);
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::WorldEvent) && typed_account.is_some() => {
            let mut world_event_effects = ResponseEffects::default();
            let result = extended_handler::handle_typed_world_event(
                state,
                typed_account.as_mut().expect("typed world event account"),
                request.method.as_str(),
                request_args,
                &mut world_event_effects,
            );
            apply_response_effects(
                world_event_effects,
                pre_pushes,
                post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if (method.is_family(MethodFamily::BattlePass)
            || method.is_family(MethodFamily::ActivityBattlePass))
            && typed_account.is_some() =>
        {
            let mut battlepass_effects = ResponseEffects::default();
            let result = extended_handler::handle_typed_battlepass(
                state,
                typed_account.as_mut().expect("typed battle pass account"),
                request.method.as_str(),
                request_args,
                &mut battlepass_effects,
            );
            apply_response_effects(
                battlepass_effects,
                pre_pushes,
                post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if (method.is_family(MethodFamily::Magazine)
            || method.is_family(MethodFamily::InteractionItem))
            && typed_account.is_some() =>
        {
            let mut misc_effects = ResponseEffects::default();
            let result = misc_extended_handler::handle_typed(
                state,
                typed_account.as_mut().expect("typed misc account"),
                request.method.as_str(),
                request_args,
                &mut misc_effects,
            );
            apply_response_effects(misc_effects, pre_pushes, post_pushes, &mut handler_error);
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
