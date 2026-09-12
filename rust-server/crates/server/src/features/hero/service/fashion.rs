//! Hero fashion ownership and equip handlers.

use super::super::*;
use super::response::push_hero_changes;
use crate::common::error::GameError;
use crate::common::response::{HandlerResult, ResponseEffects};
/// 处理英雄时装装备请求并更新时装状态。
pub(super) fn handle_fashion_equip(
    account: &mut blueoath_domain::AccountState,
    request_args: &[u8],
    catalog: Option<&FashionList>,
    state: Option<&ServerState>,
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let Ok(request) = FashionEquipRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "fashion equip request is invalid",
        ));
    };
    let Ok(hero_id) = blueoath_domain::HeroId::new(request.hero_id) else {
        return HandlerResult::Error(GameError::InvalidRequest("hero id is invalid"));
    };
    let Some(hero) = account.dock.heroes.get(&hero_id) else {
        return HandlerResult::Error(GameError::NotFound("hero"));
    };
    let sf_id = hero.template_id.get().saturating_sub(1) / 10;
    let selected = if request.fashion_tid > 0 {
        u64::try_from(request.fashion_tid).unwrap_or_default()
    } else if request.equip_status == 0 {
        sf_id
    } else {
        return HandlerResult::Error(GameError::InvalidRequest("fashion is invalid"));
    };
    let Some(selected) = blueoath_domain::TemplateId::new(selected).ok() else {
        return HandlerResult::Error(GameError::InvalidRequest("fashion is invalid"));
    };
    if let Some(catalog) = catalog {
        let Ok(sf_id) = i32::try_from(sf_id) else {
            return HandlerResult::Error(GameError::InvalidState("fashion ship id is invalid"));
        };
        let belongs = catalog
            .items
            .iter()
            .find(|item| item.sf_id == sf_id)
            .is_some_and(|item| {
                item.fashion_tids.contains(&(selected.get() as i32))
                    || selected.get() == sf_id as u64
            });
        if !belongs {
            return HandlerResult::Error(GameError::InvalidState("fashion is not for this hero"));
        }
        let owned = selected.get() == sf_id as u64
            || account
                .fashion
                .entries
                .get(&(sf_id as u64))
                .is_some_and(|items| items.contains(&selected));
        if !owned {
            return HandlerResult::Error(GameError::InvalidState("fashion is not owned by hero"));
        }
    }
    let Some(hero) = account.dock.heroes.get_mut(&hero_id) else {
        return HandlerResult::Error(GameError::NotFound("hero"));
    };
    hero.fashioning = u32::try_from(selected.get()).unwrap_or_default();
    push_hero_changes(account, state, effects, false);
    HandlerResult::PushOnly
}
