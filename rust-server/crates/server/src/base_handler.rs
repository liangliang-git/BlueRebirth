use serde_json::{json, Value};

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(super) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    state: &ServerState,
    method: &str,
    request_args: &[u8],
    pre_pushes: &mut Vec<Vec<u8>>,
) -> HandlerResult {
    match method {
        "user.BuyGold" | "user.BuySupply" | "user.BuyPvePt" => {
            if ResourcePurchaseRequest::decode(request_args).is_err() {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "resource purchase request is invalid",
                ));
            }
            let (kind, amount) = match method {
                "user.BuyGold" => (blueoath_domain::CurrencyKind::Gold, 1_000_u64),
                "user.BuySupply" => (blueoath_domain::CurrencyKind::Supply, 100_u64),
                "user.BuyPvePt" => (blueoath_domain::CurrencyKind::PvePoint, 10_u64),
                _ => unreachable!(),
            };
            if account
                .resources
                .amount(blueoath_domain::CurrencyKind::Diamond)
                .get()
                < 10
                || account.resources.amount(kind).get() > u64::MAX - amount
            {
                return HandlerResult::Error(GameError::InsufficientResource(
                    blueoath_domain::CurrencyKind::Diamond,
                ));
            }
            let _ = account
                .resources
                .debit(blueoath_domain::CurrencyKind::Diamond, 10);
            let _ = account.resources.credit(kind, amount);
            append_method_push(
                pre_pushes,
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
            );
            HandlerResult::PushOnly
        }
        "user.GetSupply" => {
            let Ok(request) = UserSupplyRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "supply request is invalid",
                ));
            };
            let mut payload = Vec::new();
            append_varint_field(&mut payload, 1, request.supply_id as u64);
            append_varint_field(&mut payload, 2, 0);
            HandlerResult::Reply(Response::raw(method, payload))
        }
        "usersvr.GetOtherInfo" => {
            let Ok(request) = UserOtherInfoRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "other user request is invalid",
                ));
            };
            HandlerResult::Reply(Response::raw(
                method,
                other_user_payload_typed(state, account, request.requested_uid),
            ))
        }
        "user.TeacherRank" => {
            let Ok(request) = TeacherRankRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "teacher rank request is invalid",
                ));
            };
            HandlerResult::Reply(Response::raw(
                method,
                teacher_rank_payload_typed(state, account, request.begin, request.offset),
            ))
        }
        _ => HandlerResult::Empty,
    }
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
    let state = context.state;
    let account = &mut *context.account;
    let hero_level_catalog = context.catalogs.hero_level;
    let pre_pushes = &mut *context.pre_pushes;
    let handler_error = &mut *context.handler_error;

    match method {
        "jopen.GetJopen" => Some(jopen_payload(account.as_deref().unwrap_or(&Value::Null))),
        "jopen.FetchHero" | "jopen.FetchEquip" => {
            if let Some(account) = account.as_deref_mut() {
                let key = if method == "jopen.FetchHero" {
                    "fetchHeroTime"
                } else {
                    "fetchEquipTime"
                };
                account
                    .as_object_mut()
                    .expect("account must be an object")
                    .entry("jopen")
                    .or_insert_with(|| json!({"fetchHeroTime": 0, "fetchEquipTime": 0}));
                account["jopen"][key] = json!(current_unix_seconds());
                append_method_push(pre_pushes, "jopen.GetJopen", jopen_payload(account));
            }
            Some(Vec::new())
        }
        "strategy.GetStrategy" => Some(strategy_info_payload(
            account.as_deref().unwrap_or(&Value::Null),
        )),
        "strategy.Learn" | "strategy.Upgrade" | "strategy.Reset" | "strategy.Apply" => {
            if let Some(account) = account.as_deref_mut() {
                if apply_strategy_state(account, method, request_args) {
                    if method == "strategy.Apply" {
                        append_method_push(
                            pre_pushes,
                            "tactic.GetHerosTactic",
                            FleetInfoCodec::encode(&fleet_info_from_account(account)),
                        );
                    }
                    append_method_push(
                        pre_pushes,
                        "strategy.GetStrategy",
                        strategy_info_payload(account),
                    );
                    append_method_push(
                        pre_pushes,
                        "user.UpdateUserInfo",
                        UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                    );
                } else {
                    *handler_error = Some(GameError::Internal(
                        "strategy request is invalid".to_owned(),
                    ));
                }
            }
            Some(Vec::new())
        }
        "supportfleet.SupportFleetInfo" => Some(support_info_payload(
            account.as_deref().unwrap_or(&Value::Null),
        )),
        "supportfleet.StartSupport" => {
            if let Some(account) = account.as_deref_mut() {
                let support_id = decode_varint_field(request_args, 1);
                let hero_ids = decode_repeated_varint_field(request_args, 2);
                let catalog = SUPPORT_CATALOG.get().cloned().unwrap_or_default();
                let known_support =
                    catalog.items.is_empty() || catalog.items.contains_key(&support_id);
                let valid_hero_list = (1..=6).contains(&hero_ids.len())
                    && hero_ids.iter().all(|hero_id| *hero_id > 0)
                    && hero_ids
                        .iter()
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                        == hero_ids.len();
                if known_support
                    && valid_hero_list
                    && start_support_state(account, request_args, current_unix_seconds()).is_some()
                {
                    append_method_push(
                        pre_pushes,
                        "supportfleet.SupportFleetInfo",
                        support_info_payload(account),
                    );
                } else {
                    *handler_error =
                        Some(GameError::Internal("support request is invalid".to_owned()));
                }
            }
            Some(Vec::new())
        }
        "supportfleet.CompleteSupport" | "supportfleet.CancelSupport" => {
            let id = decode_varint_field(request_args, 1);
            let completion_type = decode_varint_field(request_args, 2);
            if let Some(account) = account.as_deref_mut() {
                let catalog = SUPPORT_CATALOG.get().cloned().unwrap_or_default();
                if let Some(settlement) = settle_support_state(
                    account,
                    id,
                    if method == "supportfleet.CancelSupport" {
                        3
                    } else {
                        completion_type
                    },
                    current_unix_seconds(),
                    &catalog,
                ) {
                    for (goods_type, item_id, amount) in settlement
                        .base_rewards
                        .iter()
                        .chain(settlement.random_rewards.iter())
                    {
                        apply_support_reward(
                            account,
                            &settlement.hero_ids,
                            hero_level_catalog,
                            *goods_type,
                            *item_id,
                            *amount,
                        );
                    }
                    append_method_push(
                        pre_pushes,
                        "supportfleet.SupportFleetInfo",
                        support_info_payload(account),
                    );
                    if !settlement.base_rewards.is_empty() || !settlement.random_rewards.is_empty()
                    {
                        append_method_push(
                            pre_pushes,
                            "user.UpdateUserInfo",
                            UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                        );
                        append_method_push(
                            pre_pushes,
                            "bag.UpdateBagData",
                            BagInfoCodec::encode(&bag_info_from_account(account)),
                        );
                        append_method_push(
                            pre_pushes,
                            "hero.UpdateHeroBagData",
                            HeroBagCodec::encode(&hero_bag_from_account(account)),
                        );
                    }
                    return Some(encode_support_settlement(&settlement));
                } else {
                    *handler_error = Some(GameError::Internal(
                        "support entry is not ready or invalid".to_owned(),
                    ));
                }
            }
            Some(Vec::new())
        }
        "milestone.GetMilestone" => Some(milestone_info_payload(
            account.as_deref().unwrap_or(&Value::Null),
        )),
        "milestone.FetchReward" => {
            let activity_id = decode_varint_field(request_args, 1);
            let index = decode_varint_field(request_args, 2);
            if activity_id <= 0 || index <= 0 {
                *handler_error = Some(GameError::Internal(
                    "milestone request is invalid".to_owned(),
                ));
            } else if let Some(account) = account.as_deref_mut() {
                let claimed = account
                    .as_object_mut()
                    .map(|root| {
                        root.entry("milestone".to_owned())
                            .or_insert_with(|| json!({"claimed": []}))
                    })
                    .and_then(|milestone| milestone.get_mut("claimed"))
                    .and_then(Value::as_array_mut);
                if let Some(claimed) = claimed {
                    let key = format!("{activity_id}:{index}");
                    if !claimed.iter().any(|value| value.as_str() == Some(&key)) {
                        claimed.push(Value::String(key));
                    }
                }
            }
            Some(Vec::new())
        }
        "supply.SupplySwitch" => {
            if let Some(account) = account.as_deref_mut() {
                let ids = decode_repeated_varint_field(request_args, 1);
                account["supplyHeroIds"] = json!(ids);
                append_method_push(
                    pre_pushes,
                    "user.UpdateUserInfo",
                    UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                );
            }
            Some(Vec::new())
        }
        "guide.PlotReward" => {
            let plot_id = decode_varint_field(request_args, 1);
            if let Some(account) = account.as_deref_mut() {
                let rewards = account
                    .as_object_mut()
                    .map(|root| {
                        root.entry("guide".to_owned())
                            .or_insert_with(|| json!({"plotRewards": []}))
                    })
                    .and_then(|guide| guide.get_mut("plotRewards"))
                    .and_then(Value::as_array_mut);
                if let Some(rewards) = rewards {
                    if plot_id > 0
                        && !rewards
                            .iter()
                            .any(|value| value.as_i64() == Some(i64::from(plot_id)))
                    {
                        rewards.push(json!(plot_id));
                    }
                }
            }
            let mut ret = Vec::new();
            if plot_id > 0 {
                append_varint_field(&mut ret, 1, plot_id as u64);
            }
            Some(ret)
        }
        "guide.Setting" => Some(guide_setting_payload(account, request_args)),
        "user.GetSupply" => {
            let id = decode_varint_field(request_args, 1);
            let mut ret = Vec::new();
            append_varint_field(&mut ret, 1, id.max(0) as u64);
            append_varint_field(&mut ret, 2, 0);
            Some(ret)
        }
        "user.SetMiniGameScore" => {
            let chapter_id = decode_varint_field(request_args, 1);
            let score_entries = decode_repeated_message_field(request_args, 3);
            if chapter_id <= 0 || score_entries.is_empty() {
                *handler_error = Some(GameError::Internal(
                    "mini-game score request is invalid".to_owned(),
                ));
                Some(Vec::new())
            } else if let Some(account) = account.as_deref_mut() {
                let now = current_unix_seconds().min(i32::MAX as u32) as i32;
                for entry in score_entries {
                    let copy_id = decode_varint_field(&entry, 1);
                    let score = decode_varint_field(&entry, 2).max(0);
                    if copy_id <= 0 {
                        continue;
                    }
                    set_mini_game_score(account, chapter_id, copy_id, score, now);
                }
                let total = mini_game_chapter_score(account, chapter_id);
                Some(mini_game_score_response(total, now))
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "user.GetMiniGameScore" => {
            let chapter_id = decode_varint_field(request_args, 1);
            if chapter_id <= 0 {
                *handler_error = Some(GameError::Internal(
                    "mini-game chapter is invalid".to_owned(),
                ));
                Some(Vec::new())
            } else {
                Some(mini_game_score_response(
                    account
                        .as_deref()
                        .map(|value| mini_game_chapter_score(value, chapter_id))
                        .unwrap_or_default(),
                    current_unix_seconds().min(i32::MAX as u32) as i32,
                ))
            }
        }
        "user.GetMiniGameScoreRank" => {
            let chapter_id = decode_varint_field(request_args, 1);
            if chapter_id <= 0 {
                *handler_error = Some(GameError::Internal(
                    "mini-game chapter is invalid".to_owned(),
                ));
                Some(Vec::new())
            } else {
                let account_value = account.as_deref().unwrap_or(&Value::Null);
                let score = mini_game_chapter_score(account_value, chapter_id);
                let rank = if score > 0 { 1 } else { 0 };
                Some(mini_game_rank_response(
                    state,
                    account_value,
                    score,
                    rank,
                    decode_varint_field(request_args, 2),
                    decode_varint_field(request_args, 3),
                ))
            }
        }
        "user.BuyGold" | "user.BuySupply" | "user.BuyPvePt" => {
            if let Some(account) = account.as_deref_mut() {
                if buy_resource(account, method, current_unix_seconds()).is_some() {
                    append_method_push(
                        pre_pushes,
                        "user.UpdateUserInfo",
                        UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                    );
                } else {
                    *handler_error = Some(GameError::Internal(
                        "resource purchase is unavailable".to_owned(),
                    ));
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
            }
            Some(Vec::new())
        }
        "usersvr.GetOtherInfo" => Some(other_user_payload(
            state,
            account.as_deref().unwrap_or(&Value::Null),
            decode_varint_u64_field(request_args, 1),
        )),
        "user.Logoff" => {
            if let Some(account) = account.as_deref_mut() {
                account["lastLogoffTime"] = json!(current_unix_seconds());
            }
            Some(Vec::new())
        }
        "user.SetUserOrderRecord" => {
            if let Some(account) = account.as_deref_mut() {
                account["orderRecord"] = json!({
                    "type": decode_varint_field(request_args, 1),
                    "sort": decode_varint_field(request_args, 2),
                    "screen": decode_varint_field(request_args, 3),
                    "order": decode_varint_field(request_args, 4),
                    "otherInfo": decode_repeated_varint_field(request_args, 5),
                    "time": current_unix_seconds(),
                });
            }
            Some(Vec::new())
        }
        "user.Refresh" => {
            if let Some(account) = account.as_deref_mut() {
                account["refresh"] = json!({
                    "maxPowerIndex": decode_varint_field(request_args, 2),
                    "minPowerIndex": decode_varint_field(request_args, 3),
                    "time": current_unix_seconds(),
                });
            }
            Some(Vec::new())
        }
        "user.KickInfo" => Some(Vec::new()),
        "user.InitQueueInfo" => Some(queue_init_payload(
            account.as_deref().unwrap_or(&Value::Null),
        )),
        "user.UpdateQueueInfo" => Some(queue_update_payload(
            account.as_deref().unwrap_or(&Value::Null),
        )),
        "user.MedalReplaceReward" => Some(medal_replace_reward_payload(
            account.as_deref().unwrap_or(&Value::Null),
        )),
        "user.TeacherRank" => Some(teacher_rank_payload(
            state,
            account.as_deref().unwrap_or(&Value::Null),
            decode_varint_field(request_args, 1),
            decode_varint_field(request_args, 2),
        )),
        _ => None,
    }
}

#[derive(Clone)]
struct TeacherRankEntry {
    uid: u64,
    name: String,
    level: u32,
    head: u32,
    head_frame: u32,
    prestige: u64,
}

fn teacher_rank_payload(state: &ServerState, current: &Value, begin: i32, offset: i32) -> Vec<u8> {
    let mut entries = state
        .social_store
        .as_ref()
        .and_then(|store| store.list_typed_accounts().ok())
        .unwrap_or_default()
        .into_iter()
        .map(|account| TeacherRankEntry {
            uid: account.character.uid,
            name: account.character.name,
            level: account.character.level,
            head: account.character.head,
            head_frame: account.character.head_frame,
            prestige: account
                .activities
                .progress
                .get("teacher\u{1f}prestige")
                .copied()
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    let current_character = current.get("character").unwrap_or(&Value::Null);
    let current_entry = TeacherRankEntry {
        uid: json_u64(current_character, "uid").unwrap_or(1),
        name: json_string(current_character, "name").unwrap_or_else(|| state.name.clone()),
        level: json_i32(current_character, "level")
            .unwrap_or(state.level)
            .max(0) as u32,
        head: json_i32(current_character, "head")
            .unwrap_or(1021051)
            .max(0) as u32,
        head_frame: json_i32(current_character, "headFrame")
            .unwrap_or_default()
            .max(0) as u32,
        prestige: json_i32(current_character, "teacherPrestige")
            .unwrap_or_default()
            .max(0) as u64,
    };
    if let Some(existing) = entries
        .iter_mut()
        .find(|entry| entry.uid == current_entry.uid)
    {
        *existing = current_entry;
    } else {
        entries.push(current_entry);
    }
    entries.sort_by(|left, right| {
        right
            .prestige
            .cmp(&left.prestige)
            .then_with(|| left.uid.cmp(&right.uid))
    });

    let start = usize::try_from(begin.saturating_sub(1)).unwrap_or_default();
    let limit = usize::try_from(offset.max(1)).unwrap_or(50).min(50);
    let mut output = Vec::new();
    for entry in entries.iter().skip(start).take(limit) {
        append_message_field(&mut output, 1, &teacher_simple_user_payload(entry));
    }
    output
}

fn teacher_rank_payload_typed(
    state: &ServerState,
    current: &blueoath_domain::AccountState,
    begin: i32,
    offset: i32,
) -> Vec<u8> {
    let mut entries = state
        .social_store
        .as_ref()
        .and_then(|store| store.list_typed_accounts().ok())
        .unwrap_or_default()
        .into_iter()
        .map(|account| TeacherRankEntry {
            uid: account.character.uid,
            name: account.character.name,
            level: account.character.level,
            head: account.character.head,
            head_frame: account.character.head_frame,
            prestige: account
                .activities
                .progress
                .get("teacher\u{1f}prestige")
                .copied()
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    let current_entry = TeacherRankEntry {
        uid: current.character.uid,
        name: current.character.name.clone(),
        level: current.character.level,
        head: current.character.head,
        head_frame: current.character.head_frame,
        prestige: current
            .activities
            .progress
            .get("teacher\u{1f}prestige")
            .copied()
            .unwrap_or_default(),
    };
    if let Some(existing) = entries
        .iter_mut()
        .find(|entry| entry.uid == current_entry.uid)
    {
        *existing = current_entry;
    } else {
        entries.push(current_entry);
    }
    entries.sort_by(|left, right| {
        right
            .prestige
            .cmp(&left.prestige)
            .then_with(|| left.uid.cmp(&right.uid))
    });

    let start = usize::try_from(begin.saturating_sub(1)).unwrap_or_default();
    let limit = usize::try_from(offset.max(1)).unwrap_or(50).min(50);
    let mut output = Vec::new();
    for entry in entries.iter().skip(start).take(limit) {
        append_message_field(&mut output, 1, &teacher_simple_user_payload(entry));
    }
    output
}

fn teacher_simple_user_payload(entry: &TeacherRankEntry) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, entry.uid);
    append_varint_field(&mut output, 2, 0);
    append_bytes_field(&mut output, 3, entry.name.as_bytes());
    append_varint_field(&mut output, 4, u64::from(entry.level));
    append_varint_field(&mut output, 5, u64::from(entry.head));
    append_varint_field(&mut output, 6, u64::from(entry.head_frame));
    append_varint_field(&mut output, 7, 0);
    append_varint_field(&mut output, 8, 0);
    append_varint_field(&mut output, 9, 0);
    append_bytes_field(&mut output, 10, b"");
    append_varint_field(&mut output, 11, entry.prestige);
    append_bytes_field(&mut output, 12, b"");
    append_varint_field(&mut output, 13, 0);
    output
}

fn queue_init_payload(account: &Value) -> Vec<u8> {
    let queue = account.get("queue");
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        queue
            .and_then(|value| value.get("queuePos"))
            .and_then(Value::as_i64)
            .unwrap_or_default()
            .max(0) as u64,
    );
    append_varint_field(
        &mut output,
        2,
        queue
            .and_then(|value| value.get("queueLen"))
            .and_then(Value::as_i64)
            .unwrap_or_default()
            .max(0) as u64,
    );
    append_varint_field(
        &mut output,
        3,
        queue
            .and_then(|value| value.get("selfPos"))
            .and_then(Value::as_i64)
            .unwrap_or_default()
            .max(0) as u64,
    );
    append_varint_field(&mut output, 4, 1);
    output
}

fn queue_update_payload(account: &Value) -> Vec<u8> {
    let queue = account.get("queue");
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        queue
            .and_then(|value| value.get("queuePos"))
            .and_then(Value::as_i64)
            .unwrap_or_default()
            .max(0) as u64,
    );
    append_varint_field(
        &mut output,
        2,
        queue
            .and_then(|value| value.get("queueLen"))
            .and_then(Value::as_i64)
            .unwrap_or_default()
            .max(0) as u64,
    );
    append_varint_field(&mut output, 3, 1);
    output
}

fn medal_replace_reward_payload(account: &Value) -> Vec<u8> {
    let rewards = account.get("medalReplaceRewards").and_then(Value::as_array);
    let mut output = Vec::new();
    for reward in rewards.into_iter().flatten() {
        let mut common = Vec::new();
        append_varint_field(
            &mut common,
            1,
            json_i32(reward, "type").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut common,
            2,
            json_i32(reward, "configId").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut common,
            3,
            json_i32(reward, "num").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut common,
            4,
            json_i32(reward, "id").unwrap_or_default().max(0) as u64,
        );
        let mut wrapper = Vec::new();
        append_message_field(&mut wrapper, 1, &common);
        append_message_field(&mut output, 1, &wrapper);
    }
    output
}

fn set_mini_game_score(account: &mut Value, chapter_id: i32, copy_id: i32, score: i32, now: i32) {
    let scores = account
        .as_object_mut()
        .map(|root| {
            root.entry("miniGameScores".to_owned())
                .or_insert_with(|| json!([]))
        })
        .and_then(Value::as_array_mut);
    let Some(scores) = scores else {
        return;
    };
    if let Some(entry) = scores.iter_mut().find(|entry| {
        json_i32(entry, "chapterId") == Some(chapter_id)
            && json_i32(entry, "copyId") == Some(copy_id)
    }) {
        let previous = json_i32(entry, "score").unwrap_or_default();
        if score > previous {
            entry["score"] = json!(score);
            entry["time"] = json!(now);
        }
    } else {
        scores.push(json!({
            "chapterId": chapter_id,
            "copyId": copy_id,
            "score": score,
            "time": now,
        }));
    }
}

fn mini_game_chapter_score(account: &Value, chapter_id: i32) -> i32 {
    account
        .get("miniGameScores")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| json_i32(entry, "chapterId") == Some(chapter_id))
        .filter_map(|entry| json_i32(entry, "score"))
        .filter(|score| *score > 0)
        .fold(0_i32, i32::saturating_add)
}

fn mini_game_score_response(score: i32, now: i32) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, score.max(0) as u64);
    append_varint_field(&mut output, 2, now.max(0) as u64);
    output
}

fn mini_game_rank_response(
    state: &ServerState,
    account: &Value,
    score: i32,
    rank: i32,
    start: i32,
    end: i32,
) -> Vec<u8> {
    let include = score > 0 && (start <= 0 || end <= 0 || (1 >= start && 1 <= end));
    let mut rank_data = Vec::new();
    append_varint_field(
        &mut rank_data,
        1,
        json_i64(account.get("character").unwrap_or(account), "uid")
            .unwrap_or_default()
            .max(0) as u64,
    );
    append_varint_field(&mut rank_data, 2, rank.max(0) as u64);
    append_message_field(&mut rank_data, 3, &mini_game_simple_user(state, account));
    append_varint_field(&mut rank_data, 4, score.max(0) as u64);
    append_varint_field(
        &mut rank_data,
        5,
        current_unix_seconds().min(i32::MAX as u32) as u64,
    );

    let mut output = Vec::new();
    if include {
        append_message_field(&mut output, 1, &rank_data);
    }
    if score > 0 {
        append_message_field(&mut output, 2, &rank_data);
    }
    output
}

fn mini_game_simple_user(state: &ServerState, account: &Value) -> Vec<u8> {
    let character = account.get("character").unwrap_or(account);
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        json_i64(character, "uid").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(&mut output, 2, 1);
    append_bytes_field(
        &mut output,
        3,
        json_string(character, "name")
            .unwrap_or_default()
            .as_bytes(),
    );
    append_varint_field(
        &mut output,
        4,
        json_i32(character, "level").unwrap_or(state.level).max(0) as u64,
    );
    append_varint_field(
        &mut output,
        5,
        json_i32(character, "head").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(
        &mut output,
        6,
        json_i32(character, "headFrame").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(
        &mut output,
        7,
        json_i32(character, "headShow").unwrap_or_default().max(0) as u64,
    );
    output
}

fn buy_resource(account: &mut Value, method: &str, now: u32) -> Option<()> {
    let (currency, amount, diamond_cost, count_key, time_key) = match method {
        "user.BuyGold" => (1, 1_000, 10, "buyGoldNum", "buyGoldTime"),
        "user.BuySupply" => (5, 100, 10, "buySupplyNum", "buySupplyTime"),
        "user.BuyPvePt" => (30, 10, 10, "buyPvePtNum", "buyPvePtTime"),
        _ => return None,
    };
    if character_i64(account, "diamond") < i64::from(diamond_cost) {
        return None;
    }
    adjust_character_i64(account, "diamond", -i64::from(diamond_cost));
    if let Some(key) = currency_character_key(currency) {
        add_character_i64(account, key, amount);
    }
    let count = character_i64(account, count_key).saturating_add(1);
    set_character_i64(
        account,
        count_key,
        count.clamp(0, i64::from(i32::MAX)) as i32,
    );
    set_character_i64(account, time_key, now.min(i32::MAX as u32) as i32);
    Some(())
}

fn jopen_payload(account: &Value) -> Vec<u8> {
    let state = account.get("jopen").unwrap_or(&Value::Null);
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        json_i32(state, "fetchHeroTime").unwrap_or_default().max(0) as u64,
    );
    append_varint_field(
        &mut output,
        2,
        json_i32(state, "fetchEquipTime").unwrap_or_default().max(0) as u64,
    );
    output
}

fn apply_support_reward(
    account: &mut Value,
    hero_ids: &[u64],
    hero_level_catalog: Option<&HeroLevelCatalog>,
    goods_type: i32,
    item_id: i32,
    amount: i32,
) {
    if amount <= 0 || item_id <= 0 {
        return;
    }
    if goods_type == 5 && item_id == 6 {
        add_ship_battle_exp(account, hero_ids, amount, hero_level_catalog);
    } else if goods_type == 5 {
        if let Some(key) = currency_character_key(item_id) {
            add_character_i64(account, key, amount);
        }
    } else if goods_type == 1 || goods_type == 6 {
        add_bag_item(account, item_id, amount);
    }
}

fn encode_support_settlement(settlement: &SupportSettlement) -> Vec<u8> {
    let mut output = Vec::new();
    for (field, rewards) in [
        (1, &settlement.base_rewards),
        (2, &settlement.random_rewards),
    ] {
        for (goods_type, item_id, amount) in rewards {
            let mut reward = Vec::new();
            append_varint_field(&mut reward, 1, (*goods_type).max(0) as u64);
            append_varint_field(&mut reward, 2, (*item_id).max(0) as u64);
            append_varint_field(&mut reward, 3, (*amount).max(0) as u64);
            append_message_field(&mut output, field, &reward);
        }
    }
    append_varint_field(&mut output, 3, settlement.reward_type.max(0) as u64);
    output
}

pub(super) fn milestone_info_payload(account: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    let Some(claimed) = account
        .get("milestone")
        .and_then(|value| value.get("claimed"))
        .and_then(Value::as_array)
    else {
        return output;
    };
    let mut by_activity = std::collections::BTreeMap::<i32, Vec<i32>>::new();
    for value in claimed.iter().filter_map(Value::as_str) {
        let mut split = value.split(':');
        let Some(activity_id) = split.next().and_then(|part| part.parse::<i32>().ok()) else {
            continue;
        };
        let Some(index) = split.next().and_then(|part| part.parse::<i32>().ok()) else {
            continue;
        };
        by_activity.entry(activity_id).or_default().push(index);
    }
    for (activity_id, indexes) in by_activity {
        let mut sub = Vec::new();
        append_varint_field(&mut sub, 1, activity_id.max(0) as u64);
        for index in indexes {
            let mut reward = Vec::new();
            append_varint_field(&mut reward, 1, index.max(0) as u64);
            append_varint_field(&mut reward, 2, 1);
            append_message_field(&mut sub, 2, &reward);
        }
        append_message_field(&mut output, 1, &sub);
    }
    output
}

fn guide_setting_payload(account: &mut Option<&mut Value>, args: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    for nested in decode_repeated_message_field(args, 1) {
        let key = decode_string_field(&nested, 1).unwrap_or_default();
        let value = decode_string_field(&nested, 2).unwrap_or_default();
        if key.is_empty() {
            continue;
        }
        if let Some(account) = account.as_deref_mut() {
            let settings = account
                .as_object_mut()
                .map(|root| {
                    root.entry("guide".to_owned())
                        .or_insert_with(|| json!({"settings": {}}))
                })
                .and_then(|guide| guide.get_mut("settings"))
                .and_then(Value::as_object_mut);
            if let Some(settings) = settings {
                settings.insert(key.clone(), Value::String(value.clone()));
            }
        }
        let mut setting = Vec::new();
        append_bytes_field(&mut setting, 1, key.as_bytes());
        append_bytes_field(&mut setting, 2, value.as_bytes());
        append_message_field(&mut output, 3, &setting);
    }
    output
}

pub(super) fn other_user_payload(
    state: &ServerState,
    account: &Value,
    requested_uid: u64,
) -> Vec<u8> {
    let typed_account = (requested_uid > 0)
        .then(|| state.social_store.as_ref()?.list_typed_accounts().ok())
        .flatten()
        .and_then(|accounts| {
            accounts
                .into_iter()
                .find(|account| account.character.uid == requested_uid)
        });
    let character = account.get("character").unwrap_or(&Value::Null);
    let uid = typed_account
        .as_ref()
        .map(|account| account.character.uid)
        .unwrap_or_else(|| {
            if requested_uid > 0 {
                requested_uid
            } else {
                json_u64(character, "uid").unwrap_or(1)
            }
        });
    let name = typed_account
        .as_ref()
        .map(|account| account.character.name.clone())
        .unwrap_or_else(|| json_string(character, "name").unwrap_or_else(|| state.name.clone()));
    let head = typed_account
        .as_ref()
        .map(|account| account.character.head)
        .unwrap_or_else(|| json_i32(character, "head").unwrap_or(1021051).max(0) as u32);
    let level = typed_account
        .as_ref()
        .map(|account| account.character.level)
        .unwrap_or_else(|| json_i32(character, "level").unwrap_or(1).max(0) as u32);
    let secretary_id = typed_account
        .as_ref()
        .and_then(|account| account.character.secretary_id.map(|id| id.get()))
        .unwrap_or_else(|| json_i32(character, "secretaryId").unwrap_or(1).max(0) as u64);
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, uid);
    append_bytes_field(&mut output, 2, name.as_bytes());
    append_varint_field(&mut output, 3, u64::from(head));
    append_varint_field(&mut output, 5, u64::from(level));
    append_varint_field(&mut output, 10, secretary_id);
    output
}

pub(super) fn other_user_payload_typed(
    state: &ServerState,
    current: &blueoath_domain::AccountState,
    requested_uid: u64,
) -> Vec<u8> {
    let typed_account = (requested_uid > 0)
        .then(|| state.social_store.as_ref()?.list_typed_accounts().ok())
        .flatten()
        .and_then(|accounts| {
            accounts
                .into_iter()
                .find(|account| account.character.uid == requested_uid)
        });
    let character = typed_account
        .as_ref()
        .map(|account| &account.character)
        .unwrap_or(&current.character);
    let uid = typed_account
        .as_ref()
        .map(|account| account.character.uid)
        .unwrap_or(if requested_uid > 0 {
            requested_uid
        } else {
            current.character.uid
        });
    let secretary_id = character.secretary_id.map(|id| id.get()).unwrap_or(1);
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, uid);
    append_bytes_field(&mut output, 2, character.name.as_bytes());
    append_varint_field(&mut output, 3, u64::from(character.head));
    append_varint_field(&mut output, 5, u64::from(character.level));
    append_varint_field(&mut output, 10, secretary_id);
    output
}

#[cfg(test)]
mod tests {
    use crate::common::response::HandlerResult;

    use super::*;

    #[test]
    fn handler_exposes_typed_result() {
        let _: for<'state, 'account, 'scratch> fn(
            &mut GameLoginRequestContext<'state, 'account, 'scratch>,
            &str,
            &[u8],
        ) -> HandlerResult = handle;
    }

    #[test]
    fn mini_game_scores_are_idempotent_and_sum_by_chapter() {
        let mut account = json!({"miniGameScores": []});
        set_mini_game_score(&mut account, 7, 101, 120, 10);
        set_mini_game_score(&mut account, 7, 102, 80, 11);
        set_mini_game_score(&mut account, 7, 101, 60, 12);
        assert_eq!(mini_game_chapter_score(&account, 7), 200);
        assert_eq!(json_i32(&account["miniGameScores"][0], "time"), Some(10));
    }

    #[test]
    fn buying_resource_updates_currency_and_daily_counter() {
        let mut account = json!({"character": {"diamond": 100}});
        assert!(buy_resource(&mut account, "user.BuyGold", 42).is_some());
        assert_eq!(character_i64(&account, "diamond"), 90);
        assert_eq!(character_i64(&account, "gold"), 1_000);
        assert_eq!(character_i64(&account, "buyGoldNum"), 1);
        assert_eq!(character_i64(&account, "buyGoldTime"), 42);
    }

    #[test]
    fn typed_buy_resource_updates_domain_ledger() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("typed-buy-resource").unwrap(),
            "Captain",
        );
        account
            .resources
            .credit(blueoath_domain::CurrencyKind::Diamond, 20)
            .unwrap();
        let state = ServerState::new("typed-buy-resource", "Captain", "1.0.0");
        let mut pushes = Vec::new();
        let result = handle_typed(&mut account, &state, "user.BuyGold", &[], &mut pushes);
        assert!(matches!(result, HandlerResult::PushOnly));
        assert_eq!(
            account
                .resources
                .amount(blueoath_domain::CurrencyKind::Diamond)
                .get(),
            10_010
        );
        assert_eq!(
            account
                .resources
                .amount(blueoath_domain::CurrencyKind::Gold)
                .get(),
            1_000
        );
        assert_eq!(pushes.len(), 1);
    }

    #[test]
    fn typed_social_queries_do_not_require_json_account() {
        let root = std::env::temp_dir().join(format!(
            "blueoath-typed-social-{}-{}",
            std::process::id(),
            current_unix_millis()
        ));
        let store = blueoath_storage::ProfileStore::open(&root).unwrap();
        let mut friend = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("friend").unwrap(),
            "Friend",
        );
        friend.character.uid = 42;
        friend.character.head = 7;
        friend.character.level = 9;
        friend.character.secretary_id = Some(blueoath_domain::HeroId::new(3).unwrap());
        store.save_typed_account(&mut friend).unwrap();

        let mut current = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("current").unwrap(),
            "Captain",
        );
        current.character.uid = 1;
        current
            .activities
            .progress
            .insert("teacher\u{1f}prestige".to_owned(), 123);
        let mut state = ServerState::new("current", "Captain", "1.4.0");
        state.social_store = Some(store);
        let mut pushes = Vec::new();

        let result = handle_typed(
            &mut current,
            &state,
            "usersvr.GetOtherInfo",
            &[],
            &mut pushes,
        );
        let HandlerResult::Reply(response) = result else {
            panic!("expected typed other-user response");
        };
        assert_eq!(decode_varint_field(&response.payload, 1), 1);

        let mut request = Vec::new();
        append_varint_field(&mut request, 1, 1);
        append_varint_field(&mut request, 2, 10);
        let result = handle_typed(
            &mut current,
            &state,
            "user.TeacherRank",
            &request,
            &mut pushes,
        );
        let HandlerResult::Reply(response) = result else {
            panic!("expected typed teacher rank response");
        };
        let rows = decode_repeated_message_field(&response.payload, 1);
        assert_eq!(decode_varint_field(&rows[0], 1), 1);
        assert_eq!(decode_varint_field(&rows[0], 11), 123);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn other_user_payload_reads_matching_account_directory() {
        let root = std::env::temp_dir().join(format!(
            "blueoath-social-{}-{}",
            std::process::id(),
            current_unix_millis()
        ));
        let store = blueoath_storage::ProfileStore::open(&root).unwrap();
        let mut friend = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("friend").unwrap(),
            "Friend",
        );
        friend.character.uid = 42;
        friend.character.head = 7;
        friend.character.level = 9;
        friend.character.secretary_id = Some(blueoath_domain::HeroId::new(3).unwrap());
        store.save_typed_account(&mut friend).unwrap();
        let mut state = ServerState::new("local", "Local", "1.4.0");
        state.social_store = Some(store.clone());
        let own = json!({"character": {"uid": 1, "name": "Local"}});

        let payload = other_user_payload(&state, &own, 42);
        assert_eq!(decode_varint_field(&payload, 1), 42);
        assert_eq!(decode_string_field(&payload, 2).as_deref(), Some("Friend"));
        assert_eq!(decode_varint_field(&payload, 3), 7);
        assert_eq!(decode_varint_field(&payload, 5), 9);
        assert_eq!(decode_varint_field(&payload, 10), 3);
        let _ = std::fs::remove_dir_all(root);
    }
}
