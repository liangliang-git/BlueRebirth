use blueoath_domain::AccountState;
use blueoath_protocol::*;
use blueoath_transport::NetSocketFrameCodec;
use tokio::io::{AsyncRead, AsyncWrite};

use super::catalog::*;
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
#[cfg(test)]
pub(super) use super::features::{
    cooperation::service as test_coop_handler, tower as test_tower_handler,
};
use super::router::{GameMethod, KnownMethod, MethodFamily};
use super::wire::*;
use super::*;

#[cfg(not(test))]
async fn write_typed_bootstrap_push<S>(
    stream: &mut S,
    trace_methods: bool,
    method: &'static str,
    payload: Vec<u8>,
    now: u32,
) -> Result<(), ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if trace_methods {
        tracing::debug!(method, push_bytes = payload.len(), "game-login push");
    }
    Ok(
        NetSocketFrameCodec::write(stream, 0, &Response::new(method, payload).encode_push(now))
            .await?,
    )
}

#[cfg(not(test))]
async fn write_typed_user_info_bootstrap<S>(
    stream: &mut S,
    state: &ServerState,
    account: &AccountState,
    catalogs: &GameLoginCatalogs<'_>,
) -> Result<(), ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let GameLoginCatalogs {
        fashion: fashion_catalog,
        equip: _equip_catalog,
        shop: shop_catalog,
        handbook_behaviours,
        chapters: chapter_catalog,
        tasks: task_catalog,
        ..
    } = *catalogs;
    let now = current_unix_seconds();

    let mut login_time = Vec::new();
    append_varint_field(&mut login_time, 1, u64::from(now));
    append_varint_field(&mut login_time, 2, u64::from(now.saturating_sub(3600)));
    write_typed_bootstrap_push(
        stream,
        state.trace_methods,
        "user.UpdateLoginTime",
        login_time,
        now,
    )
    .await?;

    let mut server_time = Vec::new();
    append_varint_field(&mut server_time, 1, u64::from(now));
    append_varint_field(&mut server_time, 2, u64::from(now));
    write_typed_bootstrap_push(
        stream,
        state.trace_methods,
        "user.UpdateSvrTime",
        server_time,
        now,
    )
    .await?;

    if state.trace_methods {
        let secretary_id = account
            .character
            .secretary_id
            .map(|id| id.get())
            .unwrap_or(1);
        let secretary_fashioning = account
            .dock
            .heroes
            .get(&blueoath_domain::HeroId::new(secretary_id).expect("secretary id is positive"))
            .map(|hero| hero.fashioning);
        tracing::debug!(
            heroes = account.dock.heroes.len(),
            secretary_id,
            secretary_fashioning = ?secretary_fashioning,
            "game-login hero bootstrap"
        );
    }

    macro_rules! write_payload {
        ($method:expr, $payload:expr $(,)?) => {
            write_typed_bootstrap_push(stream, state.trace_methods, $method, $payload, now).await?;
        };
    }

    write_payload!(
        "user.GetUserInfo",
        UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
    );

    for (method, payload) in [
        (
            "build.BuildsInfo",
            building_handler::typed_construction_info_payload(account, now),
        ),
        (
            "bathroom.BathroomInfo",
            progression_handler::bathroom_info_payload_from_typed(account),
        ),
        (
            "study.GetStudyInfo",
            progression_handler::study_info_payload_from_typed(account, now),
        ),
        (
            "task.TaskInfo",
            task_info_payload_from_typed_account(account, task_catalog),
        ),
    ] {
        write_payload!(method, payload);
    }

    write_payload!(
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_typed_account(account)),
    );
    write_payload!(
        "fashion.updateData",
        FashionListCodec::encode(&fashion_list_from_typed_account(account, fashion_catalog)),
    );
    write_payload!(
        "equip.UpdateEquipBagData",
        EquipListCodec::encode(&equip_list_from_typed_account(account)),
    );
    write_payload!(
        "hero.UpdateHeroBagData",
        HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
    );
    write_payload!(
        "building.UpdateBuildingInfo",
        UserBuildingInfoCodec::encode(&building_info_from_typed_account(account, now)),
    );
    write_payload!(
        "tactic.GetHerosTactic",
        FleetInfoCodec::encode(&fleet_info_from_typed_account(account)),
    );
    write_payload!("shop.UpdateShopInfo", shop_info_payload(shop_catalog),);
    write_payload!("recharge.RechargeInfo", vec![0x1A, 0x00]);
    write_payload!(
        "buildship.BuildShipInfo",
        buildship_info_payload_from_typed(account, now),
    );
    write_payload!(
        "presetfleet.PresetFleetsInfo",
        PresetFleetCodec::encode(&preset_fleet_info_from_typed_account(account)),
    );

    let template_ids = account
        .dock
        .heroes
        .values()
        .map(|hero| hero.template_id.get() as i32)
        .collect::<Vec<_>>();
    for (method, payload) in [
        (
            "illustrate.IllustrateInfo",
            illustrate_info_payload_for_templates(&template_ids, handbook_behaviours),
        ),
        ("illustrate.OldIllustrateInfo", Vec::new()),
        (
            "illustrate.Memory",
            story_memory_payload(chapter_catalog.map(|catalog| catalog.memories.as_slice())),
        ),
    ] {
        write_payload!(method, payload);
    }

    let talent_catalog = current_talent_catalog();
    write_payload!(
        "talentTree.TalentTreeAllList",
        talent_tree_payload_typed(account, &talent_catalog),
    );
    Ok(())
}

#[cfg(test)]
fn apply_response_effects(
    effects: ResponseEffects,
    pre_pushes: &mut Vec<Vec<u8>>,
    post_pushes: &mut Vec<Vec<u8>>,
    handler_error: &mut Option<GameError>,
) {
    let (pre, post, error) = effects.into_parts();
    let now = current_unix_seconds();
    pre_pushes.extend(pre.into_iter().map(|response| response.encode_push(now)));
    post_pushes.extend(post.into_iter().map(|response| response.encode_push(now)));
    if let Some(error) = error {
        *handler_error = Some(error);
    }
}

#[cfg(not(test))]
fn apply_response_effects(
    effects: ResponseEffects,
    pre_pushes: &mut Vec<Response>,
    post_pushes: &mut Vec<Response>,
    handler_error: &mut Option<GameError>,
) {
    let (pre, post, error) = effects.into_parts();
    pre_pushes.extend(pre);
    post_pushes.extend(post);
    if let Some(error) = error {
        *handler_error = Some(error);
    }
}

fn response_payload(method: &str, payload: Vec<u8>) -> Option<Response> {
    Some(Response::raw(method, payload))
}

pub(crate) fn copy_info_payload(
    catalog: &ChapterCatalog,
    copy_type: i32,
    account: &AccountState,
) -> blueoath_protocol::CopyInfoPayload {
    let passed_copy_ids = account
        .battle
        .passed_copies
        .iter()
        .filter_map(|copy_id| i32::try_from(copy_id.get()).ok())
        .collect::<Vec<_>>();
    let copy_star_levels = account
        .battle
        .copy_stars
        .iter()
        .filter_map(|(copy_id, stars)| {
            Some((
                i32::try_from(copy_id.get()).ok()?,
                i32::try_from(*stars).ok()?,
            ))
        })
        .collect::<Vec<_>>();
    let (copy_ids, max_copy_id, response_passed) = match copy_type {
        2 => (
            catalog.sea.clone(),
            copy_progress_max_or_initial(&catalog.sea, &passed_copy_ids, catalog.sea_initial),
            passed_copy_ids.to_vec(),
        ),
        33 => (
            catalog.mubar.clone(),
            catalog.mubar.iter().copied().max().unwrap_or_default(),
            catalog.mubar.clone(),
        ),
        10 => (
            catalog.goods_copy.clone(),
            catalog.goods_copy.iter().copied().max().unwrap_or_default(),
            catalog.goods_copy.clone(),
        ),
        24 => (
            catalog.tower.clone(),
            catalog.tower.iter().copied().max().unwrap_or_default(),
            catalog.tower.clone(),
        ),
        34 => (
            catalog.equip_new_test.clone(),
            catalog
                .equip_new_test
                .iter()
                .copied()
                .max()
                .unwrap_or_default(),
            catalog.equip_new_test.clone(),
        ),
        9 => (
            catalog.daily.clone(),
            catalog.daily.iter().copied().max().unwrap_or_default(),
            catalog.daily.clone(),
        ),
        _ => (
            catalog.plot.clone(),
            copy_progress_max_or_first(&catalog.plot, &passed_copy_ids),
            passed_copy_ids.to_vec(),
        ),
    };
    blueoath_protocol::CopyInfoPayload {
        copy_type,
        chapter_star_infos: copy_chapter_star_infos(catalog, &copy_ids, account),
        copy_ids,
        max_copy_id,
        passed_copy_ids: response_passed,
        passed_copy_counts: Vec::new(),
        copy_star_levels,
        difficulty: if copy_type == 2 {
            account.sea.difficulty.max(1) as i32
        } else {
            1
        },
    }
}

fn copy_chapter_star_infos(
    catalog: &ChapterCatalog,
    copy_ids: &[i32],
    account: &AccountState,
) -> Vec<blueoath_protocol::CopyChapterStarInfo> {
    catalog
        .star_rewards_by_chapter
        .iter()
        .filter(|(_, chapter)| chapter.level_ids.iter().any(|id| copy_ids.contains(id)))
        .map(|(chapter_id, chapter)| {
            let passed_ids = chapter.level_ids.iter().filter_map(|copy_id| {
                let copy_id = u64::try_from(*copy_id)
                    .ok()
                    .and_then(|id| blueoath_domain::CopyId::new(id).ok())?;
                account
                    .battle
                    .passed_copies
                    .contains(&copy_id)
                    .then_some(copy_id)
            });
            let passed_ids = passed_ids.collect::<Vec<_>>();
            let star_num = passed_ids
                .iter()
                .map(|copy_id| {
                    account
                        .battle
                        .copy_stars
                        .get(copy_id)
                        .copied()
                        .unwrap_or(7)
                        .min(7)
                        .count_ones() as i32
                })
                .sum();
            let claimed_reward_indexes = account
                .battle
                .claimed_star_rewards
                .iter()
                .filter_map(|(claimed_chapter, index)| {
                    (*claimed_chapter == *chapter_id as u32)
                        .then(|| i32::try_from(*index).ok())
                        .flatten()
                })
                .collect();
            blueoath_protocol::CopyChapterStarInfo {
                chapter_id: *chapter_id,
                star_num,
                claimed_reward_indexes,
                pass_num: i32::try_from(passed_ids.len()).unwrap_or(i32::MAX),
            }
        })
        .collect()
}

fn handler_payload(result: HandlerResult, method: &str) -> Option<Response> {
    result.into_response(method)
}

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
        ..
    } = *catalogs;
    #[cfg(not(test))]
    let _ = (fashion_catalog, handbook_behaviours);

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
    let mut ret = match request.method.as_str() {
        _ if typed_account.is_some()
            && matches!(
                request.method.as_str(),
                "copy.StarReward" | "copy.FetchRewardBox"
            ) =>
        {
            let mut copy_star_effects = ResponseEffects::default();
            let result = battle_handler::handle_typed_copy_star_reward(
                typed_account.as_mut().expect("typed copy account"),
                request.method.as_str(),
                request_args,
                chapter_catalog,
                task_catalog,
                &mut copy_star_effects,
            );
            apply_response_effects(
                copy_star_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if typed_account.is_some()
            && matches!(request.method.as_str(), "copy.DotBase" | "copyinfo.DotBase") =>
        {
            if CopyIdRequest::decode(request_args).is_err() {
                handler_error = Some(GameError::InvalidRequest("copy id is invalid"));
            }
            response_payload(request.method.as_str(), Vec::new())
        }
        _ if typed_account.is_some()
            && activity_handler::handles_typed(request.method.as_str()) =>
        {
            let result = activity_handler::handle_typed(
                typed_account.as_mut().expect("typed activity account"),
                request.method.as_str(),
                request_args,
                catalogs.fashion,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if typed_account.is_some() && compat_feature::handles_typed(request.method.as_str()) => {
            let mut compat_effects = ResponseEffects::default();
            let result = compat_feature::handle_typed(
                state,
                typed_account.as_mut().expect("typed compat account"),
                request.method.as_str(),
                request_args,
                catalogs.affection,
                catalogs.combination,
                &mut compat_effects,
            );
            apply_response_effects(
                compat_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if typed_account.is_some() && legacy_only_method(request.method.as_str()) => {
            handler_error = Some(GameError::InvalidRequest(
                "request family has no typed handler",
            ));
            response_payload(request.method.as_str(), Vec::new())
        }
        _ if known_method == Some(KnownMethod::PlayerLogin) => Some(Response::user(
            request.method.as_str(),
            user_responses::player_login(state.profile_id.clone()),
        )),
        _ if known_method == Some(KnownMethod::PlayerGetUserList) => {
            let user = match typed_account.as_deref() {
                Some(account) => user_info_from_typed_account(state, account),
                None => {
                    handler_error = Some(GameError::AccountUnavailable);
                    UserInfo::default()
                }
            };
            Some(Response::user(
                request.method.as_str(),
                user_responses::user_list(user),
            ))
        }
        _ if known_method == Some(KnownMethod::PlayerCreateUser) => {
            let user = match typed_account.as_deref() {
                Some(account) => user_info_from_typed_account(state, account),
                None => {
                    handler_error = Some(GameError::AccountUnavailable);
                    UserInfo::default()
                }
            };
            Some(Response::user(
                request.method.as_str(),
                user_responses::player(user),
            ))
        }
        _ if known_method == Some(KnownMethod::CacheData)
            || known_method == Some(KnownMethod::RepairHero)
            || matches!(
                request.method.as_str(),
                "user.GetHeadBuyCount"
                    | "user.BuyHead"
                    | "user.NewHeadUnlockedList"
                    | "hero.Marry"
                    | "hero.AddAffection"
                    | "hero.HeroCombine"
                    | "hero.HeroCombineBreak"
                    | "hero.HeroCombineQuickLevelUp"
                    | "hero.HeroCombineUpLv"
                    | "illustrate.VowHero"
                    | "illustrate.VowDecTime"
                    | "illustrate.AddBehaviour"
                    | "illustrate.ModiVowHeroList"
                    | "illustrate.IllustrateNew"
                    | "illustrate.EquipNew"
                    | "fashion.fashionReplaceReward"
                    | "bag.GetNormalTreasureInfo"
                    | "bag.GetSelectTreasureInfo"
                    | "copy.DotBase"
                    | "copyinfo.DotBase"
                    | "copy.FetchRewardBox"
                    | "copy.PassMiniGame"
                    | "copy.StarReward"
                    | "task.GetPtReward"
                    | "task.GetTeachingTask"
            ) =>
        {
            let result = if typed_account.is_some() {
                HandlerResult::Error(GameError::InvalidRequest("compat request is not supported"))
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "compat request requires typed account",
                ))
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
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
            apply_response_effects(
                hero_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
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
                        append_method_push(
                            &mut post_pushes,
                            "tactic.GetHerosTactic",
                            payload.clone(),
                        );
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
                                &mut post_pushes,
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
                apply_response_effects(
                    login_effects,
                    &mut pre_pushes,
                    &mut post_pushes,
                    &mut handler_error,
                );
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
            apply_response_effects(
                base_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
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
            apply_response_effects(
                guild_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
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
            apply_response_effects(
                chat_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
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
                &mut pre_pushes,
                &mut post_pushes,
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
            apply_response_effects(
                invite_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
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
        _ if method.is_family(MethodFamily::TeachingServer) => {
            let result = if let Some(account) = typed_account.as_mut() {
                teaching_handler::handle_typed(
                    state,
                    account,
                    request.method.as_str(),
                    request_args,
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest("teaching requires typed account"))
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::Outpost) => {
            let result = if let Some(typed) = typed_account.as_mut() {
                outpost_handler::handle_typed(typed, request.method.as_str(), request_args)
            } else {
                HandlerResult::Error(GameError::InvalidRequest("outpost requires typed account"))
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::ShipTask) => {
            let mut shiptask_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                shiptask_handler::handle_typed(
                    typed,
                    state,
                    request.method.as_str(),
                    request_args,
                    &mut shiptask_effects,
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "ship task requires typed account",
                ))
            };
            apply_response_effects(
                shiptask_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::SportsMeet)
            || method.is_family(MethodFamily::SportsMeetRank) =>
        {
            let mut sportsmeet_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
                sportsmeet_handler::handle_typed(
                    state,
                    typed,
                    catalog,
                    request.method.as_str(),
                    request_args,
                    &mut sportsmeet_effects,
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "sports meet requires typed account",
                ))
            };
            apply_response_effects(
                sportsmeet_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
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
                &mut pre_pushes,
                &mut post_pushes,
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
            apply_response_effects(
                food_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
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
                &mut pre_pushes,
                &mut post_pushes,
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
                &mut pre_pushes,
                &mut post_pushes,
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
            apply_response_effects(
                misc_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
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
                &mut pre_pushes,
                &mut post_pushes,
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
        _ if typed_account.is_some()
            && matches!(
                request.method.as_str(),
                "mail.FetchItem" | "mail.FetchAllItems"
            ) =>
        {
            let fetch_one = request.method == "mail.FetchItem";
            let mid = MailIdRequest::decode(request_args)
                .map(|request| request.mail_id)
                .unwrap_or_default();
            let rewards = {
                let account = typed_account.as_mut().expect("typed mail account");
                mail_catalog
                    .unwrap_or_default()
                    .iter()
                    .filter(|mail| !fetch_one || mail.mid == mid)
                    .filter_map(|mail| apply_typed_mail_reward(account, mail))
                    .collect::<Vec<_>>()
            };
            if rewards.is_empty() {
                handler_error = Some(GameError::Internal(if fetch_one {
                    "mail was not found".to_owned()
                } else {
                    "mail list is empty".to_owned()
                }));
            } else {
                let account = typed_account.as_deref().expect("typed mail account");
                append_method_push(
                    &mut pre_pushes,
                    "user.UpdateUserInfo",
                    UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
                );
                append_method_push(
                    &mut pre_pushes,
                    "bag.UpdateBagData",
                    BagInfoCodec::encode(&bag_info_from_typed_account(account)),
                );
            }
            response_payload(
                request.method.as_str(),
                encode_mail_list_response(
                    mail_catalog.unwrap_or_default(),
                    current_unix_seconds(),
                    &rewards,
                ),
            )
        }
        "mail.GetMailList"
        | "mail.OpenMail"
        | "mail.DeleteMail"
        | "mail.DeleteAllMail"
        | "mail.ReceiveNewMail" => response_payload(
            request.method.as_str(),
            encode_mail_list_response(
                mail_catalog.unwrap_or_default(),
                current_unix_seconds(),
                &[],
            ),
        ),
        _ if method.is_family(MethodFamily::Shop)
            || method.is_family(MethodFamily::Recharge)
            || known_method == Some(KnownMethod::BagGetInfo)
            || request.method == "bag.CompositeItem"
            || request.method == "bag.SaleBagItem" =>
        {
            let mut commerce_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                let result = commerce_handler::handle_typed(
                    typed,
                    state,
                    request.method.as_str(),
                    request_args,
                    &mut commerce_effects,
                    commerce_handler::CommerceTypedCatalogs {
                        shop: shop_catalog,
                        fashion: fashion_catalog,
                    },
                );
                if matches!(result, HandlerResult::Empty) {
                    HandlerResult::Error(GameError::InvalidRequest(
                        "commerce request is not supported",
                    ))
                } else {
                    result
                }
            } else {
                HandlerResult::Error(GameError::InvalidRequest("commerce requires typed account"))
            };
            apply_response_effects(
                commerce_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::Equip)
            || method.is_family(MethodFamily::EquipTestCopy)
            || method.is_family(MethodFamily::EquipNewTestCopy)
            || method.is_family(MethodFamily::EquipActivity) =>
        {
            let mut equip_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                equip_handler::handle_typed(
                    typed,
                    request.method.as_str(),
                    request_args,
                    &mut equip_effects,
                    equip_catalog,
                    task_catalog,
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "equip request requires typed account",
                ))
            };
            apply_response_effects(
                equip_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::Building)
            || method.is_family(MethodFamily::Build)
            || method.is_family(MethodFamily::BuildNotes)
            || method.is_family(MethodFamily::Discuss) =>
        {
            let mut building_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                let result = building_handler::handle_typed_with_multipliers(
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
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::BuildShip) => {
            let mut buildship_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                buildship_handler::handle_typed(
                    typed,
                    request.method.as_str(),
                    request_args,
                    current_unix_seconds(),
                    task_catalog,
                    &mut buildship_effects,
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "buildship requires typed account",
                ))
            };
            apply_response_effects(
                buildship_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        "task.TaskInfo" => {
            let result = typed_account
                .as_ref()
                .map(|typed| task_info_payload_from_typed_account(typed, task_catalog));
            match result {
                Some(payload) => response_payload(request.method.as_str(), payload),
                None => {
                    handler_error = Some(GameError::InvalidRequest(
                        "task info requires typed account",
                    ));
                    None
                }
            }
        }
        _ if method.is_family(MethodFamily::Study)
            || method.is_family(MethodFamily::Task)
            || method.is_family(MethodFamily::Bathroom) =>
        {
            let mut feature_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                if method.is_family(MethodFamily::Bathroom) {
                    progression_handler::handle_bathroom_typed(
                        typed,
                        request.method.as_str(),
                        request_args,
                        current_unix_seconds(),
                        state.mood_recovery_multiplier,
                        &mut feature_effects,
                    )
                } else if method.is_family(MethodFamily::Study) {
                    progression_handler::handle_study_typed(
                        typed,
                        request.method.as_str(),
                        request_args,
                        current_unix_seconds(),
                        &mut feature_effects,
                    )
                } else {
                    task_handler::handle_typed(
                        typed,
                        state,
                        request.method.as_str(),
                        request_args,
                        task_catalog,
                        fashion_catalog,
                        &mut feature_effects,
                    )
                }
            } else {
                if method.is_family(MethodFamily::Bathroom) {
                    let profile_id = blueoath_domain::ProfileId::new(state.profile_id.clone())
                        .unwrap_or_else(|_| {
                            blueoath_domain::ProfileId::new("anonymous").expect("static id")
                        });
                    let mut transient =
                        blueoath_domain::NewAccountFactory::create(profile_id, &state.name);
                    progression_handler::handle_bathroom_typed(
                        &mut transient,
                        request.method.as_str(),
                        request_args,
                        current_unix_seconds(),
                        state.mood_recovery_multiplier,
                        &mut feature_effects,
                    )
                } else {
                    HandlerResult::Error(GameError::InvalidRequest(
                        "progression request requires typed account",
                    ))
                }
            };
            apply_response_effects(
                feature_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if typed_account.is_some() && coop_handler::handles_typed(request.method.as_str()) => {
            let mut coop_effects = ResponseEffects::default();
            let result = coop_handler::handle_typed(
                state,
                typed_account.as_mut().expect("typed co-op account"),
                request.method.as_str(),
                request_args,
                &mut coop_effects,
            );
            apply_response_effects(
                coop_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
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
        "dailycopy.GetData" | "dailycopy.SelectEx" => {
            let mut daily_copy_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                let result = daily_copy_handler::handle_typed(
                    typed,
                    chapter_catalog,
                    request.method.as_str(),
                    request_args,
                    &mut daily_copy_effects,
                );
                if matches!(result, HandlerResult::PushOnly | HandlerResult::Error(_)) {
                    result
                } else {
                    HandlerResult::Error(GameError::InvalidRequest(
                        "daily copy request is not supported",
                    ))
                }
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "daily copy request requires typed account",
                ))
            };
            apply_response_effects(
                daily_copy_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        "dailycopy.UpdateDailyCopyData" => typed_account.as_ref().map(|typed| {
            Response::battle_bytes(
                request.method.as_str(),
                daily_copy_snapshot_payload_from_typed_account(
                    typed,
                    chapter_catalog,
                    current_unix_seconds(),
                ),
            )
        }),
        _ if method.is_family(MethodFamily::MopUp) && typed_account.is_some() => {
            let mut mop_up_effects = ResponseEffects::default();
            let result = battle_handler::handle_typed_mop_up(
                state,
                typed_account.as_mut().expect("typed mop up account"),
                request.method.as_str(),
                request_args,
                battle_catalog,
                &mut mop_up_effects,
            );
            apply_response_effects(
                mop_up_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            handler_payload(result, request.method.as_str())
        }
        _ if method.is_family(MethodFamily::Copy)
            && !matches!(
                request.method.as_str(),
                "copy.ChooseSfLv" | "copy.GetCopy" | "copy.UnLockCopy"
            )
            || method.is_family(MethodFamily::MopUp)
            || method.is_family(MethodFamily::DailyCopy)
            || request.method == "copyinfo.GetCopyInfo" =>
        {
            let mut typed_handled = false;
            let mut battle_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                let result = battle_handler::handle_typed_with_catalog(
                    typed,
                    request.method.as_str(),
                    request_args,
                    battle_handler::TypedBattleContext::new(
                        battle_catalog,
                        fashion_catalog,
                        hero_level_catalog,
                        state.drop_multiplier,
                        state.ship_stat_multiplier,
                        state.commander_exp_multiplier,
                        state.ship_exp_multiplier,
                        &mut battle_effects,
                    ),
                );
                if !matches!(result, HandlerResult::Empty) {
                    typed_handled = true;
                    result
                } else {
                    HandlerResult::Error(GameError::InvalidRequest(
                        "battle request is not supported",
                    ))
                }
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "battle request requires typed account",
                ))
            };
            apply_response_effects(
                battle_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            let payload = handler_payload(result, request.method.as_str());
            if typed_handled && request.method == "dailycopy.CopyEnter" {
                if let Some(typed) = typed_account.as_deref() {
                    append_method_push(
                        &mut post_pushes,
                        "dailycopy.UpdateDailyCopyData",
                        daily_copy_snapshot_payload_from_typed_account(
                            typed,
                            chapter_catalog,
                            current_unix_seconds(),
                        ),
                    );
                    append_method_push(
                        &mut post_pushes,
                        "user.UpdateUserInfo",
                        UserInfoCodec::encode(&user_info_from_typed_account(state, typed)),
                    );
                }
            }
            if typed_handled && request.method == "copy.PassBase" {
                if let Some(typed) = typed_account.as_deref() {
                    append_method_push(
                        &mut post_pushes,
                        "user.UpdateUserInfo",
                        UserInfoCodec::encode(&user_info_from_typed_account(state, typed)),
                    );
                }
            }
            payload
        }
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
            apply_response_effects(
                talent_effects,
                &mut pre_pushes,
                &mut post_pushes,
                &mut handler_error,
            );
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
        "copy.ChooseSfLv" => {
            let (copy_id, requested) = match SeaDifficultyRequest::decode(request_args) {
                Ok(request) => (request.copy_id, request.difficulty),
                Err(_) => {
                    handler_error = Some(GameError::Internal(
                        "sea difficulty request is invalid".to_owned(),
                    ));
                    (-1, 0)
                }
            };
            let known_copy = chapter_catalog
                .map(|catalog| catalog.sea.contains(&copy_id))
                .unwrap_or(copy_id > 0);
            if copy_id < 0 {
                response_payload(request.method.as_str(), Vec::new())
            } else if !known_copy {
                handler_error = Some(GameError::Internal("sea copy is invalid".to_owned()));
                response_payload(request.method.as_str(), Vec::new())
            } else if !(1..=7).contains(&requested) {
                handler_error = Some(GameError::Internal("sea difficulty is invalid".to_owned()));
                response_payload(request.method.as_str(), Vec::new())
            } else if let Some(account) = typed_account.as_deref_mut() {
                let level = i32::try_from(account.character.level).unwrap_or(i32::MAX);
                if level < SEA_DIFFICULTY_UNLOCK_LEVEL && requested > 1 {
                    handler_error = Some(GameError::Internal(
                        "sea difficulty unlocks at commander level 60".to_owned(),
                    ));
                    response_payload(request.method.as_str(), Vec::new())
                } else {
                    account.sea.difficulty = requested as u32;
                    let fallback_catalog;
                    let catalog = match chapter_catalog {
                        Some(catalog) => catalog,
                        None => {
                            fallback_catalog = ChapterCatalog::fallback();
                            &fallback_catalog
                        }
                    };
                    let payload = copy_info_payload(catalog, 2, account);
                    append_method_push(
                        &mut post_pushes,
                        "copy.GetCopy",
                        CopyInfoCodec::encode_payload(&payload),
                    );
                    response_payload(request.method.as_str(), Vec::new())
                }
            } else {
                handler_error = Some(GameError::AccountUnavailable);
                response_payload(request.method.as_str(), Vec::new())
            }
        }
        "copy.GetCopy" => {
            let fallback_catalog;
            let catalog = match chapter_catalog {
                Some(catalog) => catalog,
                None => {
                    fallback_catalog = ChapterCatalog::fallback();
                    &fallback_catalog
                }
            };
            let copy_type = match CopyTypeRequest::decode(request_args) {
                Ok(request) => request.copy_type.max(1),
                Err(_) => {
                    handler_error = Some(GameError::InvalidRequest("copy type is invalid"));
                    1
                }
            };
            Some(Response::battle(
                request.method.as_str(),
                BattleResponse::CopyInfo(
                    typed_account
                        .as_deref()
                        .map(|account| copy_info_payload(catalog, copy_type, account))
                        .unwrap_or_default(),
                ),
            ))
        }
        "copy.UnLockCopy" => {
            let fallback_catalog;
            let catalog = match chapter_catalog {
                Some(catalog) => catalog,
                None => {
                    fallback_catalog = ChapterCatalog::fallback();
                    &fallback_catalog
                }
            };
            Some(Response::battle(
                request.method.as_str(),
                BattleResponse::CopyInfo(
                    typed_account
                        .as_deref()
                        .map(|account| copy_info_payload(catalog, 1, account))
                        .unwrap_or_default(),
                ),
            ))
        }
        _ => None,
    };
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
                &building_info_from_typed_account(typed_account, current_unix_seconds()),
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

fn append_typed_user_login_bootstrap(
    effects: &mut ResponseEffects,
    state: &ServerState,
    account: &AccountState,
    chapter_catalog: Option<&ChapterCatalog>,
    battle_catalog: Option<&BattleCatalog>,
) {
    let fallback_catalog;
    let catalog = match chapter_catalog {
        Some(catalog) => catalog,
        None => {
            fallback_catalog = ChapterCatalog::fallback();
            &fallback_catalog
        }
    };
    let now = current_unix_seconds();
    effects.push_pre(super::common::response::Response::raw(
        "user.UpdateUserInfo",
        UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
    ));
    effects.push_pre(super::common::response::Response::raw(
        "guide.GuideInfo",
        GuideInfoCodec::encode_initial_progress_completed(),
    ));
    let last_copy_id = account
        .battle
        .active
        .as_ref()
        .map(|active| active.copy_id.get())
        .or_else(|| {
            account
                .battle
                .records
                .last()
                .map(|record| record.copy_id.get())
        })
        .and_then(|copy_id| i32::try_from(copy_id).ok());
    let preferred_type = last_copy_id
        .and_then(|copy_id| {
            battle_catalog
                .and_then(|battle| battle.copies.get(&copy_id).map(|copy| copy.copy_type))
                .or_else(|| catalog.plot.contains(&copy_id).then_some(1))
                .or_else(|| catalog.sea.contains(&copy_id).then_some(2))
                .or_else(|| catalog.mubar.contains(&copy_id).then_some(33))
                .or_else(|| catalog.daily.contains(&copy_id).then_some(9))
        })
        .map(|copy_type| match copy_type {
            2 | 32 | 69 | 71 => 2,
            9 | 33 => copy_type,
            _ => 1,
        })
        .unwrap_or(1);
    let copy_bottom_index = match preferred_type {
        2 => 2,
        33 => 3,
        9 => 4,
        _ => 1,
    };
    let prefs = account
        .guide
        .settings
        .get(misc_handler::CLIENT_PREFS_SETTING_KEY)
        .cloned()
        .unwrap_or_else(|| format!(r#"{{"NewCopyButtomIndex":{copy_bottom_index}}}"#));
    let mut prefs_payload = Vec::new();
    append_message_field(&mut prefs_payload, 1, prefs.as_bytes());
    append_varint_field(&mut prefs_payload, 2, u64::from(now));
    effects.push_pre(super::common::response::Response::raw(
        "prefs.UpdatePrefsInfo",
        prefs_payload,
    ));
    let mut copy_pushes = [1, 2, 33, 9]
        .into_iter()
        .map(|copy_type| {
            (
                copy_type,
                CopyInfoCodec::encode_payload(&copy_info_payload(catalog, copy_type, account)),
            )
        })
        .collect::<Vec<_>>();
    copy_pushes.sort_by_key(|(copy_type, _)| *copy_type != preferred_type);
    for (_, payload) in copy_pushes {
        effects.push_pre(super::common::response::Response::raw(
            "copy.GetCopy",
            payload,
        ));
    }
    effects.push_pre(super::common::response::Response::raw(
        "dailycopy.UpdateDailyCopyData",
        daily_copy_snapshot_payload_from_typed_account(account, chapter_catalog, now),
    ));
    effects.push_pre(super::common::response::Response::raw(
        "illustrate.IllustrateInfo",
        Vec::new(),
    ));
    effects.push_pre(super::common::response::Response::raw(
        "illustrate.OldIllustrateInfo",
        Vec::new(),
    ));
    effects.push_pre(super::common::response::Response::raw(
        "illustrate.Memory",
        story_memory_payload(chapter_catalog.map(|catalog| catalog.memories.as_slice())),
    ));
}

fn legacy_only_method(method: &str) -> bool {
    if coop_handler::handles_typed(method) {
        return false;
    }
    if activity_handler::handles_typed(method) || compat_feature::handles_typed(method) {
        return false;
    }
    if misc_handler::handles_typed(method) {
        return false;
    }
    if method == "copy.PassMiniGame" {
        return false;
    }
    if matches!(
        method,
        "copy.StarReward" | "copy.FetchRewardBox" | "copy.DotBase" | "copyinfo.DotBase"
    ) {
        return false;
    }
    if matches!(
        method,
        "copy.AttackBase"
            | "copy.GetCopy"
            | "copy.PassBase"
            | "copy.PvpStartBase"
            | "copy.StartBase"
            | "copy.QuitBase"
            | "copy.GetRandomFactors"
            | "copy.ChooseSfLv"
            | "copy.UnLockCopy"
            | "copy.GetRecord"
            | "copy.DeleteRecord"
            | "copy.TacticOn"
            | "copyinfo.GetCopyInfo"
            | "dailycopy.CopyEnter"
            | "dailycopy.GetData"
            | "dailycopy.SelectEx"
            | "dailycopy.UpdateDailyCopyData"
            | "bag.GetBagInfo"
            | "player.CreateUser"
            | "player.GetUserList"
            | "player.Login"
            | "tactic.GetHerosTactic"
            | "tactic.SetHerosTactic"
    ) {
        return false;
    }
    if activity_handler::handles(method) {
        return true;
    }
    if GameMethod::parse(method).family() == MethodFamily::Unknown {
        return true;
    }
    matches!(
        GameMethod::parse(method).family(),
        MethodFamily::MatchServer
            | MethodFamily::Room
            | MethodFamily::Battle
            | MethodFamily::Copy
            | MethodFamily::DailyCopy
    )
}

#[cfg(test)]
mod route_guard_tests {
    use super::*;
    use blueoath_domain::{CopyId, CopyRecordState, NewAccountFactory, ProfileId};

    #[test]
    fn typed_runtime_rejects_unknown_and_legacy_exact_routes() {
        assert!(!legacy_only_method("repair.RepairHero"));
        assert!(!legacy_only_method("archiveCopy.IsLoad"));
        assert!(!legacy_only_method("player.Login"));
        assert!(!legacy_only_method("copy.GetCopy"));
        assert!(!legacy_only_method("matchsvr.CreateRoom"));
        assert!(!legacy_only_method("matchsvr_7.Ready"));
        assert!(!legacy_only_method("room.StartMatch"));
        assert!(!legacy_only_method("copy.DotBase"));
        assert!(!legacy_only_method("copyinfo.DotBase"));
    }

    #[test]
    fn login_bootstrap_prioritizes_last_battle_copy_type() {
        let mut account =
            NewAccountFactory::create(ProfileId::new("last-sortie").unwrap(), "Captain");
        account.battle.records.push(CopyRecordState {
            copy_id: CopyId::new(1_600_400).unwrap(),
            hero_ids: Vec::new(),
            pass_time: 60,
            secret_id: 0,
            strategy_id: 0,
            power: 0,
            record_time: 1,
            ex_buffs: Vec::new(),
        });
        let catalog = ChapterCatalog {
            plot: vec![1],
            sea: vec![1_600_400],
            ..ChapterCatalog::default()
        };
        let mut effects = ResponseEffects::default();

        append_typed_user_login_bootstrap(
            &mut effects,
            &ServerState::new("last-sortie", "Captain", "test"),
            &account,
            Some(&catalog),
            None,
        );

        let (pre, _, _) = effects.into_parts();
        let first_copy = pre
            .into_iter()
            .find(|response| response.method == "copy.GetCopy")
            .expect("copy bootstrap response");
        let fields =
            blueoath_protocol::decode_varint_fields(&first_copy.payload.into_bytes()).unwrap();
        assert_eq!(fields.get(&3), Some(&vec![2]));
    }

    #[test]
    fn login_bootstrap_includes_saved_preferences() {
        let mut account =
            NewAccountFactory::create(ProfileId::new("saved-prefs").unwrap(), "Captain");
        account.guide.settings.insert(
            "__client_prefs".to_owned(),
            r#"{"NewCopyButtomIndex":2}"#.to_owned(),
        );
        let mut effects = ResponseEffects::default();

        append_typed_user_login_bootstrap(
            &mut effects,
            &ServerState::new("saved-prefs", "Captain", "test"),
            &account,
            None,
            None,
        );

        let (pre, _, _) = effects.into_parts();
        let prefs = pre
            .into_iter()
            .find(|response| response.method == "prefs.UpdatePrefsInfo")
            .expect("saved preferences bootstrap push");
        assert_eq!(
            decode_string_field(&prefs.payload.into_bytes(), 1).as_deref(),
            Some(r#"{"NewCopyButtomIndex":2}"#)
        );
    }

    #[test]
    fn sea_copy_bootstrap_includes_accumulated_chapter_stars() {
        let mut account =
            NewAccountFactory::create(ProfileId::new("sea-stars").unwrap(), "Captain");
        for copy_id in [1_600_100, 1_600_200, 1_600_300] {
            let copy_id = CopyId::new(copy_id).unwrap();
            account.battle.passed_copies.insert(copy_id);
            account.battle.copy_stars.insert(copy_id, 7);
        }
        account.battle.claimed_star_rewards.insert((1001, 1));
        let catalog = ChapterCatalog {
            sea: vec![1_600_100, 1_600_200, 1_600_300],
            star_rewards_by_chapter: [(
                1001,
                ChapterStarRewards {
                    level_ids: vec![1_600_100, 1_600_200, 1_600_300],
                    star_conditions: vec![10, 20, 30],
                    reward_ids: vec![1, 2, 3],
                },
            )]
            .into_iter()
            .collect(),
            ..ChapterCatalog::default()
        };
        let mut effects = ResponseEffects::default();

        append_typed_user_login_bootstrap(
            &mut effects,
            &ServerState::new("sea-stars", "Captain", "test"),
            &account,
            Some(&catalog),
            None,
        );

        let (pre, _, _) = effects.into_parts();
        let sea = pre
            .into_iter()
            .filter(|response| response.method == "copy.GetCopy")
            .find(|response| decode_varint_field(&response.payload.clone().into_bytes(), 3) == 2)
            .expect("sea copy bootstrap push");
        let chapter = decode_repeated_message_field(&sea.payload.into_bytes(), 4)
            .into_iter()
            .next()
            .expect("chapter star info");
        assert_eq!(decode_varint_field(&chapter, 1), 1001);
        assert_eq!(decode_varint_field(&chapter, 2), 9);
        let claimed = decode_repeated_message_field(&chapter, 3);
        assert_eq!(claimed.len(), 1);
        assert_eq!(decode_varint_field(&claimed[0], 1), 1);
    }
}
