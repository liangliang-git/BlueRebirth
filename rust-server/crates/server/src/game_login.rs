use blueoath_domain::AccountState;
#[cfg(test)]
use blueoath_domain::{ChapterId, CopyId, FleetId};
use blueoath_game::BattleService;
use blueoath_protocol::*;
use blueoath_transport::NetSocketFrameCodec;
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};

use super::catalog::*;
use super::common::error::GameError;
use super::common::request::RequestContext;
#[cfg(not(test))]
use super::common::response::Response;
use super::common::response::{HandlerResult, ResponseEffects};
use super::router::{GameMethod, KnownMethod, MethodFamily};
use super::wire::*;
use super::*;

#[path = "activity_extra_handler.rs"]
mod activity_extra_handler;
#[path = "activity_handler.rs"]
mod activity_handler;
#[path = "adventure_handler.rs"]
mod adventure_handler;
#[path = "base_handler.rs"]
mod base_handler;
#[path = "battle_handler.rs"]
mod battle_handler;
#[path = "boss_handler.rs"]
mod boss_handler;
#[path = "building_handler.rs"]
pub(super) mod building_handler;
#[path = "buildship_handler.rs"]
mod buildship_handler;
#[path = "chat_handler.rs"]
mod chat_handler;
#[path = "commerce_handler.rs"]
mod commerce_handler;
#[path = "compat_feature.rs"]
mod compat_feature;
#[path = "coop_handler.rs"]
pub(super) mod coop_handler;
#[path = "daily_copy_handler.rs"]
mod daily_copy_handler;
#[path = "equip_handler.rs"]
pub(crate) mod equip_handler;
#[path = "extended_handler.rs"]
mod extended_handler;
#[path = "friend_handler.rs"]
mod friend_handler;
#[path = "guild_extension_handler.rs"]
mod guild_extension_handler;
#[path = "guild_handler.rs"]
mod guild_handler;
#[path = "guildbox_handler.rs"]
mod guildbox_handler;
#[path = "guildtask_handler.rs"]
mod guildtask_handler;
#[path = "hero_handler.rs"]
mod hero_handler;
#[path = "invitescore_handler.rs"]
mod invitescore_handler;
#[path = "misc_extended_handler.rs"]
mod misc_extended_handler;
#[path = "misc_handler.rs"]
mod misc_handler;
#[path = "outpost_handler.rs"]
mod outpost_handler;
#[path = "shiptask_handler.rs"]
mod shiptask_handler;
#[path = "sportsmeet_handler.rs"]
mod sportsmeet_handler;
#[path = "task_handler.rs"]
mod task_handler;
#[path = "teaching_handler.rs"]
mod teaching_handler;
#[cfg(test)]
pub(super) use compat_feature::legacy_test_handler::pass_mini_game;
#[path = "progression_handler.rs"]
mod progression_handler;
#[path = "talent_handler.rs"]
mod talent_handler;
#[path = "tower_handler.rs"]
pub(super) mod tower_handler;

#[cfg(test)]
type BattlePassDetails = (i32, i32, i32, bool, Vec<i32>, Vec<(u64, i32)>);

#[cfg(test)]
#[allow(dead_code)]
struct GameLoginRequestContext<'state, 'account, 'scratch> {
    state: &'state ServerState,
    account: &'scratch mut Option<&'account mut Value>,
    catalogs: GameLoginCatalogs<'state>,
    pre_pushes: &'scratch mut Vec<Vec<u8>>,
    post_pushes: &'scratch mut Vec<Vec<u8>>,
    handler_error: &'scratch mut Option<GameError>,
    pass_details: &'scratch mut Option<BattlePassDetails>,
    pass_rewards: &'scratch mut Vec<ShopReward>,
    pass_hero_ids: &'scratch mut Vec<u64>,
    pass_mvp_hero_id: &'scratch mut Option<u64>,
    pass_shipwrecked_ids: &'scratch mut std::collections::HashSet<u64>,
}

#[cfg(not(test))]
async fn write_typed_bootstrap_push<S>(
    stream: &mut S,
    method: &'static str,
    payload: Vec<u8>,
    now: u32,
) -> Result<(), ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
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
    NetSocketFrameCodec::write(
        stream,
        0,
        &Response::new("user.UpdateLoginTime", login_time).encode_push(now),
    )
    .await?;

    let mut server_time = Vec::new();
    append_varint_field(&mut server_time, 1, u64::from(now));
    append_varint_field(&mut server_time, 2, u64::from(now));
    NetSocketFrameCodec::write(
        stream,
        0,
        &Response::new("user.UpdateSvrTime", server_time).encode_push(now),
    )
    .await?;

    macro_rules! write_payload {
        ($method:expr, $payload:expr $(,)?) => {
            write_typed_bootstrap_push(stream, $method, $payload, now).await?;
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

pub(super) async fn process_game_login_frame_payload_with_catalogs_typed_mut<S>(
    stream: &mut S,
    state: &ServerState,
    #[cfg(test)] mut account: Option<&mut Value>,
    mut typed_account: Option<&mut AccountState>,
    frame: blueoath_transport::NetSocketFrame,
    catalogs: &GameLoginCatalogs<'_>,
) -> Result<bool, ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    #[cfg(not(test))]
    let account: Option<&mut Value> = None;

    let GameLoginCatalogs {
        fashion: fashion_catalog,
        equip: equip_catalog,
        hero_level: hero_level_catalog,
        hero_breakdown: hero_breakdown_catalog,
        shop: shop_catalog,
        mails: mail_catalog,
        handbook_behaviours,
        hero_memories,
        chapters: chapter_catalog,
        tasks: task_catalog,
        battle: battle_catalog,
        ..
    } = *catalogs;
    #[cfg(not(test))]
    let _ = (fashion_catalog, handbook_behaviours, hero_memories);

    // Keep immutable view independent from mutable account so profile mutations can update
    // the same request snapshot before response pushes are encoded.
    let account_snapshot = account.as_deref().cloned();
    let account_view = account_snapshot.as_ref();
    if frame.frame_type == 2 {
        NetSocketFrameCodec::write(stream, 2, &[]).await?;
        return Ok(true);
    }
    if frame.payload.is_empty() {
        return Ok(true);
    }

    let request = RequestContext::from(TMessageCodec::decode_request(&frame.payload)?);
    let request_args = request.args.as_slice();
    if std::env::var_os("BLUEOATH_TRACE_METHODS").is_some() {
        eprintln!(
            "game-login method={} args={} hex={}",
            request.method,
            request_args.len(),
            request_args
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    let method = GameMethod::parse(&request.method);
    let known_method = method.known();
    let is_user_info = known_method == Some(KnownMethod::UserGetUserInfo);
    #[cfg(test)]
    let is_user_login = known_method == Some(KnownMethod::UserLogin);
    #[cfg(test)]
    let is_profile_update = matches!(
        request.method.as_str(),
        "user.SetUserSecretary"
            | "user.ChangeName"
            | "user.SetMessage"
            | "user.SetPlayerHeadFrame"
            | "user.SetHead"
    );
    let mut pre_pushes = Vec::<Vec<u8>>::new();
    let mut post_pushes = Vec::<Vec<u8>>::new();
    #[allow(unused_mut)]
    #[cfg(test)]
    let mut pass_details: Option<BattlePassDetails> = None;
    #[cfg(test)]
    let mut pass_rewards = Vec::<ShopReward>::new();
    #[allow(unused_mut)]
    #[cfg(test)]
    let mut pass_hero_ids = Vec::<u64>::new();
    #[allow(unused_mut)]
    #[cfg(test)]
    let mut pass_mvp_hero_id = None;
    #[allow(unused_mut)]
    #[cfg(test)]
    let mut pass_shipwrecked_ids = std::collections::HashSet::new();
    let mut handler_error: Option<GameError> = None;
    #[cfg(test)]
    let mut typed_daily_copy_handled = false;
    let mut ret = match request.method.as_str() {
        _ if typed_account.is_some()
            && matches!(
                request.method.as_str(),
                "copy.StarReward" | "copy.FetchRewardBox"
            ) =>
        {
            let result = battle_handler::handle_typed_copy_star_reward(
                typed_account.as_mut().expect("typed copy account"),
                request.method.as_str(),
                request_args,
                chapter_catalog,
                task_catalog,
                &mut pre_pushes,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            result.into_payload()
        }
        _ if typed_account.is_some()
            && matches!(request.method.as_str(), "copy.DotBase" | "copyinfo.DotBase") =>
        {
            if CopyIdRequest::decode(request_args).is_err() {
                handler_error = Some(GameError::InvalidRequest("copy id is invalid"));
            }
            Some(Vec::new())
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
            result.into_payload()
        }
        _ if typed_account.is_some() && compat_feature::handles_typed(request.method.as_str()) => {
            let result = compat_feature::handle_typed(
                state,
                typed_account.as_mut().expect("typed compat account"),
                request.method.as_str(),
                request_args,
                catalogs.affection,
                catalogs.combination,
                &mut pre_pushes,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            result.into_payload()
        }
        _ if typed_account.is_some() && legacy_only_method(request.method.as_str()) => {
            handler_error = Some(GameError::InvalidRequest(
                "request family has no typed handler",
            ));
            Some(Vec::new())
        }
        _ if known_method == Some(KnownMethod::PlayerLogin) => {
            Some(GameLoginCodec::encode_response(&TRetLogin {
                ret: "ok".to_owned(),
                feign_role_id: state.profile_id.clone(),
                err_code: 0,
            }))
        }
        _ if known_method == Some(KnownMethod::PlayerGetUserList) => {
            let user = match typed_account.as_deref() {
                Some(account) => user_info_from_typed_account(state, account),
                None => {
                    #[cfg(test)]
                    {
                        user_info_from_account(state, account_view)
                    }
                    #[cfg(not(test))]
                    {
                        handler_error = Some(GameError::AccountUnavailable);
                        UserInfo::default()
                    }
                }
            };
            Some(UserListCodec::encode(&[user]))
        }
        _ if known_method == Some(KnownMethod::PlayerCreateUser) => {
            let user = match typed_account.as_deref() {
                Some(account) => user_info_from_typed_account(state, account),
                None => {
                    #[cfg(test)]
                    {
                        user_info_from_account(state, account_view)
                    }
                    #[cfg(not(test))]
                    {
                        handler_error = Some(GameError::AccountUnavailable);
                        UserInfo::default()
                    }
                }
            };
            Some(PlayerUserCodec::encode(&user))
        }
        _ if matches!(
            request.method.as_str(),
            "cachedata.CacheData"
                | "user.GetHeadBuyCount"
                | "user.BuyHead"
                | "user.NewHeadUnlockedList"
                | "hero.Marry"
                | "hero.AddAffection"
                | "hero.HeroCombine"
                | "hero.HeroCombineBreak"
                | "hero.HeroCombineQuickLevelUp"
                | "hero.HeroCombineUpLv"
                | "repair.RepairHero"
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
                #[cfg(test)]
                {
                    let mut context = GameLoginRequestContext {
                        state,
                        account: &mut account,
                        catalogs: *catalogs,
                        pre_pushes: &mut pre_pushes,
                        post_pushes: &mut post_pushes,
                        handler_error: &mut handler_error,
                        pass_details: &mut pass_details,
                        pass_rewards: &mut pass_rewards,
                        pass_hero_ids: &mut pass_hero_ids,
                        pass_mvp_hero_id: &mut pass_mvp_hero_id,
                        pass_shipwrecked_ids: &mut pass_shipwrecked_ids,
                    };
                    compat_feature::legacy_test_handler::handle(
                        &mut context,
                        request.method.as_str(),
                        request_args,
                    )
                }
                #[cfg(not(test))]
                {
                    HandlerResult::Error(GameError::InvalidRequest(
                        "compat request requires typed account",
                    ))
                }
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            result.into_payload()
        }
        _ if method.is_family(MethodFamily::Hero) => {
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
                        ship_exp_multiplier: state.ship_exp_multiplier,
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
            result.into_payload()
        }
        _ if known_method == Some(KnownMethod::TacticGetHeros) => {
            let fleet = match typed_account.as_deref() {
                Some(account) => fleet_info_from_typed_account(account),
                None => {
                    #[cfg(test)]
                    {
                        fleet_info_from_account(account_view.unwrap_or(&Value::Null))
                    }
                    #[cfg(not(test))]
                    {
                        handler_error = Some(GameError::AccountUnavailable);
                        FleetInfo::default()
                    }
                }
            };
            Some(FleetInfoCodec::encode(&fleet))
        }
        _ if known_method == Some(KnownMethod::BagGetInfo) => {
            let bag = match typed_account.as_deref() {
                Some(account) => bag_info_from_typed_account(account),
                None => {
                    #[cfg(test)]
                    {
                        bag_info_from_account(account_view.unwrap_or(&Value::Null))
                    }
                    #[cfg(not(test))]
                    {
                        handler_error = Some(GameError::AccountUnavailable);
                        BagInfo::default()
                    }
                }
            };
            Some(BagInfoCodec::encode(&bag))
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
                        Some(Vec::new())
                    } else {
                        Some(FleetInfoCodec::encode(&fleet))
                    }
                } else {
                    #[cfg(test)]
                    {
                        if let Some(account) = account.as_deref_mut() {
                            set_fleet_from_account(account, &fleet);
                        }
                    }
                    Some(FleetInfoCodec::encode(&fleet))
                }
            } else {
                Some(Vec::new())
            }
        }
        _ if known_method == Some(KnownMethod::PresetFleetInfo) => {
            let preset = match typed_account.as_deref() {
                Some(account) => preset_fleet_info_from_typed_account(account),
                None => {
                    #[cfg(test)]
                    {
                        preset_fleet_info_from_account(account_view.unwrap_or(&Value::Null))
                    }
                    #[cfg(not(test))]
                    {
                        handler_error = Some(GameError::AccountUnavailable);
                        PresetFleetInfo::default()
                    }
                }
            };
            Some(PresetFleetCodec::encode(&preset))
        }
        _ if known_method == Some(KnownMethod::PresetFleetSet) => {
            match PresetFleetCodec::decode(request_args) {
                Ok(preset) if preset.fleets.len() <= 100 => {
                    if let Some(typed) = typed_account.as_mut() {
                        if !set_preset_fleet_on_typed_account(typed, &preset) {
                            handler_error = Some(GameError::InvalidRequest(
                                "preset fleet contains invalid or unowned hero",
                            ));
                            Some(Vec::new())
                        } else {
                            let payload = PresetFleetCodec::encode(
                                &preset_fleet_info_from_typed_account(typed),
                            );
                            append_method_push(
                                &mut post_pushes,
                                "presetfleet.PresetFleetsInfo",
                                payload.clone(),
                            );
                            Some(payload)
                        }
                    } else {
                        #[cfg(test)]
                        {
                            if let Some(account) = account.as_deref_mut() {
                                set_preset_fleet_from_account(account, &preset);
                                let payload = PresetFleetCodec::encode(
                                    &preset_fleet_info_from_account(account),
                                );
                                append_method_push(
                                    &mut post_pushes,
                                    "presetfleet.PresetFleetsInfo",
                                    payload.clone(),
                                );
                                Some(payload)
                            } else {
                                Some(PresetFleetCodec::encode(&preset))
                            }
                        }
                        #[cfg(not(test))]
                        {
                            Some(PresetFleetCodec::encode(&preset))
                        }
                    }
                }
                _ => {
                    handler_error = Some(GameError::Internal(
                        "preset fleet request is invalid".to_owned(),
                    ));
                    Some(Vec::new())
                }
            }
        }
        _ if known_method == Some(KnownMethod::UserGetUserInfo) => {
            let user = match typed_account.as_deref() {
                Some(account) => user_info_from_typed_account(state, account),
                None => {
                    #[cfg(test)]
                    {
                        user_info_from_account(state, account_view)
                    }
                    #[cfg(not(test))]
                    {
                        handler_error = Some(GameError::AccountUnavailable);
                        UserInfo::default()
                    }
                }
            };
            Some(UserInfoCodec::encode(&user))
        }
        _ if known_method == Some(KnownMethod::UserLogin) => {
            #[cfg(test)]
            let now = current_unix_seconds();
            #[cfg(test)]
            if let Some(account) = account.as_deref_mut() {
                advance_task_event(account, task_catalog, 1, 1, now);
            }
            if let Some(typed) = typed_account.as_deref_mut() {
                advance_typed_task_event(typed, task_catalog, 1, 1);
                if typed.guild.is_some() {
                    guild_handler::push_guild_state_typed(&mut pre_pushes, typed);
                }
                append_typed_user_login_bootstrap(&mut pre_pushes, state, typed, chapter_catalog);
            } else {
                #[cfg(test)]
                if let Some(account) = account.as_deref() {
                    append_method_push(
                        &mut post_pushes,
                        "task.TaskInfo",
                        task_info_payload(account, task_catalog),
                    );
                }
            }
            Some(UserLoginCodec::encode_response("ok", "", 0))
        }
        "user.SetUserSecretary" => {
            match SetSecretaryRequest::decode(request_args) {
                Ok(typed) => {
                    if let Some(account) = typed_account.as_mut() {
                        account.character.secretary_id = u64::try_from(typed.secretary_id)
                            .ok()
                            .and_then(|id| blueoath_domain::HeroId::new(id).ok());
                    }
                    #[cfg(test)]
                    if let Some(account) = account.as_deref_mut() {
                        set_character_i64(account, "secretaryId", typed.secretary_id);
                    }
                }
                Err(_) => {
                    handler_error = Some(GameError::Internal(
                        "secretary request is invalid".to_owned(),
                    ));
                }
            }
            Some(Vec::new())
        }
        "user.ChangeName" => {
            match ChangeNameRequest::decode(request_args) {
                Ok(typed) => {
                    if let Some(account) = typed_account.as_mut() {
                        account.character.name = typed.name.clone();
                    }
                    #[cfg(test)]
                    if let Some(account) = account.as_deref_mut() {
                        set_character_string(account, "name", typed.name);
                    }
                }
                Err(_) => {
                    handler_error = Some(GameError::Internal("name request is invalid".to_owned()));
                }
            }
            Some(Vec::new())
        }
        "user.SetMessage" => {
            match SetMessageRequest::decode(request_args) {
                Ok(typed) => {
                    if let Some(account) = typed_account.as_mut() {
                        account.character.message = typed.message.clone();
                    }
                    #[cfg(test)]
                    if let Some(account) = account.as_deref_mut() {
                        set_character_string(account, "message", typed.message);
                    }
                }
                Err(_) => {
                    handler_error =
                        Some(GameError::Internal("message request is invalid".to_owned()));
                }
            }
            Some(Vec::new())
        }
        "user.SetPlayerHeadFrame" => {
            match SetHeadFrameRequest::decode(request_args) {
                Ok(typed) => {
                    if let Some(account) = typed_account.as_mut() {
                        account.character.head_frame = typed.head_frame.max(0) as u32;
                    }
                    #[cfg(test)]
                    if let Some(account) = account.as_deref_mut() {
                        set_character_i64(account, "headFrame", typed.head_frame);
                    }
                }
                Err(_) => {
                    handler_error = Some(GameError::Internal(
                        "head frame request is invalid".to_owned(),
                    ));
                }
            }
            Some(Vec::new())
        }
        "user.SetHead" => {
            match SetHeadRequest::decode(request_args) {
                Ok(typed) => {
                    if let Some(account) = typed_account.as_mut() {
                        account.character.head = typed.head.max(0) as u32;
                    }
                    #[cfg(test)]
                    if let Some(account) = account.as_deref_mut() {
                        set_character_i64(account, "head", typed.head);
                    }
                }
                Err(_) => {
                    handler_error = Some(GameError::Internal("head request is invalid".to_owned()));
                }
            }
            Some(Vec::new())
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
            let result = if let Some(typed) = typed_account.as_mut() {
                base_handler::handle_typed(
                    typed,
                    state,
                    request.method.as_str(),
                    request_args,
                    &mut pre_pushes,
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "base request requires typed account",
                ))
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            result.into_payload()
        }
        _ if method.is_family(MethodFamily::Guild) => {
            let result = if let Some(typed) = typed_account.as_mut() {
                guild_handler::handle_typed(
                    typed,
                    request.method.as_str(),
                    request_args,
                    current_unix_seconds(),
                    &mut pre_pushes,
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest("guild requires typed account"))
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            result.into_payload()
        }
        _ if method.is_family(MethodFamily::Friend) => {
            let result = if let Some(typed) = typed_account.as_mut() {
                friend_handler::handle_typed(
                    typed,
                    state,
                    request.method.as_str(),
                    request_args,
                )
            } else {
                HandlerResult::Error(GameError::InvalidRequest("friend requires typed account"))
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            result.into_payload()
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
            result.into_payload()
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
            result.into_payload()
        }
        _ if method.is_family(MethodFamily::Boss) => {
            let result = if let Some(typed) = typed_account.as_mut() {
                let result = boss_handler::handle_typed(state, typed, request.method.as_str());
                if matches!(result, HandlerResult::Reply(_) | HandlerResult::Error(_)) {
                    result
                } else {
                    HandlerResult::Error(GameError::InvalidRequest("boss request is not supported"))
                }
            } else {
                HandlerResult::Error(GameError::InvalidRequest("boss requires typed account"))
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            result.into_payload()
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
            result.into_payload()
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
            result.into_payload()
        }
        _ if activity_handler::handles(request.method.as_str()) => {
            let result = HandlerResult::Error(GameError::InvalidRequest(
                "activity request requires typed account",
            ));
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            result.into_payload()
        }
        _ if activity_extra_handler::handles(request.method.as_str()) => {
            let result = if let Some(typed) = typed_account.as_mut() {
                let result = activity_extra_handler::handle_typed(
                    state,
                    typed,
                    request.method.as_str(),
                    request_args,
                );
                if matches!(result, HandlerResult::Reply(_) | HandlerResult::Error(_)) {
                    result
                } else {
                    HandlerResult::Error(GameError::InvalidRequest(
                        "activity request is not supported",
                    ))
                }
            } else {
                HandlerResult::Error(GameError::InvalidRequest(
                    "activity request requires typed account",
                ))
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            result.into_payload()
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
            result.into_payload()
        }
        _ if request.method == "archiveCopy.IsLoad"
            || request.method == "copyextra.AddCopyRewardCount"
            || request.method == "copyextra.UpdateCopyExtraInfo"
            || request.method == "prefs.SavePrefs"
            || request.method == "statcount.GetStatCount"
            || request.method == "sign.Sign"
            || request.method == "miniGame.StartMiniGame"
            || request.method == "alchemy.StartAlchemy" =>
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
            result.into_payload()
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
            result.into_payload()
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
            result.into_payload()
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
            result.into_payload()
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
            result.into_payload()
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
            result.into_payload()
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
            result.into_payload()
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
            result.into_payload()
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
            result.into_payload()
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
            result.into_payload()
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
            result.into_payload()
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
            result.into_payload()
        }
        _ if guildtask_handler::handles(request.method.as_str()) => {
            let result = HandlerResult::Error(GameError::InvalidRequest(
                "guild task requires typed account",
            ));
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            result.into_payload()
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
            result.into_payload()
        }
        _ if guild_extension_handler::handles(request.method.as_str()) => {
            let result = HandlerResult::Error(GameError::InvalidRequest(
                "guild extension requires typed account",
            ));
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            result.into_payload()
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
            Some(encode_mail_list_response(
                mail_catalog.unwrap_or_default(),
                current_unix_seconds(),
                &rewards,
            ))
        }
        "mail.GetMailList"
        | "mail.OpenMail"
        | "mail.DeleteMail"
        | "mail.DeleteAllMail"
        | "mail.ReceiveNewMail" => Some(encode_mail_list_response(
            mail_catalog.unwrap_or_default(),
            current_unix_seconds(),
            &[],
        )),
        #[cfg(test)]
        "mail.FetchItem" | "mail.FetchAllItems" => {
            let fetch_one = request.method == "mail.FetchItem";
            let mid = MailIdRequest::decode(request_args)
                .map(|request| request.mail_id)
                .unwrap_or_default();
            let mut rewards = Vec::new();
            if let Some(account) = account.as_deref_mut() {
                for mail in mail_catalog.unwrap_or_default() {
                    if fetch_one && mail.mid != mid {
                        continue;
                    }
                    apply_mail_reward(account, mail);
                    rewards.push(ShopReward {
                        goods_type: mail.goods_type,
                        item_id: mail.config_id,
                        num: mail.num,
                        instance_id: 0,
                    });
                }
                if !rewards.is_empty() {
                    append_method_push(
                        &mut pre_pushes,
                        "user.UpdateUserInfo",
                        UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                    );
                    append_method_push(
                        &mut pre_pushes,
                        "bag.UpdateBagData",
                        BagInfoCodec::encode(&bag_info_from_account(account)),
                    );
                }
            }
            if rewards.is_empty() {
                handler_error = Some(GameError::Internal(if fetch_one {
                    "mail was not found".to_owned()
                } else {
                    "mail list is empty".to_owned()
                }));
            }
            Some(encode_mail_list_response(
                mail_catalog.unwrap_or_default(),
                current_unix_seconds(),
                &rewards,
            ))
        }
        _ if method.is_family(MethodFamily::Shop)
            || method.is_family(MethodFamily::Recharge)
            || known_method == Some(KnownMethod::BagGetInfo)
            || request.method == "bag.CompositeItem"
            || request.method == "bag.SaleBagItem"
            || request.method == "fashion.updateData"
            || request.method == "fashion.Equip" =>
        {
            let mut commerce_effects = ResponseEffects::default();
            let result = if let Some(typed) = typed_account.as_mut() {
                let result = commerce_handler::handle_typed(
                    typed,
                    state,
                    request.method.as_str(),
                    request_args,
                    &mut commerce_effects,
                    commerce_handler::CommerceTypedCatalogs { shop: shop_catalog },
                );
                if matches!(result, HandlerResult::Reply(_) | HandlerResult::Error(_)) {
                    result
                } else {
                    HandlerResult::Error(GameError::InvalidRequest(
                        "commerce request is not supported",
                    ))
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
            result.into_payload()
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
            result.into_payload()
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
                if matches!(result, HandlerResult::Reply(_) | HandlerResult::Error(_)) {
                    result
                } else {
                    HandlerResult::Error(GameError::InvalidRequest(
                        "building request is not supported",
                    ))
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
            result.into_payload()
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
            result.into_payload()
        }
        "task.TaskInfo" => {
            let result = typed_account
                .as_ref()
                .map(|typed| task_info_payload_from_typed_account(typed, task_catalog));
            match result {
                Some(payload) => Some(payload),
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
            result.into_payload()
        }
        _ if typed_account.is_some() && coop_handler::handles_typed(request.method.as_str()) => {
            let result = coop_handler::handle_typed(
                state,
                typed_account.as_mut().expect("typed co-op account"),
                request.method.as_str(),
                request_args,
                &mut post_pushes,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            result.into_payload()
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
            result.into_payload()
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
                    #[cfg(test)]
                    {
                        typed_daily_copy_handled = true;
                    }
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
            result.into_payload()
        }
        "dailycopy.UpdateDailyCopyData" => typed_account.as_ref().map(|typed| {
            daily_copy_snapshot_payload_from_typed_account(
                typed,
                chapter_catalog,
                current_unix_seconds(),
            )
        }),
        _ if method.is_family(MethodFamily::MopUp) && typed_account.is_some() => {
            let result = battle_handler::handle_typed_mop_up(
                state,
                typed_account.as_mut().expect("typed mop up account"),
                request.method.as_str(),
                request_args,
                battle_catalog,
                &mut pre_pushes,
                &mut post_pushes,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            result.into_payload()
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
            let result = if let Some(typed) = typed_account.as_mut() {
                let result = battle_handler::handle_typed_with_catalog(
                    typed,
                    request.method.as_str(),
                    request_args,
                    battle_catalog,
                    state.ship_stat_multiplier,
                );
                if matches!(result, HandlerResult::Reply(_) | HandlerResult::Error(_)) {
                    typed_handled = true;
                    result
                } else {
                    HandlerResult::Error(GameError::InvalidRequest(
                        "battle request is not supported",
                    ))
                }
            } else {
                #[cfg(test)]
                {
                    let mut context = GameLoginRequestContext {
                        state,
                        account: &mut account,
                        catalogs: *catalogs,
                        pre_pushes: &mut pre_pushes,
                        post_pushes: &mut post_pushes,
                        handler_error: &mut handler_error,
                        pass_details: &mut pass_details,
                        pass_rewards: &mut pass_rewards,
                        pass_hero_ids: &mut pass_hero_ids,
                        pass_mvp_hero_id: &mut pass_mvp_hero_id,
                        pass_shipwrecked_ids: &mut pass_shipwrecked_ids,
                    };
                    battle_handler::legacy_test_handler::handle(
                        &mut context,
                        request.method.as_str(),
                        request_args,
                    )
                }
                #[cfg(not(test))]
                {
                    HandlerResult::Error(GameError::InvalidRequest(
                        "battle request requires typed account",
                    ))
                }
            };
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            let payload = result.into_payload();
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
            result.into_payload()
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
            result.into_payload()
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
            result.into_payload()
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
                Some(Vec::new())
            } else if !known_copy {
                handler_error = Some(GameError::Internal("sea copy is invalid".to_owned()));
                Some(Vec::new())
            } else if !(1..=7).contains(&requested) {
                handler_error = Some(GameError::Internal("sea difficulty is invalid".to_owned()));
                Some(Vec::new())
            } else if let Some(account) = typed_account.as_deref_mut() {
                let level = i32::try_from(account.character.level).unwrap_or(i32::MAX);
                if level < SEA_DIFFICULTY_UNLOCK_LEVEL && requested > 1 {
                    handler_error = Some(GameError::Internal(
                        "sea difficulty unlocks at commander level 60".to_owned(),
                    ));
                    Some(Vec::new())
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
                    let passed = account
                        .battle
                        .passed_copies
                        .iter()
                        .filter_map(|copy_id| i32::try_from(copy_id.get()).ok())
                        .collect::<Vec<_>>();
                    append_method_push(
                        &mut post_pushes,
                        "copy.GetCopy",
                        CopyInfoCodec::encode_with_progress_and_difficulty_and_counts(
                            &catalog.sea,
                            copy_progress_max_or_initial(
                                &catalog.sea,
                                &passed,
                                catalog.sea_initial,
                            ),
                            &passed,
                            &[],
                            account.sea.difficulty as i32,
                        ),
                    );
                    Some(Vec::new())
                }
            } else {
                #[cfg(test)]
                {
                    if let Some(account) = account.as_deref_mut() {
                        let level = commander_level(account);
                        if level < SEA_DIFFICULTY_UNLOCK_LEVEL && requested > 1 {
                            handler_error = Some(GameError::Internal(
                                "sea difficulty unlocks at commander level 60".to_owned(),
                            ));
                            Some(Vec::new())
                        } else {
                            set_sea_difficulty(account, requested);
                            let fallback_catalog;
                            let catalog = match chapter_catalog {
                                Some(catalog) => catalog,
                                None => {
                                    fallback_catalog = ChapterCatalog::fallback();
                                    &fallback_catalog
                                }
                            };
                            let passed = completed_copy_ids(account, "seaProgress");
                            let pass_counts = completed_copy_counts(account, "seaProgress");
                            append_method_push(
                                &mut post_pushes,
                                "copy.GetCopy",
                                CopyInfoCodec::encode_with_progress_and_difficulty_and_counts(
                                    &catalog.sea,
                                    copy_progress_max_or_initial(
                                        &catalog.sea,
                                        &passed,
                                        catalog.sea_initial,
                                    ),
                                    &passed,
                                    &pass_counts,
                                    sea_difficulty_for_account(account),
                                ),
                            );
                            Some(Vec::new())
                        }
                    } else {
                        handler_error =
                            Some(GameError::Internal("account is unavailable".to_owned()));
                        Some(Vec::new())
                    }
                }
                #[cfg(not(test))]
                {
                    handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                    Some(Vec::new())
                }
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
            let passed = typed_account
                .as_deref()
                .map(|account| {
                    account
                        .battle
                        .passed_copies
                        .iter()
                        .filter_map(|copy_id| i32::try_from(copy_id.get()).ok())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| {
                    account_view
                        .map(|account| match copy_type {
                            2 => completed_copy_ids(account, "seaProgress"),
                            _ => completed_copy_ids(account, "copyProgress"),
                        })
                        .unwrap_or_default()
                });
            Some(match copy_type {
                2 => {
                    let pass_counts = if typed_account.is_some() {
                        Vec::new()
                    } else {
                        account_view
                            .map(|account| completed_copy_counts(account, "seaProgress"))
                            .unwrap_or_default()
                    };
                    CopyInfoCodec::encode_with_progress_and_difficulty_and_counts(
                        &catalog.sea,
                        copy_progress_max_or_initial(&catalog.sea, &passed, catalog.sea_initial),
                        &passed,
                        &pass_counts,
                        if typed_account.is_some() {
                            1
                        } else {
                            account_view.map(sea_difficulty_for_account).unwrap_or(1)
                        },
                    )
                }
                33 => CopyInfoCodec::encode(
                    33,
                    &catalog.mubar,
                    catalog.mubar.iter().copied().max().unwrap_or_default(),
                ),
                10 => CopyInfoCodec::encode(
                    10,
                    &catalog.goods_copy,
                    catalog.goods_copy.iter().copied().max().unwrap_or_default(),
                ),
                24 => CopyInfoCodec::encode(
                    24,
                    &catalog.tower,
                    catalog.tower.iter().copied().max().unwrap_or_default(),
                ),
                34 => CopyInfoCodec::encode(
                    34,
                    &catalog.equip_new_test,
                    catalog
                        .equip_new_test
                        .iter()
                        .copied()
                        .max()
                        .unwrap_or_default(),
                ),
                9 => CopyInfoCodec::encode(
                    9,
                    &catalog.daily,
                    catalog.daily.iter().copied().max().unwrap_or_default(),
                ),
                _ => CopyInfoCodec::encode_with_progress(
                    1,
                    &catalog.plot,
                    copy_progress_max_or_first(&catalog.plot, &passed),
                    &passed,
                ),
            })
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
            let passed = typed_account
                .as_deref()
                .map(|account| {
                    account
                        .battle
                        .passed_copies
                        .iter()
                        .filter_map(|copy_id| i32::try_from(copy_id.get()).ok())
                        .collect::<Vec<_>>()
                })
                .or_else(|| account_view.map(|account| completed_copy_ids(account, "copyProgress")))
                .unwrap_or_default();
            Some(CopyInfoCodec::encode_with_progress(
                1,
                &catalog.plot,
                copy_progress_max_or_first(&catalog.plot, &passed),
                &passed,
            ))
        }
        _ => None,
    };
    // Every known route must complete its callback. Unsupported routes return an
    // empty protobuf payload without mutating account state.
    if ret.is_none() {
        if method.is_known() {
            ret = Some(Vec::new());
        } else {
            let error = GameError::UnknownMethod(request.method.clone());
            handler_error = Some(error.clone());

            ret = Some(Vec::new());
        }
    }
    #[cfg(test)]
    if is_user_login && typed_account.is_none() {
        let now = current_unix_seconds();
        let fallback_catalog;
        let catalog = match chapter_catalog {
            Some(catalog) => catalog,
            None => {
                fallback_catalog = ChapterCatalog::fallback();
                &fallback_catalog
            }
        };
        let max_id = |ids: &[i32]| ids.iter().copied().max().unwrap_or_default();
        let plot_progress = account_view
            .map(|account| completed_copy_ids(account, "copyProgress"))
            .unwrap_or_default();
        let sea_progress = account_view
            .map(|account| completed_copy_ids(account, "seaProgress"))
            .unwrap_or_default();
        let sea_difficulty = account_view.map(sea_difficulty_for_account).unwrap_or(1);
        #[allow(unused_mut)]
        let mut pushes = vec![
            (
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_account(state, account_view)),
            ),
            (
                "guide.GuideInfo",
                GuideInfoCodec::encode_initial_progress_completed(),
            ),
            (
                "copy.GetCopy",
                CopyInfoCodec::encode_with_progress(
                    1,
                    &catalog.plot,
                    copy_progress_max_or_first(&catalog.plot, &plot_progress),
                    &plot_progress,
                ),
            ),
            (
                "copy.GetCopy",
                CopyInfoCodec::encode_with_progress_and_difficulty_and_counts(
                    &catalog.sea,
                    copy_progress_max_or_initial(&catalog.sea, &sea_progress, catalog.sea_initial),
                    &sea_progress,
                    &completed_copy_counts(account_view.unwrap_or(&Value::Null), "seaProgress"),
                    sea_difficulty,
                ),
            ),
            (
                "copy.GetCopy",
                CopyInfoCodec::encode(33, &catalog.mubar, max_id(&catalog.mubar)),
            ),
            (
                "copy.GetCopy",
                CopyInfoCodec::encode(9, &catalog.daily, max_id(&catalog.daily)),
            ),
            (
                "dailycopy.UpdateDailyCopyData",
                DailyCopyCodec::encode_with_progress(
                    &catalog.daily_chapters,
                    &catalog.daily_groups,
                    &daily_copy_progress_from_account(account_view, now),
                    &daily_copy_group_progress_from_account(account_view, "groups", now),
                    &daily_copy_group_progress_from_account(account_view, "extraGroups", now),
                ),
            ),
            (
                "illustrate.IllustrateInfo",
                illustrate_info_payload(
                    account_view.unwrap_or(&Value::Null),
                    handbook_behaviours,
                    hero_memories,
                ),
            ),
            ("illustrate.OldIllustrateInfo", Vec::new()),
            (
                "illustrate.Memory",
                story_memory_payload(chapter_catalog.map(|catalog| catalog.memories.as_slice())),
            ),
        ];
        #[cfg(test)]
        if !catalog.equip_new_test.is_empty() {
            pushes.push((
                "copy.GetCopy",
                CopyInfoCodec::encode(34, &catalog.equip_new_test, max_id(&catalog.equip_new_test)),
            ));
            pushes.push((
                "equipnewtestcopy.UpdateEquipNewData",
                equip_handler::equip_new_test_copy_payload(account_view.unwrap_or(&Value::Null)),
            ));
        }
        for (method, payload) in pushes {
            let push = TMessageCodec::encode_response(&TResponse {
                method: method.to_owned(),
                ret: Some(payload),
                time: now,
                ..TResponse::default()
            });
            NetSocketFrameCodec::write(stream, 0, &push).await?;
        }
        let goods_copy_push = TMessageCodec::encode_response(&TResponse {
            method: "goodscopy.UpdateData".to_owned(),
            ret: Some(goods_copy_snapshot_payload(
                account_view.unwrap_or(&Value::Null),
                chapter_catalog,
            )),
            time: now,
            ..TResponse::default()
        });
        post_pushes.push(goods_copy_push);
        let talent_catalog = current_talent_catalog();
        let talent_payload = typed_account
            .as_deref()
            .map(|account| talent_tree_payload_typed(account, &talent_catalog))
            .unwrap_or_default();
        let push = TMessageCodec::encode_response(&TResponse {
            method: "talentTree.TalentTreeAllList".to_owned(),
            ret: Some(talent_payload),
            time: now,
            ..TResponse::default()
        });
        post_pushes.push(push);
    }
    let trace_ret_len = ret.as_ref().map(Vec::len).unwrap_or_default();
    let trace_method = request.method.clone();
    let (client_error_code, client_error_message) = handler_error
        .as_ref()
        .map(|error| (error.client_code(), error.to_string()))
        .unwrap_or((0, String::new()));
    let trace_err_msg = client_error_message.clone();
    let trace_ret_hex = if trace_method == "copy.StartBase" {
        ret.as_deref()
            .unwrap_or_default()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    } else {
        String::new()
    };
    let response = TMessageCodec::encode_response(&TResponse {
        err: client_error_code,
        err_msg: client_error_message,
        method: request.method,
        ret,
        callback_handler: request.callback_handler,
        time: current_unix_seconds(),
        token: request.token,
        is_response: 1,
        ..TResponse::default()
    });
    if std::env::var_os("BLUEOATH_TRACE_METHODS").is_some() {
        eprintln!(
            "game-login result method={} err={} msg={} ret={} pre_pushes={} post_pushes={}",
            trace_method,
            client_error_code,
            trace_err_msg,
            trace_ret_len,
            pre_pushes.len(),
            post_pushes.len()
        );
        if trace_method == "copy.StartBase" {
            eprintln!("game-login StartBase ret_hex={trace_ret_hex}");
        }
    }
    for push in pre_pushes {
        NetSocketFrameCodec::write(stream, 0, &push).await?;
    }
    NetSocketFrameCodec::write(stream, 0, &response).await?;
    #[cfg(test)]
    if is_profile_update {
        if let Some(account) = account.as_deref() {
            let push = TMessageCodec::encode_response(&TResponse {
                method: "user.UpdateUserInfo".to_owned(),
                ret: Some(UserInfoCodec::encode(&user_info_from_account(
                    state,
                    Some(account),
                ))),
                time: current_unix_seconds(),
                ..TResponse::default()
            });
            NetSocketFrameCodec::write(stream, 0, &push).await?;
        }
    }
    #[cfg(test)]
    if is_user_info && (account.is_some() || typed_account.is_some()) {
        let account = account.as_deref().unwrap_or(&Value::Null);
        let typed_account_view = typed_account.as_deref();
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
            ret: Some(UserInfoCodec::encode(
                &typed_account_view
                    .map(|typed| user_info_from_typed_account(state, typed))
                    .unwrap_or_else(|| user_info_from_account(state, Some(account))),
            )),
            time: now,
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;

        for (method, ret) in [
            (
                "build.BuildsInfo",
                typed_account_view
                    .map(|typed| building_handler::typed_construction_info_payload(typed, now))
                    .unwrap_or_else(|| construction_info_payload(account, now)),
            ),
            (
                "bathroom.BathroomInfo",
                typed_account_view
                    .map(progression_handler::bathroom_info_payload_from_typed)
                    .unwrap_or_else(|| bathroom_info_payload(account)),
            ),
            (
                "study.GetStudyInfo",
                typed_account_view
                    .map(|typed| progression_handler::study_info_payload_from_typed(typed, now))
                    .unwrap_or_else(|| study_info_payload(account, now)),
            ),
            // TaskInfo: explicit teaching-stage row + daily count. Repeated task groups
            // may be empty when this Rust profile has no task catalog; persisted teaching
            // reward ids are retained so the client does not re-offer claimed rewards.
            (
                "task.TaskInfo",
                typed_account_view
                    .map(|typed| task_info_payload_from_typed_account(typed, task_catalog))
                    .unwrap_or_else(|| task_info_payload(account, task_catalog)),
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
            ret: Some(BagInfoCodec::encode(
                &typed_account_view
                    .map(bag_info_from_typed_account)
                    .unwrap_or_else(|| bag_info_from_account(account)),
            )),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        let push = TMessageCodec::encode_response(&TResponse {
            method: "fashion.updateData".to_owned(),
            ret: Some(FashionListCodec::encode(
                &typed_account_view
                    .map(|typed| fashion_list_from_typed_account(typed, fashion_catalog))
                    .unwrap_or_else(|| fashion_list_from_account(account, fashion_catalog)),
            )),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        let push = TMessageCodec::encode_response(&TResponse {
            method: "equip.UpdateEquipBagData".to_owned(),
            ret: Some(EquipListCodec::encode(
                &typed_account_view
                    .map(equip_list_from_typed_account)
                    .unwrap_or_else(|| equip_list_from_account(account, equip_catalog)),
            )),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        let push = TMessageCodec::encode_response(&TResponse {
            method: "hero.UpdateHeroBagData".to_owned(),
            ret: Some(HeroBagCodec::encode(
                &typed_account_view
                    .map(hero_bag_from_typed_account)
                    .unwrap_or_else(|| hero_bag_from_account(account)),
            )),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        let push = TMessageCodec::encode_response(&TResponse {
            method: "building.UpdateBuildingInfo".to_owned(),
            ret: Some(UserBuildingInfoCodec::encode(
                &typed_account_view
                    .map(|typed| building_info_from_typed_account(typed, current_unix_seconds()))
                    .unwrap_or_else(|| building_info_from_account(account, current_unix_seconds())),
            )),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        let push = TMessageCodec::encode_response(&TResponse {
            method: "tactic.GetHerosTactic".to_owned(),
            ret: Some(FleetInfoCodec::encode(
                &typed_account_view
                    .map(fleet_info_from_typed_account)
                    .unwrap_or_else(|| fleet_info_from_account(account)),
            )),
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
            ret: Some(
                typed_account_view
                    .map(|typed| buildship_info_payload_from_typed(typed, current_unix_seconds()))
                    .unwrap_or_else(|| {
                        buildship_info_payload(Some(account), current_unix_seconds())
                    }),
            ),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        let push = TMessageCodec::encode_response(&TResponse {
            method: "presetfleet.PresetFleetsInfo".to_owned(),
            ret: Some(PresetFleetCodec::encode(
                &typed_account_view
                    .map(preset_fleet_info_from_typed_account)
                    .unwrap_or_else(|| preset_fleet_info_from_account(account)),
            )),
            time: current_unix_seconds(),
            ..TResponse::default()
        });
        NetSocketFrameCodec::write(stream, 0, &push).await?;
        for (method, ret) in [
            (
                "illustrate.IllustrateInfo",
                typed_account_view
                    .map(|typed| {
                        let template_ids = typed
                            .dock
                            .heroes
                            .values()
                            .map(|hero| hero.template_id.get() as i32)
                            .collect::<Vec<_>>();
                        illustrate_info_payload_for_templates(&template_ids, handbook_behaviours)
                    })
                    .unwrap_or_else(|| {
                        illustrate_info_payload(account, handbook_behaviours, hero_memories)
                    }),
            ),
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
        let talent_payload = typed_account_view
            .map(|typed| talent_tree_payload_typed(typed, &talent_catalog))
            .unwrap_or_default();
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
    if let Some((copy_id, grade, battle_time, _first_pass, ex_buffs, exp_rewards)) = pass_details {
        if let Some(account) = account.as_deref_mut() {
            record_battle_pass(
                account,
                copy_id,
                grade,
                battle_time,
                battle_catalog,
                &ex_buffs,
            );
            if battle_task_progress_enabled(battle_catalog, copy_id) {
                advance_task_event(account, task_catalog, 2, 1, current_unix_seconds());
                if let Some(typed) = typed_account.as_deref_mut() {
                    advance_typed_task_event(typed, task_catalog, 2, 1);
                }
            }
            // Client task/achievement goals encode exact clear targets in goal[1]. Keep
            // progression scoped to current copy instead of advancing every same-event row.
            for event_type in [17, 24, 900] {
                advance_task_event_with_param(
                    account,
                    task_catalog,
                    event_type,
                    copy_id,
                    1,
                    current_unix_seconds(),
                );
                if let Some(typed) = typed_account.as_deref_mut() {
                    advance_typed_task_event(typed, task_catalog, event_type, 1);
                }
            }
            // Battle completion grants configured commander and ship experience. Apply
            // level-up rollover immediately so full XP no longer sticks at current level.
            let (commander_base_exp, ship_base_exp) =
                battle_copy_experience(battle_catalog, copy_id);
            let (evaluation_exp_multiplier, _) =
                battle_evaluation_multipliers(battle_catalog, grade);
            let commander_exp = scale_reward(
                i64::from(commander_base_exp),
                state.commander_exp_multiplier * evaluation_exp_multiplier,
            )
            .clamp(0, i64::from(i32::MAX)) as i32;
            add_commander_battle_exp(account, commander_exp, COMMANDER_LEVEL_CATALOG.get());
            let ship_exp = exp_rewards.first().map_or_else(
                || {
                    scale_reward(
                        i64::from(ship_base_exp),
                        state.ship_exp_multiplier * evaluation_exp_multiplier,
                    )
                    .clamp(0, i64::from(i32::MAX)) as i32
                },
                |(_, value)| *value,
            );
            add_ship_battle_exp(account, &pass_hero_ids, ship_exp, hero_level_catalog);
            let settlement_changed = battle_catalog
                .and_then(|catalog| catalog.settlement_by_copy.get(&copy_id).copied())
                .is_some_and(|rule| {
                    apply_battle_settlement(
                        account,
                        &pass_hero_ids,
                        pass_mvp_hero_id,
                        &pass_shipwrecked_ids,
                        rule,
                        state.affection_multiplier,
                    )
                });
            let copy_type = battle_catalog
                .and_then(|catalog| catalog.copies.get(&copy_id))
                .map(|copy| copy.copy_type)
                .unwrap_or(1);
            let specialized_event = match copy_type {
                9 => Some(3),     // daily copy
                33 => Some(3105), // special sea / material operation
                _ => None,
            };
            if let Some(event_type) = specialized_event {
                advance_task_event(account, task_catalog, event_type, 1, current_unix_seconds());
                if let Some(typed) = typed_account.as_deref_mut() {
                    advance_typed_task_event(typed, task_catalog, event_type, 1);
                }
            }
            let first_rewards = std::mem::take(&mut pass_rewards);
            if !first_rewards.is_empty() {
                append_shop_update_pushes(
                    &mut post_pushes,
                    state,
                    account,
                    first_rewards[0],
                    fashion_catalog,
                    equip_catalog,
                );
            }
            if ship_exp > 0 || settlement_changed {
                append_method_push(
                    &mut post_pushes,
                    "hero.UpdateHeroBagData",
                    HeroBagCodec::encode(&hero_bag_from_account(account)),
                );
            }
            append_method_push(
                &mut post_pushes,
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_account(account)),
            );
            let now = current_unix_seconds();
            let fallback_catalog;
            let catalog = match chapter_catalog {
                Some(catalog) => catalog,
                None => {
                    fallback_catalog = ChapterCatalog::fallback();
                    &fallback_catalog
                }
            };
            let payload = match copy_type {
                2 => {
                    let passed = completed_copy_ids(account, "seaProgress");
                    let pass_counts = completed_copy_counts(account, "seaProgress");
                    CopyInfoCodec::encode_with_progress_and_difficulty_and_counts(
                        &catalog.sea,
                        copy_progress_max_or_initial(&catalog.sea, &passed, catalog.sea_initial),
                        &passed,
                        &pass_counts,
                        sea_difficulty_for_account(account),
                    )
                }
                33 => CopyInfoCodec::encode(
                    33,
                    &catalog.mubar,
                    catalog.mubar.iter().copied().max().unwrap_or_default(),
                ),
                10 => CopyInfoCodec::encode(
                    10,
                    &catalog.goods_copy,
                    catalog.goods_copy.iter().copied().max().unwrap_or_default(),
                ),
                24 => CopyInfoCodec::encode(
                    24,
                    &catalog.tower,
                    catalog.tower.iter().copied().max().unwrap_or_default(),
                ),
                9 => DailyCopyCodec::encode_with_progress(
                    &catalog.daily_chapters,
                    &catalog.daily_groups,
                    &daily_copy_progress_from_account(Some(account), now),
                    &daily_copy_group_progress_from_account(Some(account), "groups", now),
                    &daily_copy_group_progress_from_account(Some(account), "extraGroups", now),
                ),
                _ => {
                    let passed = completed_copy_ids(account, "copyProgress");
                    CopyInfoCodec::encode_with_progress(
                        1,
                        &catalog.plot,
                        copy_progress_max_or_first(&catalog.plot, &passed),
                        &passed,
                    )
                }
            };
            let user_push = TMessageCodec::encode_response(&TResponse {
                method: "user.UpdateUserInfo".to_owned(),
                ret: Some(UserInfoCodec::encode(&user_info_from_account(
                    state,
                    Some(account),
                ))),
                time: now,
                ..TResponse::default()
            });
            NetSocketFrameCodec::write(stream, 0, &user_push).await?;
            let task_push = TMessageCodec::encode_response(&TResponse {
                method: "task.TaskInfo".to_owned(),
                ret: Some(
                    typed_account
                        .as_deref()
                        .map(|typed| task_info_payload_from_typed_account(typed, task_catalog))
                        .unwrap_or_else(|| task_info_payload(account, task_catalog)),
                ),
                time: now,
                ..TResponse::default()
            });
            NetSocketFrameCodec::write(stream, 0, &task_push).await?;
            let push = TMessageCodec::encode_response(&TResponse {
                method: "copy.GetCopy".to_owned(),
                ret: Some(payload),
                time: now,
                ..TResponse::default()
            });
            NetSocketFrameCodec::write(stream, 0, &push).await?;
            if copy_type == 10 {
                append_method_push(
                    &mut post_pushes,
                    "goodscopy.UpdateData",
                    goods_copy_snapshot_payload(account, chapter_catalog),
                );
            }
        }
    }
    #[cfg(test)]
    if !typed_daily_copy_handled {
        if let (Some(typed), Some(legacy)) = (typed_account, account.as_deref()) {
            sync_typed_daily_copy_state(typed, legacy, current_unix_seconds());
        }
    }
    for push in post_pushes {
        NetSocketFrameCodec::write(stream, 0, &push).await?;
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
        None,
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
    pushes: &mut Vec<Vec<u8>>,
    state: &ServerState,
    account: &AccountState,
    chapter_catalog: Option<&ChapterCatalog>,
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
    let passed = account
        .battle
        .passed_copies
        .iter()
        .filter_map(|copy_id| i32::try_from(copy_id.get()).ok())
        .collect::<Vec<_>>();
    append_method_push(
        pushes,
        "user.UpdateUserInfo",
        UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
    );
    append_method_push(
        pushes,
        "guide.GuideInfo",
        GuideInfoCodec::encode_initial_progress_completed(),
    );
    append_method_push(
        pushes,
        "copy.GetCopy",
        CopyInfoCodec::encode_with_progress(
            1,
            &catalog.plot,
            copy_progress_max_or_first(&catalog.plot, &passed),
            &passed,
        ),
    );
    append_method_push(
        pushes,
        "copy.GetCopy",
        CopyInfoCodec::encode_with_progress_and_difficulty_and_counts(
            &catalog.sea,
            copy_progress_max_or_initial(&catalog.sea, &passed, catalog.sea_initial),
            &passed,
            &[],
            1,
        ),
    );
    append_method_push(
        pushes,
        "copy.GetCopy",
        CopyInfoCodec::encode(
            33,
            &catalog.mubar,
            catalog.mubar.iter().copied().max().unwrap_or_default(),
        ),
    );
    append_method_push(
        pushes,
        "copy.GetCopy",
        CopyInfoCodec::encode(
            9,
            &catalog.daily,
            catalog.daily.iter().copied().max().unwrap_or_default(),
        ),
    );
    append_method_push(
        pushes,
        "dailycopy.UpdateDailyCopyData",
        daily_copy_snapshot_payload_from_typed_account(account, chapter_catalog, now),
    );
    append_method_push(pushes, "illustrate.IllustrateInfo", Vec::new());
    append_method_push(pushes, "illustrate.OldIllustrateInfo", Vec::new());
    append_method_push(
        pushes,
        "illustrate.Memory",
        story_memory_payload(chapter_catalog.map(|catalog| catalog.memories.as_slice())),
    );
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
pub(super) fn sync_typed_battle_state(
    typed_account: Option<&mut AccountState>,
    legacy_account: Option<&Value>,
    method: &str,
    request_args: &[u8],
    now: u32,
) {
    sync_typed_battle_state_with_catalog(
        typed_account,
        legacy_account,
        method,
        request_args,
        None,
        now,
    );
}

#[cfg(test)]
fn sync_typed_battle_state_with_catalog(
    typed_account: Option<&mut AccountState>,
    legacy_account: Option<&Value>,
    method: &str,
    request_args: &[u8],
    battle_catalog: Option<&BattleCatalog>,
    now: u32,
) {
    let Some(typed_account) = typed_account else {
        return;
    };

    match method {
        "copy.StartBase" | "copy.PvpStartBase" => {
            let Ok(request) = CopyStartRequest::decode(request_args) else {
                return;
            };
            if request.copy_id <= 0 || typed_account.battle.active.is_some() {
                return;
            }
            let session = legacy_account
                .and_then(|account| account.get("battleSession"))
                .and_then(Value::as_object)
                .filter(|session| {
                    session.get("copyId").and_then(Value::as_i64)
                        == Some(i64::from(request.copy_id))
                });
            let fleet_id = session
                .and_then(|session| session.get("remainingFleetIds"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_u64)
                .find(|fleet_id| *fleet_id > 0)
                .or_else(|| typed_account.fleet.fleets.keys().next().map(|id| id.get()));
            let (Some(chapter_id), Some(copy_id), Some(fleet_id)) = (
                ChapterId::new(request.copy_id as u64).ok(),
                CopyId::new(request.copy_id as u64).ok(),
                fleet_id.and_then(|id| FleetId::new(id).ok()),
            ) else {
                return;
            };
            let hero_ids = session
                .and_then(|session| session.get("heroIds"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_u64)
                .collect::<Vec<_>>();
            let hero_ids = if hero_ids.is_empty() {
                typed_account
                    .fleet
                    .fleets
                    .get(&fleet_id)
                    .map(|fleet| fleet.members.iter().map(|id| id.get()).collect::<Vec<_>>())
                    .unwrap_or_default()
            } else {
                hero_ids
            };
            let supply_ok = battle_catalog.is_none()
                || consume_battle_supply_typed(
                    typed_account,
                    battle_catalog,
                    request.copy_id,
                    &hero_ids,
                    1,
                );
            if supply_ok
                && BattleService::start(
                    typed_account,
                    chapter_id,
                    copy_id,
                    fleet_id,
                    u64::from(now),
                )
                .is_ok()
            {
                if let Some(active) = typed_account.battle.active.as_mut() {
                    active.expires_at = u64::from(now).saturating_add(1_800);
                    active.remaining_fleet_ids = session
                        .and_then(|session| session.get("remainingFleetIds"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_u64)
                        .filter_map(|id| u32::try_from(id).ok())
                        .filter(|id| *id > 0)
                        .collect();
                    active.hero_ids = hero_ids
                        .iter()
                        .filter_map(|id| blueoath_domain::HeroId::new(*id).ok())
                        .collect();
                    active.attack_count = session
                        .and_then(|session| session.get("attackCount"))
                        .and_then(Value::as_u64)
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                }
            }
        }
        "copy.PassBase" => {
            let Some(active) = typed_account.battle.active.as_ref() else {
                return;
            };
            let copy_id = active.copy_id;
            let grade = decode_battle_pass_result(request_args).grade;
            let victory = grade <= 0 || grade < 9;
            let session_finished = legacy_account
                .and_then(|account| account.get("battleSession"))
                .is_none_or(Value::is_null);
            if session_finished {
                let _ = BattleService::settle(typed_account, copy_id, victory);
            } else if victory {
                if let Some(active) = typed_account.battle.active.as_mut() {
                    let remaining = legacy_account
                        .and_then(|account| account.get("battleSession"))
                        .and_then(|session| session.get("remainingFleetIds"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_u64)
                        .filter_map(|id| u32::try_from(id).ok())
                        .filter(|id| *id > 0)
                        .collect::<Vec<_>>();
                    if let Some(next_fleet) = remaining
                        .first()
                        .copied()
                        .filter(|next| *next != active.current_fleet)
                    {
                        active.current_fleet = next_fleet;
                    } else {
                        active.current_fleet = active.current_fleet.saturating_add(1);
                    }
                    active.remaining_fleet_ids = remaining;
                    active.revision = active.revision.saturating_add(1);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod route_guard_tests {
    use super::legacy_only_method;

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
}
