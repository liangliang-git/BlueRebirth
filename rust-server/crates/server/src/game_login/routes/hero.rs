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
        _ if method.is_family(MethodFamily::Hero) || request.method == "fashion.Equip" => {
            let mut hero_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                hero_handler::handle_typed(
                    typed,
                    request.method.as_str(),
                    request_args,
                    &mut hero_effects,
                    hero_handler::HeroTypedCatalogs {
                        hero_level: hero_level_catalog,
                        tasks: task_catalog,
                        breakdown: hero_breakdown_catalog,
                        fashion: fashion_catalog,
                        ship_exp_multiplier: state.ship_exp_multiplier,
                        hero_skill_upgrade: catalogs.hero_skill_upgrade,
                        ship_intensify: catalogs.ship_intensify,
                        ship_break: catalogs.ship_break,
                        ship_advance: catalogs.ship_advance,
                        ship_remould: catalogs.ship_remould,
                        state: Some(state),
                    },
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest("hero requires typed account"))
            };
            apply_response_effects(hero_effects, pre_pushes, post_pushes, &mut handler_error);
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if request.method == "fashion.updateData" => match typed_account.as_deref() {
            Some(account) => response_payload(
                request.method.as_str(),
                FashionListCodec::encode(&fashion_list_from_typed_account(
                    account,
                    fashion_catalog,
                )),
            ),
            None => {
                handler_error = Some(GameError::AccountUnavailable);
                response_payload(request.method.as_str(), Vec::new())
            }
        },
        _ if known_method == Some(KnownMethod::TacticGetHeros) => {
            let fleet = match typed_account.as_deref() {
                Some(account) => fleet_info_from_typed_account(account),
                None => {
                    handler_error = Some(GameError::AccountUnavailable);
                    FleetInfo::default()
                }
            };
            response_payload(request.method.as_str(), FleetInfoCodec::encode(&fleet))
        }
        _ if known_method == Some(KnownMethod::BagGetInfo) => {
            let bag = match typed_account.as_deref() {
                Some(account) => bag_info_from_typed_account(account),
                None => {
                    handler_error = Some(GameError::AccountUnavailable);
                    BagInfo::default()
                }
            };
            response_payload(request.method.as_str(), BagInfoCodec::encode(&bag))
        }
        _ if known_method == Some(KnownMethod::TacticSetHeros) => {
            let fleet = match FleetInfo::decode(request_args) {
                Ok(fleet) => Some(fleet),
                Err(_) => {
                    handler_error =
                        Some(GameError::InvalidRequest("fleet tactic request is invalid"));
                    None
                }
            };
            if let Some(fleet) = fleet {
                if let Some(typed) = typed_account.as_mut() {
                    if !set_fleet_on_typed_account(typed, &fleet) {
                        handler_error = Some(GameError::InvalidRequest(
                            "fleet tactic contains invalid or unowned hero",
                        ));
                        response_payload(request.method.as_str(), Vec::new())
                    } else {
                        let payload = FleetInfoCodec::encode(&fleet_info_from_typed_account(typed));
                        append_method_push(post_pushes, "tactic.GetHerosTactic", payload.clone());
                        response_payload(request.method.as_str(), payload)
                    }
                } else {
                    handler_error = Some(GameError::AccountUnavailable);
                    response_payload(request.method.as_str(), FleetInfoCodec::encode(&fleet))
                }
            } else {
                response_payload(request.method.as_str(), Vec::new())
            }
        }
        _ if known_method == Some(KnownMethod::PresetFleetInfo) => {
            let preset = match typed_account.as_deref() {
                Some(account) => preset_fleet_info_from_typed_account(account),
                None => {
                    handler_error = Some(GameError::AccountUnavailable);
                    PresetFleetInfo::default()
                }
            };
            response_payload(request.method.as_str(), PresetFleetCodec::encode(&preset))
        }
        _ if known_method == Some(KnownMethod::PresetFleetSet) => {
            match PresetFleetCodec::decode(request_args) {
                Ok(preset) if preset.fleets.len() <= 100 => {
                    if let Some(typed) = typed_account.as_mut() {
                        if !set_preset_fleet_on_typed_account(typed, &preset) {
                            handler_error = Some(GameError::InvalidRequest(
                                "preset fleet contains invalid or unowned hero",
                            ));
                            response_payload(request.method.as_str(), Vec::new())
                        } else {
                            let payload = PresetFleetCodec::encode(
                                &preset_fleet_info_from_typed_account(typed),
                            );
                            append_method_push(
                                post_pushes,
                                "presetfleet.PresetFleetsInfo",
                                payload.clone(),
                            );
                            response_payload(request.method.as_str(), payload)
                        }
                    } else {
                        handler_error = Some(GameError::AccountUnavailable);
                        response_payload(request.method.as_str(), Vec::new())
                    }
                }
                _ => {
                    handler_error = Some(GameError::Internal(
                        "preset fleet request is invalid".to_owned(),
                    ));
                    response_payload(request.method.as_str(), Vec::new())
                }
            }
        }
        _ => {
            *handler_error_slot = handler_error;
            return (false, None);
        }
    };
    *handler_error_slot = handler_error;
    (true, result)
}
