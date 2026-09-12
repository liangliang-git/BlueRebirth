use blueoath_domain::AccountState;
use blueoath_protocol::*;
use blueoath_transport::NetSocketFrameCodec;
use tokio::io::{AsyncRead, AsyncWrite};

use super::common::error::GameError;
use super::common::request::RequestContext;
use super::common::response::Response;
use super::common::response::{BattleResponse, HandlerResult, ResponseEffects};
use super::features::user::{requests::UserRequest, responses as user_responses};
use super::features::{
    activity::{
        adventure_service as adventure_handler, extended_service as extended_handler,
        extra_service as activity_extra_handler, invite_score_service as invitescore_handler,
        misc_service as misc_extended_handler, service as activity_handler,
        ship_task_service as shiptask_handler, sports_meet_service as sportsmeet_handler,
        talent_handler,
    },
    battle::service as battle_handler,
    building::{
        buildship_service as buildship_handler, outpost_service as outpost_handler,
        service as building_handler,
    },
    cooperation::service as coop_handler,
    copy::service as daily_copy_handler,
    equip::service as equip_handler,
    hero::{compat_service as compat_feature, service as hero_handler},
    progression::service as progression_handler,
    shop::service as commerce_handler,
    social::{
        boss_handler, chat_handler, friend_handler, guild_extension_handler, guild_handler,
        guildbox_handler, guildtask_handler,
    },
    task::service as task_handler,
    tower as tower_handler,
    user::{
        misc_service as misc_handler, service as base_handler, teaching_service as teaching_handler,
    },
};

use super::game_config::*;
use super::router::{GameMethod, KnownMethod, MethodFamily};
use super::wire::*;
use super::*;
mod bootstrap;
mod copy;
mod legacy;
mod response;
#[cfg(test)]
type LoginPushes = Vec<Vec<u8>>;
#[cfg(not(test))]
type LoginPushes = Vec<Response>;

#[path = "routes/account.rs"]
mod account_routes;
#[path = "routes/activity_compat.rs"]
mod activity_compat_routes;
#[path = "routes/activity_extended.rs"]
mod activity_extended_routes;
#[path = "routes/activity.rs"]
mod activity_routes;
#[path = "routes/building.rs"]
mod building_routes;
#[path = "routes/buildship.rs"]
mod buildship_routes;
#[path = "routes/cooperation.rs"]
mod cooperation_routes;
#[path = "routes/copy_progress.rs"]
mod copy_progress_routes;
#[path = "routes/copy_rewards.rs"]
mod copy_rewards_routes;
#[path = "routes/copy.rs"]
mod copy_routes;
#[path = "routes/dailycopy.rs"]
mod dailycopy_routes;
#[path = "routes/equip.rs"]
mod equip_routes;
#[path = "routes/guild.rs"]
mod guild_routes;
#[path = "routes/hero.rs"]
mod hero_routes;
#[path = "routes/mail.rs"]
mod mail_routes;
#[path = "routes/progression.rs"]
mod progression_routes;
#[path = "routes/shop.rs"]
mod shop_routes;
#[path = "routes/social.rs"]
mod social_routes;
#[path = "routes/tasks.rs"]
mod tasks_routes;
#[path = "routes/tower.rs"]
mod tower_routes;
#[path = "routes/user.rs"]
mod user_routes;

pub(crate) use bootstrap::*;
pub(crate) use copy::*;
pub(crate) use legacy::*;
pub(crate) use response::*;

pub(super) async fn process_game_login_frame_payload_with_catalogs_typed_mut<S>(
    stream: &mut S,
    state: &ServerState,
    mut typed_account: Option<&mut AccountState>,
    frame: blueoath_transport::NetSocketFrame,
    catalogs: &GameLoginCatalogs<'_>,
) -> Result<bool, ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
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
        buildings: building_catalog,
        ..
    } = *catalogs;
    #[cfg(not(test))]
    let _ = (
        fashion_catalog,
        equip_catalog,
        hero_level_catalog,
        hero_breakdown_catalog,
        shop_catalog,
        mail_catalog,
        handbook_behaviours,
        chapter_catalog,
        task_catalog,
        battle_catalog,
        building_catalog,
    );
    #[cfg(test)]
    let _ = (
        &equip_catalog,
        &hero_level_catalog,
        &hero_breakdown_catalog,
        &mail_catalog,
        &battle_catalog,
    );

    if frame.frame_type == 2 {
        NetSocketFrameCodec::write(stream, 2, &[]).await?;
        return Ok(true);
    }
    if frame.payload.is_empty() {
        return Ok(true);
    }

    let request = RequestContext::from(TMessageCodec::decode_request(&frame.payload)?);
    let request_args = request.args.as_slice();
    if state.trace_methods {
        tracing::debug!(
            method = %request.method,
            args = request_args.len(),
            "game-login request"
        );
    }
    let method = GameMethod::parse(&request.method);
    let known_method = method.known();
    let is_user_info = known_method == Some(KnownMethod::UserGetUserInfo);
    #[cfg(test)]
    let mut pre_pushes = Vec::<Vec<u8>>::new();
    #[cfg(not(test))]
    let mut pre_pushes = Vec::<Response>::new();
    #[cfg(test)]
    let mut post_pushes = Vec::<Vec<u8>>::new();
    #[cfg(not(test))]
    let mut post_pushes = Vec::<Response>::new();
    let mut handler_error: Option<GameError> = None;
    let (mut handled, mut ret) = copy_rewards_routes::handle(
        state,
        &mut typed_account,
        &request,
        request_args,
        &method,
        known_method,
        catalogs,
        &mut pre_pushes,
        &mut post_pushes,
        &mut handler_error,
    );
    if !handled {
        let (route_handled, route_response) = activity_compat_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = account_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = hero_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = user_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = social_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = activity_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = progression_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = activity_extended_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = guild_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = mail_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = shop_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = equip_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = building_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = buildship_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = tasks_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = cooperation_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = dailycopy_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = copy_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = tower_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    if !handled {
        let (route_handled, route_response) = copy_progress_routes::handle(
            state,
            &mut typed_account,
            &request,
            request_args,
            &method,
            known_method,
            catalogs,
            &mut pre_pushes,
            &mut post_pushes,
            &mut handler_error,
        );
        if route_handled {
            handled = true;
            ret = route_response;
        }
    }
    let _ = handled;
    // Every known route must complete its callback. Unsupported routes return an
    // empty protobuf payload without mutating account state.
    if ret.is_none() {
        if method.is_known() {
            ret = response_payload(request.method.as_str(), Vec::new());
        } else {
            let error = GameError::UnknownMethod(request.method.clone());
            handler_error = Some(error.clone());

            ret = response_payload(request.method.as_str(), Vec::new());
        }
    }
    let trace_ret_len = ret
        .as_ref()
        .map(|response| response.payload.len())
        .unwrap_or_default();
    let trace_method = request.method.clone();
    let (client_error_code, client_error_message) = handler_error
        .as_ref()
        .map(|error| (error.client_code(), error.to_string()))
        .unwrap_or((0, String::new()));
    let trace_err_msg = client_error_message.clone();
    let response = ret
        .expect("known or unknown route always creates callback response")
        .encode_with_error(
            request.callback_handler,
            request.token,
            current_unix_seconds(),
            client_error_code,
            client_error_message,
        );
    if state.trace_methods {
        tracing::debug!(
            method = %trace_method,
            error_code = client_error_code,
            error_message = %trace_err_msg,
            response_bytes = trace_ret_len,
            pre_pushes = pre_pushes.len(),
            post_pushes = post_pushes.len(),
            "game-login response"
        );
    }
    #[cfg(test)]
    for push in pre_pushes {
        NetSocketFrameCodec::write(stream, 0, &push).await?;
    }
    #[cfg(not(test))]
    for push in pre_pushes {
        let wire = push.encode_push(current_unix_seconds());
        NetSocketFrameCodec::write(stream, 0, &wire).await?;
    }
    NetSocketFrameCodec::write(stream, 0, &response).await?;
    #[cfg(test)]
    if is_user_info && typed_account.is_some() {
        let typed_account = typed_account.as_deref().expect("typed account bootstrap");
        // Match the C# post-GetUserInfo bootstrap prefix. These state snapshots must
        // arrive before inventory pushes: the client enters MainStage and reads them
        // synchronously from its login state machine.
        let now = current_unix_seconds();
        let mut login_time = Vec::new();
        append_varint_field(&mut login_time, 1, u64::from(now));
        append_varint_field(&mut login_time, 2, u64::from(now.saturating_sub(3600)));
        let push = TMessageCodec::encode_response(&TResponse {
            method: "user.UpdateLoginTime".to_owned(),
            ret: Some(login_time),
            time: now,
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;

        let mut server_time = Vec::new();
        append_varint_field(&mut server_time, 1, u64::from(now));
        append_varint_field(&mut server_time, 2, u64::from(now));
        let push = TMessageCodec::encode_response(&TResponse {
            method: "user.UpdateSvrTime".to_owned(),
            ret: Some(server_time),
            time: now,
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;

        let push = TMessageCodec::encode_response(&TResponse {
            method: "user.GetUserInfo".to_owned(),
            ret: Some(UserInfoCodec::encode(&user_info_from_typed_account(
                state,
                typed_account,
            ))),
            time: now,
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;

        for (method, ret) in [
            (
                "build.BuildsInfo",
                building_handler::typed_construction_info_payload(typed_account, now),
            ),
            (
                "bathroom.BathroomInfo",
                progression_handler::bathroom_info_payload_from_typed(typed_account),
            ),
            (
                "study.GetStudyInfo",
                progression_handler::study_info_payload_from_typed(typed_account, now),
            ),
            // TaskInfo: explicit teaching-stage row + daily count. Repeated task groups
            // may be empty when this Rust profile has no task catalog; persisted teaching
            // reward ids are retained so the client does not re-offer claimed rewards.
            (
                "task.TaskInfo",
                task_info_payload_from_typed_account(typed_account, task_catalog),
            ),
        ] {
            let push = TMessageCodec::encode_response(&TResponse {
                method: method.to_owned(),
                ret: Some(ret),
                time: now,
                ..TResponse::default()
            });
            NetSocketFrameCodec::write(stream, 0, &push).await?;
        }

        let push = TMessageCodec::encode_response(&TResponse {
            method: "bag.UpdateBagData".to_owned(),
            ret: Some(BagInfoCodec::encode(&bag_info_from_typed_account(
                typed_account,
            ))),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        let push = TMessageCodec::encode_response(&TResponse {
            method: "fashion.updateData".to_owned(),
            ret: Some(FashionListCodec::encode(&fashion_list_from_typed_account(
                typed_account,
                fashion_catalog,
            ))),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        let push = TMessageCodec::encode_response(&TResponse {
            method: "equip.UpdateEquipBagData".to_owned(),
            ret: Some(EquipListCodec::encode(&equip_list_from_typed_account(
                typed_account,
            ))),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        let push = TMessageCodec::encode_response(&TResponse {
            method: "hero.UpdateHeroBagData".to_owned(),
            ret: Some(HeroBagCodec::encode(&hero_bag_from_typed_account(
                typed_account,
            ))),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        let push = TMessageCodec::encode_response(&TResponse {
            method: "building.UpdateBuildingInfo".to_owned(),
            ret: Some(UserBuildingInfoCodec::encode(
                &building_info_from_typed_account_with_catalog(
                    typed_account,
                    current_unix_seconds(),
                    building_catalog,
                ),
            )),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        let push = TMessageCodec::encode_response(&TResponse {
            method: "tactic.GetHerosTactic".to_owned(),
            ret: Some(FleetInfoCodec::encode(&fleet_info_from_typed_account(
                typed_account,
            ))),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        let push = TMessageCodec::encode_response(&TResponse {
            method: "shop.UpdateShopInfo".to_owned(),
            ret: Some(shop_info_payload(shop_catalog)),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        let push = TMessageCodec::encode_response(&TResponse {
            method: "recharge.RechargeInfo".to_owned(),
            ret: Some(vec![0x1A, 0x00]),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        let push = TMessageCodec::encode_response(&TResponse {
            method: "buildship.BuildShipInfo".to_owned(),
            ret: Some(buildship_info_payload_from_typed(
                typed_account,
                current_unix_seconds(),
            )),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        let push = TMessageCodec::encode_response(&TResponse {
            method: "presetfleet.PresetFleetsInfo".to_owned(),
            ret: Some(PresetFleetCodec::encode(
                &preset_fleet_info_from_typed_account(typed_account),
            )),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        for (method, ret) in [
            ("illustrate.IllustrateInfo", {
                let template_ids = typed_account
                    .dock
                    .heroes
                    .values()
                    .map(|hero| hero.template_id.get() as i32)
                    .collect::<Vec<_>>();
                illustrate_info_payload_for_templates(&template_ids, handbook_behaviours)
            }),
            ("illustrate.OldIllustrateInfo", Vec::new()),
            (
                "illustrate.Memory",
                story_memory_payload(chapter_catalog.map(|catalog| catalog.memories.as_slice())),
            ),
        ] {
            let push = TMessageCodec::encode_response(&TResponse {
                method: method.to_owned(),
                ret: Some(ret),
                time: now,
                ..TResponse::default()
            });
            NetSocketFrameCodec::write(stream, 0, &push).await?;
        }
        let talent_catalog = current_talent_catalog();
        let talent_payload = talent_tree_payload_typed(typed_account, &talent_catalog);
        let push = TMessageCodec::encode_response(&TResponse {
            method: "talentTree.TalentTreeAllList".to_owned(),
            ret: Some(talent_payload),
            time: now,
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
    }
    #[cfg(not(test))]
    if is_user_info {
        let typed = typed_account
            .as_deref()
            .expect("typed account required for production user bootstrap");
        write_typed_user_info_bootstrap(stream, state, typed, catalogs).await?;
    }
    #[cfg(test)]
    for push in post_pushes {
        NetSocketFrameCodec::write(stream, 0, &push).await?;
    }
    #[cfg(not(test))]
    for push in post_pushes {
        let wire = push.encode_push(current_unix_seconds());
        NetSocketFrameCodec::write(stream, 0, &wire).await?;
    }
    Ok(true)
}

pub(super) async fn process_game_login_frame_payload_with_typed_account<S>(
    stream: &mut S,
    state: &ServerState,
    typed_account: &mut AccountState,
    frame: blueoath_transport::NetSocketFrame,
    catalogs: &GameLoginCatalogs<'_>,
) -> Result<bool, ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    #[cfg(test)]
    return process_game_login_frame_payload_with_catalogs_typed_mut(
        stream,
        state,
        Some(typed_account),
        frame,
        catalogs,
    )
    .await;

    #[cfg(not(test))]
    process_game_login_frame_payload_with_catalogs_typed_mut(
        stream,
        state,
        Some(typed_account),
        frame,
        catalogs,
    )
    .await
}
