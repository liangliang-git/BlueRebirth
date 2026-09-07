use serde_json::Value;

use super::*;

pub(super) fn handle<'state, 'account, 'scratch>(
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
        buildings: building_catalog,
        ..
    } = catalogs;
    let account_view = account.as_deref();
    let pre_pushes = &mut *context.pre_pushes;
    let response_err = &mut *context.response_err;
    let response_err_msg = &mut *context.response_err_msg;

    match method {
        "building.AddBuilding" => {
            let template_id = decode_varint_field(request_args, 1);
            let land_index = decode_varint_field(request_args, 2);
            if let Some(account) = account.as_deref_mut() {
                if let Some(building_id) =
                    add_building_state(account, template_id, land_index, current_unix_seconds())
                {
                    append_building_refresh(pre_pushes, account);
                    let mut ret = Vec::new();
                    append_varint_field(&mut ret, 1, building_id.max(0) as u64);
                    Some(ret)
                } else {
                    *response_err = 1;
                    *response_err_msg = "building placement is invalid".to_owned();
                    Some(Vec::new())
                }
            } else {
                Some(Vec::new())
            }
        }
        "building.UpgradeBuilding" | "building.DegradeBuilding" => {
            let building_id = decode_varint_field(request_args, 1);
            let delta = if method == "building.UpgradeBuilding" {
                1
            } else {
                -1
            };
            if let Some(account) = account.as_deref_mut() {
                if change_building_level(account, building_id, delta) {
                    append_building_refresh(pre_pushes, account);
                    Some(Vec::new())
                } else {
                    *response_err = if method == "building.DegradeBuilding" {
                        3409
                    } else {
                        1
                    };
                    *response_err_msg = "building level change is invalid".to_owned();
                    Some(Vec::new())
                }
            } else {
                Some(Vec::new())
            }
        }
        "building.FinishBuilding" => {
            let building_id = decode_varint_field(request_args, 1);
            if let Some(account) = account.as_deref_mut() {
                if finish_building_state(account, building_id, current_unix_seconds()) {
                    append_building_refresh(pre_pushes, account);
                    Some(Vec::new())
                } else {
                    *response_err = 1;
                    *response_err_msg = "building was not found".to_owned();
                    Some(Vec::new())
                }
            } else {
                Some(Vec::new())
            }
        }
        "building.UpdateBuildingInfo" => {
            Some(UserBuildingInfoCodec::encode(&building_info_from_account(
                account_view.unwrap_or(&Value::Null),
                current_unix_seconds(),
            )))
        }
        "building.UpdateHeroAddition" => {
            append_method_push(
                pre_pushes,
                "building.UpdateBuildingInfo",
                UserBuildingInfoCodec::encode(&building_info_from_account(
                    account_view.unwrap_or(&Value::Null),
                    current_unix_seconds(),
                )),
            );
            Some(Vec::new())
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
            if let Some(account) = account.as_deref_mut() {
                if !update_building_assignments(
                    account,
                    &assignments,
                    current_unix_seconds(),
                    building_catalog,
                ) {
                    *response_err = 1;
                    *response_err_msg = "building assignment is invalid".to_owned();
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
            Some(Vec::new())
        }
        "building.ProduceItem" | "building.ComposeItem" => {
            let building_id = decode_varint_field(request_args, 1);
            let recipe_id = decode_varint_field(request_args, 2);
            let count = decode_varint_field(request_args, 3);
            if let Some(account) = account.as_deref_mut() {
                if set_building_production(
                    account,
                    building_id,
                    recipe_id,
                    count,
                    current_unix_seconds(),
                ) {
                    append_building_refresh(pre_pushes, account);
                    Some(Vec::new())
                } else {
                    *response_err = 1;
                    *response_err_msg = "building production request is invalid".to_owned();
                    Some(Vec::new())
                }
            } else {
                Some(Vec::new())
            }
        }
        "building.ReceiveBuilding" | "building.ReceiveItem" | "building.ReceiveAll" => {
            let building_id = if method == "building.ReceiveAll" {
                None
            } else {
                Some(decode_varint_field(request_args, 1))
            };
            if let Some(account) = account.as_deref_mut() {
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
                return Some(encode_rewards_list(&rewards));
            }
            Some(encode_rewards_list(&[]))
        }
        "building.ReceiveResource" => {
            let resource_id = decode_varint_field(request_args, 1);
            if let Some(account) = account.as_deref_mut() {
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
                Some(encode_rewards_list(&rewards))
            } else {
                Some(encode_rewards_list(&[]))
            }
        }
        "building.UseStrengthSpeedup" => {
            let building_id = decode_varint_field(request_args, 1);
            if let Some(account) = account.as_deref_mut() {
                if finish_building_state(account, building_id, current_unix_seconds()) {
                    append_building_refresh(pre_pushes, account);
                } else {
                    *response_err = 1;
                    *response_err_msg = "building was not found".to_owned();
                }
            }
            Some(Vec::new())
        }
        "building.TriggerNormalHeroPlot" | "building.TriggerSpecialHeroPlot" => {
            // Client removes plot marker after a successful empty TTriggerPlotRet response.
            Some(Vec::new())
        }
        "building.SaveTactic" | "building.SetTacticName" | "building.RemoveTactic" => {
            if let Some(account) = account.as_deref_mut() {
                if update_building_tactics(account, method, request_args) {
                    append_building_refresh(pre_pushes, account);
                } else {
                    *response_err = 1;
                    *response_err_msg = "building tactic request is invalid".to_owned();
                }
            }
            Some(Vec::new())
        }
        "build.BuildInfo" | "build.BuildsInfo" => Some(construction_info_payload(
            account_view.unwrap_or(&Value::Null),
            current_unix_seconds(),
        )),
        "buildnotes.GetNotesList" | "buildnotes.GiveLike" => {
            Some(build_notes_payload(current_unix_seconds()))
        }
        "discuss.GetDiscuss" => {
            let htid = decode_varint_field(request_args, 1);
            Some(discuss_payload(htid))
        }
        "discuss.HeroLike" => Some(encode_discuss_empty()),
        "discuss.Discuss" | "discuss.Like" | "discuss.Dislike" => {
            if let Some(account) = account.as_deref_mut() {
                account["lastDiscussAction"] = serde_json::json!({
                    "method": method,
                    "args": request_args,
                    "time": current_unix_seconds(),
                });
            }
            Some(encode_discuss_empty())
        }
        "build.BuildingByFormula" => {
            let projects = decode_construction_projects(request_args);
            if projects.is_empty() || projects.len() > 10 {
                *response_err = 1;
                *response_err_msg = "construction project count is invalid".to_owned();
                Some(Vec::new())
            } else if let Some(account) = account.as_deref_mut() {
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
                        Some(Vec::new())
                    }
                    Err(error) => {
                        *response_err = 1;
                        *response_err_msg = error.to_owned();
                        Some(Vec::new())
                    }
                }
            } else {
                Some(Vec::new())
            }
        }
        "build.BuildQuicklyFinish" => {
            let indexes = decode_repeated_varint_field(request_args, 1);
            if let Some(account) = account.as_deref_mut() {
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
                    Some(Vec::new())
                } else {
                    *response_err = 1;
                    *response_err_msg = "construction quick-finish failed".to_owned();
                    Some(Vec::new())
                }
            } else {
                Some(Vec::new())
            }
        }
        "build.BuildReceive" => {
            let indexes = decode_repeated_varint_field(request_args, 1);
            if let Some(account) = account.as_deref_mut() {
                let (ret, added) = receive_construction(account, &indexes, current_unix_seconds());
                if added == 0 {
                    *response_err = 1;
                    *response_err_msg = "no completed construction".to_owned();
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
                Some(ret)
            } else {
                Some(Vec::new())
            }
        }
        _ => None,
    }
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
