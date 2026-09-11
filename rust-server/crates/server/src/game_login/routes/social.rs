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
        _ if method.is_family(MethodFamily::Guild) => {
            let mut guild_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                guild_handler::handle_typed(
                    typed,
                    request.method.as_str(),
                    request_args,
                    current_unix_seconds(),
                    &mut guild_effects,
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest("guild requires typed account"))
            };
            apply_response_effects(guild_effects, pre_pushes, post_pushes, &mut handler_error);
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::Friend) => {
            let result = if let Some(typed) = typed_account.as_mut() {
                friend_handler::handle_typed(typed, state, request.method.as_str(), request_args)
            } else {
                HandlerResult::Error(GameError::InvalidRequest("friend requires typed account"))
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::Chat) => {
            let mut chat_effects = ResponseEffects::default();
            let result = if let Some(account) = typed_account.as_mut() {
                chat_handler::handle_typed(
                    account,
                    request.method.as_str(),
                    request_args,
                    &mut chat_effects,
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest("chat requires typed account"))
            };
            apply_response_effects(chat_effects, pre_pushes, post_pushes, &mut handler_error);
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::Adventure) => {
            let result = if let Some(typed) = typed_account.as_mut() {
                adventure_handler::handle_typed(typed, request.method.as_str())
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "adventure requires typed account",
                ))
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::Boss) => {
            let result = if let Some(typed) = typed_account.as_mut() {
                let result = boss_handler::handle_typed(state, typed, request.method.as_str());
                if matches!(result, HandlerResult::Empty) {
                    HandlerResult::Error(GameError::InvalidRequest("boss request is not supported"))
                } else {
                    result
                }
            } else {
                HandlerResult::Error(GameError::InvalidRequest("boss requires typed account"))
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::GuildBox) => {
            let mut guildbox_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
                guildbox_handler::handle_typed(
                    typed,
                    state,
                    request.method.as_str(),
                    request_args,
                    catalog,
                    &mut guildbox_effects,
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "guild box requires typed account",
                ))
            };
            apply_response_effects(
                guildbox_effects,
                pre_pushes,
                post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::InviteScore) => {
            let mut invite_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                invitescore_handler::handle_typed(
                    typed,
                    request.method.as_str(),
                    request_args,
                    &mut invite_effects,
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "invite score requires typed account",
                ))
            };
            apply_response_effects(invite_effects, pre_pushes, post_pushes, &mut handler_error);
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
