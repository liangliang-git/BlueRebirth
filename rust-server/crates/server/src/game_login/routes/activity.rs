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
        _ if activity_handler::handles(request.method.as_str()) => {
            let result = if let Some(typed) = typed_account.as_mut() {
                activity_handler::handle_typed(
                    typed,
                    request.method.as_str(),
                    request_args,
                    fashion_catalog,
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "activity request requires typed account",
                ))
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if activity_extra_handler::handles(request.method.as_str()) => {
            let result = if let Some(typed) = typed_account.as_mut() {
                let result = activity_extra_handler::handle_typed(
                    state,
                    typed,
                    request.method.as_str(),
                    request_args,
                );
                if matches!(result, HandlerResult::Empty) {
                    HandlerResult::Error(GameError::InvalidRequest(
                        "activity request is not supported",
                    ))
                } else {
                    result
                }
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "activity request requires typed account",
                ))
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if typed_account.is_some() && misc_handler::handles_typed(request.method.as_str()) => {
            let result = misc_handler::handle_typed(
                typed_account.as_mut().expect("typed misc account"),
                request.method.as_str(),
                request_args,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if matches!(
            known_method,
            Some(
                KnownMethod::ArchiveCopyIsLoad
                    | KnownMethod::CopyExtraAddCopyRewardCount
                    | KnownMethod::CopyExtraUpdateCopyExtraInfo
                    | KnownMethod::SavePrefs
                    | KnownMethod::GetStatCount
                    | KnownMethod::Sign
                    | KnownMethod::StartMiniGame
                    | KnownMethod::StartAlchemy
            )
        ) =>
        {
            let result = if let Some(typed) = typed_account.as_mut() {
                misc_handler::handle_typed(typed, request.method.as_str(), request_args)
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "misc request requires typed account",
                ))
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if typed_account.is_none()
            && (method.is_family(MethodFamily::Exchange)
                || method.is_family(MethodFamily::FoodCompose)
                || method.is_family(MethodFamily::BattlePass)
                || method.is_family(MethodFamily::ActivityBattlePass)
                || method.is_family(MethodFamily::WorldEvent)
                || method.is_family(MethodFamily::Magazine)
                || method.is_family(MethodFamily::InteractionItem)) =>
        {
            let result = HandlerResult::Error(GameError::InvalidRequest(
                "extended request requires typed account",
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
