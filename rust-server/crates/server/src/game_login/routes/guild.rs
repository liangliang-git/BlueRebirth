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
        _ if typed_account.is_some()
            && guildtask_handler::handles_typed(request.method.as_str()) =>
        {
            let mut guildtask_effects = ResponseEffects::default();
            let result = guildtask_handler::handle_typed(
                typed_account.as_mut().expect("typed guild task account"),
                request.method.as_str(),
                request_args,
                &mut guildtask_effects,
            );
            apply_response_effects(
                guildtask_effects,
                pre_pushes,
                post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if guildtask_handler::handles(request.method.as_str()) => {
            let result = HandlerResult::Error(GameError::InvalidRequest(
                "guild task requires typed account",
            ));
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if typed_account.is_some()
            && guild_extension_handler::handles_typed(request.method.as_str()) =>
        {
            let result = guild_extension_handler::handle_typed(
                typed_account.as_mut().expect("typed guild offer account"),
                request.method.as_str(),
                request_args,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if guild_extension_handler::handles(request.method.as_str()) => {
            let result = HandlerResult::Error(GameError::InvalidRequest(
                "guild extension requires typed account",
            ));
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
