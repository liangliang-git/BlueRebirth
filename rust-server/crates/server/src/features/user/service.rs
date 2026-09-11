#![allow(dead_code)]

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;
use crate::features::user::requests::UserRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
struct SupportSettlement {
    reward_type: i32,
    hero_ids: Vec<u64>,
    base_rewards: Vec<(i32, i32, i32)>,
    random_rewards: Vec<(i32, i32, i32)>,
}

pub(crate) fn apply_user_request(
    account: &mut blueoath_domain::AccountState,
    request: UserRequest,
) -> Result<(), GameError> {
    match request {
        UserRequest::SetSecretary(request) => {
            account.character.secretary_id = u64::try_from(request.secretary_id)
                .ok()
                .and_then(|id| blueoath_domain::HeroId::new(id).ok());
        }
        UserRequest::ChangeName(request) => account.character.name = request.name,
        UserRequest::SetMessage(request) => account.character.message = request.message,
        UserRequest::SetHeadFrame(request) => {
            account.character.head_frame = request.head_frame.max(0) as u32;
        }
        UserRequest::SetHead(request) => account.character.head = request.head.max(0) as u32,
    }
    Ok(())
}

pub(crate) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    state: &ServerState,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
    match method {
        "strategy.GetStrategy" => {
            HandlerResult::Reply(Response::raw(method, typed_strategy_info_payload(account)))
        }
        "strategy.Learn" | "strategy.Upgrade" => {
            let Ok(request) = StrategyLearnRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "strategy request is invalid",
                ));
            };
            let strategy_id = request.strategy_id;
            let level = i64::from(request.level).max(1) as u64;
            account
                .activities
                .progress
                .insert(format!("compat:strategy:{strategy_id}:level"), level);
            effects.push_pre(Response::raw(
                "strategy.GetStrategy",
                typed_strategy_info_payload(account),
            ));
            HandlerResult::PushOnly
        }
        "strategy.Reset" => {
            account.activities.progress.retain(|key, _| {
                !key.starts_with("compat:strategy:") || key == "compat:strategy:resetNum"
            });
            let reset_num = account
                .activities
                .progress
                .get("compat:strategy:resetNum")
                .copied()
                .unwrap_or_default()
                .saturating_add(1);
            account
                .activities
                .progress
                .insert("compat:strategy:resetNum".to_owned(), reset_num);
            effects.push_pre(Response::raw(
                "strategy.GetStrategy",
                typed_strategy_info_payload(account),
            ));
            HandlerResult::PushOnly
        }
        "strategy.Apply" => {
            let Ok(request) = StrategyApplyRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "strategy apply request is invalid",
                ));
            };
            let Ok(fleet_id) = blueoath_domain::FleetId::new(request.fleet_id as u64) else {
                return HandlerResult::Error(GameError::InvalidRequest("fleet id is invalid"));
            };
            let Some(fleet) = account.fleet.fleets.get_mut(&fleet_id) else {
                return HandlerResult::Error(GameError::NotFound("fleet"));
            };
            if fleet.tactic_type.max(1) != u32::try_from(request.tactic_type).unwrap_or_default() {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "strategy tactic type does not match fleet",
                ));
            }
            fleet.tactic_id = request.strategy_id as u32;
            effects.push_pre(Response::raw(
                "tactic.GetHerosTactic",
                FleetInfoCodec::encode(&fleet_info_from_typed_account(account)),
            ));
            effects.push_pre(Response::raw(
                "strategy.GetStrategy",
                typed_strategy_info_payload(account),
            ));
            HandlerResult::PushOnly
        }
        "supportfleet.SupportFleetInfo" => {
            HandlerResult::Reply(Response::raw(method, typed_support_info_payload(account)))
        }
        "supportfleet.StartSupport" => {
            let Ok(request) = SupportStartRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "support request is invalid",
                ));
            };
            let support_id = request.support_id;
            let hero_ids = request.hero_ids;
            if support_id <= 0
                || hero_ids.is_empty()
                || hero_ids
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != hero_ids.len()
                || hero_ids.iter().any(|id| {
                    blueoath_domain::HeroId::new(*id)
                        .ok()
                        .is_none_or(|hero_id| !account.dock.heroes.contains_key(&hero_id))
                })
                || (SUPPORT_CATALOG
                    .get()
                    .is_some_and(|catalog| !catalog.items.is_empty())
                    && !SUPPORT_CATALOG
                        .get()
                        .is_some_and(|catalog| catalog.items.contains_key(&support_id)))
            {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "support request is invalid",
                ));
            }
            let id = account
                .support
                .entries
                .iter()
                .map(|entry| entry.id)
                .max()
                .unwrap_or_default()
                .saturating_add(1);
            account
                .support
                .entries
                .push(blueoath_domain::SupportEntryState {
                    id,
                    support_id: support_id as u32,
                    start_time: u64::from(current_unix_seconds()),
                    hero_ids: hero_ids
                        .into_iter()
                        .filter_map(|hero_id| blueoath_domain::HeroId::new(hero_id).ok())
                        .collect(),
                });
            effects.push_pre(Response::raw(
                "supportfleet.SupportFleetInfo",
                typed_support_info_payload(account),
            ));
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "supportfleet.CompleteSupport" | "supportfleet.CancelSupport" => {
            let Ok(request) = SupportCompleteRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "support completion request is invalid",
                ));
            };
            let id = request.id;
            let completion_type = request.completion_type;
            let Some(entry) = account
                .support
                .entries
                .iter()
                .find(|entry| entry.id == id)
                .cloned()
            else {
                return HandlerResult::Error(GameError::InvalidState(
                    "support entry is not ready or invalid",
                ));
            };
            if !matches!(completion_type, 1..=3) {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "support completion type is invalid",
                ));
            }
            let catalog = SUPPORT_CATALOG.get().cloned().unwrap_or_default();
            let config = catalog
                .items
                .get(&(entry.support_id as i32))
                .cloned()
                .unwrap_or_default();
            let now = current_unix_seconds();
            let elapsed = u64::from(now).saturating_sub(entry.start_time);
            if completion_type == 1 && elapsed < config.duration_seconds.max(0) as u64 {
                return HandlerResult::Error(GameError::InvalidState(
                    "support entry is not ready or invalid",
                ));
            }
            let big_success = completion_type != 3
                && config.big_success_ratio > 0
                && (u64::from(now).saturating_add(u64::from(entry.id)) % 10_000)
                    < u64::try_from(config.big_success_ratio).unwrap_or_default();
            let base_rewards = if big_success && !config.big_success_base_rewards.is_empty() {
                config.big_success_base_rewards.clone()
            } else {
                config.base_rewards.clone()
            };
            let mut random_rewards = Vec::new();
            if completion_type != 3 {
                let drop_id = if big_success {
                    config.big_success_extra_drop_id
                } else {
                    config.extra_drop_id
                };
                if let Some(reward) = catalog
                    .drop_rewards
                    .get(&drop_id)
                    .and_then(|rewards| rewards.first())
                {
                    random_rewards.push(*reward);
                }
            }
            let rewards = base_rewards
                .iter()
                .chain(random_rewards.iter())
                .copied()
                .collect::<Vec<_>>();
            if rewards.iter().any(|(goods_type, item_id, amount)| {
                !typed_support_reward_supported(*goods_type, *item_id, *amount)
            }) {
                return HandlerResult::Error(GameError::InvalidState(
                    "support reward is unsupported",
                ));
            }
            if completion_type == 2 {
                if let Some((goods_type, item_id, amount)) =
                    config.fast_consumption.or(config.consumption)
                {
                    if !typed_support_cost_available(account, goods_type, item_id, amount) {
                        return HandlerResult::Error(GameError::InsufficientResource(
                            blueoath_domain::CurrencyKind::Gold,
                        ));
                    }
                }
            }
            if completion_type == 2 {
                if let Some((goods_type, item_id, amount)) =
                    config.fast_consumption.or(config.consumption)
                {
                    let _ = typed_support_consume(account, goods_type, item_id, amount);
                }
            }
            for (goods_type, item_id, amount) in &rewards {
                typed_support_grant(account, &entry.hero_ids, *goods_type, *item_id, *amount);
            }
            typed_support_remove(account, id);
            effects.push_pre(Response::raw(
                "supportfleet.SupportFleetInfo",
                typed_support_info_payload(account),
            ));
            if !rewards.is_empty() {
                effects.push_pre(Response::raw(
                    "user.UpdateUserInfo",
                    UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
                ));
                effects.push_pre(Response::raw(
                    "bag.UpdateBagData",
                    BagInfoCodec::encode(&bag_info_from_typed_account(account)),
                ));
                effects.push_pre(Response::raw(
                    "hero.UpdateHeroBagData",
                    HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
                ));
            }
            let settlement = SupportSettlement {
                reward_type: if completion_type == 3 {
                    0
                } else if big_success {
                    2
                } else {
                    1
                },
                hero_ids: entry.hero_ids.iter().map(|hero_id| hero_id.get()).collect(),
                base_rewards: if completion_type == 3 {
                    Vec::new()
                } else {
                    base_rewards
                },
                random_rewards: if completion_type == 3 {
                    Vec::new()
                } else {
                    random_rewards
                },
            };
            HandlerResult::Reply(Response::raw(
                method,
                encode_support_settlement(&settlement),
            ))
        }
        "supply.SupplySwitch" => {
            let Ok(request) = SupplySwitchRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "supply hero list is invalid",
                ));
            };
            let hero_ids = request
                .hero_ids
                .into_iter()
                .filter_map(|id| blueoath_domain::HeroId::new(id).ok())
                .collect::<Vec<_>>();
            if hero_ids.is_empty()
                || hero_ids
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != hero_ids.len()
                || hero_ids
                    .iter()
                    .any(|hero_id| !account.dock.heroes.contains_key(hero_id))
            {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "supply hero list is invalid",
                ));
            }
            account.supply.hero_ids = hero_ids;
            effects.push_pre(Response::raw(
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
            ));
            HandlerResult::PushOnly
        }
        "user.KickInfo"
        | "user.InitQueueInfo"
        | "user.UpdateQueueInfo"
        | "user.MedalReplaceReward" => HandlerResult::Reply(Response::raw(
            method,
            if method == "user.InitQueueInfo" {
                let mut payload = Vec::new();
                append_varint_field(&mut payload, 4, 1);
                payload
            } else {
                Vec::new()
            },
        )),
        "jopen.GetJopen" => {
            HandlerResult::Reply(Response::raw(method, typed_jopen_payload(account)))
        }
        "jopen.FetchHero" | "jopen.FetchEquip" => {
            let key = if method == "jopen.FetchHero" {
                "compat:jopen:fetchHeroTime"
            } else {
                "compat:jopen:fetchEquipTime"
            };
            account
                .activities
                .progress
                .insert(key.to_owned(), u64::from(current_unix_seconds()));
            effects.push_pre(Response::raw(
                "jopen.GetJopen",
                typed_jopen_payload(account),
            ));
            HandlerResult::PushOnly
        }
        "milestone.GetMilestone" => {
            HandlerResult::Reply(Response::raw(method, typed_milestone_info_payload(account)))
        }
        "milestone.FetchReward" => {
            let Ok(request) = MilestoneFetchRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "milestone request is invalid",
                ));
            };
            let activity_id = request.activity_id;
            let index = request.index;
            account
                .activities
                .progress
                .insert(format!("compat:milestone:{activity_id}:{index}"), 1);
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "guide.PlotReward" => {
            let Ok(request) = GuidePlotRewardRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest("guide plot id is invalid"));
            };
            let plot_id = request.plot_id;
            account.guide.plot_rewards.insert(plot_id as u64);
            let mut payload = Vec::new();
            append_varint_field(&mut payload, 1, plot_id as u64);
            HandlerResult::Reply(Response::raw(method, payload))
        }
        "guide.Setting" => {
            let mut payload = Vec::new();
            let Ok(request) = GuideSettingRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "guide setting request is invalid",
                ));
            };
            for entry in request.entries {
                let key = entry.key;
                let value = entry.value;
                tracing::debug!(key = %key, value = %value, "guide setting update");
                account.guide.settings.insert(key.clone(), value.clone());
                let mut setting = Vec::new();
                append_bytes_field(&mut setting, 1, key.as_bytes());
                append_bytes_field(&mut setting, 2, value.as_bytes());
                append_message_field(&mut payload, 3, &setting);
            }
            HandlerResult::Reply(Response::raw(method, payload))
        }
        "user.SetMiniGameScore" => {
            let Ok(request) = UserMiniGameScoreRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "mini-game score request is invalid",
                ));
            };
            let chapter_id = request.chapter_id;
            let now = current_unix_seconds();
            for entry in request.entries {
                let copy_id = entry.copy_id;
                let score = entry.score;
                let key = format!("compat:minigame:{chapter_id}:{copy_id}");
                let current = account
                    .activities
                    .progress
                    .get(&key)
                    .copied()
                    .unwrap_or_default();
                if score > current {
                    account.activities.progress.insert(key, score);
                }
            }
            HandlerResult::Reply(Response::raw(
                method,
                typed_mini_game_score_payload(account, chapter_id, now),
            ))
        }
        "user.GetMiniGameScore" => {
            let Ok(request) = MiniGameChapterRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "mini-game chapter is invalid",
                ));
            };
            HandlerResult::Reply(Response::raw(
                method,
                typed_mini_game_score_payload(account, request.chapter_id, current_unix_seconds()),
            ))
        }
        "user.GetMiniGameScoreRank" => {
            let Ok(request) = MiniGameRankRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "mini-game rank request is invalid",
                ));
            };
            let chapter_id = request.chapter_id;
            let score = typed_mini_game_chapter_score(account, chapter_id);
            HandlerResult::Reply(Response::raw(
                method,
                typed_mini_game_rank_payload(
                    account,
                    score,
                    if score > 0 { 1 } else { 0 },
                    request.start,
                    request.end,
                ),
            ))
        }
        "user.Logoff" => {
            account.activities.progress.insert(
                "compat:user:lastLogoffTime".to_owned(),
                u64::from(current_unix_seconds()),
            );
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "user.SetUserOrderRecord" => {
            let Ok(request) = UserOrderRecordRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "user order request is invalid",
                ));
            };
            for (name, value) in [
                ("type", request.record_type),
                ("sort", request.sort),
                ("screen", request.screen),
                ("order", request.order),
            ] {
                account.activities.progress.insert(
                    format!("compat:user:order:{name}"),
                    u64::try_from(value.max(0)).unwrap_or_default(),
                );
            }
            account.activities.progress.insert(
                "compat:user:order:time".to_owned(),
                u64::from(current_unix_seconds()),
            );
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "user.Refresh" => {
            let Ok(request) = UserRefreshRequest::decode(request_args) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "user refresh request is invalid",
                ));
            };
            account.activities.progress.insert(
                "compat:user:refresh:maxPowerIndex".to_owned(),
                u64::try_from(request.max_power_index.max(0)).unwrap_or_default(),
            );
            account.activities.progress.insert(
                "compat:user:refresh:minPowerIndex".to_owned(),
                u64::try_from(request.min_power_index.max(0)).unwrap_or_default(),
            );
            account.activities.progress.insert(
                "compat:user:refresh:time".to_owned(),
                u64::from(current_unix_seconds()),
            );
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
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
            effects.push_pre(Response::raw(
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
            ));
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

#[derive(Clone)]
struct TeacherRankEntry {
    uid: u64,
    name: String,
    level: u32,
    head: u32,
    head_frame: u32,
    prestige: u64,
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

fn typed_jopen_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        account
            .activities
            .progress
            .get("compat:jopen:fetchHeroTime")
            .copied()
            .unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        2,
        account
            .activities
            .progress
            .get("compat:jopen:fetchEquipTime")
            .copied()
            .unwrap_or_default(),
    );
    output
}

fn typed_support_info_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    for entry in &account.support.entries {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, u64::from(entry.id));
        append_varint_field(&mut item, 2, u64::from(entry.support_id));
        append_varint_field(&mut item, 3, entry.start_time);
        for hero_id in &entry.hero_ids {
            append_varint_field(&mut item, 4, hero_id.get());
        }
        append_message_field(&mut output, 1, &item);
    }
    output
}

fn typed_support_remove(account: &mut blueoath_domain::AccountState, id: u32) {
    account.support.entries.retain(|entry| entry.id != id);
}

fn typed_support_currency(item_id: i32) -> Option<blueoath_domain::CurrencyKind> {
    Some(match item_id {
        1 => blueoath_domain::CurrencyKind::Gold,
        2 => blueoath_domain::CurrencyKind::Diamond,
        5 => blueoath_domain::CurrencyKind::Supply,
        30 => blueoath_domain::CurrencyKind::PvePoint,
        _ => return None,
    })
}

fn typed_support_cost_available(
    account: &blueoath_domain::AccountState,
    goods_type: i32,
    item_id: i32,
    amount: i32,
) -> bool {
    let Ok(amount) = u64::try_from(amount) else {
        return false;
    };
    if goods_type == 5 {
        return typed_support_currency(item_id)
            .is_some_and(|kind| account.resources.amount(kind).get() >= amount);
    }
    matches!(goods_type, 1 | 6)
        && blueoath_domain::TemplateId::new(item_id.max(0) as u64)
            .ok()
            .is_some_and(|template| {
                account
                    .inventory
                    .items
                    .get(&template)
                    .copied()
                    .unwrap_or_default()
                    >= amount
            })
}

fn typed_support_consume(
    account: &mut blueoath_domain::AccountState,
    goods_type: i32,
    item_id: i32,
    amount: i32,
) -> bool {
    let Ok(amount) = u64::try_from(amount) else {
        return false;
    };
    if goods_type == 5 {
        return typed_support_currency(item_id)
            .is_some_and(|kind| account.resources.debit(kind, amount).is_ok());
    }
    let Some(template) = blueoath_domain::TemplateId::new(item_id.max(0) as u64).ok() else {
        return false;
    };
    let Some(current) = account.inventory.items.get_mut(&template) else {
        return false;
    };
    if *current < amount {
        return false;
    }
    *current -= amount;
    if *current == 0 {
        account.inventory.items.remove(&template);
    }
    matches!(goods_type, 1 | 6)
}

fn typed_support_reward_supported(goods_type: i32, item_id: i32, amount: i32) -> bool {
    amount > 0
        && item_id > 0
        && ((goods_type == 5 && (item_id == 6 || typed_support_currency(item_id).is_some()))
            || matches!(goods_type, 1 | 6))
}

fn typed_support_grant(
    account: &mut blueoath_domain::AccountState,
    hero_ids: &[blueoath_domain::HeroId],
    goods_type: i32,
    item_id: i32,
    amount: i32,
) {
    let amount = u64::try_from(amount).unwrap_or_default();
    if goods_type == 5 && item_id == 6 {
        for hero_id in hero_ids {
            if let Some(hero) = account.dock.heroes.get_mut(hero_id) {
                hero.exp = hero.exp.saturating_add(amount);
            }
        }
    } else if goods_type == 5 {
        if let Some(kind) = typed_support_currency(item_id) {
            let _ = account.resources.credit(kind, amount);
        }
    } else if matches!(goods_type, 1 | 6) {
        if let Ok(template) = blueoath_domain::TemplateId::new(item_id as u64) {
            let entry = account.inventory.items.entry(template).or_default();
            *entry = entry.saturating_add(amount);
        }
    }
}

pub(crate) fn typed_strategy_info_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut entries = account
        .activities
        .progress
        .iter()
        .filter_map(|(key, level)| {
            let rest = key.strip_prefix("compat:strategy:")?;
            let mut parts = rest.split(':');
            let id = parts.next()?.parse::<u64>().ok()?;
            if parts.next() != Some("level") || id == 0 {
                return None;
            }
            Some((id, *level))
        })
        .collect::<Vec<_>>();
    entries.sort_unstable_by_key(|(id, _)| *id);
    let mut output = Vec::new();
    for (id, level) in entries {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, id);
        append_varint_field(&mut item, 2, level);
        append_message_field(&mut output, 1, &item);
    }
    append_varint_field(
        &mut output,
        2,
        account
            .activities
            .progress
            .get("compat:strategy:curCost")
            .copied()
            .unwrap_or_default(),
    );
    append_varint_field(
        &mut output,
        3,
        account
            .activities
            .progress
            .get("compat:strategy:resetNum")
            .copied()
            .unwrap_or_default(),
    );
    output
}

fn typed_mini_game_chapter_score(account: &blueoath_domain::AccountState, chapter_id: i32) -> u64 {
    account
        .activities
        .progress
        .iter()
        .filter_map(|(key, score)| {
            let rest = key.strip_prefix("compat:minigame:")?;
            let mut parts = rest.split(':');
            (parts.next()?.parse::<i32>().ok() == Some(chapter_id)).then_some(*score)
        })
        .fold(0_u64, u64::saturating_add)
}

fn typed_mini_game_score_payload(
    account: &blueoath_domain::AccountState,
    chapter_id: i32,
    now: u32,
) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(
        &mut output,
        1,
        typed_mini_game_chapter_score(account, chapter_id),
    );
    append_varint_field(&mut output, 2, u64::from(now));
    output
}

fn typed_mini_game_rank_payload(
    account: &blueoath_domain::AccountState,
    score: u64,
    rank: i32,
    start: i32,
    end: i32,
) -> Vec<u8> {
    let include = score > 0 && (start <= 0 || end <= 0 || (1 >= start && 1 <= end));
    let mut user = Vec::new();
    append_varint_field(&mut user, 1, account.character.uid);
    append_varint_field(&mut user, 2, 1);
    append_bytes_field(&mut user, 3, account.character.name.as_bytes());
    append_varint_field(&mut user, 4, u64::from(account.character.level));

    let mut row = Vec::new();
    append_varint_field(&mut row, 1, account.character.uid);
    append_varint_field(&mut row, 2, rank.max(0) as u64);
    append_message_field(&mut row, 3, &user);
    append_varint_field(&mut row, 4, score);
    append_varint_field(
        &mut row,
        5,
        u64::from(current_unix_seconds().min(i32::MAX as u32)),
    );

    let mut output = Vec::new();
    if include {
        append_message_field(&mut output, 1, &row);
    }
    if score > 0 {
        append_message_field(&mut output, 2, &row);
    }
    output
}

fn typed_milestone_info_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
    let mut grouped = std::collections::BTreeMap::<u64, Vec<u64>>::new();
    for key in account.activities.progress.keys() {
        let Some(rest) = key.strip_prefix("compat:milestone:") else {
            continue;
        };
        let mut parts = rest.split(':');
        let (Some(activity_id), Some(index)) = (
            parts.next().and_then(|value| value.parse::<u64>().ok()),
            parts.next().and_then(|value| value.parse::<u64>().ok()),
        ) else {
            continue;
        };
        grouped.entry(activity_id).or_default().push(index);
    }
    let mut output = Vec::new();
    for (activity_id, indexes) in grouped {
        let mut activity = Vec::new();
        append_varint_field(&mut activity, 1, activity_id);
        for index in indexes {
            let mut reward = Vec::new();
            append_varint_field(&mut reward, 1, index);
            append_varint_field(&mut reward, 2, 1);
            append_message_field(&mut activity, 2, &reward);
        }
        append_message_field(&mut output, 1, &activity);
    }
    output
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

pub(crate) fn other_user_payload_typed(
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
