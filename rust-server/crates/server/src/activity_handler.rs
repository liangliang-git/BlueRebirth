use serde_json::{json, Value};

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(super) fn handles(method: &str) -> bool {
    GameMethod::parse(method).is_family(MethodFamily::Activity)
}

pub(super) fn handles_typed(method: &str) -> bool {
    matches!(
        method,
        "activityextract.Get"
            | "activityextract.Update"
            | "activityextract.SwitchDraw"
            | "activityextractur.Get"
            | "activityextractur.Update"
            | "activityextractur.SwitchDraw"
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
            | "activityVideo.GetActivityVideo"
            | "activitychristmasshop.UpdateActivityChristmasShopInfo"
    )
}

pub(super) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
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
                    set_activity_value(
                        progress,
                        state,
                        "selectShipId",
                        decode_varint_field(request_args, 1).max(0) as u64,
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
                    let team_id = decode_varint_field(request_args, 1).max(0);
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
        "activityVideo.GetActivityVideo" => typed_reply(method, typed_video_payload(progress)),
        "activitychristmasshop.UpdateActivityChristmasShopInfo" => {
            typed_reply(method, typed_christmas_payload(progress))
        }
        _ => HandlerResult::Error(GameError::InvalidRequest(
            "activity method requires typed activity rule",
        )),
    }
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

fn typed_christmas_payload(progress: &std::collections::BTreeMap<String, u64>) -> Vec<u8> {
    let mut output = Vec::new();
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

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let payload = handle_legacy(context, method, request_args);
    if let Some(error) = context.handler_error.clone() {
        HandlerResult::Error(error)
    } else {
        match payload {
            Some(payload) => HandlerResult::Reply(Response::raw(method, payload)),
            None => HandlerResult::Empty,
        }
    }
}

fn handle_legacy<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let catalog = GAMEPLAY_CATALOG.get_or_init(GameplayCatalog::default);
    let handler_error = &mut *context.handler_error;
    let account = context.account.as_deref_mut()?;
    ensure_activity_fashion_state(catalog, account);
    match method {
        "activityextract.Get" | "activityextract.Update" => Some(extract_info_payload(
            activity_state_mut(account, "activityExtract"),
        )),
        "activityextract.Draw" => {
            let draw_id = decode_varint_field(request_args, 1);
            let num = decode_varint_field(request_args, 2).clamp(1, 10);
            handle_extract_draw(
                catalog,
                account,
                false,
                draw_id,
                num,
                context.state,
                context.pre_pushes,
                context.catalogs.fashion,
                handler_error,
            )
        }
        "activityextract.SwitchDraw" => {
            let state = activity_state_mut(account, "activityExtract");
            let next = json_i64(state, "realDrawId")
                .unwrap_or_default()
                .saturating_add(1);
            state["realDrawId"] = json!(next);
            Some(extract_info_payload(state))
        }
        "activityextractur.Get" | "activityextractur.Update" => Some(extract_info_payload(
            activity_state_mut(account, "activityExtractUr"),
        )),
        "activityextractur.Draw" => {
            let draw_id = decode_varint_field(request_args, 1);
            let num = decode_varint_field(request_args, 2).clamp(1, 10);
            handle_extract_draw(
                catalog,
                account,
                true,
                draw_id,
                num,
                context.state,
                context.pre_pushes,
                context.catalogs.fashion,
                handler_error,
            )
        }
        "activityextractur.SwitchDraw" => {
            let state = activity_state_mut(account, "activityExtractUr");
            let next = json_i64(state, "realDrawId")
                .unwrap_or_default()
                .saturating_add(1);
            state["realDrawId"] = json!(next);
            Some(extract_info_payload(state))
        }
        "activityfashion.PushActivityFashionInfo" => Some(fashion_info_payload(
            activity_state_mut(account, "activityFashion"),
        )),
        "activityfashion.Buy" => handle_activity_fashion_buy(
            catalog,
            account,
            context.state,
            context.pre_pushes,
            context.catalogs.fashion,
            request_args,
            handler_error,
        ),
        "activityfashion.Reward" => handle_activity_fashion_reward(
            catalog,
            account,
            context.state,
            context.pre_pushes,
            context.catalogs.fashion,
            request_args,
            handler_error,
        ),
        "activitySSR.GetActivitySSRInfo" => {
            Some(ssr_info_payload(activity_state_mut(account, "activitySSR")))
        }
        "activitySSR.ActivitySSRSelect" => {
            let ship_id = decode_varint_field(request_args, 1).max(0);
            let state = activity_state_mut(account, "activitySSR");
            state["selectShipId"] = json!(ship_id);
            Some(ssr_info_payload(state))
        }
        "activitySSR.ActivitySSRRand" => {
            let state = activity_state_mut(account, "activitySSR");
            let selected = json_i64(state, "selectShipId").unwrap_or(1).max(1);
            state["saveShipId"] = json!(selected);
            state["daySelectCount"] = json!(json_i64(state, "daySelectCount")
                .unwrap_or_default()
                .saturating_add(1));
            Some(ssr_info_payload(state))
        }
        "activitySSR.ActivitySSRShare" => {
            let state = activity_state_mut(account, "activitySSR");
            state["dayShareCount"] = json!(json_i64(state, "dayShareCount")
                .unwrap_or_default()
                .saturating_add(1));
            Some(ssr_info_payload(state))
        }
        "activitySSRrolls.UpdateActivityRollsInfo"
        | "activitySSRrolls.UpdateActivityRollsInfoRPC" => Some(rolls_info_payload(
            activity_state_mut(account, "activitySSRRolls"),
        )),
        "activitySSRrolls.ActivityRollsSelect" => {
            let team_id = decode_varint_field(request_args, 1).max(0);
            let state = activity_state_mut(account, "activitySSRRolls");
            state["selectTeamId"] = json!(team_id);
            state["selectShipTeam"] = json!([team_id]);
            state["daySelectCount"] = json!(json_i64(state, "daySelectCount")
                .unwrap_or_default()
                .saturating_add(1));
            Some(rolls_info_payload(state))
        }
        "activitySSRrolls.ActivityRollsRand" => {
            let state = activity_state_mut(account, "activitySSRRolls");
            let team_id = json_i64(state, "selectTeamId").unwrap_or(1).max(1);
            state["saveShipTeam"] = json!([team_id]);
            Some(rolls_info_payload(state))
        }
        "activitybirthday.BirthdayRefresh" | "activitybirthday.UpdateBirthdayInfo" => Some(
            birthday_info_payload(activity_state_mut(account, "activityBirthday")),
        ),
        "activitybirthday.MakeBirthdayCake" => {
            let formula = decode_varint_field(request_args, 1).max(0);
            let Some(reward) = birthday_formula_reward(catalog, formula) else {
                *handler_error = Some(GameError::Internal(
                    "birthday cake formula is not configured".to_owned(),
                ));
                return Some(Vec::new());
            };
            {
                let state = activity_state_mut(account, "activityBirthday");
                state["cake"] = json!(json_i64(state, "cake")
                    .unwrap_or_default()
                    .saturating_add(1));
                state["lastCakeFormula"] = json!(formula);
            }
            let _reward = grant_reward(
                account,
                reward,
                current_unix_seconds(),
                context.catalogs.fashion,
            );
            append_method_push(
                context.pre_pushes,
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_account(context.state, Some(account))),
            );
            append_method_push(
                context.pre_pushes,
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_account(account)),
            );
            Some(birthday_info_payload(activity_state_mut(
                account,
                "activityBirthday",
            )))
        }
        "activitybirthday.FeedBirthdayCake" => {
            let team_id = decode_varint_field(request_args, 1).max(0);
            let cake = decode_varint_field(request_args, 2).max(0);
            let state = activity_state_mut(account, "activityBirthday");
            if team_id <= 0 || cake <= 0 {
                *handler_error = Some(GameError::Internal(
                    "birthday feed request is invalid".to_owned(),
                ));
                return Some(Vec::new());
            }
            state["lastFeedTeamId"] = json!(team_id);
            state["lastFeedCake"] = json!(cake);
            let girls = state
                .get_mut("girlsAndCake")
                .and_then(Value::as_array_mut)
                .expect("birthday girls array");
            if let Some(entry) = girls
                .iter_mut()
                .find(|entry| json_i64(entry, "teamId") == Some(i64::from(team_id)))
            {
                entry["cake"] = json!(cake);
            } else {
                girls.push(json!({"girl": 0, "cake": cake, "teamId": team_id}));
            }
            Some(birthday_info_payload(state))
        }
        "activitybirthday.GetCakeAffairReward" => {
            let level = decode_varint_field(request_args, 1).max(0);
            let Some(reward_id) = birthday_affair_reward_id(catalog, level) else {
                *handler_error = Some(GameError::Internal(
                    "birthday affair reward is not configured".to_owned(),
                ));
                return Some(Vec::new());
            };
            let already_claimed = account
                .get("activityBirthday")
                .and_then(|state| state.get("claimedAffairs"))
                .and_then(Value::as_array)
                .is_some_and(|values| {
                    values
                        .iter()
                        .any(|value| value.as_i64() == Some(i64::from(level)))
                });
            if already_claimed {
                return Some(birthday_info_payload(activity_state_mut(
                    account,
                    "activityBirthday",
                )));
            }
            let reward_defs = catalog
                .rewards_by_id
                .get(&reward_id)
                .cloned()
                .unwrap_or_default();
            if reward_defs.is_empty() {
                *handler_error = Some(GameError::Internal(
                    "birthday affair reward is missing".to_owned(),
                ));
                return Some(Vec::new());
            }
            for reward in reward_defs {
                grant_reward(
                    account,
                    reward,
                    current_unix_seconds(),
                    context.catalogs.fashion,
                );
            }
            add_unique_i64(
                activity_state_mut(account, "activityBirthday"),
                "claimedAffairs",
                i64::from(level),
            );
            append_method_push(
                context.pre_pushes,
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_account(context.state, Some(account))),
            );
            append_method_push(
                context.pre_pushes,
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_account(account)),
            );
            Some(birthday_info_payload(activity_state_mut(
                account,
                "activityBirthday",
            )))
        }
        "activitycodeexchange.UpdateActivityCodeExgInfo" => Some(code_exchange_payload(
            activity_state_mut(account, "activityCodeExchange"),
        )),
        "activitycodeexchange.ExchangeCode" => {
            let code = decode_varint_field(request_args, 1);
            let number = decode_varint_field(request_args, 3).clamp(1, 99);
            if code <= 0 {
                *handler_error = Some(GameError::Internal(
                    "activity exchange code is invalid".to_owned(),
                ));
                return Some(Vec::new());
            }
            let state = activity_state_mut(account, "activityCodeExchange");
            add_receipt(state, code, number);
            Some(code_exchange_payload(state))
        }
        "activitycodeexchange.ExchangeReward" => {
            let reward_index = decode_varint_field(request_args, 1).max(0);
            let number = decode_varint_field(request_args, 2).clamp(1, 99);
            let Some(activity) = code_exchange_activity(catalog) else {
                *handler_error = Some(GameError::Internal(
                    "activity exchange config is unavailable".to_owned(),
                ));
                return Some(Vec::new());
            };
            let reward_id = activity
                .get("p4")
                .and_then(Value::as_array)
                .and_then(|rewards| rewards.get(reward_index.saturating_sub(1) as usize))
                .and_then(Value::as_array)
                .and_then(|reward| reward.first())
                .and_then(Value::as_i64)
                .and_then(|id| i32::try_from(id).ok())
                .unwrap_or_default();
            let reward_defs = catalog
                .rewards_by_id
                .get(&reward_id)
                .cloned()
                .unwrap_or_default();
            if reward_index <= 0 || reward_id <= 0 || reward_defs.is_empty() {
                *handler_error = Some(GameError::Internal(
                    "activity exchange reward is invalid".to_owned(),
                ));
                return Some(Vec::new());
            }
            let state = activity_state_mut(account, "activityCodeExchange");
            let available = state
                .get("receipts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|receipt| json_i64(receipt, "rewardId") == Some(i64::from(reward_index)))
                .and_then(|receipt| json_i64(receipt, "count"))
                .unwrap_or_default();
            if available < i64::from(number) {
                *handler_error = Some(GameError::Internal(
                    "activity exchange reward count is insufficient".to_owned(),
                ));
                return Some(Vec::new());
            }
            if let Some(receipt) = state
                .get_mut("receipts")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
                .find(|receipt| json_i64(receipt, "rewardId") == Some(i64::from(reward_index)))
            {
                receipt["count"] = json!(available.saturating_sub(i64::from(number)));
            }
            let rewards = (0..number)
                .flat_map(|_| reward_defs.iter().copied())
                .map(|reward| {
                    grant_reward(
                        account,
                        reward,
                        current_unix_seconds(),
                        context.catalogs.fashion,
                    )
                })
                .collect::<Vec<_>>();
            append_method_push(
                context.pre_pushes,
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_account(context.state, Some(account))),
            );
            append_method_push(
                context.pre_pushes,
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_account(account)),
            );
            Some(encode_rewards_list(&rewards))
        }
        "activitypapercut.UpdateActivityPaperCutInfo" => Some(paper_cut_info_payload(
            activity_state_mut(account, "activityPaperCut"),
        )),
        "activitypapercut.MakePaperCut" => {
            let materials = decode_repeated_varint_field(request_args, 1);
            if materials.is_empty() {
                *handler_error = Some(GameError::Internal(
                    "paper cut materials are empty".to_owned(),
                ));
                return Some(Vec::new());
            }
            let formula_config = catalog.paper_cut_formulas.values().find(|config| {
                config
                    .get("formula")
                    .and_then(Value::as_array)
                    .is_some_and(|values| {
                        if values.len() != materials.len() {
                            return false;
                        }
                        let mut configured = values
                            .iter()
                            .filter_map(Value::as_i64)
                            .filter_map(|value| i32::try_from(value).ok())
                            .collect::<Vec<_>>();
                        if configured.len() != materials.len() {
                            return false;
                        }
                        configured.sort_unstable();
                        let mut requested = materials.to_vec();
                        requested.sort_unstable();
                        configured == requested
                    })
            });
            let Some(formula_config) = formula_config else {
                *handler_error = Some(GameError::Internal(
                    "paper cut formula is not configured".to_owned(),
                ));
                return Some(Vec::new());
            };
            let formula = json_i32(formula_config, "id").unwrap_or_default();
            let drop_id = json_i32(formula_config, "drop_id").unwrap_or_default();
            let drop = catalog
                .drop_items
                .get(&drop_id)
                .and_then(|config| config.get("drop"))
                .and_then(Value::as_array)
                .and_then(|values| values.first())
                .and_then(Value::as_array);
            let Some(drop) = drop else {
                *handler_error = Some(GameError::Internal(
                    "paper cut reward is not configured".to_owned(),
                ));
                return Some(Vec::new());
            };
            let goods_type =
                i32::try_from(drop.first().and_then(Value::as_i64).unwrap_or_default())
                    .unwrap_or_default();
            let item_id = i32::try_from(drop.get(1).and_then(Value::as_i64).unwrap_or_default())
                .unwrap_or_default();
            let amount = i32::try_from(drop.get(2).and_then(Value::as_i64).unwrap_or_default())
                .unwrap_or_default();
            let Some(account) = context.account.as_deref_mut() else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                return Some(Vec::new());
            };
            if materials
                .iter()
                .any(|item_id| !extract_can_consume(account, 1, *item_id, 1))
            {
                *handler_error = Some(GameError::Internal(
                    "paper cut materials are insufficient".to_owned(),
                ));
                return Some(Vec::new());
            }
            for item_id in materials {
                extract_consume(account, 1, item_id, 1);
            }
            let state = activity_state_mut(account, "activityPaperCut");
            increment_formula_count(state, formula);
            let rewards = if goods_type > 0 && item_id > 0 && amount > 0 {
                vec![grant_reward(
                    account,
                    ShopReward {
                        goods_type,
                        item_id,
                        num: amount,
                        instance_id: 0,
                    },
                    current_unix_seconds(),
                    context.catalogs.fashion,
                )]
            } else {
                Vec::new()
            };
            Some(paper_cut_ret_payload(formula, &rewards))
        }
        "activitysecretcopy.UpdateActivitySecretCopyInfo" => Some(secret_copy_info_payload(
            activity_state_mut(account, "activitySecretCopy"),
        )),
        "activitysecretcopy.GetReward" => {
            let rate_index = decode_varint_field(request_args, 1);
            if rate_index <= 0 {
                *handler_error = Some(GameError::Internal(
                    "secret copy reward index is invalid".to_owned(),
                ));
                return Some(Vec::new());
            }
            let state = activity_state_mut(account, "activitySecretCopy");
            let already_claimed = state
                .get("rewards")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|item| json_i64(item, "rateIndex") == Some(i64::from(rate_index)));
            if already_claimed {
                *handler_error = Some(GameError::Internal(
                    "secret copy reward was already claimed".to_owned(),
                ));
            } else {
                state["rewards"]
                    .as_array_mut()
                    .expect("rewards array")
                    .push(json!({
                        "rateIndex": rate_index,
                        "getReward": 1
                    }));
            }
            Some(secret_copy_info_payload(state))
        }
        "activityvalentineloveletter.GetReward"
        | "activityvalentineloveletter.GetRewardBySecretary" => {
            let index = decode_varint_field(request_args, 1).max(0);
            if method.ends_with("GetReward") && index <= 0 {
                *handler_error = Some(GameError::Internal(
                    "valentine reward index is invalid".to_owned(),
                ));
                return Some(Vec::new());
            }
            let key = if method.ends_with("BySecretary") {
                "secretaryClaims"
            } else {
                "claims"
            };
            let state_snapshot = account
                .get("activityValentine")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let reward_id = valentine_reward_id(catalog, &state_snapshot, method, index);
            let already_claimed = state_snapshot
                .get(key)
                .and_then(Value::as_array)
                .is_some_and(|values| {
                    values
                        .iter()
                        .any(|value| value.as_i64() == Some(i64::from(index)))
                });
            if !already_claimed && reward_id.is_none() {
                *handler_error = Some(GameError::Internal(
                    "valentine reward is not configured".to_owned(),
                ));
                return Some(Vec::new());
            }
            let rewards = if !already_claimed {
                reward_id
                    .and_then(|id| catalog.rewards_by_id.get(&id))
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|reward| {
                        grant_reward(
                            account,
                            reward,
                            current_unix_seconds(),
                            context.catalogs.fashion,
                        )
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            add_unique_i64(
                activity_state_mut(account, "activityValentine"),
                key,
                i64::from(index),
            );
            if !rewards.is_empty() {
                append_method_push(
                    context.pre_pushes,
                    "user.UpdateUserInfo",
                    UserInfoCodec::encode(&user_info_from_account(context.state, Some(account))),
                );
                append_method_push(
                    context.pre_pushes,
                    "bag.UpdateBagData",
                    BagInfoCodec::encode(&bag_info_from_account(account)),
                );
            }
            let state = activity_state_mut(account, "activityValentine");
            Some(valentine_info_payload(state))
        }
        "activityvalentineloveletter.UpdateActivityValentineLoveLetterInfo" => Some(
            valentine_info_payload(activity_state_mut(account, "activityValentine")),
        ),
        "activityVideo.GetActivityVideo" => Some(video_info_payload(activity_state_mut(
            account,
            "activityVideo",
        ))),
        "activityVideo.SetActivityVideo" => {
            let id = decode_varint_field(request_args, 1).max(0);
            let state = activity_state_mut(account, "activityVideo");
            let already_watched =
                state
                    .get("watched")
                    .and_then(Value::as_array)
                    .is_some_and(|values| {
                        values
                            .iter()
                            .any(|value| value.as_i64() == Some(i64::from(id)))
                    });
            add_unique_i64(state, "watched", i64::from(id));
            let rewards = if !already_watched {
                catalog
                    .anniversary_videos
                    .get(&id)
                    .and_then(|video| json_i32(video, "reward"))
                    .map(|reward_id| {
                        catalog
                            .rewards_by_id
                            .get(&reward_id)
                            .cloned()
                            .unwrap_or_default()
                            .into_iter()
                            .map(|reward| {
                                grant_reward(
                                    account,
                                    reward,
                                    current_unix_seconds(),
                                    context.catalogs.fashion,
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            if !rewards.is_empty() {
                let user_payload =
                    UserInfoCodec::encode(&user_info_from_account(context.state, Some(account)));
                let bag_payload = BagInfoCodec::encode(&bag_info_from_account(account));
                append_method_push(context.pre_pushes, "user.UpdateUserInfo", user_payload);
                append_method_push(context.pre_pushes, "bag.UpdateBagData", bag_payload);
            }
            Some(video_watch_ret_payload(&rewards))
        }
        "activitychristmasshop.UpdateActivityChristmasShopInfo" => Some(christmas_info_payload(
            catalog,
            activity_state_mut(account, "activityChristmasShop"),
        )),
        "activitychristmasshop.BuyBlindBox" => handle_christmas_buy_box(
            catalog,
            account,
            context.state,
            context.pre_pushes,
            request_args,
            handler_error,
        ),
        "activitychristmasshop.OpenSpecialBlindBox" => {
            let item_id = decode_varint_field(request_args, 1).max(0);
            let state = activity_state_mut(account, "activityChristmasShop");
            add_unique_i64(state, "specialBoxes", i64::from(item_id));
            let mut output = Vec::new();
            append_varint_field(&mut output, 1, item_id as u64);
            Some(output)
        }
        "activitychristmasshop.BuyBlindItem" => handle_christmas_buy_item(
            catalog,
            account,
            context.state,
            context.pre_pushes,
            request_args,
            handler_error,
        ),
        "activitychristmasshop.SetToy" => {
            let toy_id = decode_varint_field(request_args, 1).max(0);
            let state = activity_state_mut(account, "activityChristmasShop");
            state["crystalBallToyId"] = json!(toy_id);
            Some(christmas_info_payload(catalog, state))
        }
        "activitychristmasshop.GiveMeCrystalBall" => {
            let state = activity_state_mut(account, "activityChristmasShop");
            state["isGiveCrystalBall"] = json!(1);
            Some(christmas_info_payload(catalog, state))
        }
        _ if handles(method) => {
            let state = activity_state_mut(account, "activityCompat");
            state[method] = json!(request_args);
            Some(Vec::new())
        }
        _ => None,
    }
}

fn ensure_activity_fashion_state(catalog: &GameplayCatalog, account: &mut Value) {
    let current = account
        .get("activityFashion")
        .and_then(|state| state.get("activityId"))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    if current > 0 {
        return;
    }
    let activity_id = catalog
        .activity
        .iter()
        .filter(|(_, value)| {
            json_i32(value, "type") == Some(41)
                && json_i32(value, "is_open").unwrap_or_default() > 0
                && value
                    .get("p14")
                    .and_then(Value::as_array)
                    .is_some_and(|rows| !rows.is_empty())
        })
        .map(|(id, _)| *id)
        .max()
        .unwrap_or(0);
    if activity_id > 0 {
        activity_state_mut(account, "activityFashion")["activityId"] = json!(activity_id);
    }
}

fn activity_fashion_config(catalog: &GameplayCatalog, account: &Value) -> Option<Value> {
    let activity_id = account
        .get("activityFashion")
        .and_then(|state| state.get("activityId"))
        .and_then(Value::as_i64)
        .and_then(|id| i32::try_from(id).ok())
        .filter(|id| *id > 0);
    activity_id
        .and_then(|id| catalog.activity.get(&id))
        .filter(|value| json_i32(value, "type") == Some(41))
        .cloned()
        .or_else(|| {
            catalog
                .activity
                .values()
                .filter(|value| {
                    json_i32(value, "type") == Some(41)
                        && json_i32(value, "is_open").unwrap_or_default() > 0
                })
                .max_by_key(|value| json_i32(value, "id").unwrap_or_default())
                .cloned()
        })
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
        let rows = catalog
            .drop_items
            .get(&drop_id)?
            .get("drop")
            .and_then(Value::as_array)?;
        let entries = rows
            .iter()
            .filter_map(|row| {
                let row = row.as_array()?;
                let goods_type = i32::try_from(row.first()?.as_i64()?).ok()?;
                let item_id = i32::try_from(row.get(1)?.as_i64()?).ok()?;
                let min = i32::try_from(row.get(2)?.as_i64()?).ok()?;
                let max = i32::try_from(row.get(3)?.as_i64()?).ok()?;
                let rate = row.get(4)?.as_i64()?;
                (goods_type > 0 && item_id > 0 && min > 0 && max >= min && rate > 0)
                    .then_some((goods_type, item_id, min, max, rate))
            })
            .collect::<Vec<_>>();
        let total = entries.iter().map(|entry| entry.4).sum::<i64>();
        if total <= 0 {
            return None;
        }
        let mut cursor = sequence.rem_euclid(total);
        for (goods_type, item_id, min, max, rate) in entries {
            if cursor < rate {
                if goods_type == 4 {
                    return resolve(catalog, item_id, sequence, depth + 1);
                }
                let span = i64::from(max.saturating_sub(min)).saturating_add(1);
                let num = min.saturating_add((sequence.rem_euclid(span)) as i32);
                return Some(ShopReward {
                    goods_type,
                    item_id,
                    num,
                    instance_id: 0,
                });
            }
            cursor -= rate;
        }
        None
    }

    resolve(catalog, drop_id, sequence.max(0), 0)
}

fn activity_fashion_cost(config: &Value) -> Option<(i32, i32, i32)> {
    let row = config
        .get("p14")
        .and_then(Value::as_array)?
        .first()?
        .as_array()?;
    let goods_type = i32::try_from(row.first()?.as_i64()?).ok()?;
    let item_id = i32::try_from(row.get(1)?.as_i64()?).ok()?;
    let amount = i32::try_from(row.get(2)?.as_i64()?).ok()?;
    (goods_type > 0 && item_id > 0 && amount > 0).then_some((goods_type, item_id, amount))
}

fn activity_fashion_owned(account: &Value, fashion_id: i32) -> bool {
    account
        .get("fashion")
        .and_then(|fashion| fashion.get("entries"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|entry| {
            json_i32(entry, "id") == Some(fashion_id)
                || json_i32_array(entry, "fashionTids").contains(&fashion_id)
        })
}

fn push_activity_fashion_sync(
    server_state: &ServerState,
    pre_pushes: &mut Vec<Vec<u8>>,
    account: &Value,
    fashion_catalog: Option<&blueoath_protocol::FashionList>,
    rewards: &[ShopReward],
) {
    append_method_push(
        pre_pushes,
        "user.UpdateUserInfo",
        UserInfoCodec::encode(&user_info_from_account(server_state, Some(account))),
    );
    append_method_push(
        pre_pushes,
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_account(account)),
    );
    if rewards.iter().any(|reward| reward.goods_type == 18) {
        append_method_push(
            pre_pushes,
            "fashion.updateData",
            FashionListCodec::encode(&fashion_list_from_account(account, fashion_catalog)),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_activity_fashion_buy(
    catalog: &GameplayCatalog,
    account: &mut Value,
    server_state: &ServerState,
    pre_pushes: &mut Vec<Vec<u8>>,
    fashion_catalog: Option<&blueoath_protocol::FashionList>,
    request_args: &[u8],
    handler_error: &mut Option<GameError>,
) -> Option<Vec<u8>> {
    let Some(config) = activity_fashion_config(catalog, account) else {
        *handler_error = Some(GameError::Internal(
            "activity fashion configuration is unavailable".to_owned(),
        ));
        return Some(Vec::new());
    };
    let current = account
        .get("activityFashion")
        .and_then(|state| state.get("buyCount"))
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0);
    let max_count = config
        .get("p6")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let requested = decode_varint_field(request_args, 1).clamp(1, 99);
    if max_count <= current || i64::from(requested) > max_count - current {
        *handler_error = Some(GameError::Internal(
            "activity fashion purchase limit reached".to_owned(),
        ));
        return Some(Vec::new());
    }
    let gid = decode_varint_field(request_args, 2).max(0);
    let pools = config
        .get("p1")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .filter_map(|value| i32::try_from(value).ok())
        .collect::<Vec<_>>();
    if pools.len() < 3 {
        *handler_error = Some(GameError::Internal(
            "activity fashion reward pools are incomplete".to_owned(),
        ));
        return Some(Vec::new());
    }
    let Some((goods_type, item_id, unit_cost)) = activity_fashion_cost(&config) else {
        *handler_error = Some(GameError::Internal(
            "activity fashion purchase cost is invalid".to_owned(),
        ));
        return Some(Vec::new());
    };
    let unowned_fashion = config
        .get("p5")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(Value::as_i64)
        .filter_map(|value| i32::try_from(value).ok())
        .find(|id| !activity_fashion_owned(account, *id));
    let mut target_unowned = unowned_fashion.is_some();
    let requested_pool = pools.iter().position(|pool| *pool == gid);
    if gid > 0 && requested_pool.is_none() && gid != item_id {
        *handler_error = Some(GameError::Internal(
            "activity fashion reward pool is invalid".to_owned(),
        ));
        return Some(Vec::new());
    }
    let mut reward_specs = Vec::new();
    for offset in 0..requested {
        let draw_index = current.saturating_add(i64::from(offset)).saturating_add(1);
        let drop_id = if draw_index >= max_count {
            pools[2]
        } else if let Some(pool) = requested_pool {
            pools[pool]
        } else if target_unowned {
            pools[0]
        } else {
            pools[1]
        };
        let Some(reward) = activity_fashion_drop_reward(catalog, drop_id, draw_index) else {
            *handler_error = Some(GameError::Internal(
                "activity fashion reward configuration is invalid".to_owned(),
            ));
            return Some(Vec::new());
        };
        if unowned_fashion == Some(reward.item_id) && reward.goods_type == 18 {
            target_unowned = false;
        }
        reward_specs.push(reward);
    }
    let total_cost = unit_cost.saturating_mul(requested);
    if !resource_available(account, goods_type, item_id, total_cost) {
        *handler_error = Some(GameError::Internal(
            "activity fashion purchase cost is insufficient".to_owned(),
        ));
        return Some(Vec::new());
    }
    consume_resource(account, goods_type, item_id, total_cost);
    let rewards = reward_specs
        .into_iter()
        .map(|reward| grant_reward(account, reward, current_unix_seconds(), fashion_catalog))
        .collect::<Vec<_>>();
    {
        let state = activity_state_mut(account, "activityFashion");
        state["buyCount"] = json!(current.saturating_add(i64::from(requested)));
        state["lastGid"] = json!(gid);
    }
    push_activity_fashion_sync(server_state, pre_pushes, account, fashion_catalog, &rewards);
    Some(encode_rewards_list(&rewards))
}

#[allow(clippy::too_many_arguments)]
fn handle_activity_fashion_reward(
    catalog: &GameplayCatalog,
    account: &mut Value,
    server_state: &ServerState,
    pre_pushes: &mut Vec<Vec<u8>>,
    fashion_catalog: Option<&blueoath_protocol::FashionList>,
    request_args: &[u8],
    handler_error: &mut Option<GameError>,
) -> Option<Vec<u8>> {
    let index = decode_varint_field(request_args, 1);
    let Some(config) = activity_fashion_config(catalog, account) else {
        *handler_error = Some(GameError::Internal(
            "activity fashion configuration is unavailable".to_owned(),
        ));
        return Some(Vec::new());
    };
    let (threshold, drop_id) = match index {
        1 => activity_fashion_milestone(&config, "p2"),
        2 => activity_fashion_milestone(&config, "p3"),
        _ => None,
    }
    .unwrap_or_default();
    if threshold <= 0 || drop_id <= 0 {
        *handler_error = Some(GameError::Internal(
            "activity fashion reward index is invalid".to_owned(),
        ));
        return Some(Vec::new());
    }
    let already_claimed = account
        .get("activityFashion")
        .and_then(|state| state.get("specialReward"))
        .and_then(Value::as_array)
        .is_some_and(|values| {
            values
                .iter()
                .any(|value| value.as_i64() == Some(i64::from(index)))
        });
    if already_claimed {
        return Some(encode_rewards_list(&[]));
    }
    let buy_count = account
        .get("activityFashion")
        .and_then(|state| state.get("buyCount"))
        .and_then(Value::as_i64)
        .unwrap_or_default();
    if buy_count < i64::from(threshold) {
        *handler_error = Some(GameError::Internal(
            "activity fashion milestone is not reached".to_owned(),
        ));
        return Some(Vec::new());
    }
    let Some(reward) = activity_fashion_drop_reward(catalog, drop_id, i64::from(index)) else {
        *handler_error = Some(GameError::Internal(
            "activity fashion milestone reward is invalid".to_owned(),
        ));
        return Some(Vec::new());
    };
    let reward = grant_reward(account, reward, current_unix_seconds(), fashion_catalog);
    add_unique_i64(
        activity_state_mut(account, "activityFashion"),
        "specialReward",
        i64::from(index),
    );
    push_activity_fashion_sync(
        server_state,
        pre_pushes,
        account,
        fashion_catalog,
        std::slice::from_ref(&reward),
    );
    Some(encode_rewards_list(&[reward]))
}

fn activity_fashion_milestone(config: &Value, key: &str) -> Option<(i32, i32)> {
    let values = config.get(key)?.as_array()?;
    Some((
        i32::try_from(values.first()?.as_i64()?).ok()?,
        i32::try_from(values.get(1)?.as_i64()?).ok()?,
    ))
}

fn activity_state_mut<'a>(account: &'a mut Value, key: &str) -> &'a mut Value {
    let default = match key {
        "activityExtract" | "activityExtractUr" => {
            json!({"drawId": 0, "realDrawId": 0, "drawCount": 0, "rewards": []})
        }
        "activityFashion" => json!({"activityId": 0, "buyCount": 0, "specialReward": []}),
        "activitySSR" => json!({
            "activityId": 0, "daySelectCount": 0, "dayShareCount": 0,
            "selectShipId": 0, "saveShipId": 0, "rewardTime": 0
        }),
        "activitySSRRolls" => json!({
            "activityId": 0, "daySelectCount": 0, "selectShipTeam": [],
            "saveShipTeam": [], "rewardTime": 0
        }),
        "activityBirthday" => json!({
            "birthdayAffair": 0, "girlsAndCake": [], "stage": [], "cake": 0,
            "claimedAffairs": []
        }),
        "activityCodeExchange" => json!({"receipts": []}),
        "activityPaperCut" => json!({"formulaUseData": []}),
        "activitySecretCopy" => json!({"passTimePerfect": 0, "rewards": []}),
        "activityValentine" => {
            json!({"loveShip": [], "curActShip": 0, "claims": [], "secretaryClaims": []})
        }
        "activityVideo" => json!({"watched": []}),
        "activityChristmasShop" => json!({
            "buyInfo": [], "toyInfo": [], "specialBoxes": [],
            "crystalBallToyId": 0, "isGiveCrystalBall": 0
        }),
        _ => json!({}),
    };
    account
        .as_object_mut()
        .expect("account must be an object")
        .entry(key.to_owned())
        .or_insert(default)
}

#[allow(clippy::too_many_arguments)]
fn handle_extract_draw(
    catalog: &GameplayCatalog,
    account: &mut Value,
    ur: bool,
    draw_id: i32,
    num: i32,
    server_state: &ServerState,
    pre_pushes: &mut Vec<Vec<u8>>,
    fashion_catalog: Option<&blueoath_protocol::FashionList>,
    handler_error: &mut Option<GameError>,
) -> Option<Vec<u8>> {
    if draw_id <= 0 {
        *handler_error = Some(GameError::Internal(
            "activity extract draw id is invalid".to_owned(),
        ));
        return Some(Vec::new());
    }
    let configs = if ur {
        &catalog.activity_extract_ur
    } else {
        &catalog.activity_extract
    };
    let Some(config) = configs.get(&draw_id) else {
        *handler_error = Some(GameError::Internal(
            "activity extract pool is not configured".to_owned(),
        ));
        return Some(Vec::new());
    };
    let Some(cost) = extract_cost(config) else {
        *handler_error = Some(GameError::Internal(
            "activity extract cost is not configured".to_owned(),
        ));
        return Some(Vec::new());
    };
    let entries = config
        .get("drop_reward_id")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let row = row.as_array()?;
            Some((
                i32::try_from(row.first()?.as_i64()?).ok()?,
                i32::try_from(row.get(1)?.as_i64()?).ok()?,
            ))
        })
        .filter(|(reward_id, amount)| *reward_id > 0 && *amount > 0)
        .collect::<Vec<_>>();
    if entries.is_empty() {
        *handler_error = Some(GameError::Internal(
            "activity extract reward pool is empty".to_owned(),
        ));
        return Some(Vec::new());
    }
    let total_cost = cost.2.saturating_mul(num);
    if !extract_can_consume(account, cost.0, cost.1, total_cost) {
        *handler_error = Some(GameError::Internal(
            "activity extract cost is insufficient".to_owned(),
        ));
        return Some(Vec::new());
    }
    extract_consume(account, cost.0, cost.1, total_cost);
    let state_key = if ur {
        "activityExtractUr"
    } else {
        "activityExtract"
    };
    let start = activity_state_mut(account, state_key)
        .get("drawCount")
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .max(0) as usize;
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
        for reward in reward_defs {
            granted.push(grant_reward(
                account,
                reward,
                current_unix_seconds(),
                fashion_catalog,
            ));
        }
        let state = activity_state_mut(account, state_key);
        state["rewards"]
            .as_array_mut()
            .expect("extract rewards must be an array")
            .push(json!({"rewardId": reward_id, "num": amount}));
        state["drawCount"] = json!(start.saturating_add(offset + 1));
        state["drawId"] = json!(draw_id);
        state["realDrawId"] = json!(draw_id);
    }
    if !granted.is_empty() {
        let user_payload =
            UserInfoCodec::encode(&user_info_from_account(server_state, Some(account)));
        let bag_payload = BagInfoCodec::encode(&bag_info_from_account(account));
        append_method_push(pre_pushes, "user.UpdateUserInfo", user_payload);
        append_method_push(pre_pushes, "bag.UpdateBagData", bag_payload);
    }
    if ur {
        Some(extract_ur_draw_ret_payload(&selected_ids))
    } else {
        Some(extract_draw_ret_payload(&granted))
    }
}

fn extract_can_consume(account: &Value, kind: i32, item: i32, amount: i32) -> bool {
    if kind == 5 {
        currency_character_key(item)
            .is_some_and(|key| character_i64(account, key) >= i64::from(amount))
    } else {
        bag_item_count(account, item) >= i64::from(amount)
    }
}

fn extract_consume(account: &mut Value, kind: i32, item: i32, amount: i32) {
    if kind == 5 {
        if let Some(key) = currency_character_key(item) {
            add_character_i64(account, key, amount.saturating_neg());
        }
    } else {
        consume_bag_item(account, item, amount);
    }
}

fn extract_cost(config: &Value) -> Option<(i32, i32, i32)> {
    let values = config.get("item_cost")?.as_array()?;
    Some((
        i32::try_from(values.first()?.as_i64()?).ok()?,
        i32::try_from(values.get(1)?.as_i64()?).ok()?,
        i32::try_from(values.get(2)?.as_i64()?).ok()?,
    ))
}

fn extract_info_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        json_i64(state, "drawId").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(
        &mut output,
        2,
        json_i64(state, "realDrawId").unwrap_or_default().max(0) as u64,
    );
    for reward in state
        .get("rewards")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut encoded = Vec::new();
        append_varint_field(
            &mut encoded,
            1,
            json_i64(reward, "rewardId").unwrap_or(60000).max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            2,
            json_i64(reward, "num").unwrap_or(1).max(0) as u64,
        );
        append_message_field(&mut output, 3, &encoded);
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

fn extract_ur_draw_ret_payload(reward_ids: &[i32]) -> Vec<u8> {
    let mut output = Vec::new();
    for reward_id in reward_ids {
        append_varint_field(&mut output, 1, (*reward_id).max(0) as u64);
    }
    output
}

fn fashion_info_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        json_i64(state, "activityId").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(
        &mut output,
        2,
        json_i64(state, "buyCount").unwrap_or_default().max(0) as u64,
    );
    for index in state
        .get("specialReward")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        append_varint_field(
            &mut output,
            3,
            index.as_i64().unwrap_or_default().max(0) as u64,
        );
    }
    output
}

fn ssr_info_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    for (field, key) in [
        (1, "activityId"),
        (2, "daySelectCount"),
        (3, "dayShareCount"),
        (4, "selectShipId"),
        (5, "saveShipId"),
        (6, "rewardTime"),
    ] {
        append_varint_field(
            &mut output,
            field,
            json_i64(state, key).unwrap_or_default().max(0) as u64,
        );
    }
    output
}

fn rolls_info_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        json_i64(state, "activityId").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(
        &mut output,
        2,
        json_i64(state, "daySelectCount").unwrap_or_default().max(0) as u64,
    );
    append_team(&mut output, 3, state.get("selectShipTeam"));
    append_team(&mut output, 4, state.get("saveShipTeam"));
    append_varint_field(
        &mut output,
        5,
        json_i64(state, "rewardTime").unwrap_or_default().max(0) as u64,
    );
    output
}

fn append_team(output: &mut Vec<u8>, field: u8, value: Option<&Value>) {
    let Some(ids) = value.and_then(Value::as_array) else {
        return;
    };
    let mut team = Vec::new();
    for id in ids {
        append_varint_field(&mut team, 1, id.as_i64().unwrap_or_default().max(0) as u64);
    }
    append_message_field(output, field, &team);
}

fn birthday_info_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        json_i64(state, "birthdayAffair").unwrap_or_default().max(0) as u64,
    );
    for data in state
        .get("girlsAndCake")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut encoded = Vec::new();
        append_varint_field(
            &mut encoded,
            1,
            json_i64(data, "girl").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            2,
            json_i64(data, "cake").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            3,
            json_i64(data, "teamId").unwrap_or_default().max(0) as u64,
        );
        append_message_field(&mut output, 2, &encoded);
    }
    for stage in state
        .get("stage")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        append_varint_field(
            &mut output,
            3,
            stage.as_i64().unwrap_or_default().max(0) as u64,
        );
    }
    output
}

fn birthday_activity_config(catalog: &GameplayCatalog) -> Option<&Value> {
    catalog.activity.get(&103).or_else(|| {
        catalog
            .activity
            .values()
            .find(|activity| json_i32(activity, "type") == Some(103))
    })
}

fn birthday_formula_reward(catalog: &GameplayCatalog, formula: i32) -> Option<ShopReward> {
    let row = birthday_activity_config(catalog)?;
    let values = row.get("p4")?.as_array()?;
    let formula = usize::try_from(formula.checked_sub(1)?).ok()?;
    let reward = values.get(formula)?.as_array()?;
    Some(ShopReward {
        goods_type: 1,
        item_id: i32::try_from(reward.get(2)?.as_i64()?).ok()?,
        num: 1,
        instance_id: 0,
    })
}

fn birthday_affair_reward_id(catalog: &GameplayCatalog, level: i32) -> Option<i32> {
    let row = birthday_activity_config(catalog)?;
    row.get("p5")?
        .as_array()?
        .iter()
        .filter_map(Value::as_array)
        .find(|entry| entry.first().and_then(Value::as_i64) == Some(i64::from(level)))
        .and_then(|entry| entry.get(1)?.as_i64())
        .and_then(|value| i32::try_from(value).ok())
}

fn add_receipt(state: &mut Value, reward_id: i32, count: i32) {
    let receipts = state
        .get_mut("receipts")
        .and_then(Value::as_array_mut)
        .expect("receipts array");
    if let Some(existing) = receipts
        .iter_mut()
        .find(|item| json_i64(item, "rewardId") == Some(i64::from(reward_id)))
    {
        let current = json_i64(existing, "count").unwrap_or_default();
        existing["count"] = json!(current.saturating_add(i64::from(count)));
    } else {
        receipts.push(json!({"rewardId": reward_id, "count": count}));
    }
}

fn code_exchange_activity(catalog: &GameplayCatalog) -> Option<&Value> {
    catalog.activity.get(&81005).or_else(|| {
        catalog
            .activity
            .values()
            .find(|value| json_i32(value, "type") == Some(81005))
    })
}

fn code_exchange_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    for receipt in state
        .get("receipts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut encoded = Vec::new();
        append_varint_field(
            &mut encoded,
            1,
            json_i64(receipt, "rewardId").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            2,
            json_i64(receipt, "count").unwrap_or_default().max(0) as u64,
        );
        append_message_field(&mut output, 1, &encoded);
    }
    output
}

fn increment_formula_count(state: &mut Value, formula: i32) {
    let entries = state
        .get_mut("formulaUseData")
        .and_then(Value::as_array_mut)
        .expect("formula data array");
    if let Some(existing) = entries
        .iter_mut()
        .find(|item| json_i64(item, "formulaId") == Some(i64::from(formula)))
    {
        let count = json_i64(existing, "count").unwrap_or_default();
        existing["count"] = json!(count.saturating_add(1));
    } else {
        entries.push(json!({"formulaId": formula, "count": 1}));
    }
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

fn paper_cut_info_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    for entry in state
        .get("formulaUseData")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut item = Vec::new();
        append_varint_field(
            &mut item,
            1,
            json_i64(entry, "formulaId").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut item,
            2,
            json_i64(entry, "count").unwrap_or_default().max(0) as u64,
        );
        append_message_field(&mut output, 1, &item);
    }
    output
}

fn secret_copy_info_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        json_i64(state, "passTimePerfect")
            .unwrap_or_default()
            .max(0) as u64,
    );
    for reward in state
        .get("rewards")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut encoded = Vec::new();
        append_varint_field(
            &mut encoded,
            1,
            json_i64(reward, "rateIndex").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            2,
            json_i64(reward, "getReward").unwrap_or_default().max(0) as u64,
        );
        append_message_field(&mut output, 2, &encoded);
    }
    output
}

fn valentine_info_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    for hero in state
        .get("loveShip")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut encoded = Vec::new();
        append_varint_field(
            &mut encoded,
            1,
            json_i64(hero, "index").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            2,
            json_i64(hero, "heroId").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            3,
            json_i64(hero, "templateId").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            4,
            json_i64(hero, "shipTid").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(&mut encoded, 5, u64::from(json_bool(hero, "isGift")));
        append_message_field(&mut output, 1, &encoded);
    }
    append_varint_field(
        &mut output,
        2,
        json_i64(state, "curActShip").unwrap_or_default().max(0) as u64,
    );
    output
}

fn valentine_reward_id(
    catalog: &GameplayCatalog,
    state: &Value,
    method: &str,
    index: i32,
) -> Option<i32> {
    let ship_tid = if method.ends_with("BySecretary") {
        json_i32(state, "curActShip")
    } else {
        state
            .get("loveShip")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|hero| json_i32(hero, "index") == Some(index))
            .and_then(|hero| json_i32(hero, "shipTid"))
    }?;
    catalog
        .valentine_gifts
        .values()
        .find(|gift| json_i32(gift, "ship_fleet_id") == Some(ship_tid))
        .and_then(|gift| json_i32(gift, "attach_reward"))
}

fn video_info_payload(state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    for id in state
        .get("watched")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        append_varint_field(
            &mut output,
            1,
            id.as_i64().unwrap_or_default().max(0) as u64,
        );
    }
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

fn christmas_info_payload(_catalog: &GameplayCatalog, state: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    for buy_info in state
        .get("buyInfo")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut encoded = Vec::new();
        append_varint_field(
            &mut encoded,
            1,
            json_i64(buy_info, "buyIndex").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            2,
            json_i64(buy_info, "buyCount").unwrap_or_default().max(0) as u64,
        );
        append_message_field(&mut output, 1, &encoded);
    }
    for toy_info in state
        .get("toyInfo")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut encoded = Vec::new();
        append_varint_field(
            &mut encoded,
            1,
            json_i64(toy_info, "toyId").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut encoded,
            2,
            json_i64(toy_info, "ownCount").unwrap_or_default().max(0) as u64,
        );
        append_message_field(&mut output, 2, &encoded);
    }
    append_varint_field(
        &mut output,
        3,
        json_i64(state, "crystalBallToyId")
            .unwrap_or_default()
            .max(0) as u64,
    );
    append_varint_field(
        &mut output,
        4,
        json_i64(state, "isGiveCrystalBall")
            .unwrap_or_default()
            .max(0) as u64,
    );
    output
}

fn handle_christmas_buy_box(
    catalog: &GameplayCatalog,
    account: &mut Value,
    server_state: &ServerState,
    pre_pushes: &mut Vec<Vec<u8>>,
    request_args: &[u8],
    handler_error: &mut Option<GameError>,
) -> Option<Vec<u8>> {
    const BLIND_BOX_COIN: i32 = 17_007;
    const BLIND_BOX_REPEAT_TOY: i32 = 17_008;
    let buy_index = decode_varint_field(request_args, 1);
    let cost = parameter_value(catalog, 313).unwrap_or(10).max(1);
    let limit = parameter_value(catalog, 314).unwrap_or(8).max(1);
    let eligible = christmas_eligible_figures(catalog, account);
    if eligible.is_empty() {
        *handler_error = Some(GameError::Internal(
            "christmas blind box has no eligible figure".to_owned(),
        ));
        return Some(Vec::new());
    }
    let current_count = account
        .get("activityChristmasShop")
        .and_then(|state| state.get("buyInfo"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|entry| json_i64(entry, "buyIndex") == Some(i64::from(buy_index)))
        .and_then(|entry| json_i64(entry, "buyCount"))
        .unwrap_or_default();
    if current_count >= i64::from(limit) {
        *handler_error = Some(GameError::Internal(
            "christmas blind box daily limit reached".to_owned(),
        ));
        return Some(Vec::new());
    }
    if !resource_available(account, 1, BLIND_BOX_COIN, cost) {
        *handler_error = Some(GameError::Internal(
            "christmas blind box coin is insufficient".to_owned(),
        ));
        return Some(Vec::new());
    }
    consume_resource(account, 1, BLIND_BOX_COIN, cost);
    let toy_id = eligible[(current_count as usize) % eligible.len()];
    let duplicate_toy = account
        .get("activityChristmasShop")
        .and_then(|state| state.get("toyInfo"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|entry| json_i64(entry, "toyId") == Some(i64::from(toy_id)))
        .and_then(|entry| json_i64(entry, "ownCount"))
        .is_some_and(|own_count| own_count > 0);
    if duplicate_toy {
        let _ = grant_reward(
            account,
            ShopReward {
                goods_type: 1,
                item_id: BLIND_BOX_REPEAT_TOY,
                num: 1,
                instance_id: 0,
            },
            current_unix_seconds(),
            None,
        );
    }
    let state = activity_state_mut(account, "activityChristmasShop");
    let buy_info = state
        .get_mut("buyInfo")
        .and_then(Value::as_array_mut)
        .expect("christmas buyInfo must be an array");
    let entry = buy_info
        .iter_mut()
        .find(|entry| json_i64(entry, "buyIndex") == Some(i64::from(buy_index)));
    if let Some(entry) = entry {
        let next = json_i64(entry, "buyCount")
            .unwrap_or_default()
            .saturating_add(1);
        entry["buyCount"] = json!(next);
    } else {
        buy_info.push(json!({"buyIndex": buy_index, "buyCount": 1}));
    }
    if !duplicate_toy {
        let toy_info = state
            .get_mut("toyInfo")
            .and_then(Value::as_array_mut)
            .expect("christmas toyInfo must be an array");
        toy_info.push(json!({"toyId": toy_id, "ownCount": 1}));
    }
    append_method_push(
        pre_pushes,
        "user.UpdateUserInfo",
        UserInfoCodec::encode(&user_info_from_account(server_state, Some(account))),
    );
    append_method_push(
        pre_pushes,
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_account(account)),
    );
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, toy_id as u64);
    Some(output)
}

fn christmas_eligible_figures(catalog: &GameplayCatalog, account: &Value) -> Vec<i32> {
    let owned_ship_fleets = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|hero| json_i32(hero, "templateId"))
        .map(|template| template / 10)
        .collect::<std::collections::BTreeSet<_>>();
    let owned_fashion = account
        .get("fashion")
        .and_then(|fashion| fashion.get("entries"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|entry| json_i32_array(entry, "fashionTids"))
        .collect::<std::collections::BTreeSet<_>>();
    let mut figures = catalog
        .interaction_figures
        .iter()
        .filter_map(|(id, figure)| {
            if json_i32(figure, "is_drawable").unwrap_or_default() <= 0 {
                return None;
            }
            let required = json_i32(figure, "origional_ship_required").unwrap_or_default() > 0;
            let figure_type = json_i32(figure, "figure_type").unwrap_or_default();
            let available = match figure_type {
                1 => !required || owned_ship_fleets.contains(id),
                2 => owned_fashion.contains(id),
                _ => false,
            };
            available.then_some(*id)
        })
        .collect::<Vec<_>>();
    figures.sort_unstable();
    figures
}

fn handle_christmas_buy_item(
    catalog: &GameplayCatalog,
    account: &mut Value,
    server_state: &ServerState,
    pre_pushes: &mut Vec<Vec<u8>>,
    request_args: &[u8],
    handler_error: &mut Option<GameError>,
) -> Option<Vec<u8>> {
    const BUY_CUR: i32 = 1;
    const BUY_TOY: i32 = 2;
    const BLIND_BOX_COIN: i32 = 17_007;
    const BLIND_BOX_REPEAT_TOY: i32 = 17_008;
    let buy_way = decode_varint_field(request_args, 1);
    let buy_times = decode_varint_field(request_args, 2).clamp(1, 99);
    let (goods_type, item_id, unit_cost) = match buy_way {
        BUY_CUR => (5, 1, parameter_value(catalog, 311).unwrap_or(5_000).max(1)),
        BUY_TOY => (
            1,
            BLIND_BOX_REPEAT_TOY,
            parameter_value(catalog, 312).unwrap_or(10).max(1),
        ),
        _ => {
            *handler_error = Some(GameError::Internal(
                "christmas buy way is invalid".to_owned(),
            ));
            return Some(Vec::new());
        }
    };
    let total_cost = unit_cost.saturating_mul(buy_times);
    if !resource_available(account, goods_type, item_id, total_cost) {
        *handler_error = Some(GameError::Internal(
            "christmas blind box exchange cost is insufficient".to_owned(),
        ));
        return Some(Vec::new());
    }
    consume_resource(account, goods_type, item_id, total_cost);
    let reward = grant_reward(
        account,
        ShopReward {
            goods_type: 1,
            item_id: BLIND_BOX_COIN,
            num: buy_times,
            instance_id: 0,
        },
        current_unix_seconds(),
        None,
    );
    let state = activity_state_mut(account, "activityChristmasShop");
    state["lastBuyWay"] = json!(buy_way);
    state["lastBuyTimes"] = json!(buy_times);
    let user_payload = UserInfoCodec::encode(&user_info_from_account(server_state, Some(account)));
    let bag_payload = BagInfoCodec::encode(&bag_info_from_account(account));
    append_method_push(pre_pushes, "user.UpdateUserInfo", user_payload);
    append_method_push(pre_pushes, "bag.UpdateBagData", bag_payload);
    Some(encode_rewards_list(&[reward]))
}

fn parameter_value(catalog: &GameplayCatalog, id: i32) -> Option<i32> {
    catalog
        .parameters
        .get(&id)
        .and_then(|value| json_i32(value, "value"))
}

fn common_reward_payload(goods_type: i32, config_id: i32, num: i32) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, goods_type.max(0) as u64);
    append_varint_field(&mut output, 2, config_id.max(0) as u64);
    append_varint_field(&mut output, 3, num.max(0) as u64);
    output
}

fn add_unique_i64(state: &mut Value, key: &str, value: i64) {
    let values = state
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .expect("activity list array");
    if !values.iter().any(|entry| entry.as_i64() == Some(value)) {
        values.push(json!(value));
    }
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
            handle_typed(&mut account, "activitySSR.ActivitySSRSelect", &select),
            HandlerResult::Reply(_)
        ));
        assert!(matches!(
            handle_typed(&mut account, "activitySSR.ActivitySSRRand", &[]),
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
            handle_typed(&mut account, "activityextract.SwitchDraw", &[]),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account
                .activities
                .progress
                .get("activity:activityExtract:realDrawId"),
            Some(&1)
        );
    }
}
