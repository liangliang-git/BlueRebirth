use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(crate) fn handles(method: &str) -> bool {
    GameMethod::parse(method).is_family(MethodFamily::Activity)
}

pub(crate) fn handles_typed(method: &str) -> bool {
    matches!(
        method,
        "activityextract.Get"
            | "activityextract.Update"
            | "activityextract.SwitchDraw"
            | "activityextract.Draw"
            | "activityextractur.Get"
            | "activityextractur.Update"
            | "activityextractur.SwitchDraw"
            | "activityextractur.Draw"
            | "activitySSR.GetActivitySSRInfo"
            | "activitySSR.ActivitySSRSelect"
            | "activitySSR.ActivitySSRRand"
            | "activitySSR.ActivitySSRShare"
            | "activitySSRrolls.UpdateActivityRollsInfo"
            | "activitySSRrolls.UpdateActivityRollsInfoRPC"
            | "activitySSRrolls.ActivityRollsSelect"
            | "activitySSRrolls.ActivityRollsRand"
            | "activitybirthday.BirthdayRefresh"
            | "activitybirthday.UpdateBirthdayInfo"
            | "activitybirthday.FeedBirthdayCake"
            | "activitybirthday.MakeBirthdayCake"
            | "activitybirthday.GetCakeAffairReward"
            | "activityfashion.PushActivityFashionInfo"
            | "activityfashion.Buy"
            | "activityfashion.Reward"
            | "activitycodeexchange.UpdateActivityCodeExgInfo"
            | "activitycodeexchange.ExchangeCode"
            | "activitycodeexchange.ExchangeReward"
            | "activitypapercut.UpdateActivityPaperCutInfo"
            | "activitypapercut.MakePaperCut"
            | "activitysecretcopy.UpdateActivitySecretCopyInfo"
            | "activitysecretcopy.GetReward"
            | "activityvalentineloveletter.UpdateActivityValentineLoveLetterInfo"
            | "activityvalentineloveletter.GetReward"
            | "activityvalentineloveletter.GetRewardBySecretary"
            | "activityVideo.GetActivityVideo"
            | "activityVideo.SetActivityVideo"
            | "activitychristmasshop.UpdateActivityChristmasShopInfo"
            | "activitychristmasshop.BuyBlindBox"
            | "activitychristmasshop.BuyBlindItem"
            | "activitychristmasshop.OpenSpecialBlindBox"
            | "activitychristmasshop.SetToy"
            | "activitychristmasshop.GiveMeCrystalBall"
    )
}

pub(crate) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    fashion_catalog: Option<&FashionList>,
) -> HandlerResult {
    if matches!(
        method,
        "activityextract.Draw"
            | "activityextractur.Draw"
            | "activitybirthday.MakeBirthdayCake"
            | "activitybirthday.GetCakeAffairReward"
            | "activityvalentineloveletter.GetReward"
            | "activityvalentineloveletter.GetRewardBySecretary"
            | "activityVideo.SetActivityVideo"
            | "activitycodeexchange.ExchangeCode"
            | "activitycodeexchange.ExchangeReward"
            | "activitypapercut.MakePaperCut"
            | "activityfashion.Buy"
            | "activityfashion.Reward"
            | "activitychristmasshop.BuyBlindBox"
            | "activitychristmasshop.BuyBlindItem"
    ) {
        return match method {
            "activityextract.Draw" | "activityextractur.Draw" => {
                handle_typed_extract_draw(account, method, request_args)
            }
            "activitybirthday.MakeBirthdayCake" | "activitybirthday.GetCakeAffairReward" => {
                handle_typed_birthday_reward(account, method, request_args)
            }
            "activityvalentineloveletter.GetReward"
            | "activityvalentineloveletter.GetRewardBySecretary" => {
                handle_typed_valentine_reward(account, method, request_args)
            }
            "activityVideo.SetActivityVideo" => handle_typed_video_set(account, request_args),
            "activitycodeexchange.ExchangeCode" | "activitycodeexchange.ExchangeReward" => {
                handle_typed_code_exchange(account, method, request_args)
            }
            "activitypapercut.MakePaperCut" => handle_typed_paper_cut(account, request_args),
            "activityfashion.Buy" | "activityfashion.Reward" => {
                handle_typed_fashion(account, method, request_args, fashion_catalog)
            }
            "activitychristmasshop.BuyBlindBox" | "activitychristmasshop.BuyBlindItem" => {
                handle_typed_christmas_buy(account, method, request_args)
            }
            _ => unreachable!(),
        };
    }
    let progress = &mut account.activities.progress;
    match method {
        "activityextract.Get" | "activityextract.Update" => {
            typed_reply(method, typed_extract_payload(progress, "activityExtract"))
        }
        "activityextract.SwitchDraw" => {
            increment_activity_value(progress, "activityExtract", "realDrawId");
            typed_reply(method, typed_extract_payload(progress, "activityExtract"))
        }
        "activityextractur.Get" | "activityextractur.Update" => {
            typed_reply(method, typed_extract_payload(progress, "activityExtractUr"))
        }
        "activityextractur.SwitchDraw" => {
            increment_activity_value(progress, "activityExtractUr", "realDrawId");
            typed_reply(method, typed_extract_payload(progress, "activityExtractUr"))
        }
        "activitySSR.GetActivitySSRInfo"
        | "activitySSR.ActivitySSRSelect"
        | "activitySSR.ActivitySSRRand"
        | "activitySSR.ActivitySSRShare" => {
            let state = "activitySSR";
            match method {
                "activitySSR.ActivitySSRSelect" => {
                    let Ok(request) = ActivitySelectShipRequest::decode(request_args) else {
                        return HandlerResult::Error(GameError::InvalidRequest(
                            "activity ship request is invalid",
                        ));
                    };
                    set_activity_value(
                        progress,
                        state,
                        "selectShipId",
                        request.ship_id.max(0) as u64,
                    );
                }
                "activitySSR.ActivitySSRRand" => {
                    let selected = activity_value(progress, state, "selectShipId").max(1);
                    set_activity_value(progress, state, "saveShipId", selected);
                    increment_activity_value(progress, state, "daySelectCount");
                }
                "activitySSR.ActivitySSRShare" => {
                    increment_activity_value(progress, state, "dayShareCount");
                }
                _ => {}
            }
            typed_reply(method, typed_ssr_payload(progress, state))
        }
        "activitySSRrolls.UpdateActivityRollsInfo"
        | "activitySSRrolls.UpdateActivityRollsInfoRPC"
        | "activitySSRrolls.ActivityRollsSelect"
        | "activitySSRrolls.ActivityRollsRand" => {
            let state = "activitySSRRolls";
            match method {
                "activitySSRrolls.ActivityRollsSelect" => {
                    let Ok(request) = ActivitySelectTeamRequest::decode(request_args) else {
                        return HandlerResult::Error(GameError::InvalidRequest(
                            "activity team request is invalid",
                        ));
                    };
                    let team_id = request.team_id.max(0);
                    set_activity_value(progress, state, "selectTeamId", team_id as u64);
                    set_activity_value(progress, state, "selectTeam", team_id as u64);
                    increment_activity_value(progress, state, "daySelectCount");
                }
                "activitySSRrolls.ActivityRollsRand" => {
                    let team_id = activity_value(progress, state, "selectTeamId").max(1);
                    set_activity_value(progress, state, "saveTeam", team_id);
                }
                _ => {}
            }
            typed_reply(method, typed_rolls_payload(progress, state))
        }
        "activitybirthday.BirthdayRefresh" | "activitybirthday.UpdateBirthdayInfo" => {
            typed_reply(method, typed_birthday_payload(progress))
        }
        "activitybirthday.FeedBirthdayCake" => {
            let Ok(request) = BirthdayFeedRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "birthday feed request is invalid",
                ));
            };
            let team_id = request.team_id as u64;
            let cake = request.cake as u64;
            set_activity_value(
                progress,
                "activityBirthday",
                &format!("girl:{team_id}:teamId"),
                team_id,
            );
            set_activity_value(
                progress,
                "activityBirthday",
                &format!("girl:{team_id}:cake"),
                cake,
            );
            typed_reply(method, typed_birthday_payload(progress))
        }
        "activityfashion.PushActivityFashionInfo" => {
            typed_reply(method, typed_fashion_payload(progress))
        }
        "activitycodeexchange.UpdateActivityCodeExgInfo" => {
            typed_reply(method, typed_code_exchange_payload(progress))
        }
        "activitypapercut.UpdateActivityPaperCutInfo" => {
            typed_reply(method, typed_paper_cut_payload(progress))
        }
        "activitysecretcopy.UpdateActivitySecretCopyInfo" => {
            typed_reply(method, typed_secret_copy_payload(progress))
        }
        "activitysecretcopy.GetReward" => {
            let Ok(request) = ActivityRewardIndexRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "secret copy reward request is invalid",
                ));
            };
            let rate_index = request.index as u64;
            let key = format!("activity:activitySecretCopy:reward:{rate_index}:getReward");
            if progress.contains_key(&key) {
                return HandlerResult::Error(GameError::InvalidState(
                    "secret copy reward was already claimed",
                ));
            }
            progress.insert(key, 1);
            typed_reply(method, typed_secret_copy_payload(progress))
        }
        "activityvalentineloveletter.UpdateActivityValentineLoveLetterInfo" => {
            typed_reply(method, typed_valentine_payload(progress))
        }
        "activityVideo.GetActivityVideo" => typed_reply(method, typed_video_payload(progress)),
        "activitychristmasshop.UpdateActivityChristmasShopInfo" => {
            typed_reply(method, typed_christmas_payload(progress))
        }
        "activitychristmasshop.OpenSpecialBlindBox" => {
            let Ok(request) = ActivityItemIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "christmas special box request is invalid",
                ));
            };
            let item_id = request.item_id as u64;
            progress.insert(
                format!("activity:activityChristmasShop:specialBox:{item_id}"),
                1,
            );
            let mut output = Vec::new();
            append_varint_field(&mut output, 1, item_id);
            typed_reply(method, output)
        }
        "activitychristmasshop.SetToy" => {
            let Ok(request) = ActivityItemIdRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "christmas toy request is invalid",
                ));
            };
            set_activity_value(
                progress,
                "activityChristmasShop",
                "crystalBallToyId",
                request.item_id as u64,
            );
            typed_reply(method, typed_christmas_payload(progress))
        }
        "activitychristmasshop.GiveMeCrystalBall" => {
            set_activity_value(progress, "activityChristmasShop", "isGiveCrystalBall", 1);
            typed_reply(method, typed_christmas_payload(progress))
        }
        _ => HandlerResult::Error(GameError::InvalidRequest(
            "activity method requires typed activity rule",
        )),
    }
}

fn handle_typed_code_exchange(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    match method {
        "activitycodeexchange.ExchangeCode" => {
            let Ok(request) = ActivityCodeExchangeRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "activity exchange request is invalid",
                ));
            };
            let code = request.code as u64;
            let number = request.number.clamp(1, 99) as u64;
            let key = format!("activity:activityCodeExchange:receipt:{code}:count");
            let progress = &mut account.activities.progress;
            let count = progress.entry(key).or_default();
            *count = count.saturating_add(number);
            typed_reply(method, typed_code_exchange_payload(progress))
        }
        "activitycodeexchange.ExchangeReward" => {
            let Ok(request) = ActivityExchangeRewardRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "activity exchange reward request is invalid",
                ));
            };
            let reward_index = request.reward_index as u64;
            let number = request.number.clamp(1, 99) as u64;
            let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
            let Some(activity) = code_exchange_activity(catalog) else {
                return HandlerResult::Error(GameError::InvalidState(
                    "activity exchange config is unavailable",
                ));
            };
            let reward_id = activity
                .p4
                .get((reward_index - 1) as usize)
                .and_then(|reward| reward.first())
                .copied()
                .unwrap_or_default();
            let reward_defs = catalog
                .rewards_by_id
                .get(&reward_id)
                .cloned()
                .unwrap_or_default();
            let receipt_key = format!("activity:activityCodeExchange:receipt:{reward_index}:count");
            let available = account
                .activities
                .progress
                .get(&receipt_key)
                .copied()
                .unwrap_or_default();
            if reward_id <= 0 || reward_defs.is_empty() {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "activity exchange reward is invalid",
                ));
            }
            if available < number {
                return HandlerResult::Error(GameError::InvalidState(
                    "activity exchange reward count is insufficient",
                ));
            }
            let rewards = (0..number)
                .flat_map(|_| reward_defs.iter().copied())
                .collect::<Vec<_>>();
            if !task_state::can_grant_typed_task_rewards(account, &rewards) {
                return HandlerResult::Error(GameError::InvalidState(
                    "activity exchange reward is unsupported",
                ));
            }
            for reward in &rewards {
                if !task_state::grant_typed_task_reward(account, reward) {
                    return HandlerResult::Error(GameError::InvalidState(
                        "activity exchange reward is unsupported",
                    ));
                }
            }
            account
                .activities
                .progress
                .insert(receipt_key, available - number);
            typed_reply(method, encode_rewards_list(&rewards))
        }
        _ => HandlerResult::Error(GameError::InvalidRequest(
            "activity exchange method is unsupported",
        )),
    }
}

fn handle_typed_extract_draw(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let Ok(request) = ActivityExtractDrawRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "activity extract request is invalid",
        ));
    };
    let draw_id = request.draw_id;
    let num = request.num.clamp(1, 10);
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    let ur = method == "activityextractur.Draw";
    let configs = if ur {
        &catalog.activity_extract_ur
    } else {
        &catalog.activity_extract
    };
    let Some(config) = configs.get(&draw_id) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "activity extract pool is not configured",
        ));
    };
    let Some(cost) = config.cost else {
        return HandlerResult::Error(GameError::InvalidState(
            "activity extract cost is not configured",
        ));
    };
    let entries = config.rewards.clone();
    if entries.is_empty() {
        return HandlerResult::Error(GameError::InvalidState(
            "activity extract reward pool is empty",
        ));
    }
    let total_cost = cost.2.saturating_mul(num);
    if !typed_activity_can_consume(account, cost.0, cost.1, total_cost) {
        return HandlerResult::Error(GameError::InvalidState(
            "activity extract cost is insufficient",
        ));
    }
    let state = if ur {
        "activityExtractUr"
    } else {
        "activityExtract"
    };
    let start = account
        .activities
        .progress
        .get(&activity_key(state, "drawCount"))
        .copied()
        .unwrap_or_default() as usize;
    let snapshot = account.clone();
    if !typed_activity_consume(account, cost.0, cost.1, total_cost) {
        return HandlerResult::Error(GameError::InvalidState(
            "activity extract cost is insufficient",
        ));
    }
    let mut granted = Vec::new();
    let mut selected_ids = Vec::new();
    for offset in 0..num as usize {
        let (reward_id, amount) = entries[(start + offset) % entries.len()];
        selected_ids.push(reward_id);
        let reward_defs = catalog
            .rewards_by_id
            .get(&reward_id)
            .cloned()
            .unwrap_or_else(|| {
                vec![ShopReward {
                    goods_type: 1,
                    item_id: reward_id,
                    num: amount,
                    instance_id: 0,
                }]
            });
        if !task_state::can_grant_typed_task_rewards(account, &reward_defs) {
            *account = snapshot;
            return HandlerResult::Error(GameError::InvalidState(
                "activity extract reward is unsupported",
            ));
        }
        for reward in reward_defs {
            if !task_state::grant_typed_task_reward(account, &reward) {
                *account = snapshot;
                return HandlerResult::Error(GameError::InvalidState(
                    "activity extract reward is unsupported",
                ));
            }
            granted.push(reward);
        }
        let sequence = start.saturating_add(offset).saturating_add(1) as u64;
        account.activities.progress.insert(
            activity_key(state, &format!("reward:{sequence}:id")),
            reward_id.max(0) as u64,
        );
        account.activities.progress.insert(
            activity_key(state, &format!("reward:{sequence}:num")),
            amount.max(0) as u64,
        );
    }
    let next = start.saturating_add(num as usize) as u64;
    account
        .activities
        .progress
        .insert(activity_key(state, "drawCount"), next);
    account
        .activities
        .progress
        .insert(activity_key(state, "drawId"), draw_id as u64);
    account
        .activities
        .progress
        .insert(activity_key(state, "realDrawId"), draw_id as u64);
    if ur {
        typed_reply(method, extract_ur_draw_ret_payload(&selected_ids))
    } else {
        typed_reply(method, extract_draw_ret_payload(&granted))
    }
}

fn handle_typed_birthday_reward(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    match method {
        "activitybirthday.MakeBirthdayCake" => {
            let Ok(request) = ActivityFormulaRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "birthday cake request is invalid",
                ));
            };
            let formula = request.formula;
            let Some(reward) = birthday_formula_reward(catalog, formula) else {
                return HandlerResult::Error(GameError::InvalidState(
                    "birthday cake formula is not configured",
                ));
            };
            if !task_state::can_grant_typed_task_reward(account, &reward) {
                return HandlerResult::Error(GameError::InvalidState(
                    "birthday cake reward is unsupported",
                ));
            }
            if !task_state::grant_typed_task_reward(account, &reward) {
                return HandlerResult::Error(GameError::InvalidState(
                    "birthday cake reward is unsupported",
                ));
            }
            let cake = account
                .activities
                .progress
                .entry(activity_key("activityBirthday", "cake"))
                .or_default();
            *cake = cake.saturating_add(1);
            account.activities.progress.insert(
                activity_key("activityBirthday", "lastCakeFormula"),
                formula.max(0) as u64,
            );
            typed_reply(method, typed_birthday_payload(&account.activities.progress))
        }
        "activitybirthday.GetCakeAffairReward" => {
            let Ok(request) = ActivityRewardIndexRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "birthday affair request is invalid",
                ));
            };
            let level = request.index as u64;
            let claim_key = activity_key("activityBirthday", &format!("claimedAffair:{level}"));
            if account.activities.progress.contains_key(&claim_key) {
                return typed_reply(method, typed_birthday_payload(&account.activities.progress));
            }
            let Some(reward_id) = birthday_affair_reward_id(catalog, level as i32) else {
                return HandlerResult::Error(GameError::InvalidState(
                    "birthday affair reward is not configured",
                ));
            };
            let rewards = catalog
                .rewards_by_id
                .get(&reward_id)
                .cloned()
                .unwrap_or_default();
            if rewards.is_empty() || !task_state::can_grant_typed_task_rewards(account, &rewards) {
                return HandlerResult::Error(GameError::InvalidState(
                    "birthday affair reward is unsupported",
                ));
            }
            for reward in &rewards {
                if !task_state::grant_typed_task_reward(account, reward) {
                    return HandlerResult::Error(GameError::InvalidState(
                        "birthday affair reward is unsupported",
                    ));
                }
            }
            account.activities.progress.insert(claim_key, 1);
            typed_reply(method, typed_birthday_payload(&account.activities.progress))
        }
        _ => HandlerResult::Error(GameError::InvalidRequest("birthday method is unsupported")),
    }
}

fn handle_typed_valentine_reward(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let by_secretary = method.ends_with("BySecretary");
    let Ok(request) = ValentineRewardRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "valentine reward request is invalid",
        ));
    };
    let index = request.index.max(0) as u64;
    if !by_secretary && index == 0 {
        return HandlerResult::Error(GameError::InvalidRequest(
            "valentine reward index is invalid",
        ));
    }
    let claim_kind = if by_secretary { "secretary" } else { "hero" };
    let claim_key = activity_key("activityValentine", &format!("claim:{claim_kind}:{index}"));
    if account.activities.progress.contains_key(&claim_key) {
        return typed_reply(
            method,
            typed_valentine_payload(&account.activities.progress),
        );
    }
    let ship_tid = if by_secretary {
        activity_value(
            &account.activities.progress,
            "activityValentine",
            "curActShip",
        ) as i32
    } else {
        activity_value(
            &account.activities.progress,
            "activityValentine",
            &format!("loveShip:{index}:shipTid"),
        ) as i32
    };
    if ship_tid <= 0 {
        return HandlerResult::Error(GameError::InvalidState(
            "valentine ship state is unavailable",
        ));
    }
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    let reward_id = catalog
        .valentine_gifts
        .values()
        .find(|gift| gift.ship_fleet_id == ship_tid)
        .map(|gift| gift.attach_reward);
    let Some(reward_id) = reward_id else {
        return HandlerResult::Error(GameError::InvalidState(
            "valentine reward is not configured",
        ));
    };
    let rewards = catalog
        .rewards_by_id
        .get(&reward_id)
        .cloned()
        .unwrap_or_default();
    if rewards.is_empty() || !task_state::can_grant_typed_task_rewards(account, &rewards) {
        return HandlerResult::Error(GameError::InvalidState("valentine reward is unsupported"));
    }
    for reward in &rewards {
        if !task_state::grant_typed_task_reward(account, reward) {
            return HandlerResult::Error(GameError::InvalidState(
                "valentine reward is unsupported",
            ));
        }
    }
    account.activities.progress.insert(claim_key, 1);
    typed_reply(
        method,
        typed_valentine_payload(&account.activities.progress),
    )
}

fn typed_activity_currency(item_id: i32) -> Option<blueoath_domain::CurrencyKind> {
    Some(match item_id {
        1 => blueoath_domain::CurrencyKind::Gold,
        2 => blueoath_domain::CurrencyKind::Diamond,
        5 => blueoath_domain::CurrencyKind::Supply,
        30 => blueoath_domain::CurrencyKind::PvePoint,
        _ => return None,
    })
}

fn typed_activity_can_consume(
    account: &blueoath_domain::AccountState,
    goods_type: i32,
    item_id: i32,
    amount: i32,
) -> bool {
    let Ok(amount) = u64::try_from(amount) else {
        return false;
    };
    if goods_type == 5 {
        return typed_activity_currency(item_id)
            .is_some_and(|kind| account.resources.amount(kind).get() >= amount);
    }
    if matches!(goods_type, 1 | 6) {
        return blueoath_domain::TemplateId::new(item_id.max(0) as u64)
            .ok()
            .is_some_and(|template_id| {
                account
                    .inventory
                    .items
                    .get(&template_id)
                    .copied()
                    .unwrap_or_default()
                    >= amount
            });
    }
    false
}

fn typed_activity_consume(
    account: &mut blueoath_domain::AccountState,
    goods_type: i32,
    item_id: i32,
    amount: i32,
) -> bool {
    let Ok(amount) = u64::try_from(amount) else {
        return false;
    };
    if goods_type == 5 {
        return typed_activity_currency(item_id)
            .is_some_and(|kind| account.resources.debit(kind, amount).is_ok());
    }
    if matches!(goods_type, 1 | 6) {
        let Some(template_id) = blueoath_domain::TemplateId::new(item_id.max(0) as u64).ok() else {
            return false;
        };
        let Some(current) = account.inventory.items.get_mut(&template_id) else {
            return false;
        };
        if *current < amount {
            return false;
        }
        *current -= amount;
        if *current == 0 {
            account.inventory.items.remove(&template_id);
        }
        return true;
    }
    false
}

fn handle_typed_paper_cut(
    account: &mut blueoath_domain::AccountState,
    request_args: &[u8],
) -> HandlerResult {
    let Ok(request) = PaperCutRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest("paper cut request is invalid"));
    };
    let materials = request.material_ids;
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    let Some(formula_config) = catalog.paper_cut_formulas.values().find(|config| {
        let mut configured = config.materials.clone();
        let mut requested = materials.clone();
        configured.sort_unstable();
        requested.sort_unstable();
        configured == requested
    }) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "paper cut formula is not configured",
        ));
    };
    let formula = formula_config.id;
    let Some(drop) = catalog
        .drop_items
        .get(&formula_config.drop_id)
        .and_then(|config| config.entries.first())
    else {
        return HandlerResult::Error(GameError::InvalidState(
            "paper cut reward is not configured",
        ));
    };
    let reward = ShopReward {
        goods_type: drop.goods_type,
        item_id: drop.item_id,
        num: drop.min,
        instance_id: 0,
    };
    let mut required = std::collections::BTreeMap::<blueoath_domain::TemplateId, u64>::new();
    for material in materials {
        let Ok(template_id) = blueoath_domain::TemplateId::new(material.max(0) as u64) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "paper cut material id is invalid",
            ));
        };
        let count = required.entry(template_id).or_default();
        *count = count.saturating_add(1);
    }
    if required.iter().any(|(template_id, amount)| {
        account
            .inventory
            .items
            .get(template_id)
            .copied()
            .unwrap_or_default()
            < *amount
    }) {
        return HandlerResult::Error(GameError::InvalidState(
            "paper cut materials are insufficient",
        ));
    }
    if !task_state::can_grant_typed_task_reward(account, &reward) {
        return HandlerResult::Error(GameError::InvalidState("paper cut reward is unsupported"));
    }
    let snapshot = account.clone();
    for (template_id, amount) in required {
        let Some(current) = account.inventory.items.get_mut(&template_id) else {
            *account = snapshot;
            return HandlerResult::Error(GameError::InvalidState(
                "paper cut materials are insufficient",
            ));
        };
        *current -= amount;
        if *current == 0 {
            account.inventory.items.remove(&template_id);
        }
    }
    if !task_state::grant_typed_task_reward(account, &reward) {
        *account = snapshot;
        return HandlerResult::Error(GameError::InvalidState("paper cut reward is unsupported"));
    }
    let key = format!("activity:activityPaperCut:formula:{formula}:count");
    let count = account.activities.progress.entry(key).or_default();
    *count = count.saturating_add(1);
    typed_reply(
        "activitypapercut.MakePaperCut",
        paper_cut_ret_payload(formula, &[reward]),
    )
}

fn handle_typed_video_set(
    account: &mut blueoath_domain::AccountState,
    request_args: &[u8],
) -> HandlerResult {
    let Ok(request) = ActivityItemIdRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "activity video request is invalid",
        ));
    };
    let video_id = request.item_id as u64;
    let watched_key = format!("activity:activityVideo:watched:{video_id}");
    if account.activities.progress.contains_key(&watched_key) {
        return typed_reply(
            "activityVideo.SetActivityVideo",
            video_watch_ret_payload(&[]),
        );
    }
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    let Some(video) = catalog.anniversary_videos.get(&(video_id as i32)) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "activity video is not configured",
        ));
    };
    let rewards = catalog
        .rewards_by_id
        .get(&video.reward_id)
        .cloned()
        .unwrap_or_default();
    if !task_state::can_grant_typed_task_rewards(account, &rewards) {
        return HandlerResult::Error(GameError::InvalidState(
            "activity video reward is unsupported",
        ));
    }
    for reward in &rewards {
        if !task_state::grant_typed_task_reward(account, reward) {
            return HandlerResult::Error(GameError::InvalidState(
                "activity video reward is unsupported",
            ));
        }
    }
    account.activities.progress.insert(watched_key, 1);
    typed_reply(
        "activityVideo.SetActivityVideo",
        video_watch_ret_payload(&rewards),
    )
}

fn activity_key(state: &str, field: &str) -> String {
    format!("activity:{state}:{field}")
}

fn typed_reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn activity_value(
    progress: &std::collections::BTreeMap<String, u64>,
    state: &str,
    field: &str,
) -> u64 {
    progress
        .get(&activity_key(state, field))
        .copied()
        .unwrap_or_default()
}

fn set_activity_value(
    progress: &mut std::collections::BTreeMap<String, u64>,
    state: &str,
    field: &str,
    value: u64,
) {
    progress.insert(activity_key(state, field), value);
}

fn increment_activity_value(
    progress: &mut std::collections::BTreeMap<String, u64>,
    state: &str,
    field: &str,
) {
    let key = activity_key(state, field);
    let value = progress.entry(key).or_default();
    *value = value.saturating_add(1);
}

fn typed_extract_payload(
    progress: &std::collections::BTreeMap<String, u64>,
    state: &str,
) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, activity_value(progress, state, "drawId"));
    append_varint_field(
        &mut output,
        2,
        activity_value(progress, state, "realDrawId"),
    );
    let mut sequences = std::collections::BTreeSet::new();
    let prefix = format!("activity:{state}:reward:");
    for key in progress.keys() {
        if let Some(sequence) = key
            .strip_prefix(&prefix)
            .and_then(|value| value.split_once(':'))
            .and_then(|(sequence, _)| sequence.parse::<u64>().ok())
        {
            sequences.insert(sequence);
        }
    }
    for sequence in sequences {
        let mut reward = Vec::new();
        append_varint_field(
            &mut reward,
            1,
            activity_value(progress, state, &format!("reward:{sequence}:id")),
        );
        append_varint_field(
            &mut reward,
            2,
            activity_value(progress, state, &format!("reward:{sequence}:num")),
        );
        append_message_field(&mut output, 3, &reward);
    }
    output
}

fn typed_ssr_payload(progress: &std::collections::BTreeMap<String, u64>, state: &str) -> Vec<u8> {
    let mut output = Vec::new();
    for (field, name) in [
        (1, "activityId"),
        (2, "daySelectCount"),
        (3, "dayShareCount"),
        (4, "selectShipId"),
        (5, "saveShipId"),
        (6, "rewardTime"),
    ] {
        append_varint_field(&mut output, field, activity_value(progress, state, name));
    }
    output
}

fn typed_rolls_payload(progress: &std::collections::BTreeMap<String, u64>, state: &str) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        activity_value(progress, state, "activityId"),
    );
    append_varint_field(
        &mut output,
        2,
        activity_value(progress, state, "daySelectCount"),
    );
    for (field, name) in [(3, "selectTeam"), (4, "saveTeam")] {
        let team_id = activity_value(progress, state, name);
        if team_id > 0 {
            let mut team = Vec::new();
            append_varint_field(&mut team, 1, team_id);
            append_message_field(&mut output, field, &team);
        }
    }
    append_varint_field(
        &mut output,
        5,
        activity_value(progress, state, "rewardTime"),
    );
    output
}

fn typed_birthday_payload(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        activity_value(progress, "activityBirthday", "birthdayAffair"),
    );
    let mut teams = std::collections::BTreeSet::new();
    for key in progress.keys() {
        if let Some(team_id) = key
            .strip_prefix("activity:activityBirthday:girl:")
            .and_then(|value| value.split_once(':'))
            .and_then(|(id, _)| id.parse::<u64>().ok())
        {
            teams.insert(team_id);
        }
    }
    for team_id in teams {
        let mut girl = Vec::new();
        append_varint_field(&mut girl, 1, 0);
        append_varint_field(
            &mut girl,
            2,
            activity_value(
                progress,
                "activityBirthday",
                &format!("girl:{team_id}:cake"),
            ),
        );
        append_varint_field(&mut girl, 3, team_id);
        append_message_field(&mut output, 2, &girl);
    }
    output
}

fn typed_video_payload(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut output = Vec::new();
    for key in progress.keys() {
        if let Some(id) = key
            .strip_prefix("activity:activityVideo:watched:")
            .and_then(|value| value.parse::<u64>().ok())
        {
            append_varint_field(&mut output, 1, id);
        }
    }
    output
}

fn typed_fashion_payload(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        activity_value(progress, "activityFashion", "activityId"),
    );
    append_varint_field(
        &mut output,
        2,
        activity_value(progress, "activityFashion", "buyCount"),
    );
    for key in progress.keys() {
        if let Some(index) = key
            .strip_prefix("activity:activityFashion:specialReward:")
            .and_then(|value| value.parse::<u64>().ok())
        {
            append_varint_field(&mut output, 3, index);
        }
    }
    output
}

fn typed_code_exchange_payload(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut output = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for key in progress.keys() {
        if let Some(id) = key
            .strip_prefix("activity:activityCodeExchange:receipt:")
            .and_then(|value| value.split_once(':'))
            .and_then(|(id, _)| id.parse::<u64>().ok())
        {
            ids.insert(id);
        }
    }
    for id in ids {
        let mut receipt = Vec::new();
        append_varint_field(&mut receipt, 1, id);
        append_varint_field(
            &mut receipt,
            2,
            activity_value(
                progress,
                "activityCodeExchange",
                &format!("receipt:{id}:count"),
            ),
        );
        append_message_field(&mut output, 1, &receipt);
    }
    output
}

fn typed_paper_cut_payload(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut output = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for key in progress.keys() {
        if let Some(id) = key
            .strip_prefix("activity:activityPaperCut:formula:")
            .and_then(|value| value.parse::<u64>().ok())
        {
            ids.insert(id);
        }
    }
    for id in ids {
        let mut formula = Vec::new();
        append_varint_field(&mut formula, 1, id);
        append_varint_field(
            &mut formula,
            2,
            activity_value(progress, "activityPaperCut", &format!("formula:{id}:count")),
        );
        append_message_field(&mut output, 1, &formula);
    }
    output
}

fn typed_secret_copy_payload(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        activity_value(progress, "activitySecretCopy", "passTimePerfect"),
    );
    let mut ids = std::collections::BTreeSet::new();
    for key in progress.keys() {
        if let Some(id) = key
            .strip_prefix("activity:activitySecretCopy:reward:")
            .and_then(|value| value.split_once(':'))
            .and_then(|(id, _)| id.parse::<u64>().ok())
        {
            ids.insert(id);
        }
    }
    for id in ids {
        let mut reward = Vec::new();
        append_varint_field(&mut reward, 1, id);
        append_varint_field(
            &mut reward,
            2,
            activity_value(
                progress,
                "activitySecretCopy",
                &format!("reward:{id}:getReward"),
            ),
        );
        append_message_field(&mut output, 2, &reward);
    }
    output
}

fn typed_valentine_payload(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut output = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for key in progress.keys() {
        if let Some(id) = key
            .strip_prefix("activity:activityValentine:loveShip:")
            .and_then(|value| value.split_once(':'))
            .and_then(|(id, _)| id.parse::<u64>().ok())
        {
            ids.insert(id);
        }
    }
    for id in ids {
        let prefix = format!("loveShip:{id}:");
        let mut hero = Vec::new();
        append_varint_field(&mut hero, 1, id);
        for (field, name) in [
            (2, "heroId"),
            (3, "templateId"),
            (4, "shipTid"),
            (5, "isGift"),
        ] {
            append_varint_field(
                &mut hero,
                field,
                activity_value(progress, "activityValentine", &format!("{prefix}{name}")),
            );
        }
        append_message_field(&mut output, 1, &hero);
    }
    append_varint_field(
        &mut output,
        2,
        activity_value(progress, "activityValentine", "curActShip"),
    );
    output
}

fn handle_typed_christmas_buy(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    const BLIND_BOX_COIN: i32 = 17_007;
    const BLIND_BOX_REPEAT_TOY: i32 = 17_008;
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    let snapshot = account.clone();

    if method == "activitychristmasshop.BuyBlindItem" {
        let Ok(request) = ChristmasBuyItemRequest::decode(request_args) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "christmas buy request is invalid",
            ));
        };
        let buy_way = request.buy_way;
        let buy_times = request.buy_times.clamp(1, 99);
        let (goods_type, item_id, unit_cost) = match buy_way {
            1 => (5, 1, parameter_value(catalog, 311).unwrap_or(5_000).max(1)),
            2 => (
                1,
                BLIND_BOX_REPEAT_TOY,
                parameter_value(catalog, 312).unwrap_or(10).max(1),
            ),
            _ => {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "christmas buy way is invalid",
                ));
            }
        };
        let Some(total_cost) = unit_cost.checked_mul(buy_times) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "christmas blind box exchange amount is invalid",
            ));
        };
        if !typed_activity_can_consume(account, goods_type, item_id, total_cost)
            || !typed_activity_consume(account, goods_type, item_id, total_cost)
        {
            return HandlerResult::Error(GameError::InvalidState(
                "christmas blind box exchange cost is insufficient",
            ));
        }
        let reward = ShopReward {
            goods_type: 1,
            item_id: BLIND_BOX_COIN,
            num: buy_times,
            instance_id: 0,
        };
        if !task_state::can_grant_typed_task_reward(account, &reward)
            || !task_state::grant_typed_task_reward(account, &reward)
        {
            *account = snapshot;
            return HandlerResult::Error(GameError::InvalidState(
                "christmas blind box coin reward is unsupported",
            ));
        }
        set_activity_value(
            &mut account.activities.progress,
            "activityChristmasShop",
            "lastBuyWay",
            buy_way as u64,
        );
        set_activity_value(
            &mut account.activities.progress,
            "activityChristmasShop",
            "lastBuyTimes",
            buy_times as u64,
        );
        return typed_reply(method, encode_rewards_list(&[reward]));
    }

    let Ok(request) = ChristmasBuyBlindBoxRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "christmas blind box request is invalid",
        ));
    };
    let buy_index = request.buy_index;
    let limit = parameter_value(catalog, 314).unwrap_or(8).max(1) as u64;
    let cost = parameter_value(catalog, 313).unwrap_or(10).max(1);
    let buy_key = format!("activity:activityChristmasShop:buy:{buy_index}:count");
    let current_count = account
        .activities
        .progress
        .get(&buy_key)
        .copied()
        .unwrap_or_default();
    if current_count >= limit {
        return HandlerResult::Error(GameError::InvalidState(
            "christmas blind box daily limit reached",
        ));
    }
    if !typed_activity_can_consume(account, 1, BLIND_BOX_COIN, cost)
        || !typed_activity_consume(account, 1, BLIND_BOX_COIN, cost)
    {
        return HandlerResult::Error(GameError::InvalidState(
            "christmas blind box coin is insufficient",
        ));
    }
    let eligible = typed_christmas_eligible_figures(catalog, account);
    if eligible.is_empty() {
        *account = snapshot;
        return HandlerResult::Error(GameError::InvalidState(
            "christmas blind box has no eligible figure",
        ));
    }
    let toy_id = eligible[(current_count as usize) % eligible.len()];
    let toy_key = format!("activity:activityChristmasShop:toy:{toy_id}:count");
    let duplicate = account
        .activities
        .progress
        .get(&toy_key)
        .copied()
        .unwrap_or_default()
        > 0;
    let mut rewards = Vec::new();
    if duplicate {
        rewards.push(ShopReward {
            goods_type: 1,
            item_id: BLIND_BOX_REPEAT_TOY,
            num: 1,
            instance_id: 0,
        });
        if !task_state::can_grant_typed_task_rewards(account, &rewards) {
            *account = snapshot;
            return HandlerResult::Error(GameError::InvalidState(
                "christmas duplicate toy reward is unsupported",
            ));
        }
        for reward in &rewards {
            if !task_state::grant_typed_task_reward(account, reward) {
                *account = snapshot;
                return HandlerResult::Error(GameError::InvalidState(
                    "christmas duplicate toy reward failed",
                ));
            }
        }
    } else {
        account.activities.progress.insert(toy_key, 1);
    }
    account
        .activities
        .progress
        .insert(buy_key, current_count.saturating_add(1));
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, toy_id as u64);
    typed_reply(method, output)
}

fn typed_christmas_eligible_figures(
    catalog: &GameplayCatalog,
    account: &blueoath_domain::AccountState,
) -> Vec<i32> {
    let owned_ship_fleets = account
        .dock
        .heroes
        .values()
        .map(|hero| hero.template_id.get() / 10)
        .collect::<std::collections::BTreeSet<_>>();
    let mut figures = catalog
        .interaction_figures
        .iter()
        .filter_map(|(id, figure)| {
            if figure.is_drawable <= 0 || figure.figure_type != 1 {
                return None;
            }
            let required = figure.original_ship_required > 0;
            (!required || owned_ship_fleets.contains(&(*id as u64))).then_some(*id)
        })
        .collect::<Vec<_>>();
    figures.sort_unstable();
    figures
}

fn typed_activity_fashion_config<'a>(
    catalog: &'a GameplayCatalog,
    progress: &std::collections::BTreeMap<String, u64>,
) -> Option<&'a ActivityConfig> {
    let activity_id = activity_value(progress, "activityFashion", "activityId");
    if activity_id > 0 {
        if let Some(config) = catalog.activity.get(&(activity_id as i32)) {
            if config.activity_type == 41 {
                return Some(config);
            }
        }
    }
    catalog
        .activity
        .values()
        .filter(|config| config.activity_type == 41 && config.is_open > 0 && config.p14.is_some())
        .max_by_key(|config| config.id)
}

fn typed_fashion_owned(account: &blueoath_domain::AccountState, fashion_id: i32) -> bool {
    account
        .fashion
        .entries
        .values()
        .any(|items| items.iter().any(|item| item.get() == fashion_id as u64))
}

fn can_grant_typed_fashion_reward(
    account: &blueoath_domain::AccountState,
    reward: &ShopReward,
) -> bool {
    if reward.goods_type == 18 {
        return reward.item_id > 0;
    }
    task_state::can_grant_typed_task_reward(account, reward)
}

fn grant_typed_fashion_reward(
    account: &mut blueoath_domain::AccountState,
    fashion_catalog: Option<&FashionList>,
    reward: &ShopReward,
) -> bool {
    if reward.goods_type != 18 {
        return task_state::grant_typed_task_reward(account, reward);
    }
    let Ok(fashion_tid) = blueoath_domain::TemplateId::new(reward.item_id.max(0) as u64) else {
        return false;
    };
    let sf_id = fashion_catalog
        .and_then(|catalog| {
            catalog
                .items
                .iter()
                .find(|item| item.fashion_tids.contains(&reward.item_id))
                .map(|item| item.sf_id)
        })
        .filter(|id| *id > 0)
        .unwrap_or(reward.item_id) as u64;
    account
        .fashion
        .entries
        .entry(sf_id)
        .or_default()
        .insert(fashion_tid);
    true
}

fn handle_typed_fashion(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    fashion_catalog: Option<&FashionList>,
) -> HandlerResult {
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    let snapshot = account.clone();
    let config = typed_activity_fashion_config(catalog, &account.activities.progress);
    let Some(config) = config else {
        return HandlerResult::Error(GameError::CatalogUnavailable);
    };
    if method == "activityfashion.Reward" {
        let Ok(request) = ActivityRewardIndexRequest::decode(request_args) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "activity fashion reward request is invalid",
            ));
        };
        let index = request.index;
        let (threshold, drop_id) = match index {
            1 => activity_fashion_milestone(config, "p2"),
            2 => activity_fashion_milestone(config, "p3"),
            _ => None,
        }
        .unwrap_or_default();
        if threshold <= 0 || drop_id <= 0 {
            return HandlerResult::Error(GameError::InvalidRequest(
                "activity fashion reward index is invalid",
            ));
        }
        let claim_key = format!("activity:activityFashion:specialReward:{index}");
        if account.activities.progress.contains_key(&claim_key) {
            return typed_reply(method, encode_rewards_list(&[]));
        }
        if activity_value(&account.activities.progress, "activityFashion", "buyCount")
            < threshold as u64
        {
            return HandlerResult::Error(GameError::InvalidState(
                "activity fashion milestone is not reached",
            ));
        }
        let Some(reward) = activity_fashion_drop_reward(catalog, drop_id, i64::from(index)) else {
            return HandlerResult::Error(GameError::InvalidState(
                "activity fashion milestone reward is invalid",
            ));
        };
        if !can_grant_typed_fashion_reward(account, &reward)
            || !grant_typed_fashion_reward(account, fashion_catalog, &reward)
        {
            *account = snapshot;
            return HandlerResult::Error(GameError::InvalidState(
                "activity fashion milestone reward is unsupported",
            ));
        }
        account.activities.progress.insert(claim_key, 1);
        return typed_reply(method, encode_rewards_list(&[reward]));
    }

    let current = activity_value(&account.activities.progress, "activityFashion", "buyCount");
    let max_count = config.p6.first().copied().unwrap_or_default() as i64;
    let Ok(request) = FashionPurchaseRequest::decode(request_args) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "activity fashion purchase request is invalid",
        ));
    };
    let requested = request.requested.clamp(1, 99);
    if max_count <= current as i64 || i64::from(requested) > max_count - current as i64 {
        return HandlerResult::Error(GameError::InvalidState(
            "activity fashion purchase limit reached",
        ));
    }
    let gid = request.group_id.max(0);
    let pools = &config.p1;
    let Some((goods_type, item_id, unit_cost)) = activity_fashion_cost(config) else {
        return HandlerResult::Error(GameError::InvalidState(
            "activity fashion purchase cost is invalid",
        ));
    };
    if pools.len() < 3 || (gid > 0 && !pools.contains(&gid) && gid != item_id) {
        return HandlerResult::Error(GameError::InvalidRequest(
            "activity fashion reward pool is invalid",
        ));
    }
    let unowned_fashion = config
        .p5
        .iter()
        .map(|(id, _)| *id)
        .find(|id| !typed_fashion_owned(account, *id));
    let mut target_unowned = unowned_fashion.is_some();
    let requested_pool = pools.iter().position(|pool| *pool == gid);
    let mut rewards = Vec::new();
    for offset in 0..requested {
        let draw_index = current
            .saturating_add(u64::try_from(offset).unwrap_or_default())
            .saturating_add(1);
        let drop_id = if draw_index >= max_count as u64 {
            pools[2]
        } else if let Some(pool) = requested_pool {
            pools[pool]
        } else if target_unowned {
            pools[0]
        } else {
            pools[1]
        };
        let Some(reward) = activity_fashion_drop_reward(catalog, drop_id, draw_index as i64) else {
            return HandlerResult::Error(GameError::InvalidState(
                "activity fashion reward configuration is invalid",
            ));
        };
        if unowned_fashion == Some(reward.item_id) && reward.goods_type == 18 {
            target_unowned = false;
        }
        rewards.push(reward);
    }
    let Some(total_cost) = unit_cost.checked_mul(requested) else {
        return HandlerResult::Error(GameError::InvalidRequest(
            "activity fashion purchase amount is invalid",
        ));
    };
    if !typed_activity_can_consume(account, goods_type, item_id, total_cost)
        || !typed_activity_consume(account, goods_type, item_id, total_cost)
    {
        return HandlerResult::Error(GameError::InvalidState(
            "activity fashion purchase cost is insufficient",
        ));
    }
    if rewards
        .iter()
        .any(|reward| !can_grant_typed_fashion_reward(account, reward))
    {
        *account = snapshot;
        return HandlerResult::Error(GameError::InvalidState(
            "activity fashion reward is unsupported",
        ));
    }
    for reward in &rewards {
        if !grant_typed_fashion_reward(account, fashion_catalog, reward) {
            *account = snapshot;
            return HandlerResult::Error(GameError::InvalidState("activity fashion reward failed"));
        }
    }
    set_activity_value(
        &mut account.activities.progress,
        "activityFashion",
        "buyCount",
        current.saturating_add(u64::try_from(requested).unwrap_or_default()),
    );
    set_activity_value(
        &mut account.activities.progress,
        "activityFashion",
        "lastGid",
        gid as u64,
    );
    typed_reply(method, encode_rewards_list(&rewards))
}

fn typed_christmas_payload(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buy_info = std::collections::BTreeMap::new();
    let mut toy_info = std::collections::BTreeMap::new();
    let mut boxes = std::collections::BTreeSet::new();
    for key in progress.keys() {
        if let Some(index) = key
            .strip_prefix("activity:activityChristmasShop:buy:")
            .and_then(|value| value.strip_suffix(":count"))
        {
            if let (Ok(index), Some(count)) = (index.parse::<u64>(), progress.get(key)) {
                buy_info.insert(index, *count);
            }
        }
        if let Some(toy_id) = key
            .strip_prefix("activity:activityChristmasShop:toy:")
            .and_then(|value| value.strip_suffix(":count"))
        {
            if let (Ok(toy_id), Some(count)) = (toy_id.parse::<u64>(), progress.get(key)) {
                toy_info.insert(toy_id, *count);
            }
        }
        if let Some(id) = key
            .strip_prefix("activity:activityChristmasShop:specialBox:")
            .and_then(|value| value.parse::<u64>().ok())
        {
            boxes.insert(id);
        }
    }
    for (index, count) in buy_info {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, index);
        append_varint_field(&mut item, 2, count);
        append_message_field(&mut output, 1, &item);
    }
    for (toy_id, count) in toy_info {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, toy_id);
        append_varint_field(&mut item, 2, count);
        append_message_field(&mut output, 2, &item);
    }
    for id in boxes {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, id);
        append_varint_field(&mut item, 2, 1);
        append_message_field(&mut output, 5, &item);
    }
    append_varint_field(
        &mut output,
        3,
        activity_value(progress, "activityChristmasShop", "crystalBallToyId"),
    );
    append_varint_field(
        &mut output,
        4,
        activity_value(progress, "activityChristmasShop", "isGiveCrystalBall"),
    );
    output
}

fn code_exchange_activity(catalog: &GameplayCatalog) -> Option<&ActivityConfig> {
    catalog.activity.get(&81005).or_else(|| {
        catalog
            .activity
            .values()
            .find(|value| value.activity_type == 81005)
    })
}

fn extract_ur_draw_ret_payload(reward_ids: &[i32]) -> Vec<u8> {
    let mut output = Vec::new();
    for reward_id in reward_ids {
        append_varint_field(&mut output, 1, (*reward_id).max(0) as u64);
    }
    output
}

fn extract_draw_ret_payload(rewards: &[ShopReward]) -> Vec<u8> {
    let mut output = Vec::new();
    for reward in rewards {
        append_message_field(
            &mut output,
            1,
            &common_reward_payload(reward.goods_type, reward.item_id, reward.num),
        );
    }
    output
}

fn birthday_formula_reward(catalog: &GameplayCatalog, formula: i32) -> Option<ShopReward> {
    let row = birthday_activity_config(catalog)?;
    let formula = usize::try_from(formula.checked_sub(1)?).ok()?;
    let reward = row.p4.get(formula)?;
    Some(ShopReward {
        goods_type: 1,
        item_id: *reward.get(2)?,
        num: 1,
        instance_id: 0,
    })
}

fn birthday_affair_reward_id(catalog: &GameplayCatalog, level: i32) -> Option<i32> {
    let row = birthday_activity_config(catalog)?;
    row.p5
        .iter()
        .find(|(entry_level, _)| *entry_level == level)
        .map(|(_, reward_id)| *reward_id)
}

fn paper_cut_ret_payload(formula: i32, rewards: &[ShopReward]) -> Vec<u8> {
    let mut output = Vec::new();
    for reward in rewards {
        append_message_field(
            &mut output,
            1,
            &common_reward_payload(reward.goods_type, reward.item_id, reward.num),
        );
    }
    append_varint_field(&mut output, 2, formula.max(0) as u64);
    output
}

fn video_watch_ret_payload(rewards: &[ShopReward]) -> Vec<u8> {
    let mut output = Vec::new();
    for reward in rewards {
        append_message_field(
            &mut output,
            1,
            &common_reward_payload(reward.goods_type, reward.item_id, reward.num),
        );
    }
    output
}

fn parameter_value(catalog: &GameplayCatalog, id: i32) -> Option<i32> {
    catalog.parameters.get(&id).map(|config| config.value)
}

fn activity_fashion_milestone(config: &ActivityConfig, key: &str) -> Option<(i32, i32)> {
    match key {
        "p2" => config.p2,
        "p3" => config.p3,
        _ => None,
    }
}

fn activity_fashion_drop_reward(
    catalog: &GameplayCatalog,
    drop_id: i32,
    sequence: i64,
) -> Option<ShopReward> {
    fn resolve(
        catalog: &GameplayCatalog,
        drop_id: i32,
        sequence: i64,
        depth: usize,
    ) -> Option<ShopReward> {
        if depth > 8 {
            return None;
        }
        let entries = catalog.drop_items.get(&drop_id)?.entries.clone();
        let total = entries.iter().map(|entry| entry.rate).sum::<i64>();
        if total <= 0 {
            return None;
        }
        let mut cursor = sequence.rem_euclid(total);
        for entry in entries {
            if cursor < entry.rate {
                if entry.goods_type == 4 {
                    return resolve(catalog, entry.item_id, sequence, depth + 1);
                }
                let span = i64::from(entry.max.saturating_sub(entry.min)).saturating_add(1);
                let num = entry.min.saturating_add((sequence.rem_euclid(span)) as i32);
                return Some(ShopReward {
                    goods_type: entry.goods_type,
                    item_id: entry.item_id,
                    num,
                    instance_id: 0,
                });
            }
            cursor -= entry.rate;
        }
        None
    }

    resolve(catalog, drop_id, sequence.max(0), 0)
}

fn activity_fashion_cost(config: &ActivityConfig) -> Option<(i32, i32, i32)> {
    let (goods_type, item_id, amount) = config.p14?;
    (goods_type > 0 && item_id > 0 && amount > 0).then_some((goods_type, item_id, amount))
}

fn common_reward_payload(goods_type: i32, config_id: i32, num: i32) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, goods_type.max(0) as u64);
    append_varint_field(&mut output, 2, config_id.max(0) as u64);
    append_varint_field(&mut output, 3, num.max(0) as u64);
    output
}

fn birthday_activity_config(catalog: &GameplayCatalog) -> Option<&ActivityConfig> {
    catalog.activity.get(&103).or_else(|| {
        catalog
            .activity
            .values()
            .find(|activity| activity.activity_type == 103)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_activity_state_uses_progress_map() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("activity-typed").unwrap(),
            "Captain",
        );
        let mut select = Vec::new();
        append_varint_field(&mut select, 1, 42);
        assert!(matches!(
            handle_typed(&mut account, "activitySSR.ActivitySSRSelect", &select, None),
            HandlerResult::Reply(_)
        ));
        assert!(matches!(
            handle_typed(&mut account, "activitySSR.ActivitySSRRand", &[], None),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account
                .activities
                .progress
                .get("activity:activitySSR:saveShipId"),
            Some(&42)
        );
        assert!(matches!(
            handle_typed(&mut account, "activityextract.SwitchDraw", &[], None),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account
                .activities
                .progress
                .get("activity:activityExtract:realDrawId"),
            Some(&1)
        );
        let mut feed = Vec::new();
        append_varint_field(&mut feed, 1, 8);
        append_varint_field(&mut feed, 2, 3);
        assert!(matches!(
            handle_typed(
                &mut account,
                "activitybirthday.FeedBirthdayCake",
                &feed,
                None
            ),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account
                .activities
                .progress
                .get("activity:activityBirthday:girl:8:cake"),
            Some(&3)
        );
        let mut claim = Vec::new();
        append_varint_field(&mut claim, 1, 2);
        assert!(matches!(
            handle_typed(&mut account, "activitysecretcopy.GetReward", &claim, None),
            HandlerResult::Reply(_)
        ));
        assert!(account
            .activities
            .progress
            .contains_key("activity:activitySecretCopy:reward:2:getReward"));
        let mut exchange = Vec::new();
        append_varint_field(&mut exchange, 1, 55);
        append_varint_field(&mut exchange, 3, 2);
        assert!(matches!(
            handle_typed(
                &mut account,
                "activitycodeexchange.ExchangeCode",
                &exchange,
                None
            ),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account
                .activities
                .progress
                .get("activity:activityCodeExchange:receipt:55:count"),
            Some(&2)
        );
        let mut cake = Vec::new();
        append_varint_field(&mut cake, 1, 1);
        assert!(matches!(
            handle_typed(
                &mut account,
                "activitybirthday.MakeBirthdayCake",
                &cake,
                None
            ),
            HandlerResult::Error(_)
        ));
        assert!(matches!(
            handle_typed(
                &mut account,
                "activityvalentineloveletter.GetReward",
                &[],
                None,
            ),
            HandlerResult::Error(_)
        ));
    }
}
