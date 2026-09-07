use serde_json::Value;

use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

pub(crate) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    method: &str,
    request_args: &[u8],
    now: u32,
    pre_pushes: &mut Vec<Vec<u8>>,
    building_catalog: Option<&BuildingCatalog>,
) -> HandlerResult {
    match method {
        "building.UpdateBuildingInfo" => HandlerResult::Reply(Response::raw(
            method,
            UserBuildingInfoCodec::encode(&building_info_from_typed_account(account, now)),
        )),
        "building.AddBuilding" => {
            let template_id = decode_varint_field(request_args, 1);
            let land_index = decode_varint_field(request_args, 2);
            let Some(building_id) = add_typed_building(account, template_id, land_index) else {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "building placement is invalid",
                ));
            };
            append_typed_building_refresh(pre_pushes, account, now);
            let mut payload = Vec::new();
            append_varint_field(&mut payload, 1, building_id);
            HandlerResult::Reply(Response::raw(method, payload))
        }
        "building.UpgradeBuilding" | "building.DegradeBuilding" => {
            let building_id = decode_varint_field(request_args, 1);
            let delta = if method == "building.UpgradeBuilding" {
                1
            } else {
                -1
            };
            if !change_typed_building_level(account, building_id, delta) {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "building level change is invalid",
                ));
            }
            append_typed_building_refresh(pre_pushes, account, now);
            HandlerResult::PushOnly
        }
        "building.FinishBuilding" | "building.UseStrengthSpeedup" => {
            let building_id = decode_varint_field(request_args, 1);
            let Some(building_id) = u64::try_from(building_id).ok() else {
                return HandlerResult::Error(GameError::InvalidRequest("building id is invalid"));
            };
            if !account.buildings.levels.contains_key(&building_id) {
                return HandlerResult::Error(GameError::InvalidRequest("building was not found"));
            }
            append_typed_building_refresh(pre_pushes, account, now);
            HandlerResult::PushOnly
        }
        "building.UpdateHeroAddition" => {
            append_typed_building_refresh(pre_pushes, account, now);
            HandlerResult::PushOnly
        }
        "building.SetHero" | "building.SetBuildingListHero" => {
            let assignments = if method == "building.SetHero" {
                vec![(
                    decode_varint_field(request_args, 1),
                    decode_repeated_i32_field(request_args, 2),
                )]
            } else {
                decode_building_assignments(request_args)
            };
            if !set_typed_building_assignments(account, &assignments, building_catalog) {
                return HandlerResult::Error(GameError::InvalidRequest(
                    "building assignment is invalid",
                ));
            }
            append_typed_building_refresh(pre_pushes, account, now);
            HandlerResult::PushOnly
        }
        _ => HandlerResult::Empty,
    }
}

fn add_typed_building(
    account: &mut blueoath_domain::AccountState,
    template_id: i32,
    land_index: i32,
) -> Option<u64> {
    if template_id <= 0 || land_index <= 0 {
        return None;
    }
    if account
        .buildings
        .land_indices
        .values()
        .any(|index| i32::try_from(*index).ok() == Some(land_index))
    {
        return None;
    }
    let building_id = account
        .buildings
        .levels
        .keys()
        .copied()
        .max()
        .unwrap_or_default()
        .saturating_add(1);
    account.buildings.levels.insert(building_id, 1);
    account
        .buildings
        .template_ids
        .insert(building_id, u64::try_from(template_id).ok()?);
    account
        .buildings
        .land_indices
        .insert(building_id, u32::try_from(land_index).ok()?);
    Some(building_id)
}

fn change_typed_building_level(
    account: &mut blueoath_domain::AccountState,
    building_id: i32,
    delta: i32,
) -> bool {
    if building_id <= 0 || delta == 0 {
        return false;
    }
    let Some(building_id) = u64::try_from(building_id).ok() else {
        return false;
    };
    let Some(level) = account.buildings.levels.get_mut(&building_id) else {
        return false;
    };
    let next = i64::from(*level).saturating_add(i64::from(delta));
    if next <= 0 {
        return false;
    }
    *level = u32::try_from(next).unwrap_or(u32::MAX);
    true
}

fn set_typed_building_assignments(
    account: &mut blueoath_domain::AccountState,
    assignments: &[(i32, Vec<i32>)],
    catalog: Option<&BuildingCatalog>,
) -> bool {
    if assignments.is_empty() {
        return false;
    }
    let mut moving = std::collections::BTreeSet::new();
    let mut seen_buildings = std::collections::BTreeSet::new();
    for (building_id, hero_ids) in assignments {
        let Ok(building_id) = u64::try_from(*building_id) else {
            return false;
        };
        if !seen_buildings.insert(building_id)
            || !account.buildings.levels.contains_key(&building_id)
        {
            return false;
        }
        let template_id = account
            .buildings
            .template_ids
            .get(&building_id)
            .copied()
            .unwrap_or_default();
        let level = account
            .buildings
            .levels
            .get(&building_id)
            .copied()
            .unwrap_or_default();
        if hero_ids.len()
            > building_capacity(
                i32::try_from(template_id).unwrap_or_default(),
                i32::try_from(level).unwrap_or_default(),
                catalog,
            )
        {
            return false;
        }
        for hero_id in hero_ids {
            let Ok(hero_id) = u64::try_from(*hero_id) else {
                return false;
            };
            let Some(hero_id) = blueoath_domain::HeroId::new(hero_id).ok() else {
                return false;
            };
            if !account.dock.heroes.contains_key(&hero_id) || !moving.insert(hero_id) {
                return false;
            }
        }
    }
    for hero_ids in account.buildings.hero_assignments.values_mut() {
        hero_ids.retain(|hero_id| !moving.contains(hero_id));
    }
    for (building_id, hero_ids) in assignments {
        let building_id = u64::try_from(*building_id).unwrap_or_default();
        let converted = hero_ids
            .iter()
            .filter_map(|hero_id| u64::try_from(*hero_id).ok())
            .filter_map(|hero_id| blueoath_domain::HeroId::new(hero_id).ok())
            .collect();
        account
            .buildings
            .hero_assignments
            .insert(building_id, converted);
    }
    account
        .buildings
        .hero_assignments
        .retain(|_, hero_ids| !hero_ids.is_empty());
    true
}

fn append_typed_building_refresh(
    pushes: &mut Vec<Vec<u8>>,
    account: &blueoath_domain::AccountState,
    now: u32,
) {
    append_method_push(
        pushes,
        "building.UpdateBuildingInfo",
        UserBuildingInfoCodec::encode(&building_info_from_typed_account(account, now)),
    );
}

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let state = context.state;
    let account = &mut *context.account;
    let catalogs = context.catalogs;
    let GameLoginCatalogs {
        fashion: fashion_catalog,
        equip: equip_catalog,
        handbook_behaviours,
        tasks: task_catalog,
        buildings: building_catalog,
        ..
    } = catalogs;
    let account_view = account.as_deref();
    let pre_pushes = &mut *context.pre_pushes;

    match method {
        "building.AddBuilding" => {
            let template_id = decode_varint_field(request_args, 1);
            let land_index = decode_varint_field(request_args, 2);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            {
                if let Some(building_id) =
                    add_building_state(account, template_id, land_index, current_unix_seconds())
                {
                    append_building_refresh(pre_pushes, account);
                    let mut ret = Vec::new();
                    append_varint_field(&mut ret, 1, building_id.max(0) as u64);
                    reply(method, ret)
                } else {
                    invalid("building placement is invalid")
                }
            }
        }
        "building.UpgradeBuilding" | "building.DegradeBuilding" => {
            let building_id = decode_varint_field(request_args, 1);
            let delta = if method == "building.UpgradeBuilding" {
                1
            } else {
                -1
            };
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            {
                if change_building_level(account, building_id, delta) {
                    append_building_refresh(pre_pushes, account);
                    HandlerResult::PushOnly
                } else {
                    invalid("building level change is invalid")
                }
            }
        }
        "building.FinishBuilding" => {
            let building_id = decode_varint_field(request_args, 1);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            {
                if finish_building_state(account, building_id, current_unix_seconds()) {
                    append_building_refresh(pre_pushes, account);
                    HandlerResult::PushOnly
                } else {
                    invalid("building was not found")
                }
            }
        }
        "building.UpdateBuildingInfo" => reply(
            method,
            UserBuildingInfoCodec::encode(&building_info_from_account(
                account_view.unwrap_or(&Value::Null),
                current_unix_seconds(),
            )),
        ),
        "building.UpdateHeroAddition" => {
            append_method_push(
                pre_pushes,
                "building.UpdateBuildingInfo",
                UserBuildingInfoCodec::encode(&building_info_from_account(
                    account_view.unwrap_or(&Value::Null),
                    current_unix_seconds(),
                )),
            );
            HandlerResult::PushOnly
        }
        "building.SetHero" | "building.SetBuildingListHero" => {
            let assignments = if method == "building.SetHero" {
                let building_id = decode_varint_field(request_args, 1);
                // Hero arrays may be encoded packed (wire type 2) by protobuf clients.
                let hero_ids = decode_repeated_i32_field(request_args, 2);
                vec![(building_id, hero_ids)]
            } else {
                decode_building_assignments(request_args)
            };
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            {
                if !update_building_assignments(
                    account,
                    &assignments,
                    current_unix_seconds(),
                    building_catalog,
                ) {
                    return invalid("building assignment is invalid");
                } else {
                    append_method_push(
                        pre_pushes,
                        "building.UpdateBuildingInfo",
                        UserBuildingInfoCodec::encode(&building_info_from_account(
                            account,
                            current_unix_seconds(),
                        )),
                    );
                }
            }
            HandlerResult::PushOnly
        }
        "building.ProduceItem" | "building.ComposeItem" => {
            let building_id = decode_varint_field(request_args, 1);
            let recipe_id = decode_varint_field(request_args, 2);
            let count = decode_varint_field(request_args, 3);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            {
                if set_building_production(
                    account,
                    building_id,
                    recipe_id,
                    count,
                    current_unix_seconds(),
                ) {
                    append_building_refresh(pre_pushes, account);
                    HandlerResult::PushOnly
                } else {
                    invalid("building production request is invalid")
                }
            }
        }
        "building.ReceiveBuilding" | "building.ReceiveItem" | "building.ReceiveAll" => {
            let building_id = if method == "building.ReceiveAll" {
                None
            } else {
                Some(decode_varint_field(request_args, 1))
            };
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            {
                let now = current_unix_seconds();
                let rewards = collect_building_rewards(
                    account,
                    building_catalog,
                    building_id,
                    None,
                    now,
                    state.building_oil_multiplier,
                    state.building_gold_multiplier,
                )
                .into_iter()
                .map(|reward| grant_reward(account, reward, now, fashion_catalog))
                .collect::<Vec<_>>();
                if !rewards.is_empty() {
                    append_building_refresh(pre_pushes, account);
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
                }
                reply(method, encode_rewards_list(&rewards))
            }
        }
        "building.ReceiveResource" => {
            let resource_id = decode_varint_field(request_args, 1);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            {
                let now = current_unix_seconds();
                let rewards = collect_building_rewards(
                    account,
                    building_catalog,
                    None,
                    Some(resource_id),
                    now,
                    state.building_oil_multiplier,
                    state.building_gold_multiplier,
                )
                .into_iter()
                .map(|reward| grant_reward(account, reward, now, fashion_catalog))
                .collect::<Vec<_>>();
                if !rewards.is_empty() {
                    append_building_refresh(pre_pushes, account);
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
                }
                reply(method, encode_rewards_list(&rewards))
            }
        }
        "building.UseStrengthSpeedup" => {
            let building_id = decode_varint_field(request_args, 1);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            {
                if finish_building_state(account, building_id, current_unix_seconds()) {
                    append_building_refresh(pre_pushes, account);
                } else {
                    return invalid("building was not found");
                }
            }
            HandlerResult::PushOnly
        }
        "building.TriggerNormalHeroPlot" | "building.TriggerSpecialHeroPlot" => {
            // Client removes plot marker after a successful empty TTriggerPlotRet response.
            HandlerResult::PushOnly
        }
        "building.SaveTactic" | "building.SetTacticName" | "building.RemoveTactic" => {
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            {
                if update_building_tactics(account, method, request_args) {
                    append_building_refresh(pre_pushes, account);
                } else {
                    return invalid("building tactic request is invalid");
                }
            }
            HandlerResult::PushOnly
        }
        "build.BuildInfo" | "build.BuildsInfo" => reply(
            method,
            construction_info_payload(account_view.unwrap_or(&Value::Null), current_unix_seconds()),
        ),
        "buildnotes.GetNotesList" | "buildnotes.GiveLike" => {
            reply(method, build_notes_payload(current_unix_seconds()))
        }
        "discuss.GetDiscuss" => {
            let htid = decode_varint_field(request_args, 1);
            reply(method, discuss_payload(htid))
        }
        "discuss.HeroLike" => reply(method, encode_discuss_empty()),
        "discuss.Discuss" | "discuss.Like" | "discuss.Dislike" => {
            let Some(_account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            reply(method, encode_discuss_empty())
        }
        "build.BuildingByFormula" => {
            let projects = decode_construction_projects(request_args);
            if projects.is_empty() || projects.len() > 10 {
                invalid("construction project count is invalid")
            } else {
                let Some(account) = account.as_deref_mut() else {
                    return HandlerResult::Error(GameError::AccountUnavailable);
                };
                match start_construction(account, &projects, current_unix_seconds()) {
                    Ok(()) => {
                        advance_task_event(
                            account,
                            task_catalog,
                            500,
                            projects.len() as i32,
                            current_unix_seconds(),
                        );
                        append_method_push(
                            pre_pushes,
                            "task.TaskInfo",
                            task_info_payload(account, task_catalog),
                        );
                        append_method_push(
                            pre_pushes,
                            "build.BuildsInfo",
                            construction_info_payload(account, current_unix_seconds()),
                        );
                        append_method_push(
                            pre_pushes,
                            "bag.UpdateBagData",
                            BagInfoCodec::encode(&bag_info_from_account(account)),
                        );
                        append_method_push(
                            pre_pushes,
                            "user.UpdateUserInfo",
                            UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                        );
                        HandlerResult::PushOnly
                    }
                    Err(error) => invalid(error),
                }
            }
        }
        "build.BuildQuicklyFinish" => {
            let indexes = decode_repeated_varint_field(request_args, 1);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            {
                if finish_construction(account, &indexes, current_unix_seconds()) {
                    append_method_push(
                        pre_pushes,
                        "build.BuildsInfo",
                        construction_info_payload(account, current_unix_seconds()),
                    );
                    append_method_push(
                        pre_pushes,
                        "bag.UpdateBagData",
                        BagInfoCodec::encode(&bag_info_from_account(account)),
                    );
                    HandlerResult::PushOnly
                } else {
                    invalid("construction quick-finish failed")
                }
            }
        }
        "build.BuildReceive" => {
            let indexes = decode_repeated_varint_field(request_args, 1);
            let Some(account) = account.as_deref_mut() else {
                return HandlerResult::Error(GameError::AccountUnavailable);
            };
            {
                let (ret, added) = receive_construction(account, &indexes, current_unix_seconds());
                if added == 0 {
                    return invalid("no completed construction");
                } else {
                    pre_pushes.push(encode_hero_bag_push(account));
                    append_method_push(
                        pre_pushes,
                        "build.BuildsInfo",
                        construction_info_payload(account, current_unix_seconds()),
                    );
                    append_method_push(
                        pre_pushes,
                        "equip.UpdateEquipBagData",
                        EquipListCodec::encode(&equip_list_from_account(account, equip_catalog)),
                    );
                    append_method_push(
                        pre_pushes,
                        "illustrate.IllustrateInfo",
                        illustrate_info_payload(account, handbook_behaviours, None),
                    );
                }
                reply(method, ret)
            }
        }
        _ => HandlerResult::Empty,
    }
}

fn reply(method: &str, payload: Vec<u8>) -> HandlerResult {
    HandlerResult::Reply(Response::raw(method, payload))
}

fn invalid(message: &'static str) -> HandlerResult {
    HandlerResult::Error(GameError::InvalidRequest(message))
}

fn append_building_refresh(pushes: &mut Vec<Vec<u8>>, account: &Value) {
    append_method_push(
        pushes,
        "building.UpdateBuildingInfo",
        UserBuildingInfoCodec::encode(&building_info_from_account(account, current_unix_seconds())),
    );
}

fn update_building_tactics(account: &mut Value, method: &str, args: &[u8]) -> bool {
    let Some(buildings) = account
        .get_mut("building")
        .and_then(|value| value.get_mut("buildings"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut entries = Vec::new();
    if method == "building.SaveTactic" {
        for nested in decode_repeated_message_field(args, 1) {
            let building_id = decode_varint_field(&nested, 1);
            let index = decode_varint_field(&nested, 4);
            if building_id <= 0 || index <= 0 {
                return false;
            }
            entries.push((building_id, index, nested));
        }
        if entries.is_empty() {
            return false;
        }
    } else {
        entries.push((
            decode_varint_field(args, 1),
            decode_varint_field(args, 2),
            args.to_vec(),
        ));
    }
    for (building_id, index, nested) in entries {
        let Some(building) = buildings
            .iter_mut()
            .find(|building| json_i32(building, "id") == Some(building_id))
        else {
            return false;
        };
        let tactics = building
            .as_object_mut()
            .map(|object| {
                object
                    .entry("tacticList".to_owned())
                    .or_insert_with(|| serde_json::json!([]))
            })
            .and_then(Value::as_array_mut);
        let Some(tactics) = tactics else { return false };
        if method == "building.RemoveTactic" {
            tactics.retain(|tactic| json_i32(tactic, "index") != Some(index));
            continue;
        }
        let name = decode_string_field(&nested, 2).unwrap_or_default();
        let hero_ids = decode_repeated_i32_field(&nested, 3);
        let value = serde_json::json!({"index": index, "name": name, "heroIds": hero_ids});
        if let Some(existing) = tactics
            .iter_mut()
            .find(|tactic| json_i32(tactic, "index") == Some(index))
        {
            *existing = value;
        } else {
            tactics.push(value);
        }
    }
    true
}
