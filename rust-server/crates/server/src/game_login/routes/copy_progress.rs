use super::copy::copy_info_payload;
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
                        post_pushes,
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
        _ => {
            *handler_error_slot = handler_error;
            return (false, None);
        }
    };
    *handler_error_slot = handler_error;
    (true, result)
}
