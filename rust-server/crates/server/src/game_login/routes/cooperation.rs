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
        _ if typed_account.is_some() && coop_handler::handles_typed(request.method.as_str()) => {
            let mut coop_effects = ResponseEffects::default();
            let result = coop_handler::handle_typed(
                state,
                typed_account.as_mut().expect("typed co-op account"),
                request.method.as_str(),
                request_args,
                &mut coop_effects,
            );
            apply_response_effects(coop_effects, pre_pushes, post_pushes, &mut handler_error);
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::MatchServer)
            || method.is_family(MethodFamily::Room)
            || matches!(
                request.method.as_str(),
                "battle.CreateRoom"
                    | "battle.JoinRoom"
                    | "battle.LeaveRoom"
                    | "battle.MatchJoin"
                    | "battle.MatchLeave"
                    | "battle.SendAutoMsg"
                    | "battle.pvpMatchReady"
                    | "battle.pvpMatchReadyTimeout"
                    | "battle.CreateMutiBattle"
                    | "battle.createBattleInfo"
            ) =>
        {
            let result =
                HandlerResult::Error(GameError::InvalidRequest("co-op requires typed account"));
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
