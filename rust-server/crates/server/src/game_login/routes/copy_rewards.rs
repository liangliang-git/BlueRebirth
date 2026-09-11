use super::copy::copy_type_for_chapter;
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
            && matches!(
                request.method.as_str(),
                "copy.StarReward" | "copy.FetchRewardBox"
            ) =>
        {
            let mut copy_star_effects = ResponseEffects::default();
            let account = typed_account.as_deref_mut().expect("typed copy account");
            let result = battle_handler::handle_typed_copy_star_reward(
                account,
                request.method.as_str(),
                request_args,
                chapter_catalog,
                task_catalog,
                &mut copy_star_effects,
            );
            apply_response_effects(
                copy_star_effects,
                pre_pushes,
                post_pushes,
                &mut handler_error,
            );
            if let HandlerResult::Error(error) = &result {
                handler_error = Some(error.clone());
            }
            if matches!(&result, HandlerResult::Reply(_)) {
                if let (Some(catalog), Ok(reward_request)) =
                    (chapter_catalog, CopyStarRewardRequest::decode(request_args))
                {
                    let copy_type = copy_type_for_chapter(catalog, reward_request.chapter_id);
                    append_method_push(
                        pre_pushes,
                        "copy.GetCopy",
                        CopyInfoCodec::encode_payload(&copy_info_payload(
                            catalog, copy_type, account,
                        )),
                    );
                }
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
        _ => {
            *handler_error_slot = handler_error;
            return (false, None);
        }
    };
    *handler_error_slot = handler_error;
    (true, result)
}
