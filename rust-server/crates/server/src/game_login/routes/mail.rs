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
                    pre_pushes,
                    "user.UpdateUserInfo",
                    UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
                );
                append_method_push(
                    pre_pushes,
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
        _ => {
            *handler_error_slot = handler_error;
            return (false, None);
        }
    };
    *handler_error_slot = handler_error;
    (true, result)
}
