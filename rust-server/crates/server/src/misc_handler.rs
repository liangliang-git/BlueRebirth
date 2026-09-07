use serde_json::json;

use super::*;

pub(super) fn handles(method: &str) -> bool {
    [
        "activitybirthday.",
        "activitychristmasshop.",
        "activitycodeexchange.",
        "activityextract.",
        "activityextractur.",
        "activityfashion.",
        "activitypapercut.",
        "activitysecretcopy.",
        "activitySSR.",
        "activitySSRrolls.",
        "activityvalentineloveletter.",
        "activityVideo.",
        "adventure.",
        "bigactivity.",
        "boss.",
        "chat.",
        "guildbigactivity.",
        "guildbigactivityrank.",
        "guildbox.",
        "guildOffer.",
        "guildofferrank.",
        "guildtask.",
        "guildwar.",
        "heroawaken.",
        "invitescore.",
        "shiptask.",
        "sportsmeet.",
        "sportsmeetrank.",
        "worldevent.",
        "worldeventrank.",
    ]
    .iter()
    .any(|prefix| method.starts_with(prefix))
}

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> Option<Vec<u8>> {
    let account = &mut *context.account;
    let response_err = &mut *context.response_err;
    let response_err_msg = &mut *context.response_err_msg;
    let Some(account) = account.as_deref_mut() else {
        *response_err = 1;
        *response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };

    match method {
        "archiveCopy.IsLoad" => {
            account["archiveCopyLoaded"] = json!({
                "copyId": decode_varint_field(request_args, 1),
                "time": current_unix_seconds(),
            });
            Some(Vec::new())
        }
        "copyextra.AddCopyRewardCount" => {
            let chapter_id = decode_varint_field(request_args, 1);
            let reward_time = decode_varint_field(request_args, 2).max(1);
            if chapter_id <= 0 {
                *response_err = 1;
                *response_err_msg = "copy reward count target is invalid".to_owned();
                Some(Vec::new())
            } else {
                let entries = account
                    .as_object_mut()
                    .expect("account must be an object")
                    .entry("copyRewardCounts".to_owned())
                    .or_insert_with(|| json!({}));
                let key = chapter_id.to_string();
                let current = entries
                    .get(&key)
                    .and_then(Value::as_i64)
                    .unwrap_or_default();
                let total = current.saturating_add(i64::from(reward_time));
                entries[&key] = json!(total);
                Some(copy_reward_times_payload(chapter_id, total))
            }
        }
        "copyextra.UpdateCopyExtraInfo" => Some(copy_extra_info_payload(account)),
        "prefs.SavePrefs" => {
            account["prefsData"] = json!({
                "data": decode_string_field(request_args, 1).unwrap_or_default(),
                "time": current_unix_seconds(),
            });
            Some(Vec::new())
        }
        "statcount.GetStatCount" => {
            account["lastStatCountTime"] = json!(current_unix_seconds());
            Some(Vec::new())
        }
        "sign.Sign" => {
            let day = decode_varint_field(request_args, 1).max(1);
            let sign = account
                .as_object_mut()
                .expect("account must be an object")
                .entry("sign".to_owned())
                .or_insert_with(|| json!({"days": []}));
            let days = sign
                .get_mut("days")
                .and_then(Value::as_array_mut)
                .expect("sign days must be an array");
            if !days
                .iter()
                .any(|value| value.as_i64() == Some(i64::from(day)))
            {
                days.push(json!(day));
            }
            Some(Vec::new())
        }
        "miniGame.StartMiniGame" => {
            account["miniGame"] = json!({
                "args": request_args,
                "time": current_unix_seconds(),
            });
            Some(Vec::new())
        }
        "alchemy.StartAlchemy" => {
            let formula_id = decode_varint_field(request_args, 1);
            let equip_ids = decode_repeated_varint_field(request_args, 2);
            if formula_id <= 0 || equip_ids.is_empty() {
                *response_err = 1;
                *response_err_msg = "alchemy request is invalid".to_owned();
            } else {
                account["alchemy"] = json!({
                    "formulaId": formula_id,
                    "equipIds": equip_ids,
                    "time": current_unix_seconds(),
                });
            }
            Some(Vec::new())
        }
        "exchange.GetExchangeInfo" => Some(Vec::new()),
        "exchange.Exchange" => {
            account["lastExchange"] = json!({
                "args": request_args,
                "time": current_unix_seconds(),
            });
            Some(Vec::new())
        }
        "foodCompose.GetFoodComposeData" => Some(Vec::new()),
        "foodCompose.FoodCompose" => {
            account["lastFoodCompose"] = json!({
                "args": request_args,
                "time": current_unix_seconds(),
            });
            Some(Vec::new())
        }
        "battlepass.GetReward"
        | "battlepass.GetAllReward"
        | "activitybattlepass.GetReward"
        | "activitybattlepass.GetAllReward" => {
            let key = if method.starts_with("activity") {
                "activityBattlePassClaimed"
            } else {
                "battlePassClaimed"
            };
            let level = decode_varint_field(request_args, 1).max(0);
            let claims = account
                .as_object_mut()
                .expect("account must be an object")
                .entry(key.to_owned())
                .or_insert_with(|| json!([]));
            if level > 0 {
                claims
                    .as_array_mut()
                    .expect("battle pass claims must be an array")
                    .push(json!(level));
            }
            Some(Vec::new())
        }
        "battlepass.RefreshRandomTask"
        | "activitybattlepass.RefreshRandomTask"
        | "battlepass.RecieveTaskReward"
        | "activitybattlepass.RecieveTaskReward"
        | "battlepass.BuyPassType"
        | "activitybattlepass.BuyPassType"
        | "battlepass.BuyPassLevel"
        | "activitybattlepass.BuyPassLevel" => {
            account["lastBattlePassAction"] = json!({
                "method": method,
                "args": request_args,
                "time": current_unix_seconds(),
            });
            Some(Vec::new())
        }
        "magazine.Magazine" => Some(Vec::new()),
        "magazine.AddHero"
        | "magazine.Vote"
        | "magazine.FetchMagazineReward"
        | "magazine.UnLock" => {
            account["lastMagazineAction"] = json!({
                "method": method,
                "args": request_args,
                "time": current_unix_seconds(),
            });
            Some(Vec::new())
        }
        "interactionitem.GetItemReward"
        | "interactionitem.BuyChristmasFurniture"
        | "interactionitem.GetSpringPaperFlowerReward"
        | "interactionitem.SetCrystalBallToy"
        | "interactionitem.SetBagItemVisible"
        | "interactionitem.SetMutexBagGroupState"
        | "interactionitem.SetPosterState" => {
            account["lastInteractionItemAction"] = json!({
                "method": method,
                "args": request_args,
                "time": current_unix_seconds(),
            });
            Some(Vec::new())
        }
        "bigactivity.GetBigActivityInfo"
        | "bigactivity.GetBigActivityRank"
        | "bigactivity.GetBigActivityRankEx"
        | "guildbigactivity.PresentItem"
        | "guildbigactivityrank.GetGuildRankList"
        | "guildOffer.GuildOffer"
        | "guildOffer.GuildOfferUser"
        | "guildOffer.AddOffer"
        | "guildOffer.AbandonOffer"
        | "guildOffer.ReceiveOfferRewardPerson"
        | "guildOffer.ReceiveOfferRewardGuild"
        | "guildOffer.ReceiveOfferRewardAll"
        | "guildOffer.BuyOfferCount"
        | "guildOffer.GetRankList"
        | "guildofferrank.GetGuildRankList"
        | "guildtask.AcceptTask"
        | "guildtask.GuildTaskAccept"
        | "guildtask.GuildTaskFinish"
        | "guildtask.ConstantRewardPoolGetReward"
        | "guildtask.DrawTaskReward"
        | "guildtask.Donate"
        | "guildwar.GetGuildwarInfo"
        | "guildwar.GetBaseInfo"
        | "guildwar.GetHeroLockInfo"
        | "guildwar.GetRankList"
        | "guildwar.GetBattleReport"
        | "guildwar.GetGuildReward"
        | "guildwar.GetRankUserList"
        | "guildwar.GetHaveScores"
        | "guildwar.GetHaveGuildReward"
        | "heroawaken.RewardMilestone"
        | "invitescore.SetInviteStateByType"
        | "invitescore.CheckAndResetInviteState"
        | "recharge.FreeReward"
        | "recharge.GetPaybackReward"
        | "recharge.DirectBuyItem"
        | "recharge.DirectBuySelectItem"
        | "shiptask.GetShipTaskReward"
        | "shiptask.GetAchievementReward"
        | "shiptask.SetCurrentShip"
        | "sportsmeet.GetSportsTickCount"
        | "sportsmeet.GetPointsRewardDetail"
        | "sportsmeet.ReceivePointsReward"
        | "sportsmeet.ReceiveAllPointsReward"
        | "sportsmeetrank.GetOwnerRankData"
        | "worldevent.StageReward"
        | "worldeventrank.Rank" => {
            record_compat_route(account, method, request_args);
            Some(Vec::new())
        }
        m if handles(m) => {
            record_compat_route(account, m, request_args);
            Some(Vec::new())
        }
        m if m.starts_with("battlepass.") || m.starts_with("activitybattlepass.") => {
            account["lastBattlePassAction"] = json!({
                "method": m,
                "args": request_args,
                "time": current_unix_seconds(),
            });
            Some(Vec::new())
        }
        _ => None,
    }
}

fn record_compat_route(account: &mut Value, method: &str, request_args: &[u8]) {
    let calls = account
        .as_object_mut()
        .expect("account must be an object")
        .entry("compatRouteCalls".to_owned())
        .or_insert_with(|| json!([]));
    calls
        .as_array_mut()
        .expect("compat route calls must be an array")
        .push(json!({
            "method": method,
            "args": request_args,
            "time": current_unix_seconds(),
        }));
}

fn copy_reward_times_payload(chapter_id: i32, reward_time: i64) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, chapter_id.max(0) as u64);
    append_varint_field(&mut output, 2, reward_time.max(0) as u64);
    output
}

fn copy_extra_info_payload(account: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    if let Some(entries) = account.get("copyRewardCounts").and_then(Value::as_object) {
        for (chapter_id, reward_time) in entries {
            let Ok(chapter_id) = chapter_id.parse::<i32>() else {
                continue;
            };
            let reward_time = reward_time.as_i64().unwrap_or_default();
            append_message_field(
                &mut output,
                1,
                &copy_reward_times_payload(chapter_id, reward_time),
            );
        }
    }
    let append_branch = |output: &mut Vec<u8>, branch: &Value| {
        let Some(branch) = branch.as_object() else {
            return;
        };
        let mut payload = Vec::new();
        let branch_id = branch
            .get("branchId")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        let state = branch
            .get("state")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        append_varint_field(&mut payload, 1, branch_id.max(0) as u64);
        append_varint_field(&mut payload, 2, state.max(0) as u64);
        append_message_field(output, 2, &payload);
    };
    if let Some(branches) = account.get("copyGalgameBranches").and_then(Value::as_array) {
        for branch in branches {
            append_branch(&mut output, branch);
        }
    } else if let Some(branch) = account.get("copyGalgameBranch") {
        append_branch(&mut output, branch);
    }
    output
}
