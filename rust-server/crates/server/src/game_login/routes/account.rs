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
        _ => {
            *handler_error_slot = handler_error;
            return (false, None);
        }
    };
    *handler_error_slot = handler_error;
    (true, result)
}
