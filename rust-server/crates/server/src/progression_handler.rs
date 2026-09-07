use serde_json::Value;

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

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
