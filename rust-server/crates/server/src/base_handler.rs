#![allow(dead_code)]

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
        "strategy.GetStrategy" => {
            HandlerResult::Reply(Response::raw(method, typed_strategy_info_payload(account)))
        }
        "strategy.Learn" | "strategy.Upgrade" => {
            let strategy_id = decode_varint_field(request_args, 1);
            if strategy_id <= 0 {
                return HandlerResult::Error(GameError::InvalidRequest("strategy id is invalid"));
            }
            let level = decode_varint_field(request_args, 2).max(1) as u64;
            account
                .activities
                .progress
                .insert(format!("compat:strategy:{strategy_id}:level"), level);
            append_method_push(
                pre_pushes,
                "strategy.GetStrategy",
                typed_strategy_info_payload(account),
            );
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
            append_method_push(
                pre_pushes,
                "strategy.GetStrategy",
                typed_strategy_info_payload(account),
            );
            HandlerResult::PushOnly
        }
        "strategy.Apply" => {
            let strategy_id = decode_varint_field(request_args, 1);
            let fleet_id = decode_varint_field(request_args, 3);
            let tactic_type = decode_varint_field(request_args, 4);
            if strategy_id <= 0 || fleet_id <= 0 || tactic_type <= 0 {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "strategy apply request is invalid",
                ));
            }
            let Ok(fleet_id) = blueoath_domain::FleetId::new(fleet_id as u64) else {
                return HandlerResult::Error(GameError::InvalidRequest("fleet id is invalid"));
            };
            let Some(fleet) = account.fleet.fleets.get_mut(&fleet_id) else {
                return HandlerResult::Error(GameError::NotFound("fleet"));
            };
            fleet.tactic_id = strategy_id as u32;
            append_method_push(
                pre_pushes,
                "tactic.GetHerosTactic",
                FleetInfoCodec::encode(&fleet_info_from_typed_account(account)),
            );
            append_method_push(
                pre_pushes,
                "strategy.GetStrategy",
                typed_strategy_info_payload(account),
            );
            HandlerResult::PushOnly
        }
        "supportfleet.SupportFleetInfo" => {
            HandlerResult::Reply(Response::raw(method, typed_support_info_payload(account)))
        }
        "supportfleet.StartSupport" => {
            let support_id = decode_varint_field(request_args, 1);
            let hero_ids = decode_repeated_varint_field(request_args, 2)
                .into_iter()
                .filter(|id| *id > 0)
                .map(|id| id as u64)
                .collect::<Vec<_>>();
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
            append_method_push(
                pre_pushes,
                "supportfleet.SupportFleetInfo",
                typed_support_info_payload(account),
            );
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "supportfleet.CompleteSupport" | "supportfleet.CancelSupport" => {
            let id = decode_varint_field(request_args, 1);
            let completion_type = decode_varint_field(request_args, 2);
            let Some(entry) = account
                .support
                .entries
                .iter()
                .find(|entry| entry.id == id.max(0) as u32)
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
            append_method_push(
                pre_pushes,
                "supportfleet.SupportFleetInfo",
                typed_support_info_payload(account),
            );
            if !rewards.is_empty() {
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
                append_method_push(
                    pre_pushes,
                    "hero.UpdateHeroBagData",
                    HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
                );
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
            let hero_ids = decode_repeated_varint_field(request_args, 1)
                .into_iter()
                .filter(|id| *id > 0)
                .map(|id| id as u64)
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
            append_method_push(
                pre_pushes,
                "user.UpdateUserInfo",
                UserInfoCodec::encode(&user_info_from_typed_account(state, account)),
            );
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
            append_method_push(pre_pushes, "jopen.GetJopen", typed_jopen_payload(account));
            HandlerResult::PushOnly
        }
        "milestone.GetMilestone" => {
            HandlerResult::Reply(Response::raw(method, typed_milestone_info_payload(account)))
        }
        "milestone.FetchReward" => {
            let activity_id = decode_varint_field(request_args, 1);
            let index = decode_varint_field(request_args, 2);
            if activity_id <= 0 || index <= 0 {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "milestone request is invalid",
                ));
            }
            account
                .activities
                .progress
                .insert(format!("compat:milestone:{activity_id}:{index}"), 1);
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "guide.PlotReward" => {
            let plot_id = decode_varint_field(request_args, 1);
            if plot_id <= 0 {
                return HandlerResult::Error(GameError::InvalidRequest("guide plot id is invalid"));
            }
            account.guide.plot_rewards.insert(plot_id as u64);
            let mut payload = Vec::new();
            append_varint_field(&mut payload, 1, plot_id as u64);
            HandlerResult::Reply(Response::raw(method, payload))
        }
        "guide.Setting" => {
            let mut payload = Vec::new();
            for nested in decode_repeated_message_field(request_args, 1) {
                let Some(key) = decode_string_field(&nested, 1).filter(|key| !key.is_empty())
                else {
                    continue;
                };
                let value = decode_string_field(&nested, 2).unwrap_or_default();
                account.guide.settings.insert(key.clone(), value.clone());
                let mut setting = Vec::new();
                append_bytes_field(&mut setting, 1, key.as_bytes());
                append_bytes_field(&mut setting, 2, value.as_bytes());
                append_message_field(&mut payload, 3, &setting);
            }
            HandlerResult::Reply(Response::raw(method, payload))
        }
        "user.SetMiniGameScore" => {
            let chapter_id = decode_varint_field(request_args, 1);
            let entries = decode_repeated_message_field(request_args, 3);
            if chapter_id <= 0 || entries.is_empty() {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "mini-game score request is invalid",
                ));
            }
            let now = current_unix_seconds();
            for entry in entries {
                let copy_id = decode_varint_field(&entry, 1);
                let score = decode_varint_field(&entry, 2).max(0) as u64;
                if copy_id <= 0 {
                    continue;
                }
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
            let chapter_id = decode_varint_field(request_args, 1);
            if chapter_id <= 0 {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "mini-game chapter is invalid",
                ));
            }
            HandlerResult::Reply(Response::raw(
                method,
                typed_mini_game_score_payload(account, chapter_id, current_unix_seconds()),
            ))
        }
        "user.GetMiniGameScoreRank" => {
            let chapter_id = decode_varint_field(request_args, 1);
            if chapter_id <= 0 {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "mini-game chapter is invalid",
                ));
            }
            let score = typed_mini_game_chapter_score(account, chapter_id);
            HandlerResult::Reply(Response::raw(
                method,
                typed_mini_game_rank_payload(
                    account,
                    score,
                    if score > 0 { 1 } else { 0 },
                    decode_varint_field(request_args, 2),
                    decode_varint_field(request_args, 3),
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
            let fields = [(1, "type"), (2, "sort"), (3, "screen"), (4, "order")];
            for (field, name) in fields {
                account.activities.progress.insert(
                    format!("compat:user:order:{name}"),
                    decode_varint_field(request_args, field).max(0) as u64,
                );
            }
            account.activities.progress.insert(
                "compat:user:order:time".to_owned(),
                u64::from(current_unix_seconds()),
            );
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "user.Refresh" => {
            account.activities.progress.insert(
                "compat:user:refresh:maxPowerIndex".to_owned(),
                decode_varint_field(request_args, 2).max(0) as u64,
            );
            account.activities.progress.insert(
                "compat:user:refresh:minPowerIndex".to_owned(),
                decode_varint_field(request_args, 3).max(0) as u64,
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

fn typed_support_remove(account: &mut blueoath_domain::AccountState, id: i32) {
    account
        .support
        .entries
        .retain(|entry| entry.id != id.max(0) as u32);
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

fn typed_strategy_info_payload(account: &blueoath_domain::AccountState) -> Vec<u8> {
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
    fn typed_base_state_uses_progress_index() {
        let mut account = blueoath_domain::NewAccountFactory::create(
            blueoath_domain::ProfileId::new("typed-base-state").unwrap(),
            "Captain",
        );
        let state = ServerState::new("typed-base-state", "Captain", "1.0.0");
        let mut pushes = Vec::new();

        account.fleet.fleets.insert(
            blueoath_domain::FleetId::new(1).unwrap(),
            blueoath_domain::FleetRecord {
                formation_id: 1,
                tactic_id: 1,
                members: Vec::new(),
            },
        );
        let mut strategy = Vec::new();
        append_varint_field(&mut strategy, 1, 7);
        append_varint_field(&mut strategy, 2, 3);
        assert!(matches!(
            handle_typed(
                &mut account,
                &state,
                "strategy.Learn",
                &strategy,
                &mut pushes,
            ),
            HandlerResult::PushOnly
        ));
        assert_eq!(
            account.activities.progress.get("compat:strategy:7:level"),
            Some(&3)
        );
        let mut apply = Vec::new();
        append_varint_field(&mut apply, 1, 7);
        append_varint_field(&mut apply, 3, 1);
        append_varint_field(&mut apply, 4, 1);
        assert!(matches!(
            handle_typed(&mut account, &state, "strategy.Apply", &apply, &mut pushes,),
            HandlerResult::PushOnly
        ));
        assert_eq!(account.fleet.fleets.values().next().unwrap().tactic_id, 7);

        let support_id = SUPPORT_CATALOG
            .get()
            .and_then(|catalog| catalog.items.keys().next().copied())
            .unwrap_or(7_001);
        let mut support_start = Vec::new();
        append_varint_field(&mut support_start, 1, support_id as u64);
        append_varint_field(&mut support_start, 2, 1);
        assert!(matches!(
            handle_typed(
                &mut account,
                &state,
                "supportfleet.StartSupport",
                &support_start,
                &mut pushes,
            ),
            HandlerResult::Reply(_)
        ));
        assert_eq!(account.support.entries.len(), 1);
        let mut support_cancel = Vec::new();
        append_varint_field(&mut support_cancel, 1, 1);
        append_varint_field(&mut support_cancel, 2, 3);
        assert!(matches!(
            handle_typed(
                &mut account,
                &state,
                "supportfleet.CancelSupport",
                &support_cancel,
                &mut pushes,
            ),
            HandlerResult::Reply(_)
        ));
        assert!(account.support.entries.is_empty());

        let mut supply_switch = Vec::new();
        append_varint_field(&mut supply_switch, 1, 1);
        assert!(matches!(
            handle_typed(
                &mut account,
                &state,
                "supply.SupplySwitch",
                &supply_switch,
                &mut pushes,
            ),
            HandlerResult::PushOnly
        ));
        assert_eq!(
            account.supply.hero_ids,
            vec![blueoath_domain::HeroId::new(1).unwrap()]
        );

        assert!(matches!(
            handle_typed(&mut account, &state, "jopen.FetchHero", &[], &mut pushes,),
            HandlerResult::PushOnly
        ));
        assert!(account
            .activities
            .progress
            .contains_key("compat:jopen:fetchHeroTime"));
        assert_eq!(
            TMessageCodec::decode_response(pushes.last().unwrap())
                .unwrap()
                .method,
            "jopen.GetJopen"
        );

        let mut milestone = Vec::new();
        append_varint_field(&mut milestone, 1, 9);
        append_varint_field(&mut milestone, 2, 3);
        assert!(matches!(
            handle_typed(
                &mut account,
                &state,
                "milestone.FetchReward",
                &milestone,
                &mut pushes,
            ),
            HandlerResult::Reply(_)
        ));
        let result = handle_typed(
            &mut account,
            &state,
            "milestone.GetMilestone",
            &[],
            &mut pushes,
        );
        let HandlerResult::Reply(response) = result else {
            panic!("expected milestone response");
        };
        let activities = decode_repeated_message_field(&response.payload, 1);
        assert_eq!(decode_varint_field(&activities[0], 1), 9);
        let rewards = decode_repeated_message_field(&activities[0], 2);
        assert_eq!(decode_varint_field(&rewards[0], 1), 3);

        let mut plot = Vec::new();
        append_varint_field(&mut plot, 1, 42);
        assert!(matches!(
            handle_typed(&mut account, &state, "guide.PlotReward", &plot, &mut pushes,),
            HandlerResult::Reply(_)
        ));
        assert!(account.guide.plot_rewards.contains(&42));

        let mut guide_setting = Vec::new();
        let mut guide_setting_item = Vec::new();
        append_bytes_field(&mut guide_setting_item, 1, b"tutorial");
        append_bytes_field(&mut guide_setting_item, 2, b"closed");
        append_message_field(&mut guide_setting, 1, &guide_setting_item);
        assert!(matches!(
            handle_typed(
                &mut account,
                &state,
                "guide.Setting",
                &guide_setting,
                &mut pushes,
            ),
            HandlerResult::Reply(_)
        ));
        assert_eq!(
            account.guide.settings.get("tutorial"),
            Some(&"closed".to_owned())
        );

        let mut score_entry = Vec::new();
        append_varint_field(&mut score_entry, 1, 101);
        append_varint_field(&mut score_entry, 2, 80);
        let mut score_request = Vec::new();
        append_varint_field(&mut score_request, 1, 7);
        append_message_field(&mut score_request, 3, &score_entry);
        let result = handle_typed(
            &mut account,
            &state,
            "user.SetMiniGameScore",
            &score_request,
            &mut pushes,
        );
        let HandlerResult::Reply(response) = result else {
            panic!("expected mini-game score response");
        };
        assert_eq!(decode_varint_field(&response.payload, 1), 80);
        let mut lower_score_entry = Vec::new();
        append_varint_field(&mut lower_score_entry, 1, 101);
        append_varint_field(&mut lower_score_entry, 2, 20);
        let mut lower_score_request = Vec::new();
        append_varint_field(&mut lower_score_request, 1, 7);
        append_message_field(&mut lower_score_request, 3, &lower_score_entry);
        let result = handle_typed(
            &mut account,
            &state,
            "user.SetMiniGameScore",
            &lower_score_request,
            &mut pushes,
        );
        let HandlerResult::Reply(response) = result else {
            panic!("expected lower mini-game score response");
        };
        assert_eq!(decode_varint_field(&response.payload, 1), 80);
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
