//! Transitional feature boundary for low-traffic protocol methods.
//!
//! New routes must not be added here. Existing methods move into typed feature
//! modules as their request and state models are completed.

use serde_json::{json, Value};

use super::battle_state::{battle_copy_passed, record_battle_pass};
use super::catalog::GameLoginCatalogs;
use super::common::error::GameError;
use super::common::response::{HandlerResult, Response};
use super::*;

const HP_COEFFICIENT: i64 = 10_000_000_000;
const OATH_RING_TEMPLATE: i32 = 10_180;

pub(super) fn handle<'state, 'account, 'scratch>(
    context: &mut GameLoginRequestContext<'state, 'account, 'scratch>,
    method: &str,
    request_args: &[u8],
) -> HandlerResult {
    let payload = handle_legacy(context, method, request_args);
    if *context.response_err != 0 {
        HandlerResult::Error(GameError::Internal(context.response_err_msg.clone()))
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
    let GameLoginCatalogs {
        affection: affection_catalog,
        chapters: chapter_catalog,
        equip: equip_catalog,
        fashion: fashion_catalog,
        handbook_behaviours,
        tasks: task_catalog,
        combination: combination_catalog,
        ..
    } = context.catalogs;
    let pre_pushes = &mut *context.pre_pushes;
    let response_err = &mut *context.response_err;
    let response_err_msg = &mut *context.response_err_msg;

    match method {
        "cachedata.CacheData" => Some(cache_data_payload()),
        "user.GetHeadBuyCount" => Some(head_buy_count_payload()),
        "user.BuyHead" | "user.NewHeadUnlockedList" => Some(head_unlocked_payload(
            account.as_deref().unwrap_or(&Value::Null),
        )),
        "illustrate.VowDecTime" => {
            let items = decode_repeated_message_field(request_args, 1)
                .into_iter()
                .map(|item| (decode_varint_field(&item, 1), decode_varint_field(&item, 2)))
                .filter(|(item_id, count)| *item_id > 0 && *count > 0)
                .collect::<Vec<_>>();
            if items.is_empty() {
                *response_err = 1;
                *response_err_msg = "wish cooldown item list is empty".to_owned();
                Some(Vec::new())
            } else if let Some(account) = account.as_deref_mut() {
                if items
                    .iter()
                    .any(|(item_id, count)| bag_item_count(account, *item_id) < i64::from(*count))
                {
                    *response_err = 1;
                    *response_err_msg = "not enough wish cooldown items".to_owned();
                    Some(Vec::new())
                } else {
                    for (item_id, count) in items {
                        let _ = consume_bag_item(account, item_id, count);
                    }
                    let mut ret = Vec::new();
                    append_varint_field(&mut ret, 1, 0);
                    append_varint_field(&mut ret, 2, 0);
                    append_method_push(
                        pre_pushes,
                        "bag.UpdateBagData",
                        BagInfoCodec::encode(&bag_info_from_account(account)),
                    );
                    Some(ret)
                }
            } else {
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
                Some(Vec::new())
            }
        }
        "hero.Marry" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let marry_type = decode_varint_field(request_args, 2);
            let Some(account) = account.as_deref_mut() else {
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
                return Some(Vec::new());
            };
            if hero_id == 0 || !(1..=2).contains(&marry_type) {
                *response_err = 1;
                *response_err_msg = "marriage request is invalid".to_owned();
                return Some(Vec::new());
            }
            let Some(hero) = account
                .get("dock")
                .and_then(|dock| dock.get("heroes"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
            else {
                *response_err = 1;
                *response_err_msg = "hero was not found".to_owned();
                return Some(Vec::new());
            };
            if json_i64(hero, "marryTime").unwrap_or_default() != 0 {
                *response_err = 1;
                *response_err_msg = "hero is already married".to_owned();
                return Some(Vec::new());
            }
            if bag_item_count(account, OATH_RING_TEMPLATE) < 1 {
                *response_err = 1;
                *response_err_msg = "oath ring is missing".to_owned();
                return Some(Vec::new());
            }
            let now = current_unix_seconds();
            let Some(hero) = find_hero_mut(account, hero_id) else {
                *response_err = 1;
                *response_err_msg = "hero was not found".to_owned();
                return Some(Vec::new());
            };
            hero.insert("marryTime".to_owned(), json!(now));
            hero.insert("marryType".to_owned(), json!(marry_type));
            let _ = consume_bag_item(account, OATH_RING_TEMPLATE, 1);
            add_character_i64(account, "marriedNum", 1);
            pre_pushes.push(encode_hero_bag_push(account));
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
        "hero.AddAffection" => {
            let hero_id = decode_varint_u64_field(request_args, 1);
            let item_id = decode_varint_field(request_args, 2);
            let requested = decode_varint_field(request_args, 3);
            let Some(exp_per_item) = affection_catalog
                .and_then(|catalog| catalog.exp_by_item.get(&item_id))
                .copied()
                .filter(|exp| *exp > 0)
            else {
                *response_err = 1;
                *response_err_msg = "affection item configuration was not found".to_owned();
                return Some(Vec::new());
            };
            let Some(account) = account.as_deref_mut() else {
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
                return Some(Vec::new());
            };
            let Some(hero) = account
                .get("dock")
                .and_then(|dock| dock.get("heroes"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
            else {
                *response_err = 1;
                *response_err_msg = "hero was not found".to_owned();
                return Some(Vec::new());
            };
            let current = json_i64(hero, "affection").unwrap_or_default().max(0);
            let max = if json_i64(hero, "marryTime").unwrap_or_default() == 0 {
                1_000_000i64
            } else {
                2_000_000i64
            };
            let room = max.saturating_sub(current);
            let count = i64::from(requested.max(0))
                .min(bag_item_count(account, item_id))
                .min((room + i64::from(exp_per_item) - 1) / i64::from(exp_per_item));
            if count <= 0 {
                *response_err = 1;
                *response_err_msg = "affection gift cannot be used".to_owned();
                return Some(Vec::new());
            }
            let gained = count.saturating_mul(i64::from(exp_per_item)).min(room);
            let _ = consume_bag_item(account, item_id, count as i32);
            if let Some(hero) = find_hero_mut(account, hero_id) {
                hero.insert("affection".to_owned(), json!(current + gained));
            }
            pre_pushes.push(encode_hero_bag_push(account));
            append_method_push(
                pre_pushes,
                "bag.UpdateBagData",
                BagInfoCodec::encode(&bag_info_from_account(account)),
            );
            let mut ret = Vec::new();
            append_varint_field(&mut ret, 1, 0);
            append_varint_field(&mut ret, 2, hero_id);
            append_varint_field(&mut ret, 3, (current + gained) as u64);
            Some(ret)
        }
        "repair.RepairHero" => {
            let hero_ids = decode_repeated_varint_field(request_args, 1)
                .into_iter()
                .filter(|id| *id > 0)
                .map(|id| id as u64)
                .collect::<std::collections::BTreeSet<_>>();
            let Some(account) = account.as_deref_mut() else {
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
                return Some(Vec::new());
            };
            let total_cost = hero_ids
                .iter()
                .filter_map(|hero_id| {
                    let hero = account
                        .get("dock")
                        .and_then(|dock| dock.get("heroes"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .find(|hero| json_u64(hero, "heroId") == Some(*hero_id))?;
                    let cur_hp = hero
                        .get("curHp")
                        .and_then(Value::as_i64)
                        .unwrap_or(HP_COEFFICIENT);
                    if cur_hp >= HP_COEFFICIENT {
                        return Some(0);
                    }
                    let template_id = json_i32(hero, "templateId")?;
                    let fixed_money = SHIP_STAT_CATALOG
                        .get()
                        .and_then(|catalog| catalog.by_template.get(&template_id))
                        .map(|stats| stats.fixed_money)
                        .unwrap_or_default();
                    Some(
                        (fixed_money.max(0) * (HP_COEFFICIENT - cur_hp.max(0)) + HP_COEFFICIENT
                            - 1)
                            / HP_COEFFICIENT,
                    )
                })
                .sum::<i64>();
            if character_i64(account, "gold") < total_cost {
                *response_err = 1;
                *response_err_msg = "not enough gold to repair heroes".to_owned();
                return Some(Vec::new());
            }
            adjust_character_i64(account, "gold", -total_cost);
            let mut changed = false;
            for hero_id in hero_ids {
                if let Some(hero) = find_hero_mut(account, hero_id) {
                    let cur_hp = hero
                        .get("curHp")
                        .and_then(Value::as_i64)
                        .unwrap_or(HP_COEFFICIENT);
                    if cur_hp < HP_COEFFICIENT {
                        hero.insert("curHp".to_owned(), json!(HP_COEFFICIENT));
                        changed = true;
                    }
                }
            }
            if changed {
                pre_pushes.push(encode_hero_bag_push(account));
                append_method_push(
                    pre_pushes,
                    "user.UpdateUserInfo",
                    UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
                );
            }
            Some(Vec::new())
        }
        "illustrate.VowHero" => {
            let ship_info_id = decode_repeated_varint_field(request_args, 1)
                .into_iter()
                .find(|id| *id > 0)
                .unwrap_or_default();
            let Some(account) = account.as_deref_mut() else {
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
                return Some(Vec::new());
            };
            if ship_info_id == 0 {
                *response_err = 1;
                *response_err_msg = "wish hero is invalid".to_owned();
                return Some(Vec::new());
            }
            let template_id = ship_info_id.saturating_mul(10).saturating_add(1);
            let hero_id = add_ship_items(account, template_id, 1, current_unix_seconds());
            pre_pushes.push(encode_hero_bag_push(account));
            append_method_push(
                pre_pushes,
                "illustrate.IllustrateInfo",
                illustrate_info_payload_for_templates(&[template_id], handbook_behaviours),
            );
            let mut ret = Vec::new();
            append_varint_field(&mut ret, 1, 3);
            append_varint_field(&mut ret, 2, template_id as u64);
            append_varint_field(&mut ret, 3, 1);
            append_varint_field(&mut ret, 4, hero_id as u64);
            Some(ret)
        }
        "illustrate.AddBehaviour" => add_illustrate_behaviour(
            account,
            request_args,
            pre_pushes,
            handbook_behaviours,
            response_err,
            response_err_msg,
        ),
        "illustrate.IllustrateNew" => {
            illustrate_new(account, request_args, response_err, response_err_msg)
        }
        "illustrate.EquipNew" => {
            illustrate_equip_new(account, request_args, response_err, response_err_msg)
        }
        "illustrate.ModiVowHeroList" => {
            modify_vow_hero_list(account, request_args, pre_pushes, handbook_behaviours)
        }
        "bag.GetNormalTreasureInfo" => open_normal_treasure(
            account,
            request_args,
            &mut TreasureContext {
                pre_pushes,
                response_err,
                response_err_msg,
                fashion_catalog,
                equip_catalog,
            },
        ),
        "bag.GetSelectTreasureInfo" => open_select_treasure(
            account,
            request_args,
            &mut TreasureContext {
                pre_pushes,
                response_err,
                response_err_msg,
                fashion_catalog,
                equip_catalog,
            },
        ),
        "copy.StarReward" => claim_copy_star_reward(
            state,
            account,
            request_args,
            chapter_catalog,
            task_catalog,
            fashion_catalog,
            equip_catalog,
            pre_pushes,
            response_err,
            response_err_msg,
        ),
        "copy.PassMiniGame" => pass_mini_game(
            fashion_catalog,
            account,
            request_args,
            chapter_catalog,
            context.catalogs.battle,
            pre_pushes,
            response_err,
            response_err_msg,
        ),
        "copy.FetchRewardBox" => claim_copy_star_reward(
            state,
            account,
            request_args,
            chapter_catalog,
            task_catalog,
            fashion_catalog,
            equip_catalog,
            pre_pushes,
            response_err,
            response_err_msg,
        ),
        "hero.HeroCombine" => combine_hero_relation(
            account,
            request_args,
            combination_catalog,
            pre_pushes,
            response_err,
            response_err_msg,
        ),
        "hero.HeroCombineUpLv" => upgrade_combination(
            account,
            request_args,
            combination_catalog,
            false,
            pre_pushes,
            response_err,
            response_err_msg,
        ),
        "hero.HeroCombineQuickLevelUp" => upgrade_combination(
            account,
            request_args,
            combination_catalog,
            true,
            pre_pushes,
            response_err,
            response_err_msg,
        ),
        "hero.HeroCombineBreak" => break_combination(
            account,
            request_args,
            combination_catalog,
            pre_pushes,
            response_err,
            response_err_msg,
        ),
        "fashion.fashionReplaceReward" => Some(fashion_replace_reward_payload(
            account.as_deref().unwrap_or(&Value::Null),
        )),
        "copy.DotBase" | "copyinfo.DotBase" => {
            let copy_id = decode_varint_field(request_args, 1);
            if copy_id <= 0 {
                *response_err = 1;
                *response_err_msg = "copy id is invalid".to_owned();
                return Some(Vec::new());
            }
            if let Some(account) = account.as_deref_mut() {
                account["copyDotBase"] = json!({
                    "copyId": copy_id,
                    "time": current_unix_seconds(),
                });
            }
            Some(Vec::new())
        }
        "task.GetTeachingTask" => Some(task_info_payload(
            account.as_deref().unwrap_or(&Value::Null),
            task_catalog,
        )),
        "task.GetPtReward" => {
            let reward_id = decode_varint_field(request_args, 1);
            let Some(account) = account.as_deref_mut() else {
                *response_err = 1;
                *response_err_msg = "account is unavailable".to_owned();
                return Some(Vec::new());
            };
            let Some(catalog) = task_catalog else {
                *response_err = 1;
                *response_err_msg = "teaching reward catalog is unavailable".to_owned();
                return Some(Vec::new());
            };
            let already_claimed = account
                .get("tasks")
                .and_then(|tasks| tasks.get("teachingPtRewardIds"))
                .and_then(Value::as_array)
                .is_some_and(|ids| {
                    ids.iter()
                        .any(|id| id.as_i64() == Some(i64::from(reward_id)))
                });
            let Some(configured_reward_id) = catalog.teaching_rewards_by_id.get(&reward_id) else {
                *response_err = 1;
                *response_err_msg = "teaching reward does not exist".to_owned();
                return Some(Vec::new());
            };
            if already_claimed {
                *response_err = 1;
                *response_err_msg = "teaching reward was already claimed".to_owned();
                return Some(Vec::new());
            }
            let configured = catalog
                .rewards_by_id
                .get(configured_reward_id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|(goods_type, item_id, num)| ShopReward {
                    goods_type,
                    item_id,
                    num,
                    instance_id: 0,
                })
                .collect::<Vec<_>>();
            if configured.is_empty() {
                *response_err = 1;
                *response_err_msg = "teaching reward is not configured".to_owned();
                return Some(Vec::new());
            }
            let now = current_unix_seconds();
            let rewards = configured
                .into_iter()
                .map(|reward| grant_reward(account, reward, now, fashion_catalog))
                .collect::<Vec<_>>();
            let tasks = account
                .as_object_mut()
                .and_then(|root| root.get_mut("tasks"))
                .and_then(Value::as_object_mut);
            let Some(tasks) = tasks else {
                *response_err = 1;
                *response_err_msg = "task state is unavailable".to_owned();
                return Some(Vec::new());
            };
            let ids = tasks
                .entry("teachingPtRewardIds".to_owned())
                .or_insert_with(|| json!([]));
            if let Some(ids) = ids.as_array_mut() {
                ids.push(json!(reward_id));
            }
            pre_pushes.push(encode_hero_bag_push(account));
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
            Some(encode_task_reward_list(&rewards))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn pass_mini_game(
    fashion_catalog: Option<&FashionList>,
    account: &mut Option<&mut Value>,
    request_args: &[u8],
    chapter_catalog: Option<&ChapterCatalog>,
    battle_catalog: Option<&BattleCatalog>,
    pre_pushes: &mut Vec<Vec<u8>>,
    response_err: &mut i32,
    response_err_msg: &mut String,
) -> Option<Vec<u8>> {
    // JP MiniGameLimit sends TPASSBASEARG, not the old TTestPassArg range:
    // BaseId=1, IsFinishMission=19, BattleTime=12, BattleType=15.
    let copy_id = decode_varint_field(request_args, 1);
    if copy_id <= 0 {
        *response_err = 1;
        *response_err_msg = "mini-game copy id is invalid".to_owned();
        return Some(Vec::new());
    }
    let is_finish = decode_varint_field(request_args, 19) != 0;
    if !is_finish {
        *response_err = 1;
        *response_err_msg = "mini-game was not finished".to_owned();
        return Some(Vec::new());
    }
    if let Some(catalog) = battle_catalog {
        if !catalog.copies.contains_key(&copy_id)
            && !chapter_catalog.is_some_and(|chapters| chapters.mini_game_ids.contains(&copy_id))
        {
            *response_err = 1;
            *response_err_msg = "mini-game copy is not configured".to_owned();
            return Some(Vec::new());
        }
    }
    let Some(account) = account.as_deref_mut() else {
        *response_err = 1;
        *response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    let first_pass = !battle_copy_passed(account, copy_id);
    let battle_time = decode_varint_field(request_args, 12).max(1);
    let grade = 3;
    let mut rewards = Vec::new();
    if first_pass {
        for (goods_type, item_id, num) in battle_catalog
            .and_then(|catalog| catalog.copy_first_rewards.get(&copy_id))
            .into_iter()
            .flatten()
            .copied()
        {
            rewards.push(grant_reward(
                account,
                ShopReward {
                    goods_type,
                    item_id,
                    num,
                    instance_id: 0,
                },
                current_unix_seconds(),
                fashion_catalog,
            ));
        }
    }
    record_battle_pass(account, copy_id, grade, battle_time, battle_catalog, &[]);
    pre_pushes.push(encode_hero_bag_push(account));
    append_method_push(
        pre_pushes,
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_account(account)),
    );
    Some(super::battle_state::battle_pass_payload_with_rewards(
        copy_id,
        first_pass,
        grade,
        battle_time,
        &rewards,
    ))
}

fn cache_data_payload() -> Vec<u8> {
    let mut payload = Vec::new();
    append_bytes_field(&mut payload, 1, b"local");
    payload
}

fn combine_hero_relation(
    account: &mut Option<&mut Value>,
    request_args: &[u8],
    combination_catalog: Option<&CombinationCatalog>,
    pre_pushes: &mut Vec<Vec<u8>>,
    response_err: &mut i32,
    response_err_msg: &mut String,
) -> Option<Vec<u8>> {
    let main_id = decode_varint_u64_field(request_args, 1);
    let deputy_id = decode_varint_u64_field(request_args, 2);
    if main_id == 0 || main_id == deputy_id {
        *response_err = 1;
        *response_err_msg = "hero combination relation is invalid".to_owned();
        return Some(Vec::new());
    }
    let Some(account) = account.as_deref_mut() else {
        *response_err = 1;
        *response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    let has_hero = |id| {
        account
            .get("dock")
            .and_then(|dock| dock.get("heroes"))
            .and_then(Value::as_array)
            .is_some_and(|heroes| {
                heroes
                    .iter()
                    .any(|hero| json_u64(hero, "heroId") == Some(id))
            })
    };
    if !has_hero(main_id) || (deputy_id > 0 && !has_hero(deputy_id)) {
        *response_err = 1;
        *response_err_msg = "hero combination member was not found".to_owned();
        return Some(Vec::new());
    }
    if deputy_id > 0 {
        let open = |id| {
            account
                .get("dock")
                .and_then(|dock| dock.get("heroes"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|hero| json_u64(hero, "heroId") == Some(id))
                .and_then(|hero| json_i32(hero, "templateId"))
                .is_some_and(|template| {
                    combination_catalog
                        .is_none_or(|catalog| catalog.open_sf_ids.contains(&(template / 10)))
                })
        };
        if !open(main_id) || !open(deputy_id) {
            *response_err = 1;
            *response_err_msg = "hero combination is not open for this ship".to_owned();
            return Some(Vec::new());
        }
    }
    let current_deputy = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|hero| json_u64(hero, "heroId") == Some(main_id))
        .and_then(|hero| hero.get("combinationInfo"))
        .and_then(|info| json_u64(info, "combine"));
    if deputy_id > 0
        && (current_deputy.is_some_and(|id| id > 0 && id != deputy_id)
            || account
                .get("dock")
                .and_then(|dock| dock.get("heroes"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|hero| json_u64(hero, "heroId") == Some(deputy_id))
                .and_then(|hero| hero.get("combinationInfo"))
                .and_then(|info| json_u64(info, "beCombined"))
                .is_some_and(|id| id > 0 && id != main_id))
    {
        *response_err = 1;
        *response_err_msg = "hero is already in another combination".to_owned();
        return Some(Vec::new());
    }
    if let Some(old_deputy_id) = current_deputy.filter(|id| *id > 0) {
        if let Some(old_deputy) = find_hero_mut(account, old_deputy_id) {
            old_deputy
                .entry("combinationInfo".to_owned())
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .expect("combination info must be an object")
                .insert("beCombined".to_owned(), json!(0));
        }
    }
    if let Some(main) = find_hero_mut(account, main_id) {
        let info = main
            .entry("combinationInfo".to_owned())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("combination info must be an object");
        info.insert("combine".to_owned(), json!(deputy_id));
    }
    if deputy_id > 0 {
        if let Some(deputy) = find_hero_mut(account, deputy_id) {
            let info = deputy
                .entry("combinationInfo".to_owned())
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .expect("combination info must be an object");
            info.insert("beCombined".to_owned(), json!(main_id));
        }
    }
    pre_pushes.push(encode_hero_bag_push(account));
    Some(Vec::new())
}

fn combination_rule_for<'a>(
    account: &'a Value,
    hero_id: u64,
    catalog: &'a CombinationCatalog,
    level: i32,
) -> Option<&'a CombinationRule> {
    let template = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
        .and_then(|hero| json_i32(hero, "templateId"))?;
    let sf_id = template / 10;
    if !catalog.open_sf_ids.is_empty() && !catalog.open_sf_ids.contains(&sf_id) {
        return None;
    }
    let level = level.clamp(1, 100);
    catalog
        .rules_by_id
        .get(&(sf_id.saturating_mul(100) + (level - 1) / 10))
}

fn costs_available(account: &Value, costs: &[(i32, i32, i32)]) -> bool {
    costs
        .iter()
        .all(|(kind, id, amount)| resource_available(account, *kind, *id, *amount))
}

fn consume_costs(account: &mut Value, costs: &[(i32, i32, i32)]) {
    for (kind, id, amount) in costs {
        consume_resource(account, *kind, *id, *amount);
    }
}

fn upgrade_combination(
    account: &mut Option<&mut Value>,
    request_args: &[u8],
    combination_catalog: Option<&CombinationCatalog>,
    quick: bool,
    pre_pushes: &mut Vec<Vec<u8>>,
    response_err: &mut i32,
    response_err_msg: &mut String,
) -> Option<Vec<u8>> {
    let hero_id = decode_varint_u64_field(request_args, 1);
    let Some(catalog) = combination_catalog else {
        *response_err = 1;
        *response_err_msg = "hero combination catalog is unavailable".to_owned();
        return Some(Vec::new());
    };
    let Some(account) = account.as_deref_mut() else {
        *response_err = 1;
        *response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    let mut current_level = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
        .and_then(|hero| hero.get("combinationInfo"))
        .and_then(|info| json_i32(info, "comLv").or_else(|| json_i32(info, "ComLv")))
        .unwrap_or_default();
    if hero_id == 0 || current_level >= 100 {
        *response_err = 1;
        *response_err_msg = "hero combination level is already maxed".to_owned();
        return Some(Vec::new());
    }
    let mut changed = 0;
    let max_steps = if quick { 100 } else { 1 };
    while changed < max_steps && current_level < 100 {
        let Some(costs) = combination_rule_for(account, hero_id, catalog, current_level + 1)
            .map(|rule| rule.levelup_costs.clone())
        else {
            break;
        };
        if !costs_available(account, &costs) {
            break;
        }
        consume_costs(account, &costs);
        current_level += 1;
        changed += 1;
    }
    if changed == 0 {
        *response_err = 1;
        *response_err_msg = "hero combination level-up cost is insufficient".to_owned();
        return Some(Vec::new());
    }
    if let Some(hero) = find_hero_mut(account, hero_id) {
        let info = hero
            .entry("combinationInfo".to_owned())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("combination info must be an object");
        info.insert("comLv".to_owned(), json!(current_level));
    }
    pre_pushes.push(encode_hero_bag_push(account));
    append_method_push(
        pre_pushes,
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_account(account)),
    );
    Some(Vec::new())
}

fn break_combination(
    account: &mut Option<&mut Value>,
    request_args: &[u8],
    combination_catalog: Option<&CombinationCatalog>,
    pre_pushes: &mut Vec<Vec<u8>>,
    response_err: &mut i32,
    response_err_msg: &mut String,
) -> Option<Vec<u8>> {
    let hero_id = decode_varint_u64_field(request_args, 1);
    let Some(catalog) = combination_catalog else {
        *response_err = 1;
        *response_err_msg = "hero combination catalog is unavailable".to_owned();
        return Some(Vec::new());
    };
    let Some(account) = account.as_deref_mut() else {
        *response_err = 1;
        *response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    let (level, grade) = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|hero| json_u64(hero, "heroId") == Some(hero_id))
        .map(|hero| {
            let info = hero.get("combinationInfo").unwrap_or(&Value::Null);
            (
                json_i32(info, "comLv")
                    .or_else(|| json_i32(info, "ComLv"))
                    .unwrap_or_default(),
                json_i32(info, "comGrade")
                    .or_else(|| json_i32(info, "ComGrade"))
                    .unwrap_or_default(),
            )
        })
        .unwrap_or_default();
    let Some((break_costs, level_end, next_id)) =
        combination_rule_for(account, hero_id, catalog, level)
            .map(|rule| (rule.break_costs.clone(), rule.level_end, rule.next_id))
    else {
        *response_err = 1;
        *response_err_msg = "hero combination break configuration was not found".to_owned();
        return Some(Vec::new());
    };
    let Some(next_star) = catalog.rules_by_id.get(&next_id).map(|rule| rule.star) else {
        *response_err = 1;
        *response_err_msg = "hero combination is already at final stage".to_owned();
        return Some(Vec::new());
    };
    if level < level_end || grade >= next_star || !costs_available(account, &break_costs) {
        *response_err = 1;
        *response_err_msg = "hero combination break requirement is not met".to_owned();
        return Some(Vec::new());
    }
    consume_costs(account, &break_costs);
    if let Some(hero) = find_hero_mut(account, hero_id) {
        let info = hero
            .entry("combinationInfo".to_owned())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("combination info must be an object");
        info.insert("comGrade".to_owned(), json!(next_star));
    }
    pre_pushes.push(encode_hero_bag_push(account));
    append_method_push(
        pre_pushes,
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_account(account)),
    );
    Some(Vec::new())
}

fn head_buy_count_payload() -> Vec<u8> {
    vec![0x08, 0x00, 0x10, 0x00]
}

fn head_unlocked_payload(account: &Value) -> Vec<u8> {
    let mut payload = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    if let Some(heroes) = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
    {
        for hero in heroes {
            if let Some(template_id) = json_i32(hero, "templateId") {
                let id = (template_id.saturating_sub(1)) / 10;
                if id > 0 {
                    ids.insert(id);
                }
            }
        }
    }
    for id in ids {
        append_varint_field(&mut payload, 1, id as u64);
    }
    payload
}

fn add_illustrate_behaviour(
    account: &mut Option<&mut Value>,
    request_args: &[u8],
    pre_pushes: &mut Vec<Vec<u8>>,
    handbook_behaviours: Option<&[i32]>,
    response_err: &mut i32,
    response_err_msg: &mut String,
) -> Option<Vec<u8>> {
    let incoming = decode_repeated_message_field(request_args, 1);
    let Some(account) = account.as_deref_mut() else {
        *response_err = 1;
        *response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    if incoming.is_empty() {
        *response_err = 1;
        *response_err_msg = "illustrate behaviour request is invalid".to_owned();
        return Some(Vec::new());
    }
    let root = account.as_object_mut().expect("account must be an object");
    let illustrate = root
        .entry("illustrate".to_owned())
        .or_insert_with(|| json!({"entries": [], "vowHeroIds": []}));
    let entries = illustrate
        .get_mut("entries")
        .and_then(Value::as_array_mut)
        .expect("illustrate entries must be an array");
    let mut updated = Vec::new();
    for item in incoming {
        let illustrate_id = decode_varint_field(&item, 1);
        if illustrate_id <= 0 {
            continue;
        }
        let requested = decode_repeated_varint_field(&item, 2);
        let behaviours = handbook_behaviours
            .filter(|items| !items.is_empty())
            .map(|items| items.to_vec())
            .unwrap_or(requested);
        let mut behaviours = behaviours
            .into_iter()
            .filter(|id| *id > 0)
            .collect::<Vec<_>>();
        behaviours.sort_unstable();
        behaviours.dedup();
        if let Some(entry) = entries
            .iter_mut()
            .find(|entry| json_i32(entry, "illustrateId") == Some(illustrate_id))
        {
            entry["behaviourList"] = json!(behaviours);
        } else {
            entries.push(json!({
                "illustrateId": illustrate_id,
                "behaviourList": behaviours
            }));
        }
        updated.push((illustrate_id, behaviours));
    }
    if updated.is_empty() {
        *response_err = 1;
        *response_err_msg = "illustrate behaviour request is invalid".to_owned();
        return Some(Vec::new());
    }
    append_method_push(
        pre_pushes,
        "illustrate.IllustrateInfo",
        illustrate_info_payload_for_entries(&updated),
    );
    Some(Vec::new())
}

fn modify_vow_hero_list(
    account: &mut Option<&mut Value>,
    request_args: &[u8],
    pre_pushes: &mut Vec<Vec<u8>>,
    handbook_behaviours: Option<&[i32]>,
) -> Option<Vec<u8>> {
    let hero_ids = decode_repeated_varint_field(request_args, 1)
        .into_iter()
        .filter(|id| *id > 0)
        .collect::<Vec<_>>();
    let Some(account) = account.as_deref_mut() else {
        return Some(Vec::new());
    };
    let template_ids = account
        .get("dock")
        .and_then(|dock| dock.get("heroes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|hero| json_u64(hero, "heroId").is_some_and(|id| hero_ids.contains(&(id as i32))))
        .filter_map(|hero| json_i32(hero, "templateId"))
        .collect::<Vec<_>>();
    let root = account.as_object_mut().expect("account must be an object");
    let illustrate = root
        .entry("illustrate".to_owned())
        .or_insert_with(|| json!({"entries": [], "vowHeroIds": []}));
    illustrate["vowHeroIds"] = json!(hero_ids);
    if !hero_ids.is_empty() {
        append_method_push(
            pre_pushes,
            "illustrate.IllustrateInfo",
            illustrate_info_payload_for_templates(&template_ids, handbook_behaviours),
        );
    }
    Some(Vec::new())
}

#[allow(clippy::too_many_arguments)]
fn claim_copy_star_reward(
    state: &ServerState,
    account: &mut Option<&mut Value>,
    request_args: &[u8],
    chapter_catalog: Option<&ChapterCatalog>,
    task_catalog: Option<&TaskCatalog>,
    fashion_catalog: Option<&FashionList>,
    equip_catalog: Option<&EquipCatalog>,
    pre_pushes: &mut Vec<Vec<u8>>,
    response_err: &mut i32,
    response_err_msg: &mut String,
) -> Option<Vec<u8>> {
    let chapter_id = decode_varint_field(request_args, 1);
    let mut indexes = decode_repeated_varint_field(request_args, 3)
        .into_iter()
        .filter(|index| *index > 0)
        .collect::<Vec<_>>();
    if indexes.is_empty() {
        let index = decode_varint_field(request_args, 2);
        if index > 0 {
            indexes.push(index);
        }
    }
    let Some(chapter_rewards) =
        chapter_catalog.and_then(|catalog| catalog.star_rewards_by_chapter.get(&chapter_id))
    else {
        *response_err = 1;
        *response_err_msg = "copy star reward chapter was not found".to_owned();
        return Some(Vec::new());
    };
    let Some(task_catalog) = task_catalog else {
        *response_err = 1;
        *response_err_msg = "copy reward catalog is unavailable".to_owned();
        return Some(Vec::new());
    };
    let Some(account) = account.as_deref_mut() else {
        *response_err = 1;
        *response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    if chapter_id <= 0 || indexes.is_empty() {
        *response_err = 1;
        *response_err_msg = "copy star reward request is invalid".to_owned();
        return Some(Vec::new());
    }

    let star_num = chapter_rewards
        .level_ids
        .iter()
        .filter_map(|copy_id| {
            account
                .get("copyProgress")
                .and_then(|progress| progress.get("records"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|record| json_i32(record, "copyId") == Some(*copy_id))
                .map(|record| {
                    u32::try_from(json_i64(record, "starLevel").unwrap_or_default().max(0))
                        .unwrap_or_default()
                        .count_ones() as i32
                })
        })
        .sum::<i32>();
    let claimed = account
        .get("copyStarRewards")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut pending_keys = std::collections::BTreeSet::new();
    let mut pending_reward_ids = Vec::new();
    for index in indexes {
        let Some(position) = usize::try_from(index.saturating_sub(1)).ok() else {
            *response_err = 1;
            *response_err_msg = "copy star reward index is invalid".to_owned();
            return Some(Vec::new());
        };
        let Some(required_stars) = chapter_rewards.star_conditions.get(position).copied() else {
            *response_err = 1;
            *response_err_msg = "copy star reward index is invalid".to_owned();
            return Some(Vec::new());
        };
        let Some(reward_id) = chapter_rewards.reward_ids.get(position).copied() else {
            *response_err = 1;
            *response_err_msg = "copy star reward is not configured".to_owned();
            return Some(Vec::new());
        };
        if star_num < required_stars {
            *response_err = 1;
            *response_err_msg = "copy star requirement is not met".to_owned();
            return Some(Vec::new());
        }
        let already_claimed = claimed.iter().any(|value| {
            (json_i32(value, "chapterId") == Some(chapter_id)
                && json_i32(value, "index") == Some(index))
                || value.as_str() == Some(&format!("{chapter_id}:{index}"))
        });
        if already_claimed || !pending_keys.insert(index) {
            *response_err = 1;
            *response_err_msg = "copy star reward was already claimed".to_owned();
            return Some(Vec::new());
        }
        if task_catalog
            .rewards_by_id
            .get(&reward_id)
            .is_none_or(Vec::is_empty)
        {
            *response_err = 1;
            *response_err_msg = "copy star reward is not configured".to_owned();
            return Some(Vec::new());
        }
        pending_reward_ids.push((index, reward_id));
    }

    let now = current_unix_seconds();
    let mut rewards = Vec::new();
    for (index, reward_id) in pending_reward_ids {
        let configured = task_catalog
            .rewards_by_id
            .get(&reward_id)
            .cloned()
            .unwrap_or_default();
        for (goods_type, item_id, num) in configured {
            rewards.push(grant_reward(
                account,
                ShopReward {
                    goods_type,
                    item_id,
                    num,
                    instance_id: 0,
                },
                now,
                fashion_catalog,
            ));
        }
        account
            .as_object_mut()
            .expect("account must be an object")
            .entry("copyStarRewards".to_owned())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("copy star reward state must be an array")
            .push(json!({"chapterId": chapter_id, "index": index}));
    }
    pre_pushes.push(encode_hero_bag_push(account));
    append_method_push(
        pre_pushes,
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_account(account)),
    );
    append_method_push(
        pre_pushes,
        "equip.UpdateEquipBagData",
        EquipListCodec::encode(&equip_list_from_account(account, equip_catalog)),
    );
    append_method_push(
        pre_pushes,
        "user.UpdateUserInfo",
        UserInfoCodec::encode(&user_info_from_account(state, Some(account))),
    );
    Some(encode_task_reward_list(&rewards))
}

fn open_normal_treasure(
    account: &mut Option<&mut Value>,
    request_args: &[u8],
    context: &mut TreasureContext<'_>,
) -> Option<Vec<u8>> {
    let treasure_id = decode_varint_field(request_args, 1);
    let open_num = decode_varint_field(request_args, 2);
    let catalog = BUILD_SHIP_CATALOG.get_or_init(BuildShipCatalog::default);
    let Some(drop_id) = catalog.treasure_drop_by_item.get(&treasure_id).copied() else {
        *context.response_err = 1;
        *context.response_err_msg = "treasure configuration was not found".to_owned();
        return Some(Vec::new());
    };
    open_treasure(
        account,
        treasure_id,
        open_num,
        None,
        Some(drop_id),
        catalog,
        context,
    )
}

fn open_select_treasure(
    account: &mut Option<&mut Value>,
    request_args: &[u8],
    context: &mut TreasureContext<'_>,
) -> Option<Vec<u8>> {
    let treasure_id = decode_varint_field(request_args, 1);
    let position = decode_varint_field(request_args, 2);
    let open_num = decode_varint_field(request_args, 3).max(1);
    let catalog = BUILD_SHIP_CATALOG.get_or_init(BuildShipCatalog::default);
    let Some(selected) = catalog.selected_treasure_by_item.get(&treasure_id) else {
        *context.response_err = 1;
        *context.response_err_msg = "select treasure configuration was not found".to_owned();
        return Some(Vec::new());
    };
    let selected_option = if selected.options.is_empty() {
        None
    } else if position <= 0
        || usize::try_from(position)
            .ok()
            .is_none_or(|p| p > selected.options.len())
    {
        *context.response_err = 1;
        *context.response_err_msg = "select treasure position is out of range".to_owned();
        return Some(Vec::new());
    } else {
        Some(selected.options[usize::try_from(position - 1).unwrap_or_default()])
    };
    open_treasure(
        account,
        treasure_id,
        open_num,
        selected_option,
        (selected.drop_id > 0).then_some(selected.drop_id),
        catalog,
        context,
    )
}

struct TreasureContext<'a> {
    pre_pushes: &'a mut Vec<Vec<u8>>,
    response_err: &'a mut i32,
    response_err_msg: &'a mut String,
    fashion_catalog: Option<&'a FashionList>,
    equip_catalog: Option<&'a EquipCatalog>,
}

fn open_treasure(
    account: &mut Option<&mut Value>,
    treasure_id: i32,
    open_num: i32,
    selected_option: Option<(i32, i32, i32)>,
    drop_id: Option<i32>,
    catalog: &BuildShipCatalog,
    context: &mut TreasureContext<'_>,
) -> Option<Vec<u8>> {
    if treasure_id <= 0 || !(1..=99).contains(&open_num) {
        *context.response_err = 1;
        *context.response_err_msg = "treasure id or count is invalid".to_owned();
        return Some(Vec::new());
    }
    let Some(account) = account.as_deref_mut() else {
        *context.response_err = 1;
        *context.response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    if bag_item_count(account, treasure_id) < i64::from(open_num) {
        *context.response_err = 1;
        *context.response_err_msg = "treasure count is insufficient".to_owned();
        return Some(Vec::new());
    }
    let mut pending = Vec::new();
    for index in 0..open_num {
        let reward = selected_option.or_else(|| {
            let drop_id = drop_id?;
            draw_build_drop_reward_with_roll(
                catalog,
                drop_id,
                mix_build_draw_roll(
                    u64::from(current_unix_millis())
                        ^ BUILD_DRAW_SEQUENCE.fetch_add(1, Ordering::Relaxed)
                        ^ u64::try_from(index).unwrap_or_default(),
                ),
            )
        });
        let Some((goods_type, item_id, num)) = reward else {
            *context.response_err = 1;
            *context.response_err_msg = "treasure drop pool is invalid".to_owned();
            return Some(Vec::new());
        };
        pending.push(ShopReward {
            goods_type,
            item_id,
            num,
            instance_id: 0,
        });
    }
    let _ = consume_bag_item(account, treasure_id, open_num);
    let rewards = pending
        .into_iter()
        .map(|reward| {
            grant_reward(
                account,
                reward,
                current_unix_seconds(),
                context.fashion_catalog,
            )
        })
        .collect::<Vec<_>>();
    context.pre_pushes.push(encode_hero_bag_push(account));
    append_method_push(
        context.pre_pushes,
        "bag.UpdateBagData",
        BagInfoCodec::encode(&bag_info_from_account(account)),
    );
    append_method_push(
        context.pre_pushes,
        "equip.UpdateEquipBagData",
        EquipListCodec::encode(&equip_list_from_account(account, context.equip_catalog)),
    );
    Some(encode_treasure_response(&rewards, treasure_id))
}

fn encode_treasure_response(rewards: &[ShopReward], treasure_id: i32) -> Vec<u8> {
    let mut output = Vec::new();
    for reward in rewards {
        let mut item = Vec::new();
        append_varint_field(&mut item, 1, reward.goods_type.max(0) as u64);
        append_varint_field(&mut item, 2, reward.item_id.max(0) as u64);
        append_varint_field(&mut item, 3, reward.num.max(0) as u64);
        if reward.instance_id > 0 {
            append_varint_field(&mut item, 4, reward.instance_id as u64);
        }
        append_message_field(&mut output, 1, &item);
    }
    if treasure_id > 0 {
        append_varint_field(&mut output, 2, treasure_id as u64);
    }
    output
}

fn illustrate_new(
    account: &mut Option<&mut Value>,
    request_args: &[u8],
    response_err: &mut i32,
    response_err_msg: &mut String,
) -> Option<Vec<u8>> {
    let ids = decode_repeated_varint_field(request_args, 1)
        .into_iter()
        .filter(|id| *id > 0)
        .collect::<Vec<_>>();
    let Some(account) = account.as_deref_mut() else {
        *response_err = 1;
        *response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    if ids.is_empty() {
        *response_err = 1;
        *response_err_msg = "illustrate id list is empty".to_owned();
        return Some(Vec::new());
    }
    let now = current_unix_seconds();
    let illustrate = account
        .as_object_mut()
        .expect("account must be an object")
        .entry("illustrate".to_owned())
        .or_insert_with(|| json!({"entries": [], "vowHeroIds": []}));
    let entries = illustrate
        .as_object_mut()
        .expect("illustrate state must be an object")
        .entry("entries".to_owned())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("illustrate entries must be an array");
    let mut response = Vec::new();
    for illustrate_id in ids {
        let entry = if let Some(entry) = entries
            .iter_mut()
            .find(|entry| json_i32(entry, "illustrateId") == Some(illustrate_id))
        {
            entry
        } else {
            entries.push(json!({
                "illustrateId": illustrate_id,
                "getTime": now,
                "likeTime": 0,
                "newHero": true,
                "behaviourList": [],
                "marryCount": 0,
            }));
            entries.last_mut().expect("illustrate entry was inserted")
        };
        response.push((illustrate_id, json_i32_array(entry, "behaviourList")));
    }
    Some(illustrate_info_payload_for_entries(&response))
}

fn illustrate_equip_new(
    account: &mut Option<&mut Value>,
    request_args: &[u8],
    response_err: &mut i32,
    response_err_msg: &mut String,
) -> Option<Vec<u8>> {
    let ids = decode_repeated_varint_field(request_args, 1)
        .into_iter()
        .filter(|id| *id > 0)
        .collect::<Vec<_>>();
    let Some(account) = account.as_deref_mut() else {
        *response_err = 1;
        *response_err_msg = "account is unavailable".to_owned();
        return Some(Vec::new());
    };
    if ids.is_empty() {
        *response_err = 1;
        *response_err_msg = "illustrate equipment id list is empty".to_owned();
        return Some(Vec::new());
    }
    let now = current_unix_seconds();
    let illustrate = account
        .as_object_mut()
        .expect("account must be an object")
        .entry("illustrate".to_owned())
        .or_insert_with(|| json!({"entries": [], "vowHeroIds": []}));
    let object = illustrate
        .as_object_mut()
        .expect("illustrate state must be an object");
    let entries = object
        .entry("equipEntries".to_owned())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("illustrate equipment entries must be an array");
    for equip_template_id in ids {
        if let Some(entry) = entries
            .iter_mut()
            .find(|entry| json_i32(entry, "equipTemplateId") == Some(equip_template_id))
        {
            entry["newEquip"] = json!(true);
        } else {
            entries.push(json!({
                "equipTemplateId": equip_template_id,
                "getEquipTime": now,
                "newEquip": true,
            }));
        }
    }
    Some(illustrate_equip_list_payload(entries))
}

fn illustrate_equip_list_payload(entries: &[Value]) -> Vec<u8> {
    let mut output = Vec::new();
    for entry in entries {
        let mut item = Vec::new();
        append_varint_field(
            &mut item,
            1,
            json_i32(entry, "equipTemplateId")
                .unwrap_or_default()
                .max(0) as u64,
        );
        append_varint_field(
            &mut item,
            2,
            json_i64(entry, "getEquipTime").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut item,
            3,
            u64::from(
                entry
                    .get("newEquip")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
        );
        append_message_field(&mut output, 1, &item);
    }
    output
}

fn fashion_replace_reward_payload(account: &Value) -> Vec<u8> {
    let rewards = account
        .get("fashionReplaceRewards")
        .and_then(Value::as_array);
    let mut output = Vec::new();
    for reward in rewards.into_iter().flatten() {
        let mut item = Vec::new();
        append_varint_field(
            &mut item,
            1,
            json_i32(reward, "type").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut item,
            2,
            json_i32(reward, "configId").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut item,
            3,
            json_i32(reward, "num").unwrap_or_default().max(0) as u64,
        );
        append_varint_field(
            &mut item,
            4,
            json_i32(reward, "id").unwrap_or_default().max(0) as u64,
        );
        let mut wrapper = Vec::new();
        append_message_field(&mut wrapper, 1, &item);
        append_message_field(&mut output, 1, &wrapper);
    }
    output
}
