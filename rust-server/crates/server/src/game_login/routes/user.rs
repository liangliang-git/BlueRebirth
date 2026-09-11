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
        _ if known_method == Some(KnownMethod::UserGetUserInfo) => {
            let user = match typed_account.as_deref() {
                Some(account) => user_info_from_typed_account(state, account),
                None => {
                    handler_error = Some(GameError::AccountUnavailable);
                    UserInfo::default()
                }
            };
            Some(Response::user(
                request.method.as_str(),
                user_responses::info(user),
            ))
        }
        _ if known_method == Some(KnownMethod::UserLogin) => {
            if let Some(typed) = typed_account.as_deref_mut() {
                // A dropped client can leave a persisted battle session behind.
                // This server has no resume handshake, so retaining that session
                // blocks the next sortie until its timeout and makes StartBase
                // appear unresponsive after reconnect.
                if typed.battle.active.take().is_some() {
                    tracing::debug!("cleared abandoned battle session on login");
                }
                advance_typed_task_event(typed, task_catalog, 1, 1);
                let mut login_effects = ResponseEffects::default();
                if typed.guild.is_some() {
                    guild_handler::push_guild_state_typed(&mut login_effects, typed);
                }
                append_typed_user_login_bootstrap(
                    &mut login_effects,
                    state,
                    typed,
                    chapter_catalog,
                    battle_catalog,
                );
                apply_response_effects(login_effects, pre_pushes, post_pushes, &mut handler_error);
            }
            Some(Response::user(
                request.method.as_str(),
                user_responses::login(),
            ))
        }
        "user.SetUserSecretary"
        | "user.ChangeName"
        | "user.SetMessage"
        | "user.SetPlayerHeadFrame"
        | "user.SetHead" => {
            match UserRequest::decode(request.method.as_str(), request_args) {
                Ok(Some(request)) => {
                    if let Some(account) = typed_account.as_mut() {
                        if let Err(error) = base_handler::apply_user_request(account, request) {
                            handler_error = Some(error);
                        }
                    } else {
                        handler_error = Some(GameError::AccountUnavailable);
                    }
                }
                Ok(None) | Err(_) => {
                    handler_error = Some(GameError::InvalidRequest("user request is invalid"));
                }
            }
            response_payload(request.method.as_str(), Vec::new())
        }
        _ if method.is_family(MethodFamily::User)
            || method.is_family(MethodFamily::UserServer)
            || method.is_family(MethodFamily::Strategy)
            || method.is_family(MethodFamily::SupportFleet)
            || method.is_family(MethodFamily::PresetFleet)
            || method.is_family(MethodFamily::Milestone)
            || method.is_family(MethodFamily::Supply)
            || method.is_family(MethodFamily::Jopen)
            || method.is_family(MethodFamily::Guide) =>
        {
            let mut base_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                base_handler::handle_typed(
                    typed,
                    state,
                    request.method.as_str(),
                    request_args,
                    &mut base_effects,
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "base request requires typed account",
                ))
            };
            apply_response_effects(base_effects, pre_pushes, post_pushes, &mut handler_error);
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
