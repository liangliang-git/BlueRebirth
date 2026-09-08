use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;

pub(crate) fn handle_typed(
    state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    if GameMethod::parse(method).is_family(MethodFamily::Magazine) {
        return handle_typed_magazine(state, account, catalog, method, request_args, effects);
    }
    handle_typed_interaction(account, catalog, method, request_args)
}

fn handle_typed_magazine(
    state: &ServerState,
    account: &mut blueoath_domain::AccountState,
    catalog: &GameplayCatalog,
    method: &str,
    args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
    if method == "magazine.FetchMagazineReward" {
        let Ok(request) = MagazineItemRequest::decode(args) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "magazine item request is invalid",
            ));
        };
        let reward_key = request.item_id as u64;
        let already_claimed = account.magazine.claimed_rewards.contains(&reward_key);
        let reward_id = magazine_reward_id(catalog, reward_key as i32);
        let rewards = if !already_claimed && reward_id > 0 {
            catalog
                .rewards_by_id
                .get(&reward_id)
                .cloned()
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if !rewards.is_empty() && !can_grant_typed_task_rewards(account, &rewards) {
            return HandlerResult::Error(GameError::InvalidState("magazine reward is unsupported"));
        }
        for reward in &rewards {
            let _ = grant_typed_task_reward(account, reward);
        }
        account.magazine.claimed_rewards.insert(reward_key);
        if !rewards.is_empty() {
            effects.push_pre(Response::raw(
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
            ));
            effects.push_pre(Response::raw(
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            ));
            return reply(method, encode_rewards_list(&rewards));
        }
        return reply(method, typed_magazine_payload(account));
    }
    match method {
        "magazine.GetMagazine" | "magazine.Magazine" => {
            reply(method, typed_magazine_payload(account))
        }
        "magazine.UpdateMagazineInfo" => reply(method, typed_magazine_update_payload(account)),
        "magazine.AddHero" => {
            let Ok(request) = MagazineItemRequest::decode(args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "magazine item request is invalid",
                ));
            };
            let hero_id = request.item_id as u64;
            if hero_id > 0 && !account.magazine.heroes.contains(&hero_id) {
                account.magazine.heroes.push(hero_id);
            }
            reply(method, typed_magazine_payload(account))
        }
        "magazine.Vote" => {
            let Ok(request) = MagazineItemRequest::decode(args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "magazine item request is invalid",
                ));
            };
            let page_id = request.item_id as u64;
            if page_id > 0 {
                account.magazine.votes.insert(page_id);
            }
            reply(method, typed_magazine_payload(account))
        }
        "magazine.UnLock" => {
            let Ok(request) = MagazineItemRequest::decode(args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "magazine item request is invalid",
                ));
            };
            let page_id = request.item_id as u64;
            if page_id > 0 {
                account.magazine.unlocked.insert(page_id);
            }
            reply(method, typed_magazine_payload(account))
        }
        _ => HandlerResult::Empty,
    }
}

fn handle_typed_interaction(
    account: &mut blueoath_domain::AccountState,
    catalog: &GameplayCatalog,
    method: &str,
    args: &[u8],
) -> HandlerResult {
    if method == "interactionitem.RefreshInteractionItems" {
        return reply(method, typed_interaction_payload(account));
    }
    let item_request = match InteractionItemStateRequest::decode(args) {
        Ok(request) => request,
        Err(_) => {
            return HandlerResult::Error(GameError::InvalidRequest(
                "interaction item request is invalid",
            ));
        }
    };
    let item_id = item_request.item_id as u64;
    if matches!(
        method,
        "interactionitem.GetItemReward"
            | "interactionitem.BuyChristmasFurniture"
            | "interactionitem.GetSpringPaperFlowerReward"
    ) {
        let rewards = interaction_reward_defs(catalog, item_id as i32);
        if !account.interaction_items.rewards.contains(&item_id) {
            if !rewards.is_empty() && !can_grant_typed_task_rewards(account, &rewards) {
                return HandlerResult::Error(GameError::InvalidState(
                    "interaction item reward is unsupported",
                ));
            }
            for reward in &rewards {
                let _ = grant_typed_task_reward(account, reward);
            }
        }
        account.interaction_items.rewards.insert(item_id);
        if method == "interactionitem.GetItemReward" {
            return reply(method, encode_rewards_list(&rewards));
        }
        return reply(method, typed_interaction_payload(account));
    }
    match method {
        "interactionitem.SetCrystalBallToy" => {
            account.interaction_items.crystal_ball_toy = item_id;
            reply(method, typed_interaction_payload(account))
        }
        "interactionitem.SetBagItemVisible" => {
            let visible = item_request.value != 0;
            account.interaction_items.visible.insert(item_id, visible);
            reply(method, typed_interaction_payload(account))
        }
        "interactionitem.SetMutexBagGroupState" => {
            account
                .interaction_items
                .groups
                .insert(item_id, item_request.value as u64);
            reply(method, typed_interaction_payload(account))
        }
        "interactionitem.SetPosterState" => {
            account
                .interaction_items
                .posters
                .insert(item_id, item_request.value as u64);
            reply(method, typed_interaction_payload(account))
        }
        _ => HandlerResult::Empty,
    }
}

fn typed_magazine_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    for id in &account.magazine.unlocked {
        let mut magazine = Vec::new();
        append_varint_field(&mut magazine, 1, *id);
        for reward in &account.magazine.claimed_rewards {
            let mut item = Vec::new();
            append_varint_field(&mut item, 1, *reward);
            append_varint_field(&mut item, 2, u64::from(current_unix_seconds()));
            append_message_field(&mut magazine, 2, &item);
        }
        append_message_field(&mut output, 1, &magazine);
    }
    for (index, hero_id) in account.magazine.heroes.iter().enumerate() {
        let mut hero = Vec::new();
        append_varint_field(&mut hero, 1, (index as u64).saturating_add(1));
        append_varint_field(&mut hero, 2, *hero_id);
        append_varint_field(&mut hero, 3, 0);
        append_message_field(&mut output, 2, &hero);
    }
    for id in &account.magazine.unlocked {
        let mut unlock = Vec::new();
        append_varint_field(&mut unlock, 1, *id);
        append_varint_field(&mut unlock, 2, u64::from(current_unix_seconds()));
        append_message_field(&mut output, 3, &unlock);
    }
    output
}

fn typed_magazine_update_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    for id in &account.magazine.unlocked {
        append_varint_field(&mut output, 1, *id);
    }
    append_varint_field(&mut output, 2, u64::from(current_unix_seconds()));
    append_varint_field(&mut output, 3, 1);
    output
}

fn typed_interaction_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let state = &account.interaction_items;
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, state.crystal_ball_toy);
    for reward in &state.rewards {
        append_varint_field(&mut output, 2, *reward);
    }
    for (id, visible) in &state.visible {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, *id);
        append_varint_field(&mut item, 2, u64::from(*visible));
        append_message_field(&mut output, 3, &item);
    }
    output
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn magazine_reward_id(catalog: &GameplayCatalog, reward_key: i32) -> i32 {
    if catalog.rewards_by_id.contains_key(&reward_key) {
        return reward_key;
    }
    catalog
        .magazine_info
        .values()
        .flat_map(|info| info.rewards.iter().copied())
        .nth(usize::try_from(reward_key.saturating_sub(1)).unwrap_or(usize::MAX))
        .unwrap_or_default()
}

fn interaction_reward_defs(catalog: &GameplayCatalog, item_id: i32) -> Vec<ShopReward> {
    let Some(item) = catalog.interaction_items.get(&item_id) else {
        return Vec::new();
    };
    if item.reward_id > 0 {
        return catalog
            .rewards_by_id
            .get(&item.reward_id)
            .cloned()
            .unwrap_or_default();
    }
    let Some(drop) = catalog
        .drop_items
        .get(&item.drop_id)
        .filter(|_| item.drop_id > 0)
    else {
        return Vec::new();
    };
    drop.entries
        .iter()
        .map(|entry| ShopReward {
            goods_type: entry.goods_type,
            item_id: entry.item_id,
            num: entry.min,
            instance_id: 0,
        })
        .filter(|reward| reward.goods_type > 0 && reward.item_id > 0 && reward.num > 0)
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::common::response::HandlerResult;
    use blueoath_domain::AccountState;

    use super::*;

    #[test]
    fn typed_magazine_and_interaction_state_update_without_json() {
        let state = ServerState::new("misc", "Captain", "test");
        let mut account = AccountState::default();
        let mut effects = ResponseEffects::default();
        let result = handle_typed(
            &state,
            &mut account,
            "magazine.AddHero",
            &[0x08, 10],
            &mut effects,
        );
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(account.magazine.heroes, vec![10]);
        let result = handle_typed(
            &state,
            &mut account,
            "interactionitem.SetBagItemVisible",
            &[0x08, 11, 0x10, 1],
            &mut effects,
        );
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(account.interaction_items.visible.get(&11), Some(&true));
    }
}
