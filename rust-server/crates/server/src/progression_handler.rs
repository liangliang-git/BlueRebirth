use serde_json::Value;

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;
use blueoath_domain::{AccountState, BathroomHeroState};

pub(super) fn handle_bathroom_typed(
    account: &mut AccountState,
    method: &str,
    request_args: &[u8],
    now: u32,
    mood_recovery_multiplier: f64,
    post_pushes: &mut Vec<Vec<u8>>,
) -> HandlerResult {
    let requested_hero_id = decode_varint_u64_field(request_args, 1);
    let before = account
        .bathroom
        .heroes
        .iter()
        .find(|hero| hero.hero_id == requested_hero_id)
        .cloned();

    let response = match method {
        "bathroom.GetBathroomInfo" => bathroom_info_payload_from_typed(account),
        "bathroom.BathStart" => {
            let position = decode_varint_u64_field(request_args, 2);
            if requested_hero_id == 0
                || !account
                    .dock
                    .heroes
                    .contains_key(&hero_key(requested_hero_id))
            {
                return HandlerResult::Error(GameError::InvalidRequest("bathroom hero is invalid"));
            }
            start_bathroom_hero(
                account,
                requested_hero_id,
                u32::try_from(position).unwrap_or(u32::MAX),
                now,
            );
            bathroom_info_payload_from_typed(account)
        }
        "bathroom.BathEnd" | "bathroom.BathChangeHero" => {
            account
                .bathroom
                .heroes
                .retain(|hero| hero.hero_id != requested_hero_id);
            if let Some(before) = before.as_ref() {
                recover_typed_hero_mood(
                    account,
                    before.hero_id,
                    before
                        .bath_time
                        .max(u64::from(now).saturating_sub(before.start_time)),
                    mood_recovery_multiplier,
                    now,
                );
            }
            append_method_push(
                post_pushes,
                "hero.UpdateHeroBagData",
                HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
            );
            bathroom_end_payload(
                requested_hero_id,
                before.map(|hero| hero.bath_time).unwrap_or_default() as i64,
            )
        }
        "bathroom.BathService" => {
            let hero_id = requested_hero_id;
            let hero = account
                .bathroom
                .heroes
                .iter()
                .find(|hero| hero.hero_id == hero_id);
            bathroom_service_payload(
                hero_id,
                hero.map(|hero| i64::from(hero.position))
                    .unwrap_or_default(),
                hero.map(|hero| hero.bath_time as i64).unwrap_or_default(),
            )
        }
        "bathroom.BathAuto" => {
            let is_auto = decode_varint_u64_field(request_args, 2) != 0;
            if let Some(hero) = account
                .bathroom
                .heroes
                .iter_mut()
                .find(|hero| hero.hero_id == requested_hero_id)
            {
                hero.is_auto = is_auto;
            }
            Vec::new()
        }
        "bathroom.BathAllAuto" => {
            account.bathroom.is_all_auto = requested_hero_id != 0;
            Vec::new()
        }
        "bathroom.BathStartAll" => {
            let mut output = Vec::new();
            for nested in decode_repeated_message_field(request_args, 1) {
                let hero_id = decode_varint_u64_field(&nested, 1);
                let position = decode_varint_u64_field(&nested, 2);
                if hero_id == 0 {
                    continue;
                }
                start_bathroom_hero(
                    account,
                    hero_id,
                    u32::try_from(position).unwrap_or(u32::MAX),
                    now,
                );
                append_message_field(
                    &mut output,
                    1,
                    &bathroom_end_payload(
                        hero_id,
                        account
                            .bathroom
                            .heroes
                            .iter()
                            .find(|hero| hero.hero_id == hero_id)
                            .map(|hero| hero.bath_time as i64)
                            .unwrap_or_default(),
                    ),
                );
            }
            output
        }
        _ => return HandlerResult::Empty,
    };

    append_method_push(
        post_pushes,
        "bathroom.BathroomInfo",
        bathroom_info_payload_from_typed(account),
    );
    HandlerResult::Reply(Response::raw(method, response))
}

pub(super) fn handle_study_typed(
    account: &mut AccountState,
    method: &str,
    request_args: &[u8],
    now: u32,
    post_pushes: &mut Vec<Vec<u8>>,
) -> HandlerResult {
    match method {
        "study.GetStudyInfo" => HandlerResult::Reply(Response::raw(
            method,
            study_info_payload_from_typed(account, now),
        )),
        "study.StartStudyPSkill" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let skill_id = decode_varint_u64_field(request_args, 2);
            let textbook_id = decode_varint_u64_field(request_args, 3);
            let valid = hero_id > 0
                && skill_id > 0
                && textbook_id > 0
                && account.dock.heroes.contains_key(&hero_key(hero_id))
                && account.study.progress.len() < 2
                && !account
                    .study
                    .progress
                    .iter()
                    .any(|progress| progress.hero_id == hero_id)
                && typed_item_count(account, textbook_id) > 0;
            if !valid {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "study slot, hero, or textbook is invalid",
                ));
            }
            let _ = consume_typed_item(account, textbook_id, 1);
            account
                .study
                .progress
                .push(blueoath_domain::StudyProgressState {
                    hero_id,
                    skill_id,
                    textbook_id,
                    begin_time: u64::from(now),
                    end_time: u64::from(now.saturating_add(60)),
                });
            append_method_push(
                post_pushes,
                "study.GetStudyInfo",
                study_info_payload_from_typed(account, now),
            );
            append_method_push(
                post_pushes,
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_typed_account(account)),
            );
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "study.CancelStudyPSkill" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let requested_skill_id = decode_varint_u64_field(request_args, 2);
            let index = account.study.progress.iter().position(|progress| {
                progress.hero_id == hero_id
                    && (requested_skill_id == 0 || progress.skill_id == requested_skill_id)
            });
            let Some(index) = index else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "study progress is missing",
                ));
            };
            account.study.progress.remove(index);
            append_method_push(
                post_pushes,
                "study.GetStudyInfo",
                study_info_payload_from_typed(account, now),
            );
            HandlerResult::Reply(Response::raw(method, Vec::new()))
        }
        "study.EndStudyPSkill" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let skill_id = decode_varint_u64_field(request_args, 2);
            finish_study_typed(account, hero_id, skill_id, now, false)
                .map(|payload| {
                    append_method_push(
                        post_pushes,
                        "hero.UpdateHeroBagData",
                        HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
                    );
                    append_method_push(
                        post_pushes,
                        "study.GetStudyInfo",
                        study_info_payload_from_typed(account, now),
                    );
                    HandlerResult::Reply(Response::raw(method, payload))
                })
                .unwrap_or_else(|| {
                    HandlerResult::Error(GameError::InvalidRequest("study is not finished"))
                })
        }
        "study.SpeedUpStudy" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let skill_id = decode_varint_u64_field(request_args, 2);
            let items = decode_repeated_message_field(request_args, 3)
                .into_iter()
                .map(|item| {
                    (
                        decode_varint_u64_field(&item, 1),
                        decode_varint_u64_field(&item, 2),
                    )
                })
                .filter(|(item_id, count)| *item_id > 0 && *count > 0)
                .collect::<Vec<_>>();
            if items.is_empty()
                || !account
                    .study
                    .progress
                    .iter()
                    .any(|progress| progress.hero_id == hero_id && progress.skill_id == skill_id)
                || items
                    .iter()
                    .any(|(item_id, count)| typed_item_count(account, *item_id) < *count)
            {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "study progress or speedup items are missing",
                ));
            }
            for (item_id, count) in items {
                let _ = consume_typed_item(account, item_id, count);
            }
            finish_study_typed(account, hero_id, skill_id, now, true)
                .map(|payload| {
                    append_method_push(
                        post_pushes,
                        "hero.UpdateHeroBagData",
                        HeroBagCodec::encode(&hero_bag_from_typed_account(account)),
                    );
                    append_method_push(
                        post_pushes,
                        "study.GetStudyInfo",
                        study_info_payload_from_typed(account, now),
                    );
                    HandlerResult::Reply(Response::raw(method, payload))
                })
                .unwrap_or_else(|| {
                    HandlerResult::Error(GameError::InvalidState("study progress is missing"))
                })
        }
        _ => HandlerResult::Empty,
    }
}

fn typed_item_count(account: &AccountState, template_id: u64) -> u64 {
    blueoath_domain::TemplateId::new(template_id)
        .ok()
        .and_then(|id| account.inventory.items.get(&id).copied())
        .unwrap_or_default()
}

fn consume_typed_item(account: &mut AccountState, template_id: u64, count: u64) -> bool {
    let Ok(template_id) = blueoath_domain::TemplateId::new(template_id) else {
        return false;
    };
    let Some(amount) = account.inventory.items.get_mut(&template_id) else {
        return false;
    };
    if *amount < count {
        return false;
    }
    *amount -= count;
    if *amount == 0 {
        account.inventory.items.remove(&template_id);
    }
    true
}

fn finish_study_typed(
    account: &mut AccountState,
    hero_id: u64,
    skill_id: u64,
    now: u32,
    force: bool,
) -> Option<Vec<u8>> {
    let index = account
        .study
        .progress
        .iter()
        .position(|progress| progress.hero_id == hero_id && progress.skill_id == skill_id)?;
    if !force && account.study.progress[index].end_time > u64::from(now) {
        return None;
    }
    let progress = account.study.progress.remove(index);
    let hero = account.dock.heroes.get_mut(&hero_key(hero_id))?;
    let before = hero.pskills.get(&skill_id).copied().unwrap_or_default();
    let after = before.saturating_add(1).max(1);
    hero.pskills.insert(skill_id, after);
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, hero_id);
    append_varint_field(&mut output, 2, skill_id);
    append_varint_field(&mut output, 3, u64::from(before));
    append_varint_field(&mut output, 4, u64::from(after));
    append_varint_field(&mut output, 5, progress.textbook_id);
    Some(output)
}

fn study_info_payload_from_typed(account: &AccountState, _now: u32) -> Vec<u8> {
    let mut output = Vec::new();
    append_varint_field(&mut output, 1, 2);
    for progress in &account.study.progress {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, progress.hero_id);
        append_varint_field(&mut item, 2, progress.skill_id);
        append_varint_field(&mut item, 3, progress.textbook_id);
        append_varint_field(&mut item, 4, progress.begin_time);
        append_varint_field(&mut item, 5, progress.end_time);
        append_message_field(&mut output, 2, &item);
    }
    output
}

fn hero_key(value: u64) -> blueoath_domain::HeroId {
    blueoath_domain::HeroId::new(value).expect("validated positive hero id")
}

fn start_bathroom_hero(account: &mut AccountState, hero_id: u64, position: u32, now: u32) {
    account
        .bathroom
        .heroes
        .retain(|hero| hero.hero_id != hero_id);
    account.bathroom.heroes.push(BathroomHeroState {
        hero_id,
        position,
        start_time: u64::from(now),
        ..BathroomHeroState::default()
    });
}

fn recover_typed_hero_mood(
    account: &mut AccountState,
    hero_id: u64,
    bath_seconds: u64,
    multiplier: f64,
    now: u32,
) {
    let Some(hero) = account.dock.heroes.get_mut(&hero_key(hero_id)) else {
        return;
    };
    let intervals = i64::try_from(bath_seconds).unwrap_or(i64::MAX) / MOOD_BATH_INTERVAL_SECONDS;
    let base = if intervals > 0 {
        i64::from(MOOD_BATH_INTERVAL_RECOVERY)
            .saturating_mul(intervals)
            .min(i64::from(MOOD_BATH_RECOVERY))
    } else {
        i64::from(MOOD_BATH_RECOVERY)
    };
    let recovery = scale_reward(base, multiplier);
    hero.mood = hero
        .mood
        .saturating_add(u32::try_from(recovery.max(0)).unwrap_or(u32::MAX))
        .min(MOOD_MAX as u32);
    let _ = now;
}

fn bathroom_info_payload_from_typed(account: &AccountState) -> Vec<u8> {
    let mut output = Vec::new();
    if account.bathroom.heroes.is_empty() {
        output.extend_from_slice(&[0x0A, 0x00]);
    } else {
        for hero in &account.bathroom.heroes {
            let mut encoded = Vec::new();
            append_varint_field(&mut encoded, 1, hero.hero_id);
            append_varint_field(&mut encoded, 2, u64::from(hero.position));
            append_varint_field(&mut encoded, 3, u64::from(hero.is_auto));
            append_varint_field(&mut encoded, 4, hero.start_time);
            append_varint_field(&mut encoded, 5, hero.bath_time);
            append_varint_field(&mut encoded, 6, u64::from(hero.buff_id));
            append_varint_field(&mut encoded, 7, hero.buff_time);
            append_varint_field(&mut encoded, 8, u64::from(hero.power));
            append_message_field(&mut output, 1, &encoded);
        }
    }
    if account.bathroom.is_all_auto {
        append_varint_field(&mut output, 2, 1);
    }
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
    let state = context.state;
    let account = &mut *context.account;
    let catalogs = context.catalogs;
    let GameLoginCatalogs {
        fashion: fashion_catalog,
        equip: equip_catalog,
        handbook_behaviours,
        tasks: task_catalog,
        ..
    } = catalogs;
    let account_view = account.as_deref();
    let pre_pushes = &mut *context.pre_pushes;
    let post_pushes = &mut *context.post_pushes;
    let handler_error = &mut *context.handler_error;

    match method {
        "study.GetStudyInfo" => Some(study_info_payload(
            account_view.unwrap_or(&Value::Null),
            current_unix_seconds(),
        )),
        "study.StartStudyPSkill" => {
            let (hero_id, skill_id, textbook_id) = decode_study_start(request_args);
            if let Some(account) = account.as_deref_mut() {
                if start_study_state(
                    account,
                    hero_id,
                    skill_id,
                    textbook_id,
                    current_unix_seconds(),
                ) {
                    let now = current_unix_seconds();
                    append_method_push(
                        pre_pushes,
                        "study.GetStudyInfo",
                        study_info_payload(account, now),
                    );
                    append_method_push(
                        pre_pushes,
                        "bag.UpdateBagData",
                        BagInfoCodec::encode(&bag_info_from_account(account)),
                    );
                    Some(Vec::new())
                } else {
                    *handler_error = Some(GameError::Internal(
                        "study slot or hero is invalid".to_owned(),
                    ));
                    Some(Vec::new())
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "study.CancelStudyPSkill" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let requested_skill_id = decode_varint_field(request_args, 2);
            if let Some(account) = account.as_deref_mut() {
                let removed = resolve_study_skill_id(account, hero_id, requested_skill_id)
                    .is_some_and(|skill_id| {
                        account
                            .get_mut("study")
                            .and_then(|v| v.get_mut("progress"))
                            .and_then(Value::as_array_mut)
                            .map(|rows| {
                                let old = rows.len();
                                rows.retain(|r| {
                                    !(json_u64(r, "heroId") == Some(hero_id)
                                        && json_i32(r, "pSkillId") == Some(skill_id))
                                });
                                old != rows.len()
                            })
                            .unwrap_or(false)
                    });
                if removed {
                    append_method_push(
                        pre_pushes,
                        "study.GetStudyInfo",
                        study_info_payload(account, current_unix_seconds()),
                    );
                    Some(Vec::new())
                } else {
                    *handler_error =
                        Some(GameError::Internal("study progress is missing".to_owned()));
                    Some(Vec::new())
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "study.EndStudyPSkill" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let requested_skill_id = decode_varint_field(request_args, 2);
            if let Some(account) = account.as_deref_mut() {
                let ret = resolve_study_skill_id(account, hero_id, requested_skill_id).and_then(
                    |skill_id| {
                        finish_study_state(account, hero_id, skill_id, current_unix_seconds())
                    },
                );
                if let Some(ret) = ret {
                    advance_task_event(account, task_catalog, 9, 1, current_unix_seconds());
                    pre_pushes.push(encode_hero_bag_push(account));
                    append_method_push(
                        pre_pushes,
                        "study.GetStudyInfo",
                        study_info_payload(account, current_unix_seconds()),
                    );
                    Some(ret)
                } else {
                    *handler_error = Some(GameError::Internal("study is not finished".to_owned()));
                    Some(Vec::new())
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "study.SpeedUpStudy" => {
            let (hero_id, skill_id, items) = decode_study_speedup(request_args);
            if let Some(account) = account.as_deref_mut() {
                let has_progress = account
                    .get("study")
                    .and_then(|v| v.get("progress"))
                    .and_then(Value::as_array)
                    .is_some_and(|rows| {
                        rows.iter().any(|row| {
                            json_u64(row, "heroId") == Some(hero_id)
                                && json_i32(row, "pSkillId") == Some(skill_id)
                        })
                    });
                if !has_progress || items.is_empty() {
                    *handler_error = Some(GameError::Internal(
                        "study progress or speedup items are missing".to_owned(),
                    ));
                }
                for (item_id, count) in &items {
                    if handler_error.is_none()
                        && bag_item_count(account, *item_id) < i64::from(*count)
                    {
                        *handler_error =
                            Some(GameError::Internal("not enough study textbooks".to_owned()));
                    }
                }
                if handler_error.is_none() {
                    for (item_id, count) in items {
                        consume_bag_item(account, item_id, count);
                    }
                }
                if handler_error.is_none() {
                    if let Some(ret) =
                        finish_study_state_force(account, hero_id, skill_id, current_unix_seconds())
                    {
                        pre_pushes.push(encode_hero_bag_push(account));
                        append_method_push(
                            pre_pushes,
                            "study.GetStudyInfo",
                            study_info_payload(account, current_unix_seconds()),
                        );
                        append_method_push(
                            pre_pushes,
                            "bag.UpdateBagData",
                            BagInfoCodec::encode(&bag_info_from_account(account)),
                        );
                        Some(ret)
                    } else {
                        *handler_error =
                            Some(GameError::Internal("study progress is missing".to_owned()));
                        Some(Vec::new())
                    }
                } else {
                    *handler_error =
                        Some(GameError::Internal("not enough study textbooks".to_owned()));
                    Some(Vec::new())
                }
            } else {
                *handler_error = Some(GameError::Internal("account is unavailable".to_owned()));
                Some(Vec::new())
            }
        }
        "bathroom.GetBathroomInfo" => {
            let payload = bathroom_info_payload(account_view.unwrap_or(&Value::Null));
            append_method_push(post_pushes, "bathroom.BathroomInfo", payload.clone());
            Some(payload)
        }
        "bathroom.BathStart"
        | "bathroom.BathEnd"
        | "bathroom.BathChangeHero"
        | "bathroom.BathService"
        | "bathroom.BathAuto"
        | "bathroom.BathAllAuto"
        | "bathroom.BathStartAll" => {
            let requested_hero_id = decode_varint_u64_field(request_args, 1);
            let bath_time_before = account
                .as_deref()
                .and_then(|value| value.get("bath"))
                .and_then(|value| value.get("heroList"))
                .and_then(Value::as_array)
                .and_then(|heroes| {
                    heroes
                        .iter()
                        .find(|hero| json_u64(hero, "heroId") == Some(requested_hero_id))
                })
                .and_then(|hero| json_i64(hero, "bathTime"))
                .unwrap_or_default();
            let bath_start_before = account
                .as_deref()
                .and_then(|value| value.get("bath"))
                .and_then(|value| value.get("heroList"))
                .and_then(Value::as_array)
                .and_then(|heroes| {
                    heroes
                        .iter()
                        .find(|hero| json_u64(hero, "heroId") == Some(requested_hero_id))
                })
                .and_then(|hero| json_i64(hero, "startTime"))
                .unwrap_or_default();
            let bath_was_active = account
                .as_deref()
                .and_then(|value| value.get("bath"))
                .and_then(|value| value.get("heroList"))
                .and_then(Value::as_array)
                .is_some_and(|heroes| {
                    heroes
                        .iter()
                        .any(|hero| json_u64(hero, "heroId") == Some(requested_hero_id))
                });
            let now = current_unix_seconds();
            let bath_seconds =
                bath_time_before.max(i64::from(now).saturating_sub(bath_start_before));
            if let Some(account) = account.as_deref_mut() {
                update_bathroom_state(account, method, request_args, now);
                if matches!(
                    method,
                    "bathroom.BathEnd" | "bathroom.BathChangeHero" | "bathroom.BathService"
                ) {
                    let restore_id = if requested_hero_id != 0 {
                        requested_hero_id
                    } else {
                        decode_varint_u64_field(request_args, 1)
                    };
                    if restore_id != 0 && bath_was_active {
                        recover_hero_mood_from_bath(
                            account,
                            restore_id,
                            bath_seconds,
                            state.mood_recovery_multiplier,
                            now,
                        );
                    }
                    append_method_push(
                        post_pushes,
                        "hero.UpdateHeroBagData",
                        HeroBagCodec::encode(&hero_bag_from_account(account)),
                    );
                }
            }
            let response_payload = match method {
                "bathroom.BathStart" => {
                    bathroom_info_payload(account.as_deref().unwrap_or(&Value::Null))
                }
                "bathroom.BathEnd" | "bathroom.BathChangeHero" => {
                    bathroom_end_payload(requested_hero_id, bath_time_before)
                }
                "bathroom.BathService" => {
                    let hero_id = decode_varint_u64_field(request_args, 1);
                    let (pos, bath_time) = account
                        .as_deref()
                        .and_then(|value| value.get("bath"))
                        .and_then(|value| value.get("heroList"))
                        .and_then(Value::as_array)
                        .and_then(|heroes| {
                            heroes
                                .iter()
                                .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
                        })
                        .map(|hero| {
                            (
                                json_i64(hero, "pos").unwrap_or_default(),
                                json_i64(hero, "bathTime").unwrap_or_default(),
                            )
                        })
                        .unwrap_or_default();
                    bathroom_service_payload(hero_id, pos, bath_time)
                }
                "bathroom.BathStartAll" => bathroom_start_all_payload(
                    account.as_deref().unwrap_or(&Value::Null),
                    request_args,
                ),
                _ => Vec::new(),
            };
            append_method_push(
                post_pushes,
                "bathroom.BathroomInfo",
                bathroom_info_payload(account.as_deref().unwrap_or(&Value::Null)),
            );
            Some(response_payload)
        }
        "task.TaskInfo" => Some(task_info_payload(
            account_view.unwrap_or(&Value::Null),
            task_catalog,
        )),
        "task.TaskReward"
        | "task.TaskRewardByDaysActivity"
        | "task.TaskSevenDayActivity"
        | "task.TaskRewardByReturnActivity" => {
            let task_id = decode_varint_field(request_args, 1);
            let task_type = decode_varint_field(request_args, 2);
            if let Some(account) = account.as_deref_mut() {
                let goal = task_catalog
                    .and_then(|catalog| {
                        catalog.definitions.iter().find(|d| {
                            d.id == task_id && (task_type == 0 || d.task_type == task_type)
                        })
                    })
                    .map(|d| d.goal)
                    .unwrap_or(1)
                    .max(1);
                let task_type = if task_type == 0 {
                    task_catalog
                        .and_then(|catalog| catalog.definitions.iter().find(|d| d.id == task_id))
                        .map(|d| d.task_type)
                        .unwrap_or(1)
                } else {
                    task_type
                };
                if task_claimed(account, task_type, task_id) {
                    *handler_error = Some(GameError::Internal(
                        "task reward was already claimed".to_owned(),
                    ));
                    Some(Vec::new())
                } else if !task_completed(account, task_type, task_id, goal) {
                    *handler_error = Some(GameError::Internal("task is not complete".to_owned()));
                    Some(Vec::new())
                } else {
                    let definition = task_catalog.and_then(|catalog| {
                        catalog
                            .definitions
                            .iter()
                            .find(|d| d.task_type == task_type && d.id == task_id)
                    });
                    let available = match (task_catalog, definition) {
                        (Some(catalog), Some(definition)) => {
                            matches!(task_type, 6 | 10 | 12)
                                || task_is_visible(account, catalog, definition)
                        }
                        _ => false,
                    };
                    if !available {
                        *handler_error =
                            Some(GameError::Internal("task is not available".to_owned()));
                        Some(Vec::new())
                    } else {
                        let mut configured = task_rewards(task_catalog, task_type, task_id);
                        if let Some(definition) = definition {
                            if definition.medal_id > 0 {
                                configured.push(ShopReward {
                                    goods_type: 16,
                                    item_id: definition.medal_id,
                                    num: 1,
                                    instance_id: 0,
                                });
                            }
                        }
                        if configured.is_empty() {
                            *handler_error = Some(GameError::Internal(
                                "task reward is not configured".to_owned(),
                            ));
                            Some(Vec::new())
                        } else {
                            let rewards = configured
                                .into_iter()
                                .map(|reward| {
                                    grant_reward(
                                        account,
                                        reward,
                                        current_unix_seconds(),
                                        fashion_catalog,
                                    )
                                })
                                .collect::<Vec<_>>();
                            complete_task(
                                account,
                                task_type,
                                task_id,
                                goal,
                                current_unix_seconds(),
                            );
                            if let Some(catalog) = task_catalog {
                                sync_achievement_points(account, catalog);
                            }
                            pre_pushes.push(encode_hero_bag_push(account));
                            if let Some(payload) =
                                illustrate_info_payload_for_rewards(&rewards, handbook_behaviours)
                            {
                                append_method_push(
                                    pre_pushes,
                                    "illustrate.IllustrateInfo",
                                    payload,
                                );
                            }
                            append_method_push(
                                pre_pushes,
                                "task.TaskInfo",
                                task_info_payload(account, task_catalog),
                            );
                            append_method_push(
                                pre_pushes,
                                "bag.UpdateBagData",
                                BagInfoCodec::encode(&bag_info_from_account(account)),
                            );
                            append_method_push(
                                pre_pushes,
                                "fashion.updateData",
                                FashionListCodec::encode(&fashion_list_from_account(
                                    account,
                                    fashion_catalog,
                                )),
                            );
                            append_method_push(
                                pre_pushes,
                                "equip.UpdateEquipBagData",
                                EquipListCodec::encode(&equip_list_from_account(
                                    account,
                                    equip_catalog,
                                )),
                            );
                            append_method_push(
                                pre_pushes,
                                "user.UpdateUserInfo",
                                UserInfoCodec::encode(&user_info_from_account(
                                    state,
                                    Some(account),
                                )),
                            );
                            Some(encode_task_reward(task_id, &rewards))
                        }
                    }
                }
            } else {
                Some(Vec::new())
            }
        }
        "task.TaskAllReward" => {
            if let Some(account) = account.as_deref_mut() {
                let now = current_unix_seconds();
                let get_type = decode_varint_field(request_args, 1);
                let allowed_types = if get_type == 2 {
                    [5].as_slice()
                } else {
                    [1, 2, 3, 4].as_slice()
                };
                let ids = task_catalog
                    .map(|catalog| {
                        catalog
                            .definitions
                            .iter()
                            .filter(|definition| allowed_types.contains(&definition.task_type))
                            .filter(|definition| task_is_visible(account, catalog, definition))
                            .map(|d| (d.task_type, d.id, d.goal.max(1)))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let mut rewards = Vec::new();
                for (task_type, task_id, goal) in ids {
                    if task_completed(account, task_type, task_id, goal)
                        && !task_claimed(account, task_type, task_id)
                    {
                        let mut configured = task_rewards(task_catalog, task_type, task_id);
                        if let Some(definition) = task_catalog.and_then(|catalog| {
                            catalog
                                .definitions
                                .iter()
                                .find(|d| d.task_type == task_type && d.id == task_id)
                        }) {
                            if definition.medal_id > 0 {
                                configured.push(ShopReward {
                                    goods_type: 16,
                                    item_id: definition.medal_id,
                                    num: 1,
                                    instance_id: 0,
                                });
                            }
                        }
                        if configured.is_empty() {
                            continue;
                        }
                        let granted = configured
                            .into_iter()
                            .map(|reward| grant_reward(account, reward, now, fashion_catalog))
                            .collect::<Vec<_>>();
                        complete_task(account, task_type, task_id, goal, now);
                        if let Some(catalog) = task_catalog {
                            sync_achievement_points(account, catalog);
                        }
                        if let Some(payload) =
                            illustrate_info_payload_for_rewards(&granted, handbook_behaviours)
                        {
                            append_method_push(pre_pushes, "illustrate.IllustrateInfo", payload);
                        }
                        pre_pushes.push(encode_hero_bag_push(account));
                        rewards.extend(granted);
                    }
                }
                append_method_push(
                    pre_pushes,
                    "user.UpdateUserInfo",
                    UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                );
                append_method_push(
                    pre_pushes,
                    "task.TaskInfo",
                    task_info_payload(account, task_catalog),
                );
                append_method_push(
                    pre_pushes,
                    "bag.UpdateBagData",
                    BagInfoCodec::encode(&bag_info_from_account(account)),
                );
                append_method_push(
                    pre_pushes,
                    "fashion.updateData",
                    FashionListCodec::encode(&fashion_list_from_account(account, fashion_catalog)),
                );
                append_method_push(
                    pre_pushes,
                    "equip.UpdateEquipBagData",
                    EquipListCodec::encode(&equip_list_from_account(account, equip_catalog)),
                );
                Some(encode_task_reward_list(&rewards))
            } else {
                Some(Vec::new())
            }
        }
        "task.TaskTrigger" => {
            // Client sends this after local actions. Acknowledge lifecycle without trusting
            // client-reported counters; authoritative handlers advance records separately.
            Some(Vec::new())
        }
        _ => None,
    }
}

#[cfg(test)]
mod typed_tests {
    use super::*;

    #[test]
    fn typed_bathroom_start_and_end_update_domain_state() {
        let hero_id = blueoath_domain::HeroId::new(9).unwrap();
        let mut account = AccountState::default();
        account.dock.heroes.insert(
            hero_id,
            blueoath_domain::HeroState {
                id: hero_id,
                template_id: blueoath_domain::TemplateId::new(100).unwrap(),
                name: String::new(),
                change_name_time: 0,
                level: 1,
                exp: 0,
                mood: 0,
                affection: 0,
                hp: 100,
                locked: false,
                equip_slots: Vec::new(),
                pskills: std::collections::BTreeMap::new(),
            },
        );
        let mut start = Vec::new();
        append_varint_field(&mut start, 1, 9);
        append_varint_field(&mut start, 2, 3);
        let mut pushes = Vec::new();
        let result = handle_bathroom_typed(
            &mut account,
            "bathroom.BathStart",
            &start,
            100,
            1.0,
            &mut pushes,
        );
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(account.bathroom.heroes[0].position, 3);

        let mut end = Vec::new();
        append_varint_field(&mut end, 1, 9);
        let result = handle_bathroom_typed(
            &mut account,
            "bathroom.BathEnd",
            &end,
            100,
            1.0,
            &mut pushes,
        );
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert!(account.bathroom.heroes.is_empty());
        assert_eq!(account.dock.heroes[&hero_id].mood, 300_000);
        assert_eq!(pushes.len(), 3);
    }

    #[test]
    fn typed_study_consumes_textbook_and_levels_skill() {
        let hero_id = blueoath_domain::HeroId::new(9).unwrap();
        let textbook_id = blueoath_domain::TemplateId::new(7001).unwrap();
        let mut account = AccountState::default();
        account.dock.heroes.insert(
            hero_id,
            blueoath_domain::HeroState {
                id: hero_id,
                template_id: blueoath_domain::TemplateId::new(100).unwrap(),
                name: String::new(),
                change_name_time: 0,
                level: 1,
                exp: 0,
                mood: 0,
                affection: 0,
                hp: 100,
                locked: false,
                equip_slots: Vec::new(),
                pskills: std::collections::BTreeMap::new(),
            },
        );
        account.inventory.items.insert(textbook_id, 1);
        let mut start = Vec::new();
        append_varint_field(&mut start, 1, 9);
        append_varint_field(&mut start, 2, 41);
        append_varint_field(&mut start, 3, 7001);
        let mut pushes = Vec::new();
        let result = handle_study_typed(
            &mut account,
            "study.StartStudyPSkill",
            &start,
            100,
            &mut pushes,
        );
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert_eq!(typed_item_count(&account, 7001), 0);

        let mut end = Vec::new();
        append_varint_field(&mut end, 1, 9);
        append_varint_field(&mut end, 2, 41);
        let result =
            handle_study_typed(&mut account, "study.EndStudyPSkill", &end, 200, &mut pushes);
        assert!(matches!(result, HandlerResult::Reply(_)));
        assert!(account.study.progress.is_empty());
        assert_eq!(account.dock.heroes[&hero_id].pskills.get(&41), Some(&1));
    }
}
