//! Hero lookup and profile mutation handlers.

use super::super::*;
use crate::common::error::GameError;
use crate::common::response::{HandlerResult, Response, ResponseEffects};

/// 处理英雄查询、锁定和改名等基础资料请求。
pub(super) fn handle(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
    match method {
        "hero.GetHeroInfo" | "hero.GetHeroInfoByHeroIdArray" => {
            HandlerResult::Reply(Response::raw(
                method,
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            ))
        }
        "hero.LockHero" => {
            let Ok(request) = HeroLockRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "hero lock request is invalid",
                ));
            };
            let hero_id = request.hero_id;
            let locked = request.locked;
            let Some(hero_id) = blueoath_domain::HeroId::new(hero_id).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
            };
            let Some(hero) = account.dock.heroes.get_mut(&hero_id) else {
                return HandlerResult::Error(GameError::InvalidRequest("hero was not found"));
            };
            hero.locked = locked;
            effects.push_pre(Response::raw(
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            ));
            HandlerResult::PushOnly
        }
        "hero.ChangeName" => {
            let Ok(request) = HeroChangeNameRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "hero name request is invalid",
                ));
            };
            let hero_id = request.hero_id;
            let Some(hero_id) = blueoath_domain::HeroId::new(hero_id).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
            };
            let Some(hero) = account.dock.heroes.get_mut(&hero_id) else {
                return HandlerResult::Error(GameError::InvalidRequest("hero was not found"));
            };
            hero.name = request.name;
            hero.change_name_time = u64::from(current_unix_seconds());
            effects.push_pre(Response::raw(
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            ));
            HandlerResult::PushOnly
        }
        _ => HandlerResult::Empty,
    }
}
