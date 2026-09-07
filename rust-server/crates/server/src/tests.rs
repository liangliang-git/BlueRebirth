use super::catalog::GameLoginCatalogs;
use super::common::error::GameError;
use super::game_login::pass_mini_game;
use super::{
    add_bag_item, add_building_state, adjust_character_i64, advance_task_event,
    advance_task_event_with_param, append_bytes_field, append_message_field, append_varint_field,
    apply_hero_breakdown_rewards, apply_mail_reward, apply_shop_good, apply_strategy_state,
    apply_talent_change, auto_select_enhancement_materials, bag_info_from_account,
    bag_info_from_typed_account, bag_item_count, bathroom_info_payload,
    battle_attack_payload_with_damage, battle_copy_passed, battle_enemy_ids,
    battle_pass_payload_with_experience, battle_pass_payload_with_rewards,
    battle_position_fleet_id, battle_session_fleet_ids, battle_start_payload, bootstrap_response,
    building_info_from_account, building_info_from_typed_account, buildship_info_payload,
    change_building_level, collect_building_rewards, complete_support_state, complete_task,
    completed_copy_ids, construction_info_payload, consume_bag_item,
    consume_hero_skill_upgrade_materials, copy_progress_max_or_first, copy_progress_max_or_initial,
    copy_request_type, daily_copy_group_progress_from_account, daily_copy_progress_from_account,
    daily_copy_progress_from_typed_account, decode_hero_add_exp_request, decode_mop_up_arg,
    decode_repeated_message_field, decode_repeated_varint_field, decode_varint_field,
    default_account_snapshot, dismantle_equip_state, draw_build_drop_reward_with_roll,
    draw_build_ship_reward_with_roll, draw_copy_drop_with_seed, draw_draw_count,
    draw_sr_build_reward_with_roll, encode_hero_add_exp_response, encode_mail_list_response,
    encode_retire_hero_response, enhance_bind_equip_state, enhance_equip_state,
    equip_list_from_account, expand_build_drop, fashion_equip_state, fashion_list_from_account,
    finish_building_state, finish_study_state, fleet_info_from_account,
    fleet_info_from_typed_account, hero_advance_max_level_state, hero_advance_mub_state,
    hero_advance_state, hero_auto_equip_state, hero_auto_unequip_state, hero_change_equip_state,
    hero_equip_binding_state, hero_equip_effect_state, hero_equip_lock_transplant_state,
    hero_intensify_state, hero_remould_state, illustrate_info_payload,
    illustrate_info_payload_for_templates, json_i32, load_affection_catalog, load_battle_catalog,
    load_chapter_catalog, load_equip_catalog, load_server_shop_goods, load_ship_break_catalog,
    load_ship_intensify_catalog, load_ship_stat_catalog, load_shop_catalog, load_task_catalog,
    mark_battle_fleet_passed, mop_up_pass_rets, mop_up_payload, mop_up_payload_with_pass_rets,
    normalize_daily_copy_state, normalize_task_state, prepare_local_request,
    preset_fleet_info_from_account, process_game_login_frame_with_catalog_mut,
    process_game_login_frame_with_catalogs_typed_mut, receive_construction, record_battle_pass,
    renovate_equip_state, resolve_study_skill_id, return_shop_buy_response, scale_reward,
    sea_difficulty_for_account, set_preset_fleet_from_account, set_sea_difficulty, settle_mop_up,
    settle_mop_up_with_config, settle_support_state, ship_attributes_for_hero,
    ship_attributes_for_template, shop_costs_from_value, shop_info_payload, start_construction,
    start_study_state, start_support_state, story_memory_payload, study_info_payload,
    study_skill_state, sync_achievement_points, task_completed, task_info_payload,
    update_bathroom_state, update_building_assignments, update_mop_up_state,
    validate_battle_attack, BattleCatalog, BattleCopy, BattleEnemy, BattleFleetReward,
    BuildShipCatalog, BuildingCatalog, ChapterCatalog, CommanderLevelCatalog, EquipCatalog,
    EquipLevelbreakRule, EquipNewTestCatalog, EquipNum, EquipRenovateRule, HeroBreakdownCatalog,
    HeroLevelCatalog, HeroSkillUpgradeCatalog, MailTemplate, ServerConfig, ServerState,
    ShipAdvanceCatalog, ShipBreakCatalog, ShipRemouldCatalog, ShipStat, ShipStatCatalog,
    ShopCatalog, ShopCost, ShopGood, ShopReward, SupportCatalog, SupportFleetItem, TalentCatalog,
    TalentNode, TaskCatalog, TaskDefinition, UserInfoCodec, DEFAULT_GUILD_ID, GUILD_MEMBER,
};
use blueoath_domain::{FleetId, FleetRecord, HeroId, NewAccountFactory, ProfileId, TemplateId};
use blueoath_protocol::{
    CopyRecordListCodec, EquipListCodec, FashionInfo, FashionList, HeroBagCodec, PresetFleet,
    PresetFleetCodec, PresetFleetInfo, TMessageCodec, TRequest,
};
use blueoath_transport::NetSocketFrameCodec;
use serde_json::json;
use tokio::io::duplex;

#[tokio::test]
async fn typed_user_routes_update_account_state_without_json_account() {
    let state = ServerState::new("typed-user", "Fallback", "1.4.0");
    let mut account = NewAccountFactory::create(ProfileId::new("typed-user").unwrap(), "Old");
    account.character.uid = 77;
    let catalogs = GameLoginCatalogs::empty();
    let (mut client, mut server) = duplex(16_384);

    let mut args = Vec::new();
    append_bytes_field(&mut args, 1, b"Typed Captain");
    NetSocketFrameCodec::write(
        &mut client,
        0,
        &TMessageCodec::encode_request(&TRequest {
            method: "user.ChangeName".to_owned(),
            args: Some(args),
            callback_handler: 31,
            ..TRequest::default()
        }),
    )
    .await
    .unwrap();
    process_game_login_frame_with_catalogs_typed_mut(
        &mut server,
        &state,
        None,
        Some(&mut account),
        &catalogs,
    )
    .await
    .unwrap();
    let _ = NetSocketFrameCodec::read(&mut client).await.unwrap();
    assert_eq!(account.character.name, "Typed Captain");

    let mut args = Vec::new();
    append_varint_field(&mut args, 2, 1021052);
    NetSocketFrameCodec::write(
        &mut client,
        0,
        &TMessageCodec::encode_request(&TRequest {
            method: "user.SetHead".to_owned(),
            args: Some(args),
            callback_handler: 32,
            ..TRequest::default()
        }),
    )
    .await
    .unwrap();
    process_game_login_frame_with_catalogs_typed_mut(
        &mut server,
        &state,
        None,
        Some(&mut account),
        &catalogs,
    )
    .await
    .unwrap();
    let _ = NetSocketFrameCodec::read(&mut client).await.unwrap();
    assert_eq!(account.character.head, 1021052);
}

#[test]
fn typed_fleet_projection_reads_normalized_fleet_rows() {
    let mut account = NewAccountFactory::create(ProfileId::new("typed-fleet").unwrap(), "Fleet");
    account.fleet.fleets.insert(
        FleetId::new(3).unwrap(),
        FleetRecord {
            formation_id: 7,
            tactic_id: 11,
            members: vec![HeroId::new(101).unwrap(), HeroId::new(102).unwrap()],
        },
    );

    let fleet = fleet_info_from_typed_account(&account);
    assert_eq!(fleet.tactics.len(), 1);
    assert_eq!(fleet.tactics[0].mode_id, 3);
    assert_eq!(fleet.tactics[0].hero_ids, vec![101, 102]);
    assert_eq!(fleet.tactics[0].strategy_id, 11);
    assert_eq!(fleet.tactics[0].formation_id, 7);
}

#[test]
fn typed_bag_projection_reads_normalized_inventory_rows() {
    let mut account = NewAccountFactory::create(ProfileId::new("typed-bag").unwrap(), "Bag");
    account
        .inventory
        .items
        .insert(TemplateId::new(30001).unwrap(), 17);

    let bag = bag_info_from_typed_account(&account);
    assert_eq!(bag.bag_type, 1);
    assert_eq!(bag.bag_size, 100);
    assert_eq!(
        bag.items
            .iter()
            .find(|item| item.template_id == 30001)
            .map(|item| item.num),
        Some(17)
    );
    assert!(bag.items.iter().any(|item| item.template_id == 10182));
}

#[test]
fn typed_building_projection_reads_normalized_building_rows() {
    let mut account = NewAccountFactory::create(ProfileId::new("typed-building").unwrap(), "Base");
    account.buildings.levels.insert(9, 4);
    account.buildings.template_ids.insert(9, 41);
    account.buildings.land_indices.insert(9, 6);

    let building = building_info_from_typed_account(&account, 123);
    assert_eq!(building.buildings.len(), 1);
    assert_eq!(building.buildings[0].id, 9);
    assert_eq!(building.buildings[0].template_id, 41);
    assert_eq!(building.buildings[0].level, 4);
    assert_eq!(building.lands[0].index, 6);
}

#[test]
fn typed_daily_copy_projection_resets_stale_challenge_counts() {
    let mut account = NewAccountFactory::create(ProfileId::new("typed-daily").unwrap(), "Daily");
    account.daily_copy.reset_day = 0;
    account
        .daily_copy
        .challenge_times
        .insert(blueoath_domain::ChapterId::new(7).unwrap(), 4);

    assert!(daily_copy_progress_from_typed_account(&account, 86_400).is_empty());
}

#[test]
fn equipment_projection_keeps_rise_common_materials_and_hero_effects() {
    let account = json!({
        "dock": {"heroes": [{
            "heroId": 10,
            "templateId": 101,
            "equipSlots": [11, 0, 0, 0, 0, 0],
            "equipEffects": [{"type": 19, "effectIds": [1302, 1303]}]
        }]},
        "equip": {"equipBagSize": 2000, "items": [{
            "equipId": 11,
            "templateId": 30091,
            "enhanceLv": 10,
            "star": 2,
            "heroId": 10,
            "riseCommonEquips": [{"templateId": 90001, "num": 2}]
        }]}
    });

    let equip = equip_list_from_account(&account, None);
    let encoded_equip = EquipListCodec::encode(&equip);
    assert!(encoded_equip
        .windows(2)
        .any(|window| window == [0x42, 0x06]));

    let heroes = super::hero_bag_from_account(&account);
    let encoded_hero = HeroBagCodec::encode(&heroes);
    assert!(encoded_hero.windows(2).any(|window| window == [0xD2, 0x01]));
}

#[tokio::test]
async fn teacher_rank_returns_typed_current_user_row() {
    let state = ServerState::new("teacher-rank", "Captain", "1.4.0");
    let mut account = default_account_snapshot("teacher-rank", "Captain", 321);
    account["character"]["uid"] = json!(321);
    account["character"]["teacherPrestige"] = json!(123);

    let responses = battle_route_test_request(
        &mut account,
        &state,
        &BattleCatalog::default(),
        "user.TeacherRank",
        Vec::new(),
    )
    .await;
    let response = responses
        .iter()
        .find(|response| response.method == "user.TeacherRank")
        .expect("teacher rank response");
    let rows = decode_repeated_message_field(response.ret.as_deref().unwrap_or_default(), 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(decode_varint_field(&rows[0], 1), 321);
    assert_eq!(decode_varint_field(&rows[0], 11), 123);
}

#[tokio::test]
async fn friend_update_user_state_returns_typed_status_event() {
    let state = ServerState::new("friend-state", "Captain", "1.4.0");
    let mut account = default_account_snapshot("friend-state", "Captain", 321);
    let mut args = Vec::new();
    append_varint_field(&mut args, 1, 2);
    append_varint_field(&mut args, 2, 987);

    let responses = battle_route_test_request(
        &mut account,
        &state,
        &BattleCatalog::default(),
        "friend.UpdateUserState",
        args,
    )
    .await;
    let response = responses
        .iter()
        .find(|response| response.method == "friend.UpdateUserState")
        .expect("friend state response");
    let payload = response.ret.as_deref().unwrap_or_default();
    assert_eq!(decode_varint_field(payload, 1), 2);
    assert_eq!(decode_varint_field(payload, 2), 987);
}

#[test]
fn auto_equipment_changes_and_clears_slots_atomically() {
    let mut account = json!({
        "dock": {"heroes": [
            {"heroId": 10, "equipSlots": [0, 0, 0, 0, 0, 0]},
            {"heroId": 20, "equipSlots": [0, 0, 0, 0, 0, 0]}
        ]},
        "equip": {"items": [
            {"equipId": 11, "templateId": 30091, "heroId": 0},
            {"equipId": 12, "templateId": 30092, "heroId": 0}
        ]}
    });

    hero_auto_equip_state(&mut account, 1, &[(10, vec![(1, 11), (2, 12)])]).unwrap();
    assert_eq!(
        account["dock"]["heroes"][0]["equipSlots"],
        json!([11, 12, 0, 0, 0, 0])
    );
    assert_eq!(account["equip"]["items"][0]["heroId"], json!(10));

    hero_auto_unequip_state(&mut account, 1, &[10]).unwrap();
    assert_eq!(
        account["dock"]["heroes"][0]["equipSlots"],
        json!([0, 0, 0, 0, 0, 0])
    );
    assert_eq!(account["equip"]["items"][0]["heroId"], json!(0));
}

#[test]
fn equipment_binding_and_effects_persist_in_hero_state() {
    let mut account = json!({
        "dock": {"heroes": [{
            "heroId": 10,
            "equipSlots": [11, 0, 0, 0, 0, 0],
            "equipStates": [0, 0, 0, 0, 0, 0]
        }]},
        "equip": {"items": [{"equipId": 11, "templateId": 30091, "heroId": 10}]}
    });

    hero_equip_binding_state(&mut account, 10, 11, 1).unwrap();
    assert_eq!(account["dock"]["heroes"][0]["equipStates"][0], json!(1));
    hero_equip_effect_state(&mut account, 10, &[(19, vec![1302, 1303])]).unwrap();
    assert_eq!(
        account["dock"]["heroes"][0]["equipEffects"][0]["effectIds"],
        json!([1302, 1303])
    );
}

#[test]
fn locked_equipment_transplant_copies_normal_slots_to_special_fleet() {
    let mut account = json!({
        "dock": {"heroes": [{
            "heroId": 10,
            "equipSlots": [11, 0, 0, 0, 0, 0],
            "equipStates": [1, 0, 0, 0, 0, 0]
        }]},
        "equip": {"items": [{"equipId": 11, "templateId": 30091, "heroId": 10}]}
    });

    hero_equip_lock_transplant_state(&mut account, &[10], 2).unwrap();
    assert_eq!(
        account["dock"]["heroes"][0]["equipSlotsByType"]["2"],
        json!([11, 0, 0, 0, 0, 0])
    );
    assert_eq!(
        account["dock"]["heroes"][0]["equipStatesByType"]["2"],
        json!([1, 0, 0, 0, 0, 0])
    );
}

#[test]
fn equipment_extension_states_are_idempotent_and_projectable() {
    let mut account = json!({
        "equipTestCopy": {
            "maxDamage": 123,
            "receivedRewards": [{"rewardId": 7, "receiveTime": 9}]
        },
        "equipNewTestCopy": {"infos": []},
        "equipActivity": {
            "infos": [{"equipId": 11, "templateId": 30091, "powerPoint": 10, "isReward": 0, "extraRule": 2}]
        }
    });

    assert!(super::equip_test_copy_payload(&account)
        .windows(2)
        .any(|window| window == [0x08, 0x7B]));
    assert!(super::mark_new_test_reward(&mut account, 3, 2));
    assert!(!super::mark_new_test_reward(&mut account, 3, 2));
    assert!(super::mark_equip_activity_reward(&mut account, 11));
    assert!(!super::mark_equip_activity_reward(&mut account, 11));
    assert!(super::equip_new_test_copy_payload(&account).contains(&0x1A));
    assert!(super::equip_activity_payload(&account).contains(&0x20));
}

#[test]
fn default_account_snapshot_has_csharp_required_shape() {
    let account = default_account_snapshot("alice", "Alice", 123);

    assert_eq!(account["profileId"], "alice");
    assert_eq!(account["character"]["uid"], 1);
    assert_eq!(account["character"]["name"], "Alice");
    assert_eq!(account["character"]["secretaryId"], 1);
    assert_eq!(account["character"]["createTime"], 123);
    assert_eq!(account["character"]["pvePt"], 0);
    assert_eq!(account["dock"]["heroes"][0]["heroId"], 1);
    assert_eq!(account["dock"]["heroes"][0]["templateId"], 10210511);
    assert_eq!(account["dock"]["heroes"][0]["createTime"], 123);
    assert_eq!(account["dock"]["heroes"][0]["mood"], 1_500_000);
    assert_eq!(account["equip"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(account["fleet"]["tactics"].as_array().unwrap().len(), 5);
    assert_eq!(
        account["building"]["buildings"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn default_account_snapshot_starts_as_new_account() {
    let account = default_account_snapshot("new-player", "New Player", 123);
    let character = &account["character"];

    assert_eq!(character["level"], 1);
    assert_eq!(character["gold"], 0);
    assert_eq!(character["diamond"], 10_000);
    assert_eq!(character["supply"], 10_000);
    assert_eq!(character["pvePt"], 0);
    assert_eq!(character["plotChapterId"], 1);
    assert!(account["seaProgress"]["records"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(account["copyProgress"]["records"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(sea_difficulty_for_account(&account), 1);
}

#[test]
fn default_account_snapshot_initializes_server_feature_state() {
    let now = 123;
    let account = default_account_snapshot("new-player", "New Player", now);
    let reset_day = (i64::from(now) + 8 * 60 * 60) / 86_400;

    for key in [
        "dailyCopy",
        "tasks",
        "construction",
        "bath",
        "study",
        "illustrate",
        "sweep",
        "shiptask",
        "buildState",
        "talent",
        "battlePass",
        "activityBattlePass",
        "friend",
        "chat",
        "adventure",
        "outpost",
        "sportsMeet",
    ] {
        assert!(account.get(key).is_some(), "missing default state: {key}");
    }

    assert_eq!(account["dailyCopy"]["resetDay"], reset_day);
    assert_eq!(account["tasks"]["dailyResetDay"], reset_day);
    assert_eq!(account["tasks"]["records"], json!([]));
    assert_eq!(account["construction"]["nextSequence"], 1);
    assert_eq!(account["foodCompose"]["recipes"], json!({}));
    assert_eq!(account["copyStarRewards"], json!([]));
    assert_eq!(account["outpost"]["buildings"].as_array().unwrap().len(), 1);
    assert_eq!(account["sportsMeet"]["tickCount"], 10);
    assert_eq!(account["character"]["mainGun"], 0);
    assert_eq!(account["character"]["achievePoint"], 0);
    assert!(account.get("guild").is_none());
    assert!(account.get("battleSession").is_none());
}

#[test]
fn commander_battle_exp_rolls_into_next_level() {
    let mut account = json!({"character": {"level": 1, "exp": 119}});
    let catalog = CommanderLevelCatalog {
        exp_needed: [(1, 120), (2, 140)].into_iter().collect(),
    };
    super::add_commander_battle_exp(&mut account, 1, Some(&catalog));
    assert_eq!(account["character"]["level"], 2);
    assert_eq!(account["character"]["exp"], 0);
    super::add_commander_battle_exp(&mut account, 140, Some(&catalog));
    assert_eq!(account["character"]["level"], 3);
    assert_eq!(account["character"]["exp"], 0);
}

#[test]
fn battle_exp_updates_participating_ship_and_bath_restores_mood() {
    let mut account = json!({
        "dock": {"heroes": [
            {"heroId": 1, "level": 1, "exp": 199, "mood": 5000},
            {"heroId": 2, "level": 1, "exp": 0}
        ]}
    });
    let catalog = HeroLevelCatalog {
        exp_needed: [(1, 200), (2, 300)].into_iter().collect(),
        ..HeroLevelCatalog::default()
    };
    super::add_ship_battle_exp(&mut account, &[1], 1, Some(&catalog));
    assert_eq!(account["dock"]["heroes"][0]["level"], 2);
    assert_eq!(account["dock"]["heroes"][0]["exp"], 0);
    assert_eq!(account["dock"]["heroes"][1]["exp"], 0);
    assert!(super::ensure_hero_mood_state(&mut account));
    assert_eq!(account["dock"]["heroes"][0]["mood"], 5000);
    assert_eq!(account["dock"]["heroes"][1]["mood"], 1_500_000);
    assert!(super::restore_hero_mood(&mut account, 1));
    assert_eq!(account["dock"]["heroes"][0]["mood"], 1_500_000);
}

#[test]
fn missing_ship_mood_defaults_to_full_in_hero_bag() {
    let account = json!({
        "dock": {"heroes": [{"heroId": 1, "templateId": 10210511}]}
    });

    let bag = super::hero_bag_from_account(&account);
    assert_eq!(bag.heroes[0].mood, 1_500_000);
}

#[test]
fn newly_added_ship_starts_with_full_mood() {
    let mut account = json!({"dock": {"heroes": [], "bagSize": 200}});

    let hero_id = super::add_ship_items(&mut account, 10210511, 1, 123);

    assert_eq!(hero_id, 1);
    assert_eq!(account["dock"]["heroes"][0]["mood"], 1_500_000);
}

#[test]
fn zero_mood_is_preserved_on_account_load() {
    let mut account = json!({
        "dock": {"heroes": [{"heroId": 1, "mood": 0}]}
    });

    assert!(!super::ensure_hero_mood_state(&mut account));
    assert_eq!(account["dock"]["heroes"][0]["mood"], 0);
}

#[test]
fn ship_mood_is_clamped_to_client_upper_bound() {
    let mut account = json!({
        "dock": {"heroes": [{"heroId": 1, "mood": 2_000_000}]}
    });

    assert!(super::ensure_hero_mood_state(&mut account));
    assert_eq!(account["dock"]["heroes"][0]["mood"], 1_500_000);
}

#[test]
fn natural_mood_recovery_uses_elapsed_intervals_and_multiplier() {
    let mut account = json!({
        "dock": {"heroes": [
            {"heroId": 1, "mood": 1_000_000, "moodUpdateTime": 1000, "marryTime": 0},
            {"heroId": 2, "mood": 1_000_000, "moodUpdateTime": 1000, "marryTime": 1}
        ]}
    });

    assert!(super::apply_natural_mood_recovery(&mut account, 2200, 2.0));
    assert_eq!(account["dock"]["heroes"][0]["mood"], 1_000_600);
    assert_eq!(account["dock"]["heroes"][1]["mood"], 1_001_200);
    assert_eq!(account["dock"]["heroes"][0]["moodUpdateTime"], 2080);
}

#[test]
fn natural_mood_recovery_stops_at_normal_limit_and_keeps_partial_time() {
    let mut account = json!({
        "dock": {"heroes": [
            {"heroId": 1, "mood": 1_189_950, "moodUpdateTime": 1000, "marryTime": 0}
        ]}
    });

    assert!(super::apply_natural_mood_recovery(&mut account, 1450, 1.0));
    assert_eq!(account["dock"]["heroes"][0]["mood"], 1_190_000);
    assert_eq!(account["dock"]["heroes"][0]["moodUpdateTime"], 1360);
}

#[test]
fn battle_settlement_applies_configured_mood_and_affection_rules() {
    let mut account = json!({
        "dock": {"heroes": [
            {"heroId": 1, "affection": 500000, "mood": 1_500_000},
            {"heroId": 2, "affection": 500000, "mood": 1_500_000}
        ]}
    });
    let wrecked = [2_u64].into_iter().collect();
    let rule = super::BattleSettlementRule {
        affection_add: 500,
        affection_flagship_add: 125,
        affection_mvp_add: 125,
        affection_reduce: 10_000,
        mood_reduce: 20_000,
        mood_shipwrecks_reduce: 100_000,
    };

    assert!(super::apply_battle_settlement(
        &mut account,
        &[1, 2],
        Some(2),
        &wrecked,
        rule,
        1.0,
    ));
    assert_eq!(account["dock"]["heroes"][0]["affection"], 500750);
    assert_eq!(account["dock"]["heroes"][0]["mood"], 1_480_000);
    assert_eq!(account["dock"]["heroes"][1]["affection"], 490750);
    assert_eq!(account["dock"]["heroes"][1]["mood"], 1_400_000);
}

#[test]
fn battle_mood_stage_bonus_uses_mood_not_affection() {
    let mut account = json!({
        "dock": {"heroes": [
            {"heroId": 1, "affection": 500000, "marryTime": 1, "mood": 1200000},
            {"heroId": 2, "affection": 1200000, "marryTime": 1, "mood": 1000000}
        ]}
    });
    let wrecked = std::collections::HashSet::new();
    let rule = super::BattleSettlementRule {
        affection_add: 500,
        affection_flagship_add: 125,
        affection_mvp_add: 125,
        affection_reduce: 10_000,
        mood_reduce: 20_000,
        mood_shipwrecks_reduce: 100_000,
    };

    assert!(super::apply_battle_settlement(
        &mut account,
        &[1, 2],
        Some(1),
        &wrecked,
        rule,
        2.0,
    ));
    assert_eq!(account["dock"]["heroes"][0]["affection"], 501800);
    assert_eq!(account["dock"]["heroes"][1]["affection"], 1201000);
}

#[test]
fn zero_mood_blocks_affection_gain_and_high_mood_boosts_ship_exp() {
    let mut account = json!({
        "dock": {"heroes": [
            {"heroId": 1, "affection": 500000, "mood": 0, "level": 1, "exp": 0},
            {"heroId": 2, "affection": 500000, "mood": 1200000, "level": 1, "exp": 0}
        ]}
    });
    let rule = super::BattleSettlementRule {
        affection_add: 500,
        affection_flagship_add: 0,
        affection_mvp_add: 0,
        affection_reduce: 0,
        mood_reduce: 0,
        mood_shipwrecks_reduce: 0,
    };
    assert!(super::apply_battle_settlement(
        &mut account,
        &[1, 2],
        None,
        &std::collections::HashSet::new(),
        rule,
        1.0,
    ));
    assert_eq!(account["dock"]["heroes"][0]["affection"], 500000);
    assert_eq!(account["dock"]["heroes"][1]["affection"], 500600);

    super::add_ship_battle_exp(&mut account, &[1, 2], 100, None);
    assert_eq!(account["dock"]["heroes"][0]["exp"], 100);
    assert_eq!(account["dock"]["heroes"][1]["exp"], 120);
}

#[test]
fn battle_evaluation_ratios_follow_client_grade_config() {
    let mut catalog = BattleCatalog::default();
    catalog.evaluation_by_grade.insert(
        4,
        super::BattleEvaluationRule {
            exp_ratio: 5000,
            settle_drop_ratio: 2500,
            other_drop_ratio: 1250,
        },
    );

    assert_eq!(
        super::battle_evaluation_multipliers(Some(&catalog), 4),
        (0.5, 0.25)
    );
    assert_eq!(
        super::battle_evaluation_multipliers(Some(&catalog), 8),
        (1.0, 1.0)
    );
    assert_eq!(
        super::battle_other_drop_multiplier(Some(&catalog), 4),
        0.125
    );
}

#[test]
fn battle_rank_drop_selects_reward_for_grade() {
    let mut catalog = BattleCatalog::default();
    catalog.copy_rank_drop_ids.insert(19999, 1);
    catalog
        .rank_drop_rewards
        .insert(1, vec![(1, 5, 3000189), (2, 5, 3000190), (3, 5, 3000191)]);
    let reward = super::battle_rank_drop_reward(Some(&catalog), 19999, 2).expect("rank reward");
    assert_eq!(
        (reward.goods_type, reward.item_id, reward.num),
        (5, 3000190, 1)
    );
    assert!(super::battle_rank_drop_reward(Some(&catalog), 19999, 8).is_none());
}

#[test]
fn bath_mood_recovery_uses_client_values_and_multiplier() {
    let mut account = json!({
        "dock": {"heroes": [
            {"heroId": 1, "mood": 1000000, "moodUpdateTime": 1000}
        ]}
    });

    assert!(super::recover_hero_mood_from_bath(
        &mut account,
        1,
        1200,
        2.0,
        2200
    ));
    assert_eq!(account["dock"]["heroes"][0]["mood"], 1_160_000);
    assert_eq!(account["dock"]["heroes"][0]["moodUpdateTime"], 2200);
}

#[test]
fn battle_pass_result_decodes_mvp_and_shipwrecked_heroes() {
    let mut payload = Vec::new();
    append_varint_field(&mut payload, 8, 2);
    append_varint_field(&mut payload, 9, 2);
    append_varint_field(&mut payload, 12, 37);

    let mut healthy = Vec::new();
    append_varint_field(&mut healthy, 1, 1);
    append_varint_field(&mut healthy, 2, 10_000);
    append_message_field(&mut payload, 18, &healthy);

    let mut shipwrecked = Vec::new();
    append_varint_field(&mut shipwrecked, 1, 2);
    append_varint_field(&mut shipwrecked, 2, 0);
    append_message_field(&mut payload, 18, &shipwrecked);

    let result = super::decode_battle_pass_result(&payload);
    assert_eq!(result.grade, 2);
    assert_eq!(result.battle_time, 37);
    assert_eq!(result.mvp_hero_id, Some(2));
    assert!(!result.shipwrecked_ids.contains(&1));
    assert!(result.shipwrecked_ids.contains(&2));
}

#[test]
fn battle_pass_result_persists_returned_hp() {
    let mut account = json!({
        "dock": {"heroes": [
            {"heroId": 1, "curHp": 1000},
            {"heroId": 2, "curHp": 2000}
        ]}
    });
    let heroes = vec![
        super::BattleHeroResult {
            hero_id: 1,
            hp: 700,
        },
        super::BattleHeroResult { hero_id: 2, hp: 0 },
    ];

    assert!(super::save_battle_hero_hp(&mut account, &heroes, &[1, 2]));
    assert_eq!(account["dock"]["heroes"][0]["curHp"], 700);
    assert_eq!(account["dock"]["heroes"][1]["curHp"], 0);
}

#[test]
fn supply_updates_have_no_server_storage_cap() {
    let mut account = json!({"character": {"supply": 9_900}});
    adjust_character_i64(&mut account, "supply", 500);
    assert_eq!(account["character"]["supply"], 10_400);
    adjust_character_i64(&mut account, "supply", -20_000);
    assert_eq!(account["character"]["supply"], 0);
}

#[test]
fn completed_copy_ids_read_only_passed_records() {
    let account = json!({
        "seaProgress": {
            "records": [
                {"copyId": 1001, "starLevel": 7},
                {"copyId": 1002, "starLevel": 0, "passCount": 0},
                {"copyId": 1003, "starLevel": 0, "passCount": 1}
            ]
        }
    });

    assert_eq!(
        completed_copy_ids(&account, "seaProgress"),
        vec![1001, 1003]
    );
}

#[test]
fn copy_progress_max_uses_first_catalog_stage_when_unpassed() {
    assert_eq!(copy_progress_max_or_first(&[101, 102], &[]), 101);
    assert_eq!(copy_progress_max_or_first(&[101, 102], &[102]), 102);
    assert_eq!(copy_progress_max_or_first(&[], &[]), 0);
    assert_eq!(
        copy_progress_max_or_initial(&[5011, 1600100], &[], 1600100),
        1600100
    );
    assert_eq!(
        copy_progress_max_or_initial(&[5011, 1600100], &[5011], 1600100),
        5011
    );
}

#[test]
fn gameplay_event_state_helpers_apply_only_valid_actions() {
    let mut account = default_account_snapshot("events", "Events", 123);
    assert!(study_skill_state(&mut account, 1, 41));
    assert_eq!(account["dock"]["heroes"][0]["pSkills"][0]["pSkillId"], 41);
    account["dock"]["heroes"][0]["pSkills"][0]["pSkillLv"] = json!(7);
    account["dock"]["heroes"][0]["pSkills"][0]
        .as_object_mut()
        .unwrap()
        .remove("level");
    assert!(study_skill_state(&mut account, 1, 41));
    assert_eq!(account["dock"]["heroes"][0]["pSkills"][0]["level"], 8);
    assert!(!study_skill_state(&mut account, 999, 41));
    assert!(!study_skill_state(&mut account, 1, 0));

    add_bag_item(&mut account, 70001, 2);
    let mut equip_catalog = EquipCatalog::default();
    equip_catalog.enhance_max_by_template.insert(30091, 30);
    equip_catalog.enhance_materials.insert(70001, (1, None));
    equip_catalog.enhance_level_exp.insert(1, 1);
    equip_catalog.enhance_level_exp.insert(2, 1);
    equip_catalog.renovate_rules.insert(
        1,
        EquipRenovateRule {
            costs: Vec::new(),
            self_count: 1,
            need_level: 1,
        },
    );
    equip_catalog.star_max_by_template.insert(30091, 5);
    equip_catalog.quality_by_template.insert(30091, 1);
    equip_catalog.type_by_template.insert(30091, 1);
    equip_catalog.levelbreak_rules.insert(
        1,
        EquipLevelbreakRule {
            level_rank: Some((0, 40)),
            costs: vec![(5, 1, 1)],
        },
    );
    account["character"]["gold"] = json!(1);
    assert_eq!(
        enhance_equip_state(&mut account, Some(&equip_catalog), 1, &[(70001, 1)]),
        Some((1, 1))
    );
    assert_eq!(account["equip"]["items"][0]["enhanceLv"], 1);
    assert_eq!(bag_item_count(&account, 70001), 1);
    assert_eq!(
        enhance_equip_state(&mut account, Some(&equip_catalog), 1, &[(70001, 2)]),
        None
    );
    assert_eq!(
        enhance_bind_equip_state(&mut account, Some(&equip_catalog), 1),
        Some((2, 0))
    );
    assert_eq!(account["character"]["gold"], 0);

    account["equip"]["items"][1]["heroId"] = json!(0);
    account["equip"]["items"][1]["templateId"] = json!(30091);
    let before = account["equip"]["items"].as_array().unwrap().len();
    assert!(renovate_equip_state(
        &mut account,
        Some(&equip_catalog),
        1,
        &[2]
    ));
    assert_eq!(account["equip"]["items"][0]["star"], 1);
    assert_eq!(
        account["equip"]["items"].as_array().unwrap().len(),
        before - 1
    );
    assert!(!renovate_equip_state(
        &mut account,
        Some(&equip_catalog),
        1,
        &[1]
    ));
}

#[test]
fn one_click_enhance_selects_only_level_valid_materials() {
    let mut account = default_account_snapshot("auto-enhance", "Auto", 123);
    account["bag"]["items"] = json!([
        {"templateId": 60000, "num": 5},
        {"templateId": 60001, "num": 5},
        {"templateId": 60002, "num": 5},
        {"templateId": 60003, "num": 5}
    ]);
    let mut catalog = EquipCatalog::default();
    catalog.enhance_materials.insert(60000, (100, Some((0, 9))));
    catalog
        .enhance_materials
        .insert(60001, (200, Some((10, 19))));
    catalog
        .enhance_materials
        .insert(60002, (500, Some((20, 29))));
    catalog
        .enhance_materials
        .insert(60003, (500, Some((30, 100))));
    assert_eq!(
        auto_select_enhancement_materials(&account, Some(&catalog), 1),
        vec![(60000, 5)]
    );
    account["equip"]["items"][0]["enhanceLv"] = json!(20);
    assert_eq!(
        auto_select_enhancement_materials(&account, Some(&catalog), 1),
        vec![(60002, 5)]
    );
}

#[test]
fn study_training_round_trip_updates_skill_and_info() {
    let mut account = default_account_snapshot("study", "Study", 100);
    add_bag_item(&mut account, 70000, 1);
    assert!(start_study_state(&mut account, 1, 41, 70000, 100));
    assert_eq!(bag_item_count(&account, 70000), 0);
    let info = study_info_payload(&account, 100);
    assert!(info.contains(&0x12));
    assert!(finish_study_state(&mut account, 1, 41, 200).is_some());
    assert_eq!(account["dock"]["heroes"][0]["pSkills"][0]["level"], 2);
    assert_eq!(study_info_payload(&account, 200), vec![0x08, 0x02]);
}

#[test]
fn build_drop_pool_expands_nested_entries() {
    let mut catalog = BuildShipCatalog::default();
    catalog.pools.insert(1, vec![(4, 2, 0, 0, 1)]);
    catalog.pools.insert(2, vec![(3, 10210511, 1, 1, 1)]);
    let entries = expand_build_drop(&catalog, 1);
    assert_eq!(entries, vec![(3, 10210511, 1, 1, 1)]);
}

#[test]
fn build_draw_rolls_each_pull_independently() {
    let mut catalog = BuildShipCatalog::default();
    catalog.extract_to_drop.insert(1, 10);
    catalog
        .pools
        .insert(10, vec![(3, 1001, 1, 1, 50), (3, 1002, 1, 1, 50)]);
    assert_eq!(
        draw_build_ship_reward_with_roll(&catalog, 1, 0),
        Some((3, 1001, 1))
    );
    assert_eq!(
        draw_build_ship_reward_with_roll(&catalog, 1, 99),
        Some((3, 1002, 1))
    );
}

#[test]
fn build_count_box_draws_one_nested_reward() {
    let mut catalog = BuildShipCatalog::default();
    catalog
        .pools
        .insert(20, vec![(3, 1001, 1, 1, 50), (3, 1002, 1, 1, 50)]);
    assert_eq!(
        draw_build_drop_reward_with_roll(&catalog, 20, 0),
        Some((3, 1001, 1))
    );
    assert_eq!(
        draw_build_drop_reward_with_roll(&catalog, 20, 99),
        Some((3, 1002, 1))
    );
}

#[test]
fn ten_pull_guarantee_uses_positive_nested_sr_weights() {
    let mut catalog = BuildShipCatalog::default();
    catalog
        .pools
        .insert(10, vec![(4, 20, 1, 1, 1), (4, 30, 1, 1, 3)]);
    catalog
        .pools
        .insert(20, vec![(3, 1001, 1, 1, 1), (3, 1002, 1, 1, 0)]);
    catalog.pools.insert(30, vec![(3, 1003, 1, 1, 1)]);
    catalog.ship_quality.insert(1001, 3);
    catalog.ship_quality.insert(1002, 3);
    catalog.ship_quality.insert(1003, 3);
    assert_eq!(
        draw_sr_build_reward_with_roll(&catalog, 10, 0),
        Some((3, 1001, 1))
    );
    assert_eq!(
        draw_sr_build_reward_with_roll(&catalog, 10, u64::MAX / 2),
        Some((3, 1003, 1))
    );
}

#[test]
fn ten_pull_guarantee_normalizes_each_nested_pool() {
    let mut catalog = BuildShipCatalog::default();
    catalog
        .pools
        .insert(10, vec![(4, 20, 1, 1, 1), (4, 30, 1, 1, 1)]);
    catalog
        .pools
        .insert(20, vec![(3, 1001, 1, 1, 1), (3, 2001, 1, 1, 99)]);
    catalog.pools.insert(30, vec![(3, 1002, 1, 1, 1)]);
    catalog.ship_quality.insert(1001, 3);
    catalog.ship_quality.insert(1002, 3);
    catalog.ship_quality.insert(2001, 1);

    assert_eq!(
        draw_sr_build_reward_with_roll(&catalog, 10, 0),
        Some((3, 1001, 1))
    );
    assert_eq!(
        draw_sr_build_reward_with_roll(&catalog, 10, u64::MAX / 4 + 1),
        Some((3, 1002, 1))
    );
}

#[test]
fn build_catalog_loads_extract_costs_defaults_and_build_times() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/config");
    let catalog = super::load_build_ship_catalog(Some(&path));
    assert!(!catalog.extract_to_drop.is_empty());
    assert_eq!(catalog.extract_type_by_pool.get(&102), Some(&2));
    assert_eq!(
        catalog.expend_by_pool.get(&102).unwrap(),
        &vec![(1, 10007, 1)]
    );
    assert!(catalog.box_drop_by_pool_count.contains_key(&(102, 20)));
    assert!(catalog.reward_by_pool_count.contains_key(&(102, 100)));
    assert_eq!(catalog.ship_build_time.get(&10210511), Some(&3600));
    assert_eq!(
        catalog.ship_defaults.get(&10210511),
        Some(&vec![30091, 30221])
    );
    assert_eq!(catalog.ship_quality.get(&10210511), Some(&3));
    assert_eq!(catalog.treasure_drop_by_item.get(&10002), Some(&11002));
    assert_eq!(
        catalog
            .selected_treasure_by_item
            .get(&80001)
            .map(|v| v.drop_id),
        Some(50002)
    );
    let formulas = super::load_build_formula_catalog(Some(&path));
    assert!(formulas.0.len() > 30);
    assert_eq!(super::select_construction_template(30, 37, 43), 10210511);
}

#[test]
fn battle_catalog_loads_drop_info_rewards_for_sweeps() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/config");
    let catalog = super::load_battle_catalog(Some(&path));
    let drop_ids = catalog.copy_drop_ids.get(&20502).expect("copy drop info");
    assert!(!drop_ids.is_empty());
    assert!(drop_ids
        .iter()
        .flat_map(|id| catalog.drop_pools.get(id).into_iter().flatten())
        .any(|(goods_type, item_id, num, _, _)| *goods_type > 0 && *item_id > 0 && *num > 0));
    let fleet = catalog.fleet_rewards.get(&1101).expect("fleet rewards");
    assert!(fleet.ship_exp > 0);
    assert!(fleet.commander_exp > 0);
    assert!(catalog.evaluation_by_grade.contains_key(&1));
    assert_eq!(catalog.copy_rank_drop_ids.get(&19999), Some(&1));
    assert!(catalog
        .rank_drop_rewards
        .get(&1)
        .is_some_and(|rewards| !rewards.is_empty()));
}

#[test]
fn battle_settlement_includes_fleet_drop_pools() {
    let mut account = json!({"character": {"gold": 0}});
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        10001,
        BattleCopy {
            fleet_ids: vec![42],
            ..BattleCopy::default()
        },
    );
    catalog.fleet_drop_ids.insert(42, vec![900]);
    catalog.drop_pools.insert(900, vec![(5, 1, 7, 100, 1)]);

    let rewards =
        super::draw_battle_drop_rewards(&mut account, Some(&catalog), 10001, 1.0, 100, None);
    assert_eq!(rewards.len(), 1);
    assert_eq!(account["character"]["gold"], 7);
}

#[test]
fn sea_battle_settlement_includes_attached_fleet_rewards_and_experience() {
    let mut account = json!({"character": {"gold": 0}});
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        1600500,
        BattleCopy {
            config_id: 1600500,
            copy_type: 2,
            fleet_ids: vec![160050000],
        },
    );
    catalog
        .attached_fleet_ids
        .insert(160050000, vec![160050001]);
    catalog.fleet_rewards.insert(
        160050000,
        BattleFleetReward {
            commander_exp: 10,
            ship_exp: 20,
        },
    );
    catalog.fleet_rewards.insert(
        160050001,
        BattleFleetReward {
            commander_exp: 30,
            ship_exp: 40,
        },
    );
    catalog.fleet_drop_ids.insert(160050000, vec![900]);
    catalog.fleet_drop_ids.insert(160050001, vec![901]);
    catalog.drop_pools.insert(900, vec![(5, 1, 7, 100, 1)]);
    catalog.drop_pools.insert(901, vec![(5, 2, 11, 100, 1)]);

    assert_eq!(
        super::battle_copy_experience(Some(&catalog), 1600500),
        (40, 60)
    );
    let rewards =
        super::draw_battle_drop_rewards(&mut account, Some(&catalog), 1600500, 1.0, 100, None);
    assert_eq!(rewards.len(), 2);
    assert_eq!(account["character"]["gold"], 7);
    assert!(rewards
        .iter()
        .any(|reward| reward.item_id == 2 && reward.num == 11));
}

#[test]
fn battle_task_progress_respects_client_disabled_copy_types() {
    let mut catalog = BattleCatalog::default();
    assert!(super::battle_task_progress_enabled(Some(&catalog), 10001));
    catalog.task_disabled_copies.insert(10001);
    assert!(!super::battle_task_progress_enabled(Some(&catalog), 10001));
}

#[test]
fn battle_catalog_loads_mood_and_affection_rules_for_sea_stage() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/config");
    let catalog = super::load_battle_catalog(Some(&path));
    let rule = catalog
        .settlement_by_copy
        .get(&1_600_100)
        .expect("first sea stage settlement rule");

    assert_eq!(rule.affection_add, 500);
    assert_eq!(rule.affection_flagship_add, 125);
    assert_eq!(rule.affection_mvp_add, 125);
    assert_eq!(rule.affection_reduce, 10_000);
    assert_eq!(rule.mood_reduce, 20_000);
    assert_eq!(rule.mood_shipwrecks_reduce, 100_000);
}

#[test]
fn hero_skill_catalog_repairs_default_hero_rows() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/config");
    let catalog = super::load_hero_skill_catalog(Some(&path));
    let configured = catalog.get(&10210511).expect("starter hero skills");
    assert!(configured.contains(&4001));
    let mut account = json!({"dock": {"heroes": [{"heroId": 1, "templateId": 10210511}]} });
    assert!(super::ensure_hero_pskills(&mut account, &catalog));
    assert_eq!(account["dock"]["heroes"][0]["pSkills"][0]["level"], 1);
    assert!(!super::ensure_hero_pskills(&mut account, &catalog));
}

#[test]
fn client_catalog_loads_skill_costs_and_offline_lucky_bag_rewards() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/config");
    let skills = super::load_hero_skill_upgrade_catalog(Some(&path));
    assert_eq!(
        skills
            .costs_by_skill
            .get(&10642)
            .and_then(|levels| levels.first()),
        Some(&vec![(1, 10182, 4)])
    );

    let recharge = super::load_recharge_catalog(Some(&path));
    let rewards = recharge
        .rewards_by_recharge_id
        .get(&1004)
        .expect("lucky bag recharge product");
    assert!(rewards
        .iter()
        .any(|reward| reward.goods_type == 5 && reward.item_id == 2 && reward.num == 2450));
}

#[test]
fn study_skill_state_increments_existing_level() {
    let mut account = json!({
        "dock": {"heroes": [{"heroId": 3, "pSkills": [{"pSkillId": 10642, "level": 1}]}]}
    });
    assert!(study_skill_state(&mut account, 3, 10642));
    assert_eq!(account["dock"]["heroes"][0]["pSkills"][0]["level"], 2);
    assert!(study_skill_state(&mut account, 3, 10642));
    assert_eq!(account["dock"]["heroes"][0]["pSkills"][0]["level"], 3);
}

#[test]
fn study_skill_state_completes_legacy_missing_skill_at_level_two() {
    let mut account = json!({
        "dock": {"heroes": [{"heroId": 3, "pSkills": []}]}
    });
    assert!(study_skill_state(&mut account, 3, 10642));
    assert_eq!(account["dock"]["heroes"][0]["pSkills"][0]["level"], 2);
}

#[test]
fn skill_upgrade_consumes_configured_materials_atomically() {
    let mut account = json!({
        "character": {"gold": 0},
        "bag": {"items": [{"templateId": 10182, "num": 4}]},
    });
    let catalog = HeroSkillUpgradeCatalog {
        costs_by_skill: [(10642, vec![vec![(1, 10182, 4)], vec![(1, 10182, 6)]])]
            .into_iter()
            .collect(),
    };
    assert!(consume_hero_skill_upgrade_materials(
        &mut account,
        &catalog,
        10642,
        1
    ));
    assert_eq!(bag_item_count(&account, 10182), 0);
    assert!(!consume_hero_skill_upgrade_materials(
        &mut account,
        &catalog,
        10642,
        2
    ));
    assert_eq!(bag_item_count(&account, 10182), 0);
}

#[test]
fn study_stop_request_resolves_skill_from_hero_only() {
    let mut account = json!({
        "dock": {"heroes": [{
            "heroId": 3,
            "pSkills": [{"pSkillId": 10642, "level": 1}]
        }]},
        "study": {"progress": [{
            "heroId": 3,
            "pSkillId": 10642,
            "textbookId": 70000,
            "endTime": 100
        }]}
    });
    let skill_id = resolve_study_skill_id(&account, 3, 0);
    assert_eq!(skill_id, Some(10642));
    assert!(finish_study_state(&mut account, 3, skill_id.unwrap(), 100).is_some());
    assert_eq!(account["dock"]["heroes"][0]["pSkills"][0]["level"], 2);
}

#[test]
fn completed_sweep_settles_configured_rewards_once() {
    let mut account = json!({
        "character": {"gold": 0},
        "sweep": {"entries": [{"fleetId": 1, "copyId": 10001, "endTime": 100, "sweepCounts": 3}]}
    });
    let mut catalog = BattleCatalog::default();
    catalog.copy_drop_ids.insert(10001, vec![900]);
    catalog.drop_pools.insert(900, vec![(5, 1, 100, 100, 1)]);
    let rewards = settle_mop_up(&mut account, Some(&catalog), None, 100);
    assert_eq!(rewards.len(), 3);
    assert_eq!(account["character"]["gold"], 300);
    assert!(account["sweep"]["entries"].as_array().unwrap().is_empty());
    assert!(settle_mop_up(&mut account, Some(&catalog), None, 100).is_empty());
}

#[test]
fn sweep_equipment_materials_persist_in_bag_and_equip_counts() {
    let mut account = json!({"bag": {"items": []}, "equip": {"items": []}});
    let mut catalog = EquipCatalog::default();
    for template_id in [60000, 60001, 60002] {
        let reward = super::grant_reward(
            &mut account,
            ShopReward {
                goods_type: 6,
                item_id: template_id,
                num: 5,
                instance_id: 0,
            },
            100,
            None,
        );
        assert_eq!(reward.num, 5);
        assert_eq!(bag_item_count(&account, template_id), 5);
        catalog.enhance_materials.insert(template_id, (100, None));
    }
    assert_eq!(
        equip_list_from_account(&account, Some(&catalog))
            .nums
            .into_iter()
            .map(|item| (item.template_id, item.num))
            .collect::<Vec<_>>(),
        vec![(60000, 5), (60001, 5), (60002, 5)]
    );
}

#[test]
fn medal_reward_persists_and_round_trips_in_user_info() {
    let mut account = json!({"character": {"uid": 1}, "medals": []});
    let reward = super::grant_reward(
        &mut account,
        ShopReward {
            goods_type: 16,
            item_id: 168226,
            num: 1,
            instance_id: 0,
        },
        1_725_000_000,
        None,
    );
    assert_eq!(reward.item_id, 168226);
    assert_eq!(account["medals"][0]["medalId"], 168226);
    assert_eq!(account["medals"][0]["time"], 1_725_000_000u32);

    let state = ServerState::new("medal-test", "Captain", "1.4.0");
    let user = super::user_info_from_account(&state, Some(&account));
    let payload = UserInfoCodec::encode(&user);
    let decoded = UserInfoCodec::decode(&payload).unwrap();
    assert_eq!(decoded.medal_acquired_times, user.medal_acquired_times);
    assert_eq!(decoded.medal_acquired_times[0].medal_id, 168226);
}

#[test]
fn drop_multiplier_uses_independent_draws_without_scaling_item_quantity() {
    assert_eq!(draw_draw_count(0.0, 1), 0);
    assert_eq!(draw_draw_count(1.0, 1), 1);
    assert_eq!(draw_draw_count(2.0, 1), 2);
    assert_eq!(draw_draw_count(2.5, 0), 3);
    assert_eq!(draw_draw_count(2.5, 999_999), 2);

    let mut catalog = BattleCatalog::default();
    catalog.drop_pools.insert(900, vec![(5, 1, 7, 100, 1)]);
    let first = draw_copy_drop_with_seed(&catalog, 900, 0, 1).expect("drop");
    let second = draw_copy_drop_with_seed(&catalog, 900, 0, 2).expect("drop");
    assert_eq!(first.num, 7);
    assert_eq!(second.num, 7);
    catalog
        .drop_pools
        .insert(900, vec![(5, 1, 7, 1, 1), (5, 2, 3, 1, 1)]);
    catalog.copy_drop_ids.insert(10001, vec![900]);
    let mut account = json!({"character": {"gold": 0, "diamond": 0}});
    let rewards =
        super::draw_battle_drop_rewards(&mut account, Some(&catalog), 10001, 100.0, 100, None);
    assert_eq!(rewards.len(), 100);
    assert!(rewards.iter().any(|r| r.item_id == 1));
    assert!(rewards.iter().any(|r| r.item_id == 2));
    assert!(rewards
        .iter()
        .all(|r| r.num == if r.item_id == 1 { 7 } else { 3 }));
}

#[test]
fn completed_sweep_grants_configured_experience() {
    let mut account = json!({
        "character": {"level": 1, "exp": 0, "gold": 0, "supply": 10000},
        "dock": {"heroes": [{"heroId": 7, "level": 1, "exp": 0}]},
        "fleet": {"tactics": [{"fleetId": 1, "heroIds": [7]}]},
        "sweep": {"entries": [{"fleetId": 1, "copyId": 10001, "endTime": 100, "sweepCounts": 3}]}
    });
    let mut catalog = BattleCatalog::default();
    catalog.copy_drop_ids.insert(10001, vec![900]);
    catalog.drop_pools.insert(900, vec![(5, 1, 1, 100, 1)]);
    catalog.copies.insert(
        10001,
        BattleCopy {
            config_id: 10001,
            copy_type: 1,
            fleet_ids: vec![1101],
        },
    );
    catalog.fleet_rewards.insert(
        1101,
        BattleFleetReward {
            commander_exp: 10,
            ship_exp: 20,
        },
    );
    let mut commander = CommanderLevelCatalog::default();
    commander.exp_needed.insert(1, 100);
    let mut heroes = HeroLevelCatalog::default();
    heroes.exp_needed.insert(1, 100);

    let rewards = settle_mop_up_with_config(
        &mut account,
        Some(&catalog),
        None,
        100,
        2.0,
        2.0,
        3.0,
        Some(&commander),
        Some(&heroes),
    );
    assert_eq!(rewards.len(), 6);
    assert_eq!(account["character"]["gold"], json!(6));
    assert_eq!(account["character"]["exp"], json!(60));
    assert_eq!(account["dock"]["heroes"][0]["level"], json!(2));
    assert_eq!(account["dock"]["heroes"][0]["exp"], json!(116));
    assert_eq!(account["character"]["supply"], json!(10000));
    let settled = account.clone();
    assert!(settle_mop_up_with_config(
        &mut account,
        Some(&catalog),
        None,
        100,
        2.0,
        2.0,
        3.0,
        Some(&commander),
        Some(&heroes)
    )
    .is_empty());
    assert_eq!(account, settled);
}

#[test]
fn battle_supply_charges_fleet_and_sweep_count_without_partial_debit() {
    let mut account = json!({"character":{"supply":100}, "dock":{"heroes":[
        {"heroId":7,"templateId":101}, {"heroId":8,"templateId":102}
    ]}});
    let mut catalog = BattleCatalog::default();
    catalog.supply_cost_by_copy.insert(10001, (10, 20000));
    catalog.ship_supply_cost.insert(101, 4);
    catalog.ship_supply_cost.insert(102, 6);
    assert!(super::consume_battle_supply(
        &mut account,
        Some(&catalog),
        10001,
        &[7, 8],
        3
    ));
    assert_eq!(account["character"]["supply"], 10);
    let before = account.clone();
    assert!(!super::consume_battle_supply(
        &mut account,
        Some(&catalog),
        10001,
        &[7, 8],
        1
    ));
    assert_eq!(account, before);
    assert!(!super::consume_battle_supply(
        &mut account,
        Some(&catalog),
        10001,
        &[],
        1
    ));
    assert!(!super::consume_battle_supply(
        &mut account,
        Some(&catalog),
        10001,
        &[99],
        1
    ));
}

async fn battle_route_test_request(
    account: &mut serde_json::Value,
    state: &ServerState,
    catalog: &BattleCatalog,
    method: &str,
    args: Vec<u8>,
) -> Vec<blueoath_protocol::TResponse> {
    let (mut client, mut server) = duplex(1_048_576);
    let request = TMessageCodec::encode_request(&TRequest {
        method: method.to_owned(),
        args: Some(args),
        callback_handler: 71,
        ..TRequest::default()
    });
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();
    process_game_login_frame_with_catalog_mut(
        &mut server,
        state,
        Some(account),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(catalog),
        None,
        None,
    )
    .await
    .unwrap();
    drop(server);
    let mut responses = Vec::new();
    while let Some(frame) = NetSocketFrameCodec::read(&mut client).await.unwrap() {
        responses.push(TMessageCodec::decode_response(&frame.payload).unwrap());
    }
    responses
}

#[tokio::test]
async fn battle_match_routes_return_typed_local_responses() {
    let mut account = default_account_snapshot("match-test", "Captain", 123);
    let mut state = ServerState::new("match-test", "Captain", "1.4.0");
    state.battle_port = 19_090;
    let catalog = BattleCatalog::default();

    let responses = battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "battle.CreateRoom",
        Vec::new(),
    )
    .await;
    let create_response = responses
        .iter()
        .find(|response| response.method == "battle.CreateRoom")
        .expect("create room response");
    assert_eq!(create_response.err, 0);
    let room_id = decode_varint_field(create_response.ret.as_deref().unwrap_or_default(), 1);
    assert!(room_id > 0);
    assert_eq!(account["battleRoom"]["roomId"], json!(room_id));

    let mut guest = default_account_snapshot("match-guest", "Guest", 123);
    guest["character"]["uid"] = json!(2);
    let mut join_args = Vec::new();
    append_varint_field(&mut join_args, 1, room_id as u64);
    let join_responses =
        battle_route_test_request(&mut guest, &state, &catalog, "battle.JoinRoom", join_args).await;
    assert!(join_responses
        .iter()
        .find(|response| response.method == "battle.JoinRoom")
        .is_some_and(|response| response.err == 0));

    let mut owner_chat_args = Vec::new();
    append_varint_field(&mut owner_chat_args, 1, 9);
    battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "battle.SendAutoMsg",
        owner_chat_args,
    )
    .await;
    let guest_poll = battle_route_test_request(
        &mut guest,
        &state,
        &catalog,
        "battle.MatchLeave",
        Vec::new(),
    )
    .await;
    assert!(guest_poll
        .iter()
        .any(|response| response.method == "battle.receiveAutoMsg"));

    let mut match_args = Vec::new();
    append_varint_field(&mut match_args, 1, 2);
    let guest_match = battle_route_test_request(
        &mut guest,
        &state,
        &catalog,
        "battle.MatchJoin",
        match_args.clone(),
    )
    .await;
    assert!(guest_match.iter().any(|response| {
        response.method == "battle.MatchJoin"
            && response.err == 0
            && response.ret.as_deref().unwrap_or_default()
                == [
                    0x0a, 0x0b, 0x0a, 0x09, 0x08, 0x02, 0x12, 0x05, b'l', b'o', b'c', b'a', b'l',
                ]
    }));
    let duplicate_guest_match = battle_route_test_request(
        &mut guest,
        &state,
        &catalog,
        "battle.MatchJoin",
        match_args.clone(),
    )
    .await;
    assert!(duplicate_guest_match.iter().any(|response| {
        response.method == "battle.MatchJoin"
            && response.err == 0
            && response.ret.as_deref().unwrap_or_default()
                == [
                    0x0a, 0x0b, 0x0a, 0x09, 0x08, 0x02, 0x12, 0x05, b'l', b'o', b'c', b'a', b'l',
                ]
    }));
    let responses = battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "battle.MatchJoin",
        match_args,
    )
    .await;
    let match_response = responses
        .iter()
        .find(|response| response.method == "battle.MatchJoin")
        .expect("match join response");
    assert_eq!(match_response.err, 0);
    assert_eq!(
        match_response.ret.as_deref().unwrap_or_default(),
        &[
            0x0a, 0x16, 0x0a, 0x09, 0x08, 0x02, 0x12, 0x05, b'l', b'o', b'c', b'a', b'l', 0x0a,
            0x09, 0x08, 0x01, 0x12, 0x05, b'l', b'o', b'c', b'a', b'l'
        ]
    );
    let guest_match_push = battle_route_test_request(
        &mut guest,
        &state,
        &catalog,
        "battle.createBattleInfo",
        Vec::new(),
    )
    .await;
    assert!(guest_match_push
        .iter()
        .any(|response| response.method == "battle.MatchJoin"));

    let mut chat_args = Vec::new();
    append_varint_field(&mut chat_args, 1, 9);
    let responses = battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "battle.SendAutoMsg",
        chat_args,
    )
    .await;
    assert!(responses
        .iter()
        .any(|response| response.method == "battle.receiveAutoMsg"));

    let multi_battle = battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "battle.CreateMutiBattle",
        Vec::new(),
    )
    .await;
    let multi_response = multi_battle
        .iter()
        .find(|response| response.method == "battle.CreateMutiBattle")
        .expect("multi battle response");
    assert_eq!(multi_response.err, 0);
    assert_eq!(
        decode_varint_field(multi_response.ret.as_deref().unwrap_or_default(), 3),
        19_090
    );
}

#[tokio::test]
async fn copy_extra_and_pvp_ready_routes_return_typed_state() {
    let mut account = default_account_snapshot("battle-extra", "Captain", 123);
    let state = ServerState::new("battle-extra", "Captain", "1.4.0");
    let catalog = BattleCatalog::default();

    let mut add_args = Vec::new();
    append_varint_field(&mut add_args, 1, 77);
    append_varint_field(&mut add_args, 2, 4);
    let added = battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "copyextra.AddCopyRewardCount",
        add_args,
    )
    .await;
    let add_response = added
        .iter()
        .find(|response| response.method == "copyextra.AddCopyRewardCount")
        .expect("copy reward count response");
    assert_eq!(add_response.err, 0);
    assert_eq!(
        decode_varint_field(add_response.ret.as_deref().unwrap(), 1),
        77
    );
    assert_eq!(
        decode_varint_field(add_response.ret.as_deref().unwrap(), 2),
        4
    );

    let extra = battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "copyextra.UpdateCopyExtraInfo",
        Vec::new(),
    )
    .await;
    let extra_response = extra
        .iter()
        .find(|response| response.method == "copyextra.UpdateCopyExtraInfo")
        .expect("copy extra response");
    let reward_rows = decode_repeated_message_field(extra_response.ret.as_deref().unwrap(), 1);
    assert_eq!(reward_rows.len(), 1);
    assert_eq!(decode_varint_field(&reward_rows[0], 1), 77);
    assert_eq!(decode_varint_field(&reward_rows[0], 2), 4);

    let ready = battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "battle.pvpMatchReady",
        Vec::new(),
    )
    .await;
    let ready_response = ready
        .iter()
        .find(|response| response.method == "battle.pvpMatchReady")
        .expect("pvp ready response");
    let room_id = decode_varint_field(ready_response.ret.as_deref().unwrap(), 1);
    assert!(room_id > 0);
    assert_eq!(account["pvpMatch"]["state"], "ready");

    let timeout = battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "battle.pvpMatchReadyTimeout",
        Vec::new(),
    )
    .await;
    let timeout_response = timeout
        .iter()
        .find(|response| response.method == "battle.pvpMatchReadyTimeout")
        .expect("pvp timeout response");
    assert_eq!(
        decode_varint_field(timeout_response.ret.as_deref().unwrap(), 1),
        room_id
    );
    assert_eq!(account["pvpMatch"]["state"], "timeout");
}

#[tokio::test]
async fn choose_sea_safe_level_uses_copy_id_and_safe_level_fields() {
    let mut account = default_account_snapshot("safe-level", "Captain", 123);
    account["character"]["level"] = json!(60);
    let state = ServerState::new("safe-level", "Captain", "1.4.0");
    let catalog = BattleCatalog::default();
    let mut args = Vec::new();
    append_varint_field(&mut args, 1, 1_600_100);
    append_varint_field(&mut args, 2, 5);

    let responses =
        battle_route_test_request(&mut account, &state, &catalog, "copy.ChooseSfLv", args).await;
    let response = responses
        .iter()
        .find(|response| response.method == "copy.ChooseSfLv")
        .expect("safe level response");
    assert_eq!(response.err, 0);
    assert_eq!(account["character"]["seaDifficulty"], 5);
}

#[tokio::test]
async fn daily_copy_enter_returns_nested_start_base_and_opens_session() {
    let state = ServerState::new("daily-enter", "Captain", "1.4.0");
    let mut account = default_account_snapshot("daily-enter", "Captain", 123);
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        10001,
        BattleCopy {
            config_id: 10001,
            copy_type: 1,
            fleet_ids: vec![1000100],
        },
    );
    let mut args = Vec::new();
    append_varint_field(&mut args, 1, 1);
    append_varint_field(&mut args, 2, 10001);
    append_varint_field(&mut args, 3, 1);

    let responses =
        battle_route_test_request(&mut account, &state, &catalog, "dailycopy.CopyEnter", args)
            .await;
    let response = responses
        .iter()
        .find(|response| response.method == "dailycopy.CopyEnter")
        .expect("daily enter response");
    assert_eq!(response.err, 0);
    let start = decode_repeated_message_field(response.ret.as_deref().unwrap_or_default(), 1);
    assert_eq!(start.len(), 1);
    assert_eq!(decode_varint_field(&start[0], 6), 10001);
    assert_eq!(account["battleSession"]["copyId"], 10001);
    assert_eq!(account["dailyCopy"]["chapters"][0]["lastTacticId"], 1);
    assert!(responses
        .iter()
        .any(|response| response.method == "dailycopy.UpdateDailyCopyData"));
}

#[tokio::test]
async fn shared_pve_room_is_visible_across_accounts() {
    let state = ServerState::new("match-test", "Captain", "1.4.0");
    let catalog = BattleCatalog::default();
    let mut owner = default_account_snapshot("owner", "Owner", 123);
    let owner_uid = owner["character"]["uid"].as_u64().unwrap();
    let mut live_events = state.shared_social.lock().unwrap().push_tx.subscribe();
    let mut create_args = Vec::new();
    append_varint_field(&mut create_args, 2, 5011);
    battle_route_test_request(
        &mut owner,
        &state,
        &catalog,
        "matchsvr.CreateRoom",
        create_args,
    )
    .await;
    let room_id = owner["pveRoom"]["roomId"].as_u64().unwrap();

    let mut guest = default_account_snapshot("guest", "Guest", 123);
    guest["character"]["uid"] = json!(2);
    let mut enter_args = Vec::new();
    append_varint_field(&mut enter_args, 1, room_id);
    battle_route_test_request(
        &mut guest,
        &state,
        &catalog,
        "matchsvr.EnterRoom",
        enter_args,
    )
    .await;
    assert_eq!(guest["pveRoom"]["users"].as_array().unwrap().len(), 2);
    let room_event = live_events.try_recv().expect("room broadcast");
    assert_eq!(room_event.recipient_uid, owner_uid);
    assert_eq!(room_event.method, "match.UpdateRoomInfo");

    let responses = battle_route_test_request(
        &mut owner,
        &state,
        &catalog,
        "matchsvr.GetRoomList",
        Vec::new(),
    )
    .await;
    assert_eq!(owner["pveRoom"]["users"].as_array().unwrap().len(), 2);
    let list_response = responses
        .iter()
        .find(|response| response.method == "matchsvr.GetRoomList")
        .expect("room list response");
    assert_eq!(
        decode_repeated_message_field(list_response.ret.as_deref().unwrap_or_default(), 1).len(),
        1
    );
    assert!(responses
        .iter()
        .any(|response| response.method == "match.UpdateRoomInfo"));
}

async fn guild_route_test_request(
    account: &mut serde_json::Value,
    state: &ServerState,
    method: &str,
    args: Vec<u8>,
) -> Vec<blueoath_protocol::TResponse> {
    let (mut client, mut server) = duplex(1_048_576);
    let request = TMessageCodec::encode_request(&TRequest {
        method: method.to_owned(),
        args: Some(args),
        callback_handler: 72,
        ..TRequest::default()
    });
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();
    process_game_login_frame_with_catalog_mut(
        &mut server,
        state,
        Some(account),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    drop(server);
    let mut responses = Vec::new();
    while let Some(frame) = NetSocketFrameCodec::read(&mut client).await.unwrap() {
        responses.push(TMessageCodec::decode_response(&frame.payload).unwrap());
    }
    responses
}

#[tokio::test]
async fn guild_create_pushes_own_info_and_member_info() {
    let state = ServerState::new("guild-test", "Captain", "1.4.0");
    let mut account = default_account_snapshot("guild-test", "Captain", 100);
    let mut args = Vec::new();
    append_message_field(&mut args, 1, b"Test Fleet");

    let responses = guild_route_test_request(&mut account, &state, "guild.Create", args).await;

    assert_eq!(account["guild"]["name"], "Test Fleet");
    assert!(responses
        .iter()
        .any(|response| response.method == "guild.UpdateOurGuildData"));
    assert!(responses
        .iter()
        .any(|response| response.method == "guild.UpdateMyGuildData"));

    let login = guild_route_test_request(&mut account, &state, "user.UserLogin", Vec::new()).await;
    assert!(login
        .iter()
        .any(|response| response.method == "guild.UpdateOurGuildData"));
}

#[tokio::test]
async fn guild_list_apply_member_and_quit_round_trip() {
    let state = ServerState::new("guild-apply-test", "Captain", "1.4.0");
    let mut account = default_account_snapshot("guild-apply-test", "Captain", 100);

    let mut list_args = Vec::new();
    append_varint_field(&mut list_args, 1, 0);
    append_varint_field(&mut list_args, 2, 10);
    let list = guild_route_test_request(&mut account, &state, "guild.GetList", list_args).await;
    let list_response = list.last().expect("guild list response");
    assert_eq!(
        decode_varint_field(list_response.ret.as_deref().unwrap(), 1),
        1
    );
    assert_eq!(
        decode_repeated_message_field(list_response.ret.as_deref().unwrap(), 2).len(),
        1
    );

    let mut apply_args = Vec::new();
    append_varint_field(&mut apply_args, 1, DEFAULT_GUILD_ID);
    let apply = guild_route_test_request(&mut account, &state, "guild.Apply", apply_args).await;
    assert_eq!(account["guild"]["myPost"], GUILD_MEMBER);
    assert!(apply
        .iter()
        .any(|response| response.method == "guild.UpdateOurGuildData"));

    let members =
        guild_route_test_request(&mut account, &state, "guild.GetMemberList", Vec::new()).await;
    assert_eq!(
        decode_repeated_message_field(members.last().unwrap().ret.as_deref().unwrap(), 1).len(),
        1
    );

    guild_route_test_request(&mut account, &state, "guild.Quit", Vec::new()).await;
    assert!(account.get("guild").is_none());
}

#[tokio::test]
async fn battle_routes_charge_supply_and_refresh_sweep_exp_with_zero_drops() {
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        5011,
        BattleCopy {
            config_id: 5011,
            copy_type: 1,
            fleet_ids: vec![1101],
        },
    );
    catalog.supply_cost_by_copy.insert(5011, (10, 20000));
    catalog.ship_supply_cost.insert(10210511, 5);
    catalog.fleet_rewards.insert(
        1101,
        BattleFleetReward {
            commander_exp: 10,
            ship_exp: 20,
        },
    );
    let mut state = ServerState::new("battle-test", "test", "1.4.0");
    state.drop_multiplier = 0.0;
    state.ship_exp_multiplier = 3.0;
    state.commander_exp_multiplier = 2.0;
    let mut account = default_account_snapshot("battle-test", "test", 100);
    account["character"]["supply"] = json!(100);
    account["fleet"]["tactics"] = json!([{"heroInfo":[1]}]);
    let mut start = Vec::new();
    append_varint_field(&mut start, 2, 5011);
    let responses = battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "copy.StartBase",
        start.clone(),
    )
    .await;
    assert!(responses
        .iter()
        .any(|r| r.method == "copy.StartBase" && r.err == 0));
    assert!(responses.iter().any(|r| r.method == "user.UpdateUserInfo"));
    assert_eq!(account["character"]["supply"], 80);
    account["battleSession"] = serde_json::Value::Null;
    record_battle_pass(&mut account, 5011, 3, 10, Some(&catalog), &[]);
    let mut sweep = Vec::new();
    append_varint_field(&mut sweep, 1, 1);
    append_varint_field(&mut sweep, 2, 5011);
    append_varint_field(&mut sweep, 3, 3);
    let responses = battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "mopUp.StartSweep",
        sweep.clone(),
    )
    .await;
    assert!(responses
        .iter()
        .any(|r| r.method == "mopUp.StartSweep" && r.err == 0));
    assert!(responses
        .iter()
        .any(|r| r.method == "hero.UpdateHeroBagData"));
    assert!(responses.iter().any(|r| r.method == "user.UpdateUserInfo"));
    assert_eq!(account["character"]["supply"], 20);
    assert_eq!(account["character"]["exp"], 60);
    assert_eq!(account["dock"]["heroes"][0]["exp"], 216);
    let settled = account.clone();
    battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "mopUp.GetMopUpData",
        Vec::new(),
    )
    .await;
    assert_eq!(account, settled);
    let responses =
        battle_route_test_request(&mut account, &state, &catalog, "mopUp.StartSweep", sweep).await;
    assert!(responses
        .iter()
        .any(|r| r.method == "mopUp.StartSweep" && r.err != 0));
    assert_eq!(account, settled);
    account["character"]["supply"] = json!(0);
    let empty = account.clone();
    let responses =
        battle_route_test_request(&mut account, &state, &catalog, "copy.StartBase", start).await;
    assert!(responses
        .iter()
        .any(|r| r.method == "copy.StartBase" && r.err != 0));
    assert_eq!(account, empty);
}

fn battle_pass_args(
    copy_id: i32,
    grade: i32,
    battle_time: i32,
    fleet_id: i32,
    hero_id: u64,
    hp: u64,
) -> Vec<u8> {
    let mut pass = Vec::new();
    append_varint_field(&mut pass, 1, copy_id as u64);
    append_varint_field(&mut pass, 8, grade as u64);
    append_varint_field(&mut pass, 12, battle_time as u64);
    let mut fleet = Vec::new();
    append_varint_field(&mut fleet, 1, fleet_id as u64);
    append_message_field(&mut pass, 17, &fleet);
    let mut hero = Vec::new();
    append_varint_field(&mut hero, 1, hero_id);
    append_varint_field(&mut hero, 2, hp);
    append_message_field(&mut pass, 18, &hero);
    pass
}

#[tokio::test]
async fn battle_pass_route_persists_client_time_and_hp() {
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        5011,
        BattleCopy {
            config_id: 5011,
            copy_type: 1,
            fleet_ids: vec![1101],
        },
    );
    catalog.fleet_enemies.insert(1101, vec![900]);
    catalog.enemies.insert(
        900,
        BattleEnemy {
            hp: 100,
            ..BattleEnemy::default()
        },
    );
    catalog.supply_cost_by_copy.insert(5011, (10, 0));
    let state = ServerState::new("battle-pass-route", "test", "1.4.0");
    let mut account = default_account_snapshot("battle-pass-route", "test", 100);
    account["character"]["supply"] = json!(100);
    account["fleet"]["tactics"] = json!([{"heroInfo":[1]}]);

    let mut start = Vec::new();
    append_varint_field(&mut start, 2, 5011);
    let start_responses =
        battle_route_test_request(&mut account, &state, &catalog, "copy.StartBase", start).await;
    assert!(start_responses
        .iter()
        .any(|response| response.method == "copy.StartBase" && response.err == 0));

    let mut attack = Vec::new();
    append_varint_field(&mut attack, 1, 2);
    append_varint_field(&mut attack, 2, 5011);
    append_varint_field(&mut attack, 3, 1);
    append_varint_field(&mut attack, 4, 900);
    let attack_responses =
        battle_route_test_request(&mut account, &state, &catalog, "copy.AttackBase", attack).await;
    assert!(attack_responses
        .iter()
        .any(|response| response.method == "copy.AttackBase" && response.err == 0));

    let pass = battle_pass_args(5011, 2, 37, 1101, 1, 777);
    let pass_responses =
        battle_route_test_request(&mut account, &state, &catalog, "copy.PassBase", pass).await;
    assert!(pass_responses
        .iter()
        .any(|response| response.method == "copy.PassBase" && response.err == 0));
    assert_eq!(account["dock"]["heroes"][0]["curHp"], 777);
    assert!(account["battleSession"].is_null());
    assert_eq!(account["copyProgress"]["records"][0]["grade"], 2);
    assert_eq!(account["copyProgress"]["records"][0]["passTime"], 37);
}

#[tokio::test]
async fn battle_pass_route_settles_only_after_all_enemy_fleets() {
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        5012,
        BattleCopy {
            config_id: 5012,
            copy_type: 2,
            fleet_ids: vec![1101, 1102],
        },
    );
    catalog.fleet_enemies.insert(1101, vec![900]);
    catalog.fleet_enemies.insert(1102, vec![901]);
    catalog.enemies.insert(900, BattleEnemy::default());
    catalog.enemies.insert(901, BattleEnemy::default());
    catalog.supply_cost_by_copy.insert(5012, (0, 0));

    let state = ServerState::new("battle-pass-multi-fleet", "test", "1.4.0");
    let mut account = default_account_snapshot("battle-pass-multi-fleet", "test", 100);
    let mut start = Vec::new();
    append_varint_field(&mut start, 2, 5012);
    let start_responses =
        battle_route_test_request(&mut account, &state, &catalog, "copy.StartBase", start).await;
    assert!(start_responses
        .iter()
        .any(|response| response.method == "copy.StartBase" && response.err == 0));

    let mut first_pass = Vec::new();
    append_varint_field(&mut first_pass, 1, 1);
    append_varint_field(&mut first_pass, 8, 3);
    let mut first_fleet = Vec::new();
    append_varint_field(&mut first_fleet, 1, 1101);
    append_message_field(&mut first_pass, 20, &first_fleet);
    let first_responses =
        battle_route_test_request(&mut account, &state, &catalog, "copy.PassBase", first_pass)
            .await;
    assert!(first_responses
        .iter()
        .any(|response| response.method == "copy.PassBase" && response.err == 0));
    assert_eq!(account["battleSession"]["remainingFleetIds"], json!([1102]));
    assert!(account["copyProgress"]["records"]
        .as_array()
        .unwrap()
        .is_empty());

    let mut second_pass = Vec::new();
    append_varint_field(&mut second_pass, 1, 1);
    append_varint_field(&mut second_pass, 8, 3);
    let mut second_fleet = Vec::new();
    append_varint_field(&mut second_fleet, 1, 1102);
    append_message_field(&mut second_pass, 20, &second_fleet);
    let second_responses =
        battle_route_test_request(&mut account, &state, &catalog, "copy.PassBase", second_pass)
            .await;
    assert!(second_responses
        .iter()
        .any(|response| response.method == "copy.PassBase" && response.err == 0));
    assert!(account["battleSession"].is_null());
    assert_eq!(account["seaProgress"]["records"][0]["copyId"], json!(5012));
}

#[tokio::test]
async fn battle_pass_placeholder_advances_one_enemy_fleet_for_one_client_encounter() {
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        5013,
        BattleCopy {
            config_id: 5013,
            copy_type: 2,
            fleet_ids: vec![1101, 1102],
        },
    );
    catalog.fleet_enemies.insert(1101, vec![900]);
    catalog.fleet_enemies.insert(1102, vec![901]);
    catalog.enemies.insert(900, BattleEnemy::default());
    catalog.enemies.insert(901, BattleEnemy::default());
    catalog.supply_cost_by_copy.insert(5013, (0, 0));

    let state = ServerState::new("battle-pass-placeholder-multi", "test", "1.4.0");
    let mut account = default_account_snapshot("battle-pass-placeholder-multi", "test", 100);
    let mut start = Vec::new();
    append_varint_field(&mut start, 2, 5013);
    battle_route_test_request(&mut account, &state, &catalog, "copy.StartBase", start).await;

    let mut placeholder = Vec::new();
    append_varint_field(&mut placeholder, 1, 1);
    append_varint_field(&mut placeholder, 8, 3);
    let first_responses = battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "copy.PassBase",
        placeholder.clone(),
    )
    .await;
    assert!(first_responses
        .iter()
        .any(|response| response.method == "copy.PassBase" && response.err == 0));
    assert_eq!(account["battleSession"]["remainingFleetIds"], json!([1102]));
    assert!(account["seaProgress"]["records"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn bundled_weekly_sea_copy_repeats_after_one_placeholder_pass() {
    let catalog_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/config");
    let chapters = load_chapter_catalog(Some(&catalog_dir));
    assert!(chapters.sea.contains(&1601000));
    let catalog = load_battle_catalog(Some(&catalog_dir));
    assert_eq!(battle_session_fleet_ids(1601000, Some(&catalog)).len(), 3);

    let state = ServerState::new("bundled-weekly-sea-repeat", "test", "1.4.0");
    let mut account = default_account_snapshot("bundled-weekly-sea-repeat", "test", 100);
    account["character"]["supply"] = json!(100_000);

    for expected_count in [1, 2] {
        let mut start = Vec::new();
        append_varint_field(&mut start, 2, 1601000);
        let start_responses =
            battle_route_test_request(&mut account, &state, &catalog, "copy.StartBase", start)
                .await;
        assert!(start_responses
            .iter()
            .any(|response| response.method == "copy.StartBase" && response.err == 0));

        for encounter in 0..3 {
            let mut pass = Vec::new();
            append_varint_field(&mut pass, 1, 1);
            append_varint_field(&mut pass, 8, 3);
            let pass_responses =
                battle_route_test_request(&mut account, &state, &catalog, "copy.PassBase", pass)
                    .await;
            assert!(pass_responses
                .iter()
                .any(|response| response.method == "copy.PassBase" && response.err == 0));
            if encounter < 2 {
                assert!(!account["battleSession"].is_null());
            } else {
                assert!(account["battleSession"].is_null());
            }
        }
        assert_eq!(
            account["seaProgress"]["records"][0]["passCount"],
            expected_count
        );
    }
}

#[test]
fn bundled_1_1_keeps_first_non_final_fleet_id() {
    let catalog_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/config");
    let catalog = load_battle_catalog(Some(&catalog_dir));
    assert_eq!(
        battle_position_fleet_id(5011, 1101, 0, Some(&catalog)),
        1101
    );
    assert!(catalog.fleet_is_last.get(&1101) == Some(&false));
    assert!(catalog.fleet_is_last.get(&1102) == Some(&true));
    let account = json!({
        "character": {"uid": 9, "level": 60, "name": "A"},
        "dock": {"heroes": [{"heroId": 1, "templateId": 100, "level": 10}]}
    });
    let enemy_fleets = decode_repeated_message_field(
        &battle_start_payload(&account, 5011, &[1], Some(&catalog)),
        24,
    );
    assert_eq!(
        enemy_fleets
            .iter()
            .map(|fleet| decode_varint_field(fleet, 1))
            .collect::<Vec<_>>(),
        vec![1101, 1102]
    );
    assert_eq!(decode_repeated_message_field(&enemy_fleets[0], 3).len(), 3);
}

#[tokio::test]
async fn battle_pass_route_grade_nine_clears_session_without_progress_or_rewards() {
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        5011,
        BattleCopy {
            config_id: 5011,
            copy_type: 1,
            fleet_ids: vec![1101],
        },
    );
    catalog.fleet_enemies.insert(1101, vec![900]);
    catalog.enemies.insert(
        900,
        BattleEnemy {
            hp: 100,
            ..BattleEnemy::default()
        },
    );
    catalog.supply_cost_by_copy.insert(5011, (10, 0));
    let state = ServerState::new("battle-fail-route", "test", "1.4.0");
    let mut account = default_account_snapshot("battle-fail-route", "test", 100);
    account["character"]["supply"] = json!(100);
    account["fleet"]["tactics"] = json!([{"heroInfo":[1]}]);

    let mut start = Vec::new();
    append_varint_field(&mut start, 2, 5011);
    battle_route_test_request(&mut account, &state, &catalog, "copy.StartBase", start).await;
    let mut attack = Vec::new();
    append_varint_field(&mut attack, 1, 2);
    append_varint_field(&mut attack, 2, 5011);
    append_varint_field(&mut attack, 3, 1);
    append_varint_field(&mut attack, 4, 900);
    battle_route_test_request(&mut account, &state, &catalog, "copy.AttackBase", attack).await;

    let pass = battle_pass_args(5011, 9, 12, 1101, 1, 555);
    let pass_responses =
        battle_route_test_request(&mut account, &state, &catalog, "copy.PassBase", pass).await;
    assert!(pass_responses
        .iter()
        .any(|response| response.method == "copy.PassBase" && response.err == 0));
    assert!(account["battleSession"].is_null());
    assert!(account["copyProgress"]["records"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(account["dock"]["heroes"][0]["curHp"], 555);
}

#[tokio::test]
async fn battle_pass_accepts_client_placeholder_base_without_attack_or_fleet_fields() {
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        5011,
        BattleCopy {
            config_id: 5011,
            copy_type: 1,
            fleet_ids: vec![1101],
        },
    );
    catalog.fleet_enemies.insert(1101, vec![900]);
    catalog.enemies.insert(
        900,
        BattleEnemy {
            hp: 100,
            ..BattleEnemy::default()
        },
    );
    catalog.supply_cost_by_copy.insert(5011, (10, 0));
    let state = ServerState::new("battle-pass-placeholder", "test", "1.4.0");
    let mut account = default_account_snapshot("battle-pass-placeholder", "test", 100);
    account["character"]["supply"] = json!(100);
    account["fleet"]["tactics"] = json!([{"heroInfo":[1]}]);

    let mut start = Vec::new();
    append_varint_field(&mut start, 2, 5011);
    let start_responses =
        battle_route_test_request(&mut account, &state, &catalog, "copy.StartBase", start).await;
    assert!(start_responses
        .iter()
        .any(|response| response.method == "copy.StartBase" && response.err == 0));

    let mut pass = Vec::new();
    append_varint_field(&mut pass, 1, 1);
    append_varint_field(&mut pass, 8, 3);
    append_varint_field(&mut pass, 12, 12);
    let pass_responses =
        battle_route_test_request(&mut account, &state, &catalog, "copy.PassBase", pass).await;
    assert!(pass_responses
        .iter()
        .any(|response| response.method == "copy.PassBase" && response.err == 0));
    assert!(account["battleSession"].is_null());
    assert_eq!(account["copyProgress"]["records"][0]["copyId"], 5011);
}

#[tokio::test]
async fn tutorial_start_falls_back_when_field_13_is_fleet_reference() {
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        100,
        BattleCopy {
            config_id: 1000,
            copy_type: 1,
            fleet_ids: vec![100],
        },
    );
    catalog.fleet_enemies.insert(100, vec![101]);
    catalog.enemies.insert(
        101,
        BattleEnemy {
            hp: 100,
            ..BattleEnemy::default()
        },
    );

    let state = ServerState::new("tutorial-test", "test", "1.4.0");
    let mut account = default_account_snapshot("tutorial-test", "test", 100);

    let mut fleet_reference = Vec::new();
    append_varint_field(&mut fleet_reference, 1, 999);
    append_varint_field(&mut fleet_reference, 2, 0);
    let mut start = Vec::new();
    append_varint_field(&mut start, 2, 100);
    append_message_field(&mut start, 13, &fleet_reference);

    let responses =
        battle_route_test_request(&mut account, &state, &catalog, "copy.StartBase", start).await;

    assert!(responses
        .iter()
        .any(|response| response.method == "copy.StartBase" && response.err == 0));
    assert_eq!(account["battleSession"]["heroIds"], json!([1]));
}

#[tokio::test]
async fn start_base_collects_all_client_fleet_groups() {
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        100,
        BattleCopy {
            config_id: 1000,
            copy_type: 1,
            fleet_ids: vec![100],
        },
    );
    catalog.fleet_enemies.insert(100, vec![101]);
    catalog.enemies.insert(
        101,
        BattleEnemy {
            hp: 100,
            ..BattleEnemy::default()
        },
    );

    let state = ServerState::new("multi-fleet-test", "test", "1.4.0");
    let mut account = default_account_snapshot("multi-fleet-test", "test", 100);
    let mut second_hero = account["dock"]["heroes"][0].clone();
    second_hero["heroId"] = json!(2);
    account["dock"]["heroes"]
        .as_array_mut()
        .unwrap()
        .push(second_hero);
    let mut first = Vec::new();
    append_varint_field(&mut first, 1, 1);
    let mut second = Vec::new();
    append_varint_field(&mut second, 1, 2);
    let mut start = Vec::new();
    append_varint_field(&mut start, 2, 100);
    append_message_field(&mut start, 13, &first);
    append_message_field(&mut start, 13, &second);

    let responses =
        battle_route_test_request(&mut account, &state, &catalog, "copy.StartBase", start).await;

    assert_eq!(account["battleSession"]["heroIds"], json!([1, 2]));
    assert_eq!(account["battleSession"]["heroGroups"], json!([[1], [2]]));
    assert!(responses.iter().any(|response| {
        response.method == "copy.StartBase"
            && response
                .ret
                .as_ref()
                .is_some_and(|payload| payload.contains(&0x7A))
    }));
}

#[tokio::test]
async fn start_base_persists_daily_flags_and_ex_buffs() {
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        100,
        BattleCopy {
            config_id: 1000,
            copy_type: 9,
            fleet_ids: vec![100],
        },
    );
    catalog.fleet_enemies.insert(100, vec![101]);
    catalog.enemies.insert(101, BattleEnemy::default());

    let state = ServerState::new("daily-flags-test", "test", "1.4.0");
    let mut account = default_account_snapshot("daily-flags-test", "test", 100);
    let mut start = Vec::new();
    append_varint_field(&mut start, 2, 100);
    let mut fleet = Vec::new();
    append_varint_field(&mut fleet, 1, 1);
    append_message_field(&mut start, 13, &fleet);
    append_varint_field(&mut start, 12, 7001);
    append_varint_field(&mut start, 12, 7002);
    append_varint_field(&mut start, 17, 1);

    battle_route_test_request(&mut account, &state, &catalog, "copy.StartBase", start).await;

    assert_eq!(account["battleSession"]["exBuff"], json!([7001, 7002]));
    assert_eq!(account["battleSession"]["isPvePtMode"], json!(true));
}

#[tokio::test]
async fn copy_info_route_returns_daily_record_payload() {
    let state = ServerState::new("copy-info-test", "test", "1.4.0");
    let mut account = default_account_snapshot("copy-info-test", "test", 100);
    account["copyRecords"] = json!([{
        "copyId": 100,
        "passTime": 37,
        "recTime": 99,
        "strategyId": 17,
        "heroIds": [1],
        "exBuff": [7001, 7002]
    }]);
    let mut request = Vec::new();
    append_varint_field(&mut request, 1, 100);
    append_varint_field(&mut request, 2, 3);

    let responses = battle_route_test_request(
        &mut account,
        &state,
        &BattleCatalog::default(),
        "copyinfo.GetCopyInfo",
        request,
    )
    .await;
    let response = responses
        .iter()
        .find(|response| response.method == "copyinfo.GetCopyInfo")
        .expect("copy info response");

    assert!(response
        .ret
        .as_ref()
        .is_some_and(|payload| payload.len() > 2));
}

#[test]
fn stage_quantity_rules_vary_materials_but_preserve_explicit_counts_and_ships() {
    let rules: super::BattleDropQuantities =
        serde_json::from_str(include_str!("../../../catalog/battle-drop-quantities.json")).unwrap();
    let mut material = ShopReward {
        goods_type: 1,
        item_id: 10182,
        num: 1,
        instance_id: 0,
    };
    rules.apply(5011, &mut material, 123);
    assert!((2..=4).contains(&material.num));
    material.num = 1;
    rules.apply(5124, &mut material, 123);
    assert!((5..=7).contains(&material.num));
    material.num = 12;
    rules.apply(5124, &mut material, 123);
    assert_eq!(material.num, 12);
    material.goods_type = 3;
    material.num = 1;
    rules.apply(5124, &mut material, 123);
    assert_eq!(material.num, 1);
}

#[test]
fn equipment_dismantle_removes_instances_returns_configured_rewards_and_skips_no_resolve() {
    let mut account = json!({
        "character": {"gold": 0},
        "bag": {"items": []},
        "equip": {"items": [
            {"equipId": 11, "templateId": 30091, "enhanceLv": 0},
            {"equipId": 12, "templateId": 30091, "enhanceLv": 0},
            {"equipId": 13, "templateId": 39999, "enhanceLv": 0}
        ]}
    });
    let mut catalog = EquipCatalog::default();
    catalog
        .dismantle_rewards_by_template
        .insert(30091, vec![(1, 70000, 3)]);
    catalog
        .dismantle_rewards_by_template
        .insert(39999, vec![(1, 70000, 99)]);
    catalog.no_resolve_templates.insert(39999);
    let (rewards, removed) = dismantle_equip_state(&mut account, Some(&catalog), &[11, 11, 13]);
    assert_eq!(removed, vec![11]);
    assert_eq!(rewards.len(), 1);
    assert_eq!(rewards[0].goods_type, 1);
    assert_eq!(rewards[0].item_id, 70000);
    assert_eq!(rewards[0].num, 3);
    assert_eq!(bag_item_count(&account, 70000), 3);
    assert_eq!(account["equip"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(account["equip"]["items"][0]["equipId"], json!(12));
    assert_eq!(account["equip"]["items"][1]["equipId"], json!(13));
}

#[test]
fn ship_intensify_updates_progress_consumes_material_and_diamond() {
    let catalog_root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/config");
    let catalog = load_ship_intensify_catalog(Some(&catalog_root));
    assert_eq!(catalog.same_type_ratio, 15_000);
    assert_eq!(catalog.diamond_cost_per_hero, 5);
    let mut account = default_account_snapshot("intensify", "Intensify", 123);
    account["character"]["diamond"] = json!(100);
    account["dock"]["heroes"] = json!([
        {"heroId": 10, "templateId": 10210511, "level": 1, "lock": true},
        {"heroId": 20, "templateId": 10210511, "level": 1, "lock": false}
    ]);
    let removed = hero_intensify_state(&mut account, &catalog, 10, &[20], true).unwrap();
    assert_eq!(removed, vec![20]);
    assert_eq!(account["character"]["diamond"], json!(95));
    assert_eq!(account["dock"]["heroes"].as_array().unwrap().len(), 1);
    assert!(account["dock"]["heroes"][0]["intensify"]
        .as_array()
        .is_some_and(|attrs| attrs.iter().any(|attr| {
            json_i32(attr, "attrType") == Some(8)
                && json_i32(attr, "intensifyLvl").unwrap_or_default() > 0
        })));
}

#[test]
fn ship_advance_updates_template_and_consumes_currency() {
    let catalog_root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/config");
    let catalog = load_ship_break_catalog(Some(&catalog_root));
    let mut account = default_account_snapshot("advance", "Advance", 123);
    account["character"]["gold"] = json!(25_000);
    account["dock"]["heroes"] = json!([
        {"heroId": 10, "templateId": 40440111, "level": 80, "lock": true, "advance": 0}
    ]);
    let (target_id, removed) = hero_advance_state(&mut account, &catalog, 10, &[], &[]).unwrap();
    assert_eq!(target_id, 10);
    assert!(removed.is_empty());
    assert_eq!(account["dock"]["heroes"][0]["templateId"], json!(40440112));
    assert_eq!(account["dock"]["heroes"][0]["advance"], json!(1));
    assert_eq!(account["character"]["gold"], json!(5_000));
}

#[test]
fn change_equip_is_atomic_and_rejects_equipment_owned_by_another_hero() {
    let mut account = json!({
        "dock": {"heroes": [
            {"heroId": 10, "equipSlots": [11, 0, 0, 0, 0, 0]},
            {"heroId": 20, "equipSlots": [12, 0, 0, 0, 0, 0]}
        ]},
        "equip": {"items": [
            {"equipId": 11, "heroId": 10},
            {"equipId": 12, "heroId": 20},
            {"equipId": 13, "heroId": 0}
        ]}
    });
    assert_eq!(
        hero_change_equip_state(&mut account, 10, 1, 12),
        Err("equipment belongs to another hero")
    );
    assert_eq!(account["dock"]["heroes"][0]["equipSlots"][0], json!(11));
    assert_eq!(account["equip"]["items"][0]["heroId"], json!(10));

    hero_change_equip_state(&mut account, 10, 1, 13).unwrap();
    assert_eq!(account["dock"]["heroes"][0]["equipSlots"][0], json!(13));
    assert_eq!(account["equip"]["items"][0]["heroId"], json!(0));
    assert_eq!(account["equip"]["items"][2]["heroId"], json!(10));
}

#[test]
fn hero_max_level_breakthrough_uses_client_advance_config() {
    let mut catalog = ShipAdvanceCatalog::default();
    catalog
        .by_level
        .insert(1, json!({"initial_level": 80, "max_level": 85}));
    catalog
        .by_level
        .insert(2, json!({"initial_level": 85, "max_level": 90}));
    let mut account = json!({"dock": {"heroes": [
        {"heroId": 10, "level": 80, "advLv": 0}
    ]}});

    hero_advance_max_level_state(&mut account, &catalog, 10).unwrap();
    assert_eq!(account["dock"]["heroes"][0]["advLv"], json!(1));

    account["dock"]["heroes"][0]["level"] = json!(84);
    assert_eq!(
        hero_advance_max_level_state(&mut account, &catalog, 10),
        Err("hero level is too low for max-level breakthrough")
    );
    assert_eq!(account["dock"]["heroes"][0]["advLv"], json!(1));
}

#[test]
fn hero_mub_breakthrough_consumes_fragments_and_updates_template() {
    let mut catalog = ShipBreakCatalog::default();
    catalog.by_template.insert(
        101,
        json!({
            "min_level": 1,
            "break_to": "102",
            "break_item_mub": [60000, 2],
            "break_usableitem_mub": [],
            "currency_cost": [5, 1, 100]
        }),
    );
    let mut account = json!({
        "dock": {"heroes": [{"heroId": 10, "templateId": 101, "level": 1, "advance": 0}]},
        "bag": {"items": [{"templateId": 60000, "num": 2}]},
        "character": {"gold": 100}
    });
    hero_advance_mub_state(&mut account, &catalog, 10, &[(60000, 2)]).unwrap();
    assert_eq!(account["dock"]["heroes"][0]["templateId"], json!(102));
    assert_eq!(account["dock"]["heroes"][0]["advance"], json!(1));
    assert_eq!(account["bag"]["items"][0]["num"], json!(0));
    assert_eq!(account["character"]["gold"], json!(0));
}

#[test]
fn hero_remould_consumes_cost_updates_level_and_unlocks_skill() {
    let mut catalog = ShipRemouldCatalog::default();
    catalog
        .ship_info_by_sf_id
        .insert(10, json!({"remould_template": [100], "min_level": 1}));
    catalog
        .templates
        .insert(100, json!({"remould_item_group": [200]}));
    catalog.effects.insert(
        200,
        json!({
            "limit_level": 1,
            "limit_star": 0,
            "cost": [[1, 60000, 2]],
            "remould_prev": [],
            "remould_effect_type": [[4, 9001]]
        }),
    );
    let mut account = json!({
        "dock": {"heroes": [{"heroId": 10, "templateId": 101, "level": 1,
            "advance": 0, "remouldEffects": [], "remouldLevel": 0, "pSkills": []}]},
        "bag": {"items": [{"templateId": 60000, "num": 2}]},
        "character": {"gold": 0}
    });

    hero_remould_state(&mut account, &catalog, 10, 200).unwrap();
    assert_eq!(account["bag"]["items"][0]["num"], json!(0));
    assert_eq!(account["dock"]["heroes"][0]["remouldEffects"], json!([200]));
    assert_eq!(account["dock"]["heroes"][0]["remouldLevel"], json!(1));
    assert_eq!(
        account["dock"]["heroes"][0]["pSkills"][0]["pSkillId"],
        json!(9001)
    );
}

#[test]
fn fashion_equip_requires_owned_fashion_for_same_ship() {
    let catalog = FashionList {
        items: vec![FashionInfo {
            sf_id: 10,
            fashion_tids: vec![1001, 1002],
        }],
    };
    let mut account = json!({
        "dock": {"heroes": [{"heroId": 10, "templateId": 101, "fashioning": 10}]},
        "fashion": {"entries": [{"sfId": 10, "fashionTids": [1001]}]}
    });
    fashion_equip_state(&mut account, Some(&catalog), 10, 1001, 1).unwrap();
    assert_eq!(account["dock"]["heroes"][0]["fashioning"], json!(1001));
    assert_eq!(
        fashion_equip_state(&mut account, Some(&catalog), 10, 1002, 1),
        Err("fashion is not owned by hero")
    );
}

#[test]
fn talent_unlock_and_upgrade_follow_chain_and_consume_configured_costs() {
    let mut catalog = TalentCatalog {
        roots: vec![10],
        ..TalentCatalog::default()
    };
    catalog.nodes.insert(
        10,
        TalentNode {
            next_talent: 11,
            costs: vec![(1, 60000, 2)],
            ..TalentNode::default()
        },
    );
    catalog.nodes.insert(
        11,
        TalentNode {
            belong_talent: 10,
            costs: vec![(1, 60000, 3)],
            ..TalentNode::default()
        },
    );
    catalog.nodes.insert(
        12,
        TalentNode {
            belong_talent: 10,
            costs: vec![(1, 60000, 99)],
            ..TalentNode::default()
        },
    );
    let mut account = json!({
        "character": {},
        "bag": {"items": [{"templateId": 60000, "num": 5}]}
    });
    apply_talent_change(&mut account, &catalog, 10).unwrap();
    assert_eq!(bag_item_count(&account, 60000), 3);
    assert_eq!(
        apply_talent_change(&mut account, &catalog, 12),
        Err("talent is not the next level")
    );
    assert_eq!(bag_item_count(&account, 60000), 3);
    apply_talent_change(&mut account, &catalog, 11).unwrap();
    assert_eq!(bag_item_count(&account, 60000), 0);
}

#[test]
fn completed_sweep_payload_contains_pass_rewards() {
    let account = json!({
        "sweep": {"entries": [{"fleetId": 1, "copyId": 10001, "endTime": 100, "sweepCounts": 1}]}
    });
    let reward = ShopReward {
        goods_type: 1,
        item_id: 60000,
        num: 3,
        instance_id: 0,
    };
    let pass_rets = mop_up_pass_rets(10001, &[reward]);
    let payload = mop_up_payload_with_pass_rets(&account, 100, &pass_rets);
    // TMopUpRet.passRets is field 3 and must carry TPassBaseRet.reward field 1.
    assert!(payload.contains(&0x1A));
    assert!(payload.contains(&0x0A));
}

#[test]
fn completed_sweep_without_drops_still_emits_completion_dialog_record() {
    let pass_rets = mop_up_pass_rets(10001, &[]);
    assert_eq!(pass_rets.len(), 1);
    assert!(pass_rets[0].windows(2).any(|window| window == [0x60, 0x91]));
}

#[test]
fn default_account_starter_materials_are_seeded_once() {
    let account = default_account_snapshot("starter", "Starter", 123);
    assert_eq!(bag_item_count(&account, 60000), 1000);
    for textbook_id in [10182, 10185, 10187] {
        assert_eq!(bag_item_count(&account, textbook_id), 100);
    }
    for template_id in [10007, 10181, 12201, 10031] {
        assert_eq!(bag_item_count(&account, template_id), 100);
    }
    assert_eq!(bag_item_count(&account, 10029), 1000);
    assert_eq!(bag_item_count(&account, 10030), 1000);
}

#[test]
fn bag_info_persists_equip_enhancement_materials() {
    let mut account = default_account_snapshot("bag-filter", "Bag Filter", 123);
    account["bag"]["items"] = json!([
        {"templateId": 60000, "num": 1000},
        {"templateId": 60001, "num": 1000},
        {"templateId": 60002, "num": 1000},
        {"templateId": 10182, "num": 100}
    ]);
    let info = bag_info_from_account(&account);
    assert_eq!(
        info.items
            .iter()
            .map(|item| item.template_id)
            .collect::<Vec<_>>(),
        vec![60000, 60001, 60002, 10182, 10185, 10187, 10007, 10181, 12201, 10029, 10030, 10031,]
    );
}

#[test]
fn starter_build_items_allow_formula_build_and_quick_finish() {
    let mut account = default_account_snapshot("build-items", "Build Items", 123);
    account["character"]["gold"] = json!(30);
    assert!(bag_item_count(&account, 10029) >= 30);
    assert!(bag_item_count(&account, 10030) >= 30);
    assert!(bag_item_count(&account, 10031) >= 1);
    let project = json!({
        "items": [{"resId": 10029, "count": 30}, {"resId": 10030, "count": 30}],
        "gold": 30
    });
    assert!(start_construction(&mut account, &[project], 123).is_ok());
    assert!(super::finish_construction(&mut account, &[1], 123));
}

#[test]
fn expansion_items_increase_capacity_once_per_item() {
    let mut account = default_account_snapshot("expand", "Expand", 123);
    add_bag_item(&mut account, 140001, 2);
    add_bag_item(&mut account, 140002, 3);
    assert_eq!(account["dock"]["bagSize"], 220);
    assert_eq!(account["equip"]["equipBagSize"], 2030);
    assert_eq!(bag_item_count(&account, 140001), 2);
    assert_eq!(bag_item_count(&account, 140002), 3);
}

#[test]
fn reward_multiplier_is_bounded_and_rounds() {
    assert_eq!(scale_reward(100, 1.5), 150);
    assert_eq!(scale_reward(100, -1.0), 100);
    assert_eq!(scale_reward(100, f64::NAN), 100);
}

#[test]
fn server_config_accepts_runtime_multipliers() {
    let config = ServerConfig::from_args([
        "--drop-multiplier=2.5".to_owned(),
        "--ship-exp-multiplier=3".to_owned(),
        "--commander-exp-multiplier=0.5".to_owned(),
        "--ship-stat-multiplier=2".to_owned(),
    ])
    .unwrap();
    assert_eq!(config.drop_multiplier, 2.5);
    assert_eq!(config.ship_exp_multiplier, 3.0);
    assert_eq!(config.commander_exp_multiplier, 0.5);
    assert_eq!(config.ship_stat_multiplier, 2.0);
}

#[test]
fn ship_stats_apply_multiplier_to_base_and_level_growth() {
    let mut catalog = ShipStatCatalog::default();
    catalog.by_template.insert(
        10210511,
        ShipStat {
            fixed_money: 0,
            hp: 1_000,
            hp_levelup: 100,
            attack: 100,
            attack_levelup: 10,
            defense: 80,
            defense_levelup: 8,
            torpedo_attack: 70,
            torpedo_attack_levelup: 7,
            torpedo_defense: 60,
            torpedo_defense_levelup: 6,
            ship_bomb_attack: 50,
            ship_bomb_attack_levelup: 5,
            ship_torpedo_attack: 40,
            ship_torpedo_attack_levelup: 4,
            carry_plane_count: 2,
            hit: 90,
            dodge: 20,
            crit: 3,
            anti_crit: 2,
        },
    );

    let attrs = ship_attributes_for_template(10210511, 3, Some(&catalog), 2.0);
    assert_eq!(attrs.get(&1), Some(&2_400));
    assert_eq!(attrs.get(&8), Some(&240));
    assert_eq!(attrs.get(&9), Some(&192));
    assert_eq!(attrs.get(&10), Some(&168));
    assert_eq!(attrs.get(&11), Some(&144));
    assert_eq!(attrs.get(&5), Some(&2));
    assert_eq!(attrs.get(&14), Some(&120));
    assert_eq!(attrs.get(&15), Some(&96));
    assert_eq!(attrs.get(&19), Some(&180));
    assert_eq!(attrs.get(&20), Some(&40));
    assert_eq!(attrs.get(&17), Some(&6));
    assert_eq!(attrs.get(&18), Some(&4));
}

#[test]
fn ship_stats_keep_fallback_attributes_when_catalog_row_missing() {
    let attrs = ship_attributes_for_template(999, 1, None, 2.0);
    assert_eq!(attrs.get(&1), Some(&2_000));
    assert_eq!(attrs.get(&5), Some(&1));
    assert_eq!(attrs.get(&8), Some(&200));
    assert_eq!(attrs.get(&9), Some(&100));
    assert_eq!(attrs.get(&19), Some(&200));
    assert_eq!(attrs.get(&20), Some(&70));
}

#[test]
fn ship_stats_include_intensify_levels_before_multiplier() {
    let mut catalog = ShipStatCatalog::default();
    catalog.by_template.insert(
        10210511,
        ShipStat {
            hp: 1_000,
            attack: 100,
            carry_plane_count: 2,
            ..ShipStat::default()
        },
    );
    let hero = json!({
        "intensify": [
            {"attrType": 1, "intensifyLvl": 5},
            {"attrType": 8, "intensifyLvl": 10}
        ]
    });
    let attrs = ship_attributes_for_hero(Some(&hero), 10210511, 1, Some(&catalog), 2.0);
    assert_eq!(attrs.get(&1), Some(&2_010));
    assert_eq!(attrs.get(&8), Some(&220));
    assert_eq!(attrs.get(&5), Some(&2));
}

#[test]
fn construction_start_and_receive_adds_ship() {
    let mut account = default_account_snapshot("build", "Build", 123);
    account["character"]["gold"] = json!(30);
    add_bag_item(&mut account, 10029, 30);
    add_bag_item(&mut account, 10030, 30);
    let project =
        json!({"items":[{"resId":10029,"count":30},{"resId":10030,"count":30}],"gold":30});
    start_construction(&mut account, std::slice::from_ref(&project), 100).unwrap();
    assert_eq!(account["construction"]["jobs"].as_array().unwrap().len(), 1);
    account["construction"]["jobs"][0]["completed"] = json!(true);
    let (ret, count) = receive_construction(&mut account, &[1], 100);
    assert_eq!(count, 1);
    assert!(!ret.is_empty());
    assert_eq!(account["dock"]["heroes"].as_array().unwrap().len(), 2);
}

#[test]
fn illustration_bootstrap_emits_story_and_behaviour_lists() {
    let account = json!({
        "dock": {"heroes": [{"templateId": 10210511}]}
    });
    let payload = illustrate_info_payload(&account, Some(&[7001, 7002]), Some(&[(2064011, 1001)]));
    assert!(payload.contains(&0x42));
    let persisted = json!({
        "dock": {"heroes": [{"templateId": 10210511}]},
        "illustrate": {"entries": [{"illustrateId": 1021051, "behaviourList": [9001]}]}
    });
    let persisted_payload = illustrate_info_payload(&persisted, Some(&[7001, 7002]), None);
    let entry = decode_repeated_message_field(&persisted_payload, 1)
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(decode_repeated_varint_field(&entry, 5), vec![9001]);
    let memory = story_memory_payload(Some(&[(9001, 3)]));
    assert_eq!(memory, vec![0x0A, 0x05, 0x08, 0xA9, 0x46, 0x10, 0x03]);
}

#[test]
fn incremental_illustration_payload_deduplicates_ship_templates() {
    let payload =
        illustrate_info_payload_for_templates(&[10210511, 10210511, 10210521], Some(&[7001, 7002]));
    let entries = decode_repeated_message_field(&payload, 1);
    assert_eq!(entries.len(), 2);
    assert_eq!(decode_varint_field(&entries[0], 1), 1021051);
    assert_eq!(decode_varint_field(&entries[1], 1), 1021052);
    assert_eq!(
        decode_repeated_varint_field(&entries[0], 5),
        vec![7001, 7002]
    );
    assert_eq!(decode_varint_field(&entries[0], 6), 0);
}

#[tokio::test]
async fn illustrate_behaviour_route_persists_requested_entry() {
    let mut account = default_account_snapshot("illustrate-route", "Captain", 123);
    let mut entry = Vec::new();
    append_varint_field(&mut entry, 1, 1021051);
    append_varint_field(&mut entry, 2, 7001);
    let mut args = Vec::new();
    append_message_field(&mut args, 1, &entry);
    let request = TMessageCodec::encode_request(&TRequest {
        method: "illustrate.AddBehaviour".to_owned(),
        args: Some(args),
        ..TRequest::default()
    });
    let (mut client, mut server) = duplex(16_384);
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();
    process_game_login_frame_with_catalog_mut(
        &mut server,
        &ServerState::new("illustrate-route", "Captain", "1.4.0"),
        Some(&mut account),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let push = TMessageCodec::decode_response(
        &NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap()
            .payload,
    )
    .unwrap();
    assert_eq!(push.method, "illustrate.IllustrateInfo");
    assert_eq!(account["illustrate"]["entries"][0]["illustrateId"], 1021051);
    assert_eq!(
        account["illustrate"]["entries"][0]["behaviourList"],
        json!([7001])
    );
}

#[tokio::test]
async fn illustrate_vow_list_route_persists_selected_heroes() {
    let mut account = default_account_snapshot("illustrate-vow-list", "Captain", 123);
    let mut args = Vec::new();
    append_varint_field(&mut args, 1, 1);
    let request = TMessageCodec::encode_request(&TRequest {
        method: "illustrate.ModiVowHeroList".to_owned(),
        args: Some(args),
        ..TRequest::default()
    });
    let (mut client, mut server) = duplex(16_384);
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();
    process_game_login_frame_with_catalog_mut(
        &mut server,
        &ServerState::new("illustrate-vow-list", "Captain", "1.4.0"),
        Some(&mut account),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let push = TMessageCodec::decode_response(
        &NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap()
            .payload,
    )
    .unwrap();
    assert_eq!(push.method, "illustrate.IllustrateInfo");
    assert_eq!(account["illustrate"]["vowHeroIds"], json!([1]));
}

#[test]
fn shop_goods_grant_currency_and_encode_reward() {
    let mut account = default_account_snapshot("alice", "Alice", 123);
    let good = ShopGood {
        shop_id: 1,
        goods_type: 5,
        item_id: 1,
        num: 25,
        costs: vec![],
    };
    let reward = apply_shop_good(&mut account, &good, 2, 123, None).unwrap();
    assert_eq!(reward.num, 50);
    assert_eq!(account["character"]["gold"], 50);
    let payload = return_shop_buy_response(7, 2, Some(reward));
    assert!(payload.windows(2).any(|window| window == [0x10, 0x07]));
    assert!(payload.windows(2).any(|window| window == [0x18, 0x02]));
}

#[test]
fn shop_purchase_deducts_configured_currency_cost_atomically() {
    let mut account = default_account_snapshot("alice", "Alice", 123);
    account["character"]["gold"] = json!(10_000);
    let starting_gold = account["character"]["gold"].as_i64().unwrap();
    let good = ShopGood {
        shop_id: 1,
        goods_type: 1,
        item_id: 10_000,
        num: 1,
        costs: vec![ShopCost {
            goods_type: 5,
            item_id: 1,
            amount: 5_000,
        }],
    };

    let reward = apply_shop_good(&mut account, &good, 2, 123, None).unwrap();

    assert_eq!(reward.num, 2);
    assert_eq!(account["character"]["gold"], starting_gold - 10_000);
    assert_eq!(bag_item_count(&account, 10_000), 2);

    let mut poor = default_account_snapshot("poor", "Poor", 123);
    poor["character"]["gold"] = json!(1);
    assert!(apply_shop_good(&mut poor, &good, 1, 123, None).is_none());
    assert_eq!(poor["character"]["gold"], 1);
    assert_eq!(bag_item_count(&poor, 10_000), 0);
}

#[test]
fn shop_cost_parser_reads_multiple_currency_price_pairs() {
    let costs = shop_costs_from_value(&json!({
        "currency": [[5, 25], [1, 13000]],
        "price": [[680], [5]],
    }));
    assert_eq!(costs.len(), 2);
    assert_eq!(costs[0].goods_type, 5);
    assert_eq!(costs[0].item_id, 25);
    assert_eq!(costs[0].amount, 680);
    assert_eq!(costs[1].goods_type, 1);
    assert_eq!(costs[1].item_id, 13000);
    assert_eq!(costs[1].amount, 5);
}

#[test]
fn server_catalog_loads_equipment_shop_costs() {
    let catalog_root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/config");
    let catalog = load_shop_catalog(Some(&catalog_root));
    let costs = catalog.costs_by_good_id.get(&20_000).unwrap();
    assert_eq!(costs.len(), 1);
    assert_eq!(costs[0].goods_type, 5);
    assert_eq!(costs[0].item_id, 8);
    assert_eq!(costs[0].amount, 200);
    let equip = load_equip_catalog(Some(&catalog_root));
    assert_eq!(
        equip.dismantle_rewards_by_template.get(&40253),
        Some(&vec![(5, 8, 40)])
    );
}

#[test]
fn shop_info_uses_server_goods_ids_over_client_shelves() {
    let mut catalog = ShopCatalog::default();
    catalog.goods_by_shop.insert(5, vec![20_000]);
    catalog.goods_by_id.insert(
        20_000,
        ShopGood {
            shop_id: 5,
            goods_type: 2,
            item_id: 30_582,
            num: 1,
            costs: vec![],
        },
    );

    let payload = shop_info_payload(Some(&catalog));
    assert!(payload
        .windows(4)
        .any(|window| window == [0x08, 0xA0, 0x9C, 0x01]));
}

#[test]
fn invalid_server_shop_inventory_does_not_retain_client_shelves() {
    let mut catalog = ShopCatalog::default();
    catalog.goods_by_shop.insert(5, vec![805]);
    catalog.goods_by_id.insert(
        805,
        ShopGood {
            shop_id: 5,
            goods_type: 2,
            item_id: 30_582,
            num: 1,
            costs: vec![],
        },
    );

    load_server_shop_goods(&mut catalog, std::path::Path::new("missing-data-root"));

    assert!(catalog.goods_by_shop.is_empty());
    assert!(catalog.goods_by_id.is_empty());
}

#[test]
fn shop_equipment_quantity_creates_distinct_instances() {
    let mut account = default_account_snapshot("alice", "Alice", 123);
    let before = account["equip"]["items"].as_array().unwrap().len();
    let good = ShopGood {
        shop_id: 1,
        goods_type: 2,
        item_id: 9001,
        num: 2,
        costs: vec![],
    };
    let reward = apply_shop_good(&mut account, &good, 2, 123, None).unwrap();
    let items = account["equip"]["items"].as_array().unwrap();
    assert_eq!(reward.num, 4);
    assert_eq!(items.len(), before + 4);
    assert_eq!(
        items
            .iter()
            .filter(|item| json_i32(item, "templateId") == Some(9001))
            .count(),
        4
    );
}

#[test]
fn shop_ship_quantity_returns_last_instance_and_single_reward_unit() {
    let mut account = default_account_snapshot("alice", "Alice", 123);
    let good = ShopGood {
        shop_id: 1,
        goods_type: 3,
        item_id: 10210511,
        num: 2,
        costs: vec![],
    };
    let reward = apply_shop_good(&mut account, &good, 2, 123, None).unwrap();
    let heroes = account["dock"]["heroes"].as_array().unwrap();
    assert_eq!(reward.num, 1);
    assert_eq!(heroes.len(), 5);
    assert_eq!(reward.instance_id, 5);
}

#[test]
fn mail_claim_updates_currency_and_bag_and_encodes_repeatable_mail() {
    let mut account = default_account_snapshot("alice", "Alice", 123);
    let mail = MailTemplate {
        mid: 42,
        goods_type: 5,
        config_id: 1,
        num: 100,
        subject: "Gold".to_owned(),
        content: "Claim".to_owned(),
    };
    apply_mail_reward(&mut account, &mail);
    assert_eq!(account["character"]["gold"], 100);
    let item_mail = MailTemplate {
        mid: 43,
        goods_type: 1,
        config_id: 10000,
        num: 2,
        subject: "Item".to_owned(),
        content: "Claim".to_owned(),
    };
    apply_mail_reward(&mut account, &item_mail);
    assert_eq!(bag_item_count(&account, 10000), 2);
    let payload = encode_mail_list_response(&[mail, item_mail], 123, &[]);
    assert!(payload.windows(2).any(|window| window == [0x08, 0x02]));
    assert!(payload.windows(2).any(|window| window == [0x10, 0x00]));
    assert!(payload.windows(2).any(|window| window == [0x58, 0x00]));
}

#[test]
fn hero_exp_request_decodes_nested_items_and_consumption_is_bounded() {
    let mut item = Vec::new();
    append_varint_field(&mut item, 2, 150001);
    append_varint_field(&mut item, 3, 3);
    let mut payload = Vec::new();
    append_varint_field(&mut payload, 1, 1);
    append_message_field(&mut payload, 2, &item);
    let (hero_id, items) = decode_hero_add_exp_request(&payload);
    assert_eq!(hero_id, 1);
    assert_eq!(items, vec![(150001, 3)]);

    let mut account = json!({"bag": {"items": [{"templateId": 150001, "num": 2}]}});
    assert_eq!(consume_bag_item(&mut account, 150001, 3), 2);
    assert_eq!(account["bag"]["items"][0]["num"], json!(0));

    let catalog = HeroLevelCatalog {
        exp_per_item: [(150001, 100)].into_iter().collect(),
        exp_needed: [(1, 200)].into_iter().collect(),
    };
    assert_eq!(catalog.exp_per_item[&150001], 100);
    assert_eq!(
        encode_hero_add_exp_response(1, &items),
        vec![0x08, 0x01, 0x12, 0x06, 0x10, 0xF1, 0x93, 0x09, 0x18, 0x03]
    );
}

#[tokio::test]
async fn hero_add_exp_route_persists_level_and_emits_refreshes() {
    let mut account = default_account_snapshot("alice", "Alice", 123);
    account["bag"]["items"] = json!([{"templateId": 150001, "num": 3}]);
    let catalog = HeroLevelCatalog {
        exp_per_item: [(150001, 100)].into_iter().collect(),
        exp_needed: [(1, 200)].into_iter().collect(),
    };
    let mut item = Vec::new();
    append_varint_field(&mut item, 2, 150001);
    append_varint_field(&mut item, 3, 2);
    let mut args = Vec::new();
    append_varint_field(&mut args, 1, 1);
    append_message_field(&mut args, 2, &item);
    let request = TMessageCodec::encode_request(&TRequest {
        method: "hero.AddExp".to_owned(),
        args: Some(args),
        callback_handler: 7,
        ..TRequest::default()
    });
    let (mut client, mut server) = duplex(16384);
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();
    let state = ServerState::new("alice", "Alice", "1.4.0");
    assert!(process_game_login_frame_with_catalog_mut(
        &mut server,
        &state,
        Some(&mut account),
        None,
        None,
        Some(&catalog),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap());
    let hero_push = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        TMessageCodec::decode_response(&hero_push.payload)
            .unwrap()
            .method,
        "hero.UpdateHeroBagData"
    );
    let bag_push = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        TMessageCodec::decode_response(&bag_push.payload)
            .unwrap()
            .method,
        "bag.UpdateBagData"
    );
    let response = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    let response = TMessageCodec::decode_response(&response.payload).unwrap();
    assert_eq!(response.method, "hero.AddExp");
    assert_eq!(response.callback_handler, 7);
    assert_eq!(account["dock"]["heroes"][0]["level"], 2);
    assert_eq!(account["dock"]["heroes"][0]["exp"], 0);
    assert_eq!(account["bag"]["items"][0]["num"], 1);
}

#[tokio::test]
async fn hero_marry_route_consumes_oath_ring_and_updates_hero() {
    let mut account = default_account_snapshot("marry-route", "Captain", 123);
    account["bag"]["items"] = json!([{"templateId": 10180, "num": 1}]);
    let mut args = Vec::new();
    append_varint_field(&mut args, 1, 1);
    append_varint_field(&mut args, 2, 1);
    let request = TMessageCodec::encode_request(&TRequest {
        method: "hero.Marry".to_owned(),
        args: Some(args),
        ..TRequest::default()
    });
    let (mut client, mut server) = duplex(16_384);
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();
    let state = ServerState::new("marry-route", "Captain", "1.4.0");
    process_game_login_frame_with_catalog_mut(
        &mut server,
        &state,
        Some(&mut account),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_ne!(account["dock"]["heroes"][0]["marryTime"], json!(0));
    assert_eq!(account["dock"]["heroes"][0]["marryType"], json!(1));
    assert_eq!(account["bag"]["items"][0]["num"], json!(0));
}

#[tokio::test]
async fn repair_route_restores_damaged_heroes() {
    let mut account = default_account_snapshot("repair-route", "Captain", 123);
    account["dock"]["heroes"][0]["curHp"] = json!(1);
    let mut args = Vec::new();
    append_varint_field(&mut args, 1, 1);
    let request = TMessageCodec::encode_request(&TRequest {
        method: "repair.RepairHero".to_owned(),
        args: Some(args),
        ..TRequest::default()
    });
    let (mut client, mut server) = duplex(16_384);
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();
    let state = ServerState::new("repair-route", "Captain", "1.4.0");
    process_game_login_frame_with_catalog_mut(
        &mut server,
        &state,
        Some(&mut account),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        account["dock"]["heroes"][0]["curHp"],
        json!(10_000_000_000i64)
    );
}

#[tokio::test]
async fn study_speedup_route_refreshes_hero_study_and_bag() {
    let mut account = default_account_snapshot("study-route", "Study Route", 123);
    account["dock"]["heroes"][0]["pSkills"] = json!([{"pSkillId": 41, "level": 1}]);
    add_bag_item(&mut account, 70000, 2);
    let mut start_args = Vec::new();
    append_varint_field(&mut start_args, 1, 1);
    append_varint_field(&mut start_args, 2, 41);
    append_varint_field(&mut start_args, 3, 70000);
    let start_request = TMessageCodec::encode_request(&TRequest {
        method: "study.StartStudyPSkill".to_owned(),
        args: Some(start_args),
        callback_handler: 1,
        ..TRequest::default()
    });
    let (mut client, mut server) = duplex(16_384);
    NetSocketFrameCodec::write(&mut client, 0, &start_request)
        .await
        .unwrap();
    let state = ServerState::new("study-route", "Study Route", "1.4.0");
    assert!(process_game_login_frame_with_catalog_mut(
        &mut server,
        &state,
        Some(&mut account),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap());
    for _ in 0..3 {
        NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap();
    }

    let mut item = Vec::new();
    append_varint_field(&mut item, 1, 70000);
    append_varint_field(&mut item, 2, 1);
    let mut speedup_args = Vec::new();
    append_varint_field(&mut speedup_args, 1, 1);
    append_varint_field(&mut speedup_args, 2, 41);
    append_message_field(&mut speedup_args, 3, &item);
    let speedup_request = TMessageCodec::encode_request(&TRequest {
        method: "study.SpeedUpStudy".to_owned(),
        args: Some(speedup_args),
        callback_handler: 2,
        ..TRequest::default()
    });
    NetSocketFrameCodec::write(&mut client, 0, &speedup_request)
        .await
        .unwrap();
    assert!(process_game_login_frame_with_catalog_mut(
        &mut server,
        &state,
        Some(&mut account),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap());
    for method in [
        "hero.UpdateHeroBagData",
        "study.GetStudyInfo",
        "bag.UpdateBagData",
    ] {
        let frame = NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap();
        let push = TMessageCodec::decode_response(&frame.payload).unwrap();
        assert_eq!(push.method, method);
        assert_eq!(push.is_response, 0);
    }
    let response = TMessageCodec::decode_response(
        &NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap()
            .payload,
    )
    .unwrap();
    assert_eq!(response.method, "study.SpeedUpStudy");
    assert_eq!(response.err, 0);
    assert_eq!(account["dock"]["heroes"][0]["pSkills"][0]["level"], 2);
    assert_eq!(bag_item_count(&account, 70000), 0);
}

#[test]
fn fashion_catalog_merge_preserves_account_entries() {
    let account = json!({
        "fashion": {
            "entries": [{"sfId": 10, "fashionTids": [12, 11]}]
        }
    });
    let merged = fashion_list_from_account(
        &account,
        Some(&FashionList {
            items: vec![FashionInfo {
                sf_id: 10,
                fashion_tids: vec![11, 13],
            }],
        }),
    );
    assert_eq!(merged.items[0].sf_id, 10);
    assert_eq!(merged.items[0].fashion_tids, vec![11, 12, 13]);
}

#[test]
fn equip_catalog_generates_star_skills_when_account_omits_them() {
    let account = json!({
        "equip": {
            "equipBagSize": 2000,
            "items": [{"equipId": 1, "templateId": 30091, "star": 2}]
        }
    });
    let mut skills_by_template = std::collections::BTreeMap::new();
    skills_by_template.insert(30091, vec![(50001, 1)]);
    let list = equip_list_from_account(
        &account,
        Some(&EquipCatalog {
            skills_by_template,
            ..EquipCatalog::default()
        }),
    );
    assert_eq!(list.items[0].pskills[0].pskill_id, 50001);
    assert_eq!(list.items[0].pskills[0].level, 1);
}

#[test]
fn equip_list_exposes_enhancement_material_counts() {
    let account = json!({
        "equip": {"equipBagSize": 2000, "items": []},
        "bag": {"items": [
            {"templateId": 60000, "num": 7},
            {"templateId": 99999, "num": 4},
            {"templateId": 60001, "num": 0}
        ]}
    });
    let mut enhance_materials = std::collections::BTreeMap::new();
    enhance_materials.insert(60000, (100, None));
    let list = equip_list_from_account(
        &account,
        Some(&EquipCatalog {
            enhance_materials,
            ..EquipCatalog::default()
        }),
    );
    assert_eq!(
        list.nums,
        vec![EquipNum {
            template_id: 60000,
            num: 7
        }]
    );
}

#[test]
fn missing_building_and_fleet_use_csharp_defaults() {
    let account = json!({});
    let building = building_info_from_account(&account, 1234);
    assert_eq!(building.buildings.len(), 2);
    assert_eq!(building.buildings[0].last_build_update_time, 1234);
    assert_eq!(building.worker_strength, 1_500_000);
    assert_eq!(building.lands[1].building_id, 2);

    let fleet = fleet_info_from_account(&account);
    assert_eq!(fleet.tactics.len(), 5);
    assert_eq!(fleet.tactics[0].mode_id, 1);
    assert_eq!(fleet.tactics[0].formation_id, 2);
    assert_eq!(fleet.tactics[0].tactic_type, 1);
}

#[test]
fn task_normalization_clears_expired_daily_and_weekly_records() {
    let now = 1_700_000_000u32;
    let day = (i64::from(now) + 8 * 60 * 60) / 86_400;
    let mut account = json!({
        "tasks": {
            "dailyResetDay": day - 1,
            "weeklyResetWeek": 1,
            "records": [
                {"taskType": 1, "taskId": 10, "count": 1},
                {"taskType": 2, "taskId": 20, "count": 2},
                {"taskType": 3, "taskId": 30, "count": 3},
                {"taskType": 8, "taskId": 80, "count": 4}
            ]
        }
    });
    assert!(normalize_task_state(&mut account, now));
    let records = account["tasks"]["records"].as_array().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["taskType"], 1);
    assert_eq!(account["tasks"]["dailyResetDay"], day);
    assert_eq!(account["tasks"]["weeklyResetWeek"], (day + 3) / 7);
}

#[test]
fn task_completion_persists_completed_flag() {
    let mut account = json!({"tasks": {"records": []}});
    complete_task(&mut account, 1, 42, 3, 123);
    let record = &account["tasks"]["records"][0];
    assert_eq!(record["count"], 3);
    assert_eq!(record["completed"], 1);
    assert_eq!(record["rewardTime"], 123);
    assert!(task_completed(&account, 1, 42, 3));
}

#[test]
fn achievement_points_recompute_from_claimed_achievements() {
    let mut account = json!({
        "character": {"achievePoint": 0},
        "tasks": {"records": [
            {"taskType": 5, "taskId": 100, "rewardTime": 20},
            {"taskType": 5, "taskId": 101, "rewardTime": 0},
            {"taskType": 1, "taskId": 102, "rewardTime": 30}
        ]}
    });
    let catalog = TaskCatalog {
        definitions: vec![
            TaskDefinition {
                task_type: 5,
                id: 100,
                point: 7,
                ..TaskDefinition::default()
            },
            TaskDefinition {
                task_type: 5,
                id: 101,
                point: 11,
                ..TaskDefinition::default()
            },
            TaskDefinition {
                task_type: 1,
                id: 102,
                point: 99,
                ..TaskDefinition::default()
            },
        ],
        rewards_by_id: Default::default(),
        teaching_rewards_by_id: Default::default(),
    };

    assert!(sync_achievement_points(&mut account, &catalog));
    assert_eq!(account["character"]["achievePoint"], 7);
    assert!(!sync_achievement_points(&mut account, &catalog));
}

#[test]
fn trusted_task_event_advances_progress_and_completes() {
    let mut account = json!({"character": {"level": 80}, "tasks": {"records": []}});
    let catalog = TaskCatalog {
        definitions: vec![TaskDefinition {
            task_type: 2,
            id: 20,
            event_type: 401,
            goal: 2,
            ..TaskDefinition::default()
        }],
        rewards_by_id: Default::default(),
        teaching_rewards_by_id: Default::default(),
    };
    assert!(advance_task_event(
        &mut account,
        Some(&catalog),
        401,
        1,
        100
    ));
    assert_eq!(account["tasks"]["records"][0]["count"], 1);
    assert!(!task_completed(&account, 2, 20, 2));
    assert!(advance_task_event(
        &mut account,
        Some(&catalog),
        401,
        1,
        101
    ));
    assert_eq!(account["tasks"]["records"][0]["completed"], 1);
    assert!(task_completed(&account, 2, 20, 2));
}

#[test]
fn targeted_task_event_only_advances_matching_parameter() {
    let mut account = json!({"character": {"level": 80}, "tasks": {"records": []}});
    let catalog = TaskCatalog {
        definitions: vec![
            TaskDefinition {
                task_type: 5,
                id: 100,
                event_type: 24,
                event_param: Some(15022),
                goal: 1,
                ..TaskDefinition::default()
            },
            TaskDefinition {
                task_type: 5,
                id: 101,
                event_type: 24,
                event_param: Some(15023),
                goal: 1,
                ..TaskDefinition::default()
            },
        ],
        rewards_by_id: Default::default(),
        teaching_rewards_by_id: Default::default(),
    };

    assert!(advance_task_event_with_param(
        &mut account,
        Some(&catalog),
        24,
        15022,
        1,
        100,
    ));
    assert_eq!(account["tasks"]["records"].as_array().unwrap().len(), 1);
    assert_eq!(account["tasks"]["records"][0]["taskId"], 100);
}

#[test]
fn client_task_catalog_preserves_targeted_clear_goals() {
    let config_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/config");
    let catalog = load_task_catalog(Some(&config_dir));

    let main_clear = catalog
        .definitions
        .iter()
        .find(|definition| definition.task_type == 1 && definition.id == 60)
        .unwrap();
    assert_eq!(main_clear.event_type, 17);
    assert_eq!(main_clear.event_param, Some(5064));
    assert_eq!(main_clear.goal, 1);

    let achievement_clear = catalog
        .definitions
        .iter()
        .find(|definition| definition.task_type == 5 && definition.event_type == 24)
        .unwrap();
    assert!(achievement_clear.event_param.is_some());
    assert_eq!(achievement_clear.goal, 1);
}

#[test]
fn affection_catalog_loads_client_gift_values() {
    let config_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/config");
    let catalog = load_affection_catalog(Some(&config_dir));
    assert_eq!(catalog.exp_by_item.get(&280001), Some(&10_000));
}

#[test]
fn buildship_payload_preserves_used_reward_buckets() {
    let account = json!({
        "buildState": {
            "usedBoxInfo": {"106": [20, 40]},
            "usedRewardInfo": {"106": [100]}
        }
    });
    let payload = buildship_info_payload(Some(&account), 1_700_000_000);
    assert!(payload.windows(2).any(|w| w == [0x32, 0x06]));
    assert!(payload.windows(2).any(|w| w == [0x3A, 0x04]));
}

#[test]
fn battle_codecs_emit_client_start_attack_and_pass_shapes() {
    let account = default_account_snapshot("alice", "Alice", 123);
    let start = battle_start_payload(&account, 10001, &[1], None);
    assert!(start.windows(2).any(|w| w == [0x30, 0x91]));
    assert!(start.contains(&0xC2));
    // TStartBaseRet.CopyMission is consumed by the client's MissionNode setup,
    // including non-story copies such as the weekly stages.
    assert_eq!(
        decode_repeated_varint_field(&start, 23),
        vec![101, 102, 103]
    );

    let mut attack_args = Vec::new();
    super::append_varint_field(&mut attack_args, 1, 2);
    super::append_varint_field(&mut attack_args, 2, 10001);
    super::append_varint_field(&mut attack_args, 3, 1);
    let attack = battle_attack_payload_with_damage(&attack_args, 1_000_000_000);
    assert!(attack.windows(2).any(|w| w == [0x28, 0x80]));

    let pass = battle_pass_payload_with_rewards(10001, true, 3, 60, &[]);
    assert!(pass.windows(2).any(|w| w == [0x60, 0x91]));
    assert!(pass.windows(2).any(|w| w == [0x20, 0x03]));
}

#[test]
fn tutorial_battle_enemy_hp_is_playable_for_new_account() {
    assert_eq!(super::battle_enemy_hp_for_client(100, 50_000), 1_000);
    assert_eq!(super::battle_enemy_hp_for_client(10001, 50_000), 50_000);
}

#[test]
fn battle_attack_validation_binds_session_targets() {
    let session_value = json!({
        "copyId": 10001,
        "heroIds": [1, 2],
        "remainingEnemyIds": [900]
    });
    let session = session_value.as_object().expect("session object");
    let mut args = Vec::new();
    append_varint_field(&mut args, 1, 2);
    append_varint_field(&mut args, 2, 10001);
    append_varint_field(&mut args, 3, 2);
    append_varint_field(&mut args, 4, 900);
    assert_eq!(validate_battle_attack(session, &args), Some(900));

    let mut wrong_enemy = Vec::new();
    append_varint_field(&mut wrong_enemy, 1, 2);
    append_varint_field(&mut wrong_enemy, 2, 10001);
    append_varint_field(&mut wrong_enemy, 3, 2);
    append_varint_field(&mut wrong_enemy, 4, 901);
    assert_eq!(validate_battle_attack(session, &wrong_enemy), None);

    let mut wrong_hero = Vec::new();
    append_varint_field(&mut wrong_hero, 1, 2);
    append_varint_field(&mut wrong_hero, 2, 10001);
    append_varint_field(&mut wrong_hero, 3, 99);
    append_varint_field(&mut wrong_hero, 4, 900);
    assert_eq!(validate_battle_attack(session, &wrong_hero), None);
}

#[test]
fn battle_pass_keeps_session_until_last_enemy_fleet() {
    let mut session = json!({"remainingFleetIds": [1202, 1203]});
    let mut first_pass = Vec::new();
    let mut first_fleet = Vec::new();
    append_varint_field(&mut first_fleet, 1, 1202);
    append_message_field(&mut first_pass, 17, &first_fleet);
    assert!(!mark_battle_fleet_passed(&mut session, &first_pass));
    assert_eq!(session["remainingFleetIds"], json!([1203]));

    let mut second_pass = Vec::new();
    let mut second_fleet = Vec::new();
    append_varint_field(&mut second_fleet, 1, 1203);
    append_message_field(&mut second_pass, 17, &second_fleet);
    assert!(mark_battle_fleet_passed(&mut session, &second_pass));
    assert_eq!(session["remainingFleetIds"], json!([]));
}

#[test]
fn battle_pass_consumes_wire_enemy_fleets_for_multi_fleet_sorties() {
    let mut session = json!({"remainingFleetIds": [202431, 202432]});
    let mut first_pass = Vec::new();
    let mut first_fleet = Vec::new();
    append_varint_field(&mut first_fleet, 1, 202431);
    append_message_field(&mut first_pass, 20, &first_fleet);
    assert!(super::validate_battle_fleet_pass(&session, &first_pass));
    assert!(!mark_battle_fleet_passed(&mut session, &first_pass));
    assert_eq!(session["remainingFleetIds"], json!([202432]));

    let mut second_pass = Vec::new();
    let mut second_fleet = Vec::new();
    append_varint_field(&mut second_fleet, 1, 202432);
    append_message_field(&mut second_pass, 20, &second_fleet);
    assert!(super::validate_battle_fleet_pass(&session, &second_pass));
    assert!(mark_battle_fleet_passed(&mut session, &second_pass));
    assert_eq!(session["remainingFleetIds"], json!([]));
}

#[test]
fn challenge_sea_start_keeps_configured_fleet_id_for_position_and_heading() {
    let mut catalog = BattleCatalog::default();
    catalog.search_3d.insert(202431);
    catalog.copies.insert(
        202431,
        BattleCopy {
            config_id: 202431,
            copy_type: 2,
            fleet_ids: vec![202431],
        },
    );

    assert_eq!(
        super::battle_position_fleet_id(202431, 202431, 0, Some(&catalog)),
        202431
    );
}

#[test]
fn battle_pass_rejects_fleet_not_in_active_session() {
    let mut session = json!({"remainingFleetIds": [1202]});
    let mut payload = Vec::new();
    let mut fleet = Vec::new();
    append_varint_field(&mut fleet, 1, 9999);
    append_message_field(&mut payload, 17, &fleet);

    assert!(!super::validate_battle_fleet_pass(&session, &payload));
    assert!(!mark_battle_fleet_passed(&mut session, &payload));
    assert_eq!(session["remainingFleetIds"], json!([1202]));
}

#[test]
fn battle_enemy_instances_follow_catalog() {
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        10001,
        BattleCopy {
            fleet_ids: vec![42],
            ..BattleCopy::default()
        },
    );
    catalog.fleet_enemies.insert(42, vec![17, 17, 17]);
    catalog.enemies.insert(
        17,
        BattleEnemy {
            hp: 1000,
            ..BattleEnemy::default()
        },
    );
    assert_eq!(battle_enemy_ids(10001, Some(&catalog)), vec![17, 17, 17]);
}

#[test]
fn battle_start_uses_copy_fleet_enemy_catalog_and_requested_heroes() {
    let account = json!({
        "character": {"uid": 9, "level": 20, "name": "A"},
        "dock": {"heroes": [
            {"heroId": 1, "templateId": 100, "level": 10},
            {"heroId": 2, "templateId": 200, "level": 20}
        ]}
    });
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        10001,
        BattleCopy {
            config_id: 77,
            copy_type: 2,
            fleet_ids: vec![42, 43],
        },
    );
    catalog.search_3d.insert(10001);
    catalog.fleet_enemies.insert(42, vec![900]);
    catalog.fleet_is_last.insert(42, false);
    catalog.enemies.insert(
        900,
        BattleEnemy {
            hp: 123,
            attack: 456,
            ..BattleEnemy::default()
        },
    );
    catalog.fleet_enemies.insert(43, vec![901]);
    catalog.enemies.insert(
        901,
        BattleEnemy {
            hp: 321,
            attack: 654,
            ..BattleEnemy::default()
        },
    );
    let payload = battle_start_payload(&account, 10001, &[2], Some(&catalog));
    assert_eq!(decode_varint_field(&payload, 3), 77);
    assert_eq!(decode_repeated_varint_field(&payload, 5), vec![42, 43]);
    assert!(payload.windows(2).any(|window| window == [0xC2, 0x01]));
    assert!(payload
        .windows(3)
        .any(|window| window == [0x08, 0x84, 0x07]));
    // Search3D/Challenge Sea fields: IsFinal, AnimMode, WeatherGroupId,
    // and safe ConfigData entries required before local fleet movement starts.
    assert!(payload.windows(2).any(|window| window == [0x98, 0x01]));
    assert!(payload.windows(2).any(|window| window == [0xA0, 0x01]));
    assert!(payload.windows(2).any(|window| window == [0xB0, 0x01]));
    assert!(payload.windows(2).any(|window| window == [0xCA, 0x01]));
    let config_data = decode_repeated_message_field(&payload, 25);
    assert_eq!(config_data.len(), 2);
    assert_eq!(
        config_data
            .iter()
            .map(|config| decode_varint_field(config, 1))
            .collect::<Vec<_>>(),
        vec![50000, 0]
    );
    let enemy_fleets = decode_repeated_message_field(&payload, 24);
    assert_eq!(enemy_fleets.len(), 2);
    assert_eq!(
        enemy_fleets
            .iter()
            .map(|fleet| decode_varint_field(fleet, 1))
            .collect::<Vec<_>>(),
        vec![42, 43]
    );
    let skip_vcr = decode_repeated_message_field(&payload, 17);
    assert!(!skip_vcr.is_empty());
    assert_eq!(decode_varint_field(&skip_vcr[0], 2), 1);
    assert_eq!(decode_varint_field(&skip_vcr[0], 3), 1);
}

#[test]
fn sea_battle_start_uses_stable_position_anchor_without_losing_real_fleet_id() {
    let account = json!({
        "character": {"uid": 9, "level": 60, "name": "A"},
        "dock": {"heroes": [{"heroId": 1, "templateId": 100, "level": 10}]}
    });
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        1610100,
        BattleCopy {
            config_id: 1610100,
            copy_type: 2,
            fleet_ids: vec![161010000],
        },
    );
    catalog.search_3d.insert(1610100);
    catalog.fleet_enemies.insert(161010000, vec![1610101]);
    catalog.enemies.insert(
        1610101,
        BattleEnemy {
            hp: 1000,
            ship_info_id: 16101,
            ..BattleEnemy::default()
        },
    );

    let payload = battle_start_payload(&account, 1610100, &[1], Some(&catalog));
    assert_eq!(decode_repeated_varint_field(&payload, 5), vec![161010000]);
    let enemy_fleets = decode_repeated_message_field(&payload, 24);
    assert_eq!(enemy_fleets.len(), 1);
    assert_eq!(decode_varint_field(&enemy_fleets[0], 1), 160010000);
    assert_eq!(decode_repeated_message_field(&enemy_fleets[0], 3).len(), 1);
}

#[test]
fn sea_battle_start_includes_attached_enemy_fleets_for_every_stage() {
    let account = json!({
        "character": {"uid": 9, "level": 60, "name": "A"},
        "dock": {"heroes": [{"heroId": 1, "templateId": 100, "level": 10}]}
    });
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        1600500,
        BattleCopy {
            config_id: 1600500,
            copy_type: 2,
            fleet_ids: vec![160050000],
        },
    );
    catalog.search_3d.insert(1600500);
    catalog
        .attached_fleet_ids
        .insert(160050000, vec![160050001]);
    catalog.fleet_enemies.insert(160050000, vec![1600501]);
    catalog.fleet_enemies.insert(160050001, vec![1600502]);
    catalog.enemies.insert(1600501, BattleEnemy::default());
    catalog.enemies.insert(1600502, BattleEnemy::default());

    let payload = battle_start_payload(&account, 1600500, &[1], Some(&catalog));
    assert_eq!(
        decode_repeated_varint_field(&payload, 5),
        vec![160050000, 160050001]
    );
    let enemy_fleets = decode_repeated_message_field(&payload, 24);
    assert_eq!(
        enemy_fleets
            .iter()
            .map(|fleet| decode_varint_field(fleet, 1))
            .collect::<Vec<_>>(),
        vec![160010000, 160050001]
    );
}

#[test]
fn bundled_sea_catalog_start_payload_contains_attached_fleets() {
    let catalog_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/config");
    let chapters = load_chapter_catalog(Some(&catalog_dir));
    let battle = load_battle_catalog(Some(&catalog_dir));
    let account = json!({
        "character": {"uid": 9, "level": 60, "name": "A"},
        "dock": {"heroes": [{"heroId": 1, "templateId": 100, "level": 10}]}
    });
    let mut attached_stages = 0;

    for copy_id in chapters.sea {
        let copy = battle
            .copies
            .get(&copy_id)
            .unwrap_or_else(|| panic!("sea copy {copy_id} missing battle config"));
        assert!(
            battle.search_3d.contains(&copy_id),
            "sea copy {copy_id} missing search_3d flag"
        );
        let session_fleet_ids = battle_session_fleet_ids(copy_id, Some(&battle));
        for fleet_id in &session_fleet_ids {
            let enemy_ids = battle
                .fleet_enemies
                .get(fleet_id)
                .unwrap_or_else(|| panic!("sea copy {copy_id} fleet {fleet_id} missing enemies"));
            assert!(!enemy_ids.is_empty());
            for enemy_id in enemy_ids {
                assert!(
                    battle.enemies.contains_key(enemy_id),
                    "sea copy {copy_id} enemy {enemy_id} missing stats"
                );
            }
        }
        if session_fleet_ids.len() > copy.fleet_ids.len() {
            let payload = battle_start_payload(&account, copy_id, &[1], Some(&battle));
            assert_eq!(decode_repeated_varint_field(&payload, 5), session_fleet_ids);
            assert_eq!(
                decode_repeated_message_field(&payload, 24).len(),
                session_fleet_ids.len()
            );
            attached_stages += 1;
        }
    }

    assert!(
        attached_stages > 0,
        "bundled sea catalog has no attached fleet stage"
    );
}

#[tokio::test]
async fn sea_battle_pass_accepts_position_anchor_fleet_id() {
    let mut catalog = BattleCatalog::default();
    catalog.copies.insert(
        1610100,
        BattleCopy {
            config_id: 1610100,
            copy_type: 2,
            fleet_ids: vec![161010000],
        },
    );
    catalog.search_3d.insert(1610100);
    catalog.fleet_enemies.insert(161010000, vec![1610101]);
    catalog
        .attached_fleet_ids
        .insert(161010000, vec![161010001, 161010002]);
    catalog.fleet_enemies.insert(161010001, vec![1610111]);
    catalog.enemies.insert(
        1610101,
        BattleEnemy {
            hp: 1000,
            ..BattleEnemy::default()
        },
    );
    catalog.enemies.insert(
        1610111,
        BattleEnemy {
            hp: 1000,
            ..BattleEnemy::default()
        },
    );
    catalog.supply_cost_by_copy.insert(1610100, (0, 0));

    let state = ServerState::new("sea-anchor-pass", "test", "1.4.0");
    let mut account = default_account_snapshot("sea-anchor-pass", "test", 100);
    let mut start = Vec::new();
    append_varint_field(&mut start, 2, 1610100);
    battle_route_test_request(&mut account, &state, &catalog, "copy.StartBase", start).await;

    let mut attack = Vec::new();
    append_varint_field(&mut attack, 1, 2);
    append_varint_field(&mut attack, 2, 1610100);
    append_varint_field(&mut attack, 3, 1);
    append_varint_field(&mut attack, 4, 1610111);
    battle_route_test_request(&mut account, &state, &catalog, "copy.AttackBase", attack).await;

    let attached_pass = battle_pass_args(1610100, 3, 30, 161010001, 1, 1000);
    let responses = battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "copy.PassBase",
        attached_pass,
    )
    .await;
    assert!(responses
        .iter()
        .any(|response| response.method == "copy.PassBase" && response.err == 0));
    assert!(!account["battleSession"].is_null());

    let attached_pass = battle_pass_args(1610100, 3, 30, 161010002, 1, 1000);
    battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "copy.PassBase",
        attached_pass,
    )
    .await;

    let mut attack = Vec::new();
    append_varint_field(&mut attack, 1, 2);
    append_varint_field(&mut attack, 2, 1610100);
    append_varint_field(&mut attack, 3, 1);
    append_varint_field(&mut attack, 4, 1610101);
    battle_route_test_request(&mut account, &state, &catalog, "copy.AttackBase", attack).await;

    let pass = battle_pass_args(1610100, 3, 30, 160010000, 1, 1000);
    let responses =
        battle_route_test_request(&mut account, &state, &catalog, "copy.PassBase", pass).await;
    assert!(responses
        .iter()
        .any(|response| response.method == "copy.PassBase" && response.err == 0));
    assert!(account["battleSession"].is_null());
    assert_eq!(account["seaProgress"]["records"][0]["copyId"], 1610100);

    // Sea stages are repeatable. A pass only updates progress; it must not
    // invalidate the next StartBase session.
    let mut start_again = Vec::new();
    append_varint_field(&mut start_again, 2, 1610100);
    let responses = battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "copy.StartBase",
        start_again,
    )
    .await;
    assert!(responses
        .iter()
        .any(|response| response.method == "copy.StartBase" && response.err == 0));
    let mut attack_again = Vec::new();
    append_varint_field(&mut attack_again, 1, 2);
    append_varint_field(&mut attack_again, 2, 1610100);
    append_varint_field(&mut attack_again, 3, 1);
    append_varint_field(&mut attack_again, 4, 1610111);
    battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "copy.AttackBase",
        attack_again,
    )
    .await;
    battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "copy.PassBase",
        battle_pass_args(1610100, 3, 30, 161010001, 1, 1000),
    )
    .await;
    battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "copy.PassBase",
        battle_pass_args(1610100, 3, 30, 161010002, 1, 1000),
    )
    .await;
    let mut attack_again = Vec::new();
    append_varint_field(&mut attack_again, 1, 2);
    append_varint_field(&mut attack_again, 2, 1610100);
    append_varint_field(&mut attack_again, 3, 1);
    append_varint_field(&mut attack_again, 4, 1610101);
    battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "copy.AttackBase",
        attack_again,
    )
    .await;
    battle_route_test_request(
        &mut account,
        &state,
        &catalog,
        "copy.PassBase",
        battle_pass_args(1610100, 3, 30, 160010000, 1, 1000),
    )
    .await;
    assert_eq!(account["seaProgress"]["records"][0]["passCount"], 2);
}

#[test]
fn battle_pass_state_marks_first_pass_once() {
    let mut account = json!({});
    assert!(!battle_copy_passed(&account, 10001));
    record_battle_pass(&mut account, 10001, 3, 60, None, &[]);
    assert!(battle_copy_passed(&account, 10001));
    record_battle_pass(&mut account, 10001, 2, 30, None, &[]);
    assert_eq!(account["copyProgress"]["records"][0]["passCount"], 2);
    assert_eq!(account["copyProgress"]["records"][0]["grade"], 3);
    assert_eq!(account["copyProgress"]["records"][0]["starLevel"], 7);
    assert!(!battle_pass_payload_with_rewards(10001, false, 2, 30, &[]).contains(&0x50));
}

#[test]
fn battle_pass_payload_contains_per_hero_experience() {
    let payload = battle_pass_payload_with_experience(10001, true, 3, 60, &[], &[(42, 250)]);
    let exp_rewards = decode_repeated_message_field(&payload, 11);
    assert_eq!(exp_rewards.len(), 1);
    assert_eq!(decode_varint_field(&exp_rewards[0], 1), 42);
    assert_eq!(decode_varint_field(&exp_rewards[0], 2), 250);
}

#[test]
fn daily_battle_pass_updates_attempt_and_group_counters() {
    let mut account = json!({
        "dailyCopy": {
            "chapters": [{"chapterId": 40, "challengeTimes": 2, "passCopy": []}],
            "groups": [{"dailyGroupId": 7, "successTimes": 3}]
        }
    });
    let catalog = BattleCatalog {
        copies: [(
            500,
            BattleCopy {
                copy_type: 9,
                ..BattleCopy::default()
            },
        )]
        .into_iter()
        .collect(),
        daily_group_by_copy: [(500, 7)].into_iter().collect(),
        ..BattleCatalog::default()
    };
    record_battle_pass(&mut account, 500, 4, 60, Some(&catalog), &[7001]);

    assert_eq!(account["dailyCopy"]["chapters"][0]["challengeTimes"], 3);
    assert_eq!(
        account["dailyCopy"]["chapters"][0]["passCopy"],
        json!([500])
    );
    assert_eq!(account["dailyCopy"]["groups"][0]["successTimes"], 4);
    assert_eq!(account["dailyCopy"]["chapters"][0]["exBuff"], json!([7001]));
    assert_eq!(account["dailyCopy"]["records"][0]["exBuff"], json!([7001]));
}

#[test]
fn login_bootstrap_preserves_construction_bath_and_task_state() {
    let account = json!({
        "construction": {
            "jobs": [{
                "templateId": 123,
                "endTime": 456,
                "completed": false,
                "project": {"items": [{"resId": 10029, "count": 300}], "gold": 99}
            }],
            "lastProject": {"items": [], "gold": 88}
        },
        "bath": {
            "heroList": [{"heroId": 9, "pos": 2, "startTime": 100, "buffId": 0}],
            "isAllAuto": 1
        },
        "tasks": {"teachingPtRewardIds": [7, 8]}
    });

    let construction = construction_info_payload(&account, 100);
    assert!(construction.starts_with(&[0x12]));
    assert!(construction
        .windows(3)
        .any(|window| window == [0x08, 0xC8, 0x03]));

    let bath = bathroom_info_payload(&account);
    assert!(bath.windows(2).any(|window| window == [0x08, 0x09]));
    assert!(bath.windows(2).any(|window| window == [0x10, 0x01]));

    let tasks = task_info_payload(&account, None);
    assert!(tasks.windows(2).any(|window| window == [0x58, 0x07]));
    assert!(tasks.windows(2).any(|window| window == [0x58, 0x08]));
    assert!(tasks.windows(2).any(|window| window == [0x62, 0x02]));
}

#[test]
fn bathroom_operations_persist_state_for_post_snapshot() {
    let mut account = json!({
        "dock": {"heroes": [{"heroId": 9}, {"heroId": 10}]},
        "bath": {"heroList": [], "isAllAuto": 0}
    });
    let mut start = Vec::new();
    append_varint_field(&mut start, 1, 9);
    append_varint_field(&mut start, 2, 2);
    update_bathroom_state(&mut account, "bathroom.BathStart", &start, 100);
    assert_eq!(account["bath"]["heroList"][0]["heroId"], 9);
    assert_eq!(account["bath"]["heroList"][0]["pos"], 2);
    assert_eq!(account["bath"]["heroList"][0]["startTime"], 100);

    let mut nested = Vec::new();
    append_varint_field(&mut nested, 1, 10);
    append_varint_field(&mut nested, 2, 3);
    let mut start_all = Vec::new();
    super::append_message_field(&mut start_all, 1, &nested);
    update_bathroom_state(&mut account, "bathroom.BathStartAll", &start_all, 101);
    assert!(account["bath"]["heroList"]
        .as_array()
        .unwrap()
        .iter()
        .any(|hero| hero["heroId"] == 10 && hero["pos"] == 3));

    let mut auto = Vec::new();
    append_varint_field(&mut auto, 1, 9);
    append_varint_field(&mut auto, 2, 1);
    update_bathroom_state(&mut account, "bathroom.BathAuto", &auto, 102);
    assert_eq!(account["bath"]["heroList"][0]["isAuto"], 1);

    let mut all_auto = Vec::new();
    append_varint_field(&mut all_auto, 1, 1);
    update_bathroom_state(&mut account, "bathroom.BathAllAuto", &all_auto, 103);
    assert_eq!(account["bath"]["isAllAuto"], 1);

    let mut changed = Vec::new();
    append_varint_field(&mut changed, 1, 9);
    append_varint_field(&mut changed, 2, 10);
    update_bathroom_state(&mut account, "bathroom.BathChangeHero", &changed, 104);
    assert!(account["bath"]["heroList"]
        .as_array()
        .unwrap()
        .iter()
        .all(|hero| hero["heroId"] != 9));

    let mut end = Vec::new();
    append_varint_field(&mut end, 1, 9);
    update_bathroom_state(&mut account, "bathroom.BathEnd", &end, 105);
    let remaining = account["bath"]["heroList"].as_array().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0]["heroId"], 10);
}

#[test]
fn mop_up_state_round_trips_snapshot_entries() {
    let mut account = json!({});
    let mut start = Vec::new();
    append_varint_field(&mut start, 1, 1);
    append_varint_field(&mut start, 2, 10001);
    append_varint_field(&mut start, 3, 3);
    update_mop_up_state(&mut account, "mopUp.StartSweep", &start, 100);
    let payload = mop_up_payload(&account, 100);
    assert!(payload.windows(2).any(|window| window == [0x08, 0x01]));
    assert!(payload.windows(2).any(|window| window == [0x10, 0x91]));

    let mut stop = Vec::new();
    append_varint_field(&mut stop, 1, 1);
    update_mop_up_state(&mut account, "mopUp.StopSweep", &stop, 101);
    assert_eq!(mop_up_payload(&account, 101), vec![0x08, 0x00]);

    // Client sends TMopUpArg wrapped as request args field 1 (length-delimited).
    let mut inner = Vec::new();
    append_varint_field(&mut inner, 1, 2);
    append_varint_field(&mut inner, 2, 10001);
    append_varint_field(&mut inner, 3, 4);
    let mut wrapped = Vec::new();
    append_message_field(&mut wrapped, 1, &inner);
    assert_eq!(decode_mop_up_arg(&wrapped), (2, 10001, 4));
}

#[test]
fn building_assignments_validate_ownership_and_clear_previous_slots() {
    let mut account = json!({
        "dock": {"heroes": [{"heroId": 1}, {"heroId": 2}, {"heroId": 3}]},
        "building": {"buildings": [
            {"id": 1, "tid": 2, "level": 2, "heroIds": [1]},
            {"id": 2, "tid": 41, "level": 1, "heroIds": [2]}
        ], "lands": []}
    });
    assert!(update_building_assignments(
        &mut account,
        &[(2, vec![1])],
        100,
        None
    ));
    assert_eq!(account["building"]["buildings"][0]["heroIds"], json!([]));
    assert_eq!(account["building"]["buildings"][1]["heroIds"], json!([1]));
    assert!(!update_building_assignments(
        &mut account,
        &[(1, vec![99])],
        101,
        None
    ));
    assert!(!update_building_assignments(
        &mut account,
        &[(1, vec![1]), (2, vec![1])],
        102,
        None
    ));

    account["building"]["buildings"][0]["status"] = json!(2);
    assert!(!update_building_assignments(
        &mut account,
        &[(1, vec![2])],
        103,
        None
    ));
    account["building"]["buildings"][0]["status"] = json!(1);
    // Office template capacity follows level; level 2 accepts two heroes, not three.
    assert!(!update_building_assignments(
        &mut account,
        &[(1, vec![1, 2, 3])],
        104,
        None
    ));
}

#[test]
fn daily_copy_normalization_rebuilds_configured_state_and_resets_counts() {
    let catalog = ChapterCatalog {
        daily_chapters: vec![(40, 7), (41, 7)],
        daily_groups: vec![7],
        ..ChapterCatalog::default()
    };
    let mut account = json!({
        "dailyCopy": {
            "resetDay": 1,
            "chapters": [
                {"chapterId": 40, "challengeTimes": 9, "passCopy": [401, 401], "selectEx": true},
                {"chapterId": 999, "challengeTimes": 8, "passCopy": [999]}
            ],
            "groups": [{"dailyGroupId": 7, "successTimes": 5}],
            "extraGroups": [{"dailyGroupId": 7, "successTimes": 6}]
        }
    });
    assert!(normalize_daily_copy_state(
        &mut account,
        Some(&catalog),
        86_400 * 2
    ));
    assert_eq!(
        account["dailyCopy"]["chapters"].as_array().unwrap().len(),
        2
    );
    assert_eq!(account["dailyCopy"]["chapters"][0]["challengeTimes"], 0);
    assert_eq!(
        account["dailyCopy"]["chapters"][0]["passCopy"],
        json!([401])
    );
    assert_eq!(account["dailyCopy"]["groups"][0]["successTimes"], 0);
    assert_eq!(account["dailyCopy"]["extraGroups"][0]["successTimes"], 6);
}

#[test]
fn hero_retire_breakdown_rewards_update_account_and_response() {
    let mut account = json!({"character": {"retire": 0}, "bag": {"items": []}});
    let mut catalog = HeroBreakdownCatalog::default();
    catalog
        .rewards_by_template
        .insert(123, vec![(5, 12, 7), (1, 9001, 2)]);
    let rewards = apply_hero_breakdown_rewards(&mut account, &[123, 123], Some(&catalog));
    assert_eq!(account["character"]["retire"], 14);
    assert_eq!(
        account["bag"]["items"][0],
        json!({"templateId": 9001, "num": 4})
    );
    assert_eq!(rewards.len(), 2);
    assert!(encode_retire_hero_response(&rewards).starts_with(&[0x0A]));
}

#[test]
fn chapter_catalog_classifies_copy_types_and_treaty_levels() {
    let catalog = ChapterCatalog::from_rows([
        (
            10,
            json!({"class_type": 1, "chapter_plot_type": 1, "level_list": [101, 102]}),
        ),
        (20, json!({"class_type": 2, "level_list": [201, 202]})),
        (30, json!({"class_type": 33, "level_list": [301]})),
        (50, json!({"class_type": 10, "level_list": [501, 502]})),
        (60, json!({"class_type": 24, "level_list": [601]})),
        (
            40,
            json!({"class_type": 9, "level_list": [401], "treaty_copy": [402]}),
        ),
    ]);
    assert_eq!(catalog.plot, vec![101, 102]);
    assert_eq!(catalog.sea, vec![201, 202]);
    assert_eq!(catalog.mubar, vec![301]);
    assert_eq!(catalog.goods_copy, vec![501, 502]);
    assert_eq!(catalog.tower, vec![601]);
    assert_eq!(catalog.daily, vec![401, 402]);
    assert_eq!(catalog.daily_chapters, vec![(40, 0)]);
    assert!(catalog.daily_groups.is_empty());
}

#[test]
fn chapter_catalog_tracks_first_story_sea_stage() {
    let catalog = ChapterCatalog::from_rows([
        (8001, json!({"class_type": 2, "level_list": [5011, 5012]})),
        (
            1001,
            json!({"class_type": 2, "level_list": [1600100, 1600200]}),
        ),
    ]);
    assert_eq!(catalog.sea_initial, 1_600_100);
    assert_eq!(catalog.sea, vec![1_600_100, 1_600_200, 5011, 5012]);
}

#[tokio::test]
async fn copy_star_reward_claims_configured_reward_once() {
    let mut account = default_account_snapshot("copy-star-reward", "Captain", 123);
    account["copyProgress"]["records"] = json!([{"copyId": 5011, "starLevel": 7}]);

    let chapter_catalog = ChapterCatalog::from_rows([(
        18001,
        json!({
            "class_type": 2,
            "level_list": [5011],
            "star_cond": [3],
            "star_reward": [9001]
        }),
    )]);
    let mut task_catalog = TaskCatalog::default();
    task_catalog.rewards_by_id.insert(9001, vec![(1, 9002, 2)]);
    let request = TMessageCodec::encode_request(&TRequest {
        method: "copy.StarReward".to_owned(),
        args: Some({
            let mut args = Vec::new();
            append_varint_field(&mut args, 1, 18001);
            append_varint_field(&mut args, 2, 1);
            args
        }),
        callback_handler: 75,
        ..TRequest::default()
    });
    let (mut client, mut server) = duplex(1_048_576);
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();
    let state = ServerState::new("copy-star-reward", "Captain", "1.4.0");
    process_game_login_frame_with_catalog_mut(
        &mut server,
        &state,
        Some(&mut account),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(&chapter_catalog),
        Some(&task_catalog),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    drop(server);

    let mut responses = Vec::new();
    while let Some(frame) = NetSocketFrameCodec::read(&mut client).await.unwrap() {
        responses.push(TMessageCodec::decode_response(&frame.payload).unwrap());
    }
    assert_eq!(responses.len(), 5);
    assert_eq!(responses[0].method, "hero.UpdateHeroBagData");
    assert_eq!(responses[1].method, "bag.UpdateBagData");
    assert_eq!(responses[2].method, "equip.UpdateEquipBagData");
    assert_eq!(responses[3].method, "user.UpdateUserInfo");
    assert_eq!(responses[4].method, "copy.StarReward");
    assert_eq!(responses[4].err, 0);
    assert_eq!(responses[4].callback_handler, 75);
    assert_eq!(bag_item_count(&account, 9002), 2);
    assert_eq!(
        account["copyStarRewards"],
        json!([{"chapterId": 18001, "index": 1}])
    );
    let reward = decode_repeated_message_field(responses[4].ret.as_deref().unwrap(), 1)
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(decode_varint_field(&reward, 1), 1);
    assert_eq!(decode_varint_field(&reward, 2), 9002);
    assert_eq!(decode_varint_field(&reward, 3), 2);

    let (mut client, mut server) = duplex(16_384);
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();
    process_game_login_frame_with_catalog_mut(
        &mut server,
        &state,
        Some(&mut account),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(&chapter_catalog),
        Some(&task_catalog),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let duplicate = TMessageCodec::decode_response(
        &NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap()
            .payload,
    )
    .unwrap();
    assert_eq!(duplicate.method, "copy.StarReward");
    assert_eq!(duplicate.err, 1);
    assert_eq!(bag_item_count(&account, 9002), 2);
}

#[test]
fn pass_mini_game_uses_tpassbase_and_records_single_copy() {
    let mut account = default_account_snapshot("mini-game", "Captain", 123);
    let mut handler_error = None;
    let mut args = Vec::new();
    append_varint_field(&mut args, 1, 1001);
    append_varint_field(&mut args, 12, 42);
    append_varint_field(&mut args, 19, 1);
    let response = {
        let mut account_ref = Some(&mut account);
        pass_mini_game(
            None,
            &mut account_ref,
            &args,
            None,
            None,
            &mut Vec::new(),
            &mut handler_error,
        )
        .unwrap()
    };
    assert!(handler_error.is_none());
    assert_eq!(decode_varint_field(&response, 12), 1001);
    assert_eq!(decode_varint_field(&response, 4), 3);
    assert_eq!(decode_varint_field(&response, 8), 60);
    assert_eq!(decode_varint_field(&response, 10), 1);
    assert_eq!(completed_copy_ids(&account, "copyProgress"), vec![1001]);
    assert_eq!(account["copyProgress"]["records"][0]["starLevel"], 7);

    let mut invalid_args = Vec::new();
    append_varint_field(&mut invalid_args, 1, 3000);
    append_varint_field(&mut invalid_args, 19, 0);
    let invalid = {
        let mut account_ref = Some(&mut account);
        pass_mini_game(
            None,
            &mut account_ref,
            &invalid_args,
            None,
            None,
            &mut Vec::new(),
            &mut handler_error,
        )
        .unwrap()
    };
    assert!(invalid.is_empty());
    assert_eq!(
        handler_error,
        Some(GameError::Internal("mini-game was not finished".to_owned()))
    );
}

#[test]
fn equip_new_test_catalog_reads_active_reward_matrix() {
    let catalog = EquipNewTestCatalog::from_rows([(
        1503,
        json!({
            "type": 70,
            "is_open": 1,
            "banner_gotopage_activity": "EquipNewTestPage",
            "p1": [61001, 61002, 61003],
            "p4": [[216000, 756000, 1200000], [192000, 672000, 1200000], [252000, 1032000, 1560000]],
            "p5": [[509256, 509257, 509258], [509259, 509260, 509261], [509262, 509263, 509264]]
        }),
    )]);

    assert_eq!(catalog.activity_id, 1503);
    assert_eq!(catalog.copy_ids, vec![61001, 61002, 61003]);
    assert_eq!(catalog.damage_thresholds[1][2], 1_200_000);
    assert_eq!(catalog.reward_ids[2][0], 509262);
}

#[test]
fn equip_new_test_catalog_rejects_empty_reward_matrix() {
    let catalog = EquipNewTestCatalog::from_rows([(
        58,
        json!({
            "type": 34,
            "is_open": 1,
            "p1": [1],
            "p4": [[255801, 509357, 50]],
            "p5": []
        }),
    )]);

    assert_eq!(catalog.activity_id, 0);
    assert!(catalog.copy_ids.is_empty());
}

#[test]
fn equip_new_test_reward_requires_damage_and_is_idempotent() {
    let catalog = EquipNewTestCatalog {
        activity_id: 1503,
        copy_ids: vec![61001],
        damage_thresholds: vec![vec![100, 200]],
        reward_ids: vec![vec![9001, 9002]],
    };
    let mut account = json!({"equipNewTestCopy": {"infos": [{
        "id": 1, "maxDamage": 150, "receivedRewards": []
    }]}});

    assert_eq!(
        super::resolve_new_test_reward(&catalog, &account, 1, 1),
        Ok(9001)
    );
    assert!(super::mark_new_test_reward(&mut account, 1, 1));
    assert_eq!(
        super::resolve_new_test_reward(&catalog, &account, 1, 1),
        Err("already claimed")
    );
    assert_eq!(
        super::resolve_new_test_reward(&catalog, &account, 1, 2),
        Err("damage threshold not reached")
    );
}

#[test]
fn copy_get_copy_request_type_selects_matching_catalog() {
    assert_eq!(copy_request_type(&[0x08, 0x02]), 2);
    assert_eq!(copy_request_type(&[0x08, 0x01]), 1);
    assert_eq!(copy_request_type(&[]), 1);
    assert_eq!(copy_request_type(&[0x08, 0x0a]), 10);
    assert_eq!(copy_request_type(&[0x08, 0x18]), 24);
}

#[test]
fn goods_copy_snapshot_contains_each_configured_copy() {
    let catalog = ChapterCatalog {
        goods_copy: vec![60000, 60001],
        ..ChapterCatalog::default()
    };
    let payload = super::goods_copy_snapshot_payload(&json!({}), Some(&catalog));
    let entries = decode_repeated_message_field(&payload, 1);

    assert_eq!(entries.len(), 2);
    assert_eq!(decode_varint_field(&entries[0], 4), 60000);
    assert_eq!(decode_varint_field(&entries[1], 4), 60001);
}

#[test]
fn pve_room_payload_contains_room_owner_and_ready_state() {
    let account = json!({
        "character": {"uid": 1, "name": "司令", "head": 1021051},
        "pveRoom": {
            "roomId": 1234,
            "copyId": 81041,
            "ownerId": 1,
            "isPublic": true,
            "capacity": 2,
            "createTime": 99,
            "users": [{"uid": 1, "name": "司令", "isReady": true, "enterTime": 99, "heroIds": [1]}]
        }
    });
    let payload = super::game_login::coop_handler::pve_room_payload(&account);
    let users = decode_repeated_message_field(&payload, 5);

    assert_eq!(decode_varint_field(&payload, 1), 1234);
    assert_eq!(decode_varint_field(&payload, 2), 81041);
    assert_eq!(decode_varint_field(&payload, 4), 1);
    assert_eq!(decode_varint_field(&users[0], 1), 1);
    assert_eq!(decode_varint_field(&users[0], 5), 1);
}

#[test]
fn tower_info_payload_contains_initial_progress_and_reset_time() {
    let account = json!({"tower": {"chapterId": 30001, "resetTime": 123}});
    let catalog = ChapterCatalog {
        tower_chapter_id: 30001,
        ..ChapterCatalog::default()
    };
    let payload =
        super::game_login::tower_handler::tower_info_payload(&account, Some(&catalog), 456);

    assert_eq!(decode_varint_field(&payload, 1), 30001);
    assert_eq!(decode_varint_field(&payload, 2), 0);
    assert_eq!(decode_varint_field(&payload, 3), 0);
    assert_eq!(decode_varint_field(&payload, 6), 123);
    assert_eq!(decode_varint_field(&payload, 13), 0);
}

#[test]
fn activity_tower_payload_contains_reset_time() {
    let payload = super::game_login::tower_handler::activity_tower_payload(&json!({}), 456);
    assert_eq!(decode_varint_field(&payload, 1), 0);
    assert_eq!(decode_varint_field(&payload, 2), 456);
}

#[test]
fn sea_difficulty_is_locked_below_commander_level_60() {
    let mut account = json!({
        "character": {"level": 1, "seaDifficulty": 7}
    });
    assert_eq!(sea_difficulty_for_account(&account), 1);

    account["character"]["level"] = json!(60);
    assert_eq!(sea_difficulty_for_account(&account), 7);
    set_sea_difficulty(&mut account, 4);
    assert_eq!(sea_difficulty_for_account(&account), 4);
}

#[test]
fn battle_catalog_loads_xor_config_rows() {
    let root = std::env::temp_dir().join(format!("blueoath-battle-catalog-{}", std::process::id()));
    let config = root.join("blueoath_Data/StreamingAssets/config");
    std::fs::create_dir_all(&config).unwrap();
    for (name, rows) in [
        (
            "config_copy.db",
            vec![(
                77,
                r#"{"copy_id":10001,"fleet_id":[42],"blood_range_lower":-1,"random_weight":1000}"#,
            )],
        ),
        (
            "config_fleet.db",
            vec![(42, r#"{"copy_enemys":[900],"copy_attacheds":[[43,1]]}"#)],
        ),
        (
            "config_ship_enemy.db",
            vec![(900, r#"{"hp":123,"attack":456,"ship_info_id":7}"#)],
        ),
        (
            "config_copy_display.db",
            vec![(10001, r#"{"search_3d":1}"#)],
        ),
    ] {
        let connection = rusqlite::Connection::open(config.join(name)).unwrap();
        connection
            .execute("CREATE TABLE DBObject (id INTEGER, jsonbytes BLOB)", [])
            .unwrap();
        for (id, json) in rows {
            let encoded: Vec<u8> = json.as_bytes().iter().map(|byte| byte ^ 0x55).collect();
            connection
                .execute(
                    "INSERT INTO DBObject (id, jsonbytes) VALUES (?1, ?2)",
                    rusqlite::params![id, encoded],
                )
                .unwrap();
        }
    }
    let catalog = load_battle_catalog(Some(&root));
    assert_eq!(catalog.copies[&10001].config_id, 77);
    assert_eq!(catalog.copies[&10001].fleet_ids, vec![42]);
    assert!(catalog.search_3d.contains(&10001));
    assert_eq!(catalog.fleet_enemies[&42], vec![900]);
    assert_eq!(catalog.attached_fleet_ids[&42], vec![43]);
    assert_eq!(catalog.enemies[&900].hp, 123);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn battle_catalog_loads_plain_json_rows_without_database() {
    let root = std::env::temp_dir().join(format!(
        "blueoath-battle-json-catalog-{}",
        std::process::id()
    ));
    let config = root.join("blueoath_Data/StreamingAssets/config");
    std::fs::create_dir_all(&config).unwrap();
    for (name, rows) in [
        (
            "config_copy.json",
            json!([{
                "id": 77,
                "value": {"copy_id":10001,"fleet_id":[42],"blood_range_lower":-1,"random_weight":1000}
            }]),
        ),
        (
            "config_fleet.json",
            json!([{"id":42,"value":{"copy_enemys":[900],"copy_attacheds":[[43,1]]}}]),
        ),
        (
            "config_ship_enemy.json",
            json!([{"id":900,"value":{"hp":123,"attack":456,"ship_info_id":7}}]),
        ),
        (
            "config_copy_display.json",
            json!([{"id":10001,"value":{"search_3d":1}}]),
        ),
    ] {
        let document = json!({
            "format": "blueoath-catalog-json",
            "version": 1,
            "rows": rows,
        });
        std::fs::write(config.join(name), serde_json::to_vec(&document).unwrap()).unwrap();
    }

    let catalog = load_battle_catalog(Some(&root));
    assert_eq!(catalog.copies[&10001].config_id, 77);
    assert_eq!(catalog.copies[&10001].fleet_ids, vec![42]);
    assert!(catalog.search_3d.contains(&10001));
    assert_eq!(catalog.fleet_enemies[&42], vec![900]);
    assert_eq!(catalog.attached_fleet_ids[&42], vec![43]);
    assert_eq!(catalog.enemies[&900].hp, 123);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ship_stat_catalog_loads_xor_config_rows() {
    let root =
        std::env::temp_dir().join(format!("blueoath-ship-stat-catalog-{}", std::process::id()));
    let config = root.join("blueoath_Data/StreamingAssets/config");
    std::fs::create_dir_all(&config).unwrap();
    let connection = rusqlite::Connection::open(config.join("config_ship_main.db")).unwrap();
    connection
        .execute("CREATE TABLE DBObject (id INTEGER, jsonbytes BLOB)", [])
        .unwrap();
    let json = br#"{"hp":1083,"attack":157,"defense":216,"torpedo_attack":716,"torpedo_defense":217,"ship_bomb_attack":216,"ship_torpedo_attack":87,"carry_plane_count":2,"to_air_attack":216,"hit":100,"dodge":35}"#;
    let encoded: Vec<u8> = json.iter().map(|byte| byte ^ 0x55).collect();
    connection
        .execute(
            "INSERT INTO DBObject (id, jsonbytes) VALUES (?1, ?2)",
            rusqlite::params![10210511, encoded],
        )
        .unwrap();
    drop(connection);

    let catalog = load_ship_stat_catalog(Some(&root));
    let stats = &catalog.by_template[&10210511];
    assert_eq!(stats.hp, 1083);
    assert_eq!(stats.attack, 157);
    assert_eq!(stats.torpedo_attack, 716);
    assert_eq!(stats.ship_bomb_attack, 216);
    assert_eq!(stats.ship_torpedo_attack, 87);
    assert_eq!(stats.carry_plane_count, 2);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn battle_catalog_includes_new_account_tutorial_copy() {
    let config_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("catalog/config");
    let catalog = load_battle_catalog(Some(&config_dir));

    assert!(catalog.copies.contains_key(&100));
    assert_eq!(catalog.copies[&100].fleet_ids, vec![100]);
}

#[test]
fn chapter_catalog_loads_xor_config_db_from_client_path() {
    let root =
        std::env::temp_dir().join(format!("blueoath-chapter-catalog-{}", std::process::id()));
    let config_dir = root.join("blueoath_Data/StreamingAssets/config");
    std::fs::create_dir_all(&config_dir).unwrap();
    let path = config_dir.join("config_chapter.db");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute("CREATE TABLE DBObject (id INTEGER, jsonbytes BLOB)", [])
        .unwrap();
    let json = br#"{"class_type":2,"level_list":[201,202]}"#;
    let encoded: Vec<u8> = json.iter().map(|byte| byte ^ 0x55).collect();
    connection
        .execute(
            "INSERT INTO DBObject (id, jsonbytes) VALUES (?1, ?2)",
            rusqlite::params![20, encoded],
        )
        .unwrap();
    drop(connection);

    let catalog = load_chapter_catalog(Some(&root));
    assert_eq!(catalog.sea, vec![201, 202]);
    assert!(catalog.plot.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn daily_copy_login_snapshot_normalizes_day_and_progress_values() {
    let now = 1_700_000_000u32;
    let reset_day = (u64::from(now) + 8 * 60 * 60) / 86_400;
    let account = json!({
        "dailyCopy": {
            "resetDay": reset_day - 1,
            "chapters": [{
                "chapterId": 1,
                "challengeTimes": -2,
                "passCopy": [101, 101, 102],
                "selectEx": true,
                "exStar": -4
            }],
            "groups": [{"dailyGroupId": 1, "successTimes": -3}],
            "extraGroups": [{"dailyGroupId": 1, "successTimes": -5}]
        }
    });
    let chapters = daily_copy_progress_from_account(Some(&account), now);
    assert_eq!(chapters[0].challenge_times, 0);
    assert_eq!(chapters[0].pass_copy, vec![101, 102]);
    assert!(chapters[0].select_ex);
    assert_eq!(chapters[0].ex_star, 0);
    assert_eq!(
        daily_copy_group_progress_from_account(Some(&account), "groups", now)[0].success_times,
        0
    );
    assert_eq!(
        daily_copy_group_progress_from_account(Some(&account), "extraGroups", now)[0].success_times,
        0
    );
}

#[test]
fn building_resources_and_fleet_entries_read_account_values() {
    let account = json!({
        "building": {
            "buildings": [
                {"id": 2, "tid": 41, "level": 1, "productivity": 12, "produceSpeed": 13,
                 "productCount": 14, "recipeId": 15, "itemCount": 16,
                 "lastMoodUpdateTime": 17, "lastBuildUpdateTime": 18,
                 "recipeTime": 19, "floatCount": 20,
                 "tacticList": [{"index": 1, "name": "Plan A", "heroIds": [7]}]},
                {"id": 1, "tid": 2, "level": 3}
            ],
            "lands": [{"index": 6, "buildingId": 2}, {"index": 1, "buildingId": 1}],
            "workerStrength": 987654,
            "workerRecover": 22,
            "foodMax": 333,
            "electricMax": 444
        },
        "fleet": {"tactics": [{"modeId": 3, "heroInfo": [7], "exHeroInfo": [8]}]}
    });
    let building = building_info_from_account(&account, 99);
    assert_eq!(building.worker_strength, 2_000_000);
    assert_eq!(building.worker_recover, 22);
    assert_eq!(building.food_max, 333);
    assert_eq!(building.electric_max, 444);
    assert_eq!(building.buildings[0].id, 1);
    let production = &building.buildings[1];
    assert_eq!(production.productivity, 12);
    assert_eq!(production.produce_speed, 13);
    assert_eq!(production.product_count, 14);
    assert_eq!(production.recipe_id, 15);
    assert_eq!(production.item_count, 16);
    assert_eq!(production.last_mood_update_time, 17);
    assert_eq!(production.last_build_update_time, 18);
    assert_eq!(production.recipe_time, 19);
    assert_eq!(production.float_count, 20);
    assert_eq!(production.tactic_list[0].name, "Plan A");
    assert_eq!(production.tactic_list[0].hero_ids, vec![7]);

    let fleet = fleet_info_from_account(&account);
    assert_eq!(fleet.tactics[0].hero_ids, vec![7]);
    assert_eq!(fleet.tactics[0].ex_hero_ids, vec![8]);
    assert_eq!(fleet.tactics[0].formation_id, 2);
    assert_eq!(fleet.tactics[0].tactic_type, 1);
}

#[test]
fn base_building_mutations_keep_client_state_consistent() {
    let mut account = default_account_snapshot("base-building", "Base", 100);
    let added_id = add_building_state(&mut account, 41, 2, 101).expect("free land");
    assert_eq!(added_id, 3);
    assert_eq!(
        account["building"]["lands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|land| land["index"] == 2)
            .unwrap()["buildingId"],
        3
    );

    assert!(change_building_level(&mut account, added_id, 1));
    assert_eq!(account["building"]["buildings"][2]["level"], 2);
    assert!(finish_building_state(&mut account, added_id, 102));
    assert_eq!(account["building"]["buildings"][2]["status"], 1);
}

#[test]
fn building_production_claim_settles_resource_and_is_idempotent() {
    let mut account = serde_json::json!({
        "character": {"gold": 10, "supply": 20},
        "building": {"buildings": [{
            "id": 7, "tid": 31, "status": 3, "productCount": 2,
            "lastUpdateTime": 100
        }]}
    });
    let catalog = BuildingCatalog {
        capacities: std::collections::BTreeMap::new(),
        building_configs: [(
            31,
            serde_json::json!({
                "type": 4, "productid": [5, 1], "productmax": 500,
                "productivity": 600000
            }),
        )]
        .into_iter()
        .collect(),
        recipe_configs: std::collections::BTreeMap::new(),
        resource_time_seconds: [(1, 600)].into_iter().collect(),
    };

    let rewards =
        collect_building_rewards(&mut account, Some(&catalog), Some(7), None, 1300, 1.0, 2.0);
    assert_eq!(rewards.len(), 1);
    assert_eq!(rewards[0].goods_type, 5);
    assert_eq!(rewards[0].item_id, 1);
    assert_eq!(rewards[0].num, 240);
    assert_eq!(account["building"]["buildings"][0]["productCount"], 0);
    assert_eq!(account["building"]["buildings"][0]["lastUpdateTime"], 1300);
    assert!(
        collect_building_rewards(&mut account, Some(&catalog), Some(7), None, 1301, 1.0, 2.0)
            .is_empty()
    );
}

#[test]
fn building_production_claim_settles_item_and_batch_list() {
    let mut account = serde_json::json!({
        "character": {"gold": 10, "supply": 20},
        "building": {"buildings": [
            {"id": 7, "tid": 31, "status": 3, "productCount": 2, "lastUpdateTime": 100},
            {"id": 8, "tid": 61, "status": 3, "recipeId": 1, "itemCount": 2,
             "productCount": 1, "lastUpdateTime": 100}
        ]}
    });
    let catalog = BuildingCatalog {
        capacities: std::collections::BTreeMap::new(),
        building_configs: [
            (
                31,
                serde_json::json!({
                    "type": 4, "productid": [5, 1], "productmax": 500,
                    "productivity": 600000
                }),
            ),
            (61, serde_json::json!({"type": 7, "productmax": 100})),
        ]
        .into_iter()
        .collect(),
        recipe_configs: [(1, serde_json::json!({"time": 30, "item": [1, 14001, 2]}))]
            .into_iter()
            .collect(),
        resource_time_seconds: [(1, 600)].into_iter().collect(),
    };

    let rewards = collect_building_rewards(&mut account, Some(&catalog), None, None, 160, 1.0, 1.0);
    assert_eq!(rewards.len(), 2);
    assert!(rewards
        .iter()
        .any(|reward| { reward.goods_type == 5 && reward.item_id == 1 && reward.num == 7 }));
    assert!(rewards
        .iter()
        .any(|reward| { reward.goods_type == 1 && reward.item_id == 14001 && reward.num == 6 }));
    assert_eq!(account["building"]["buildings"][1]["productCount"], 0);
    assert_eq!(account["building"]["buildings"][1]["itemCount"], 0);
    assert_eq!(account["building"]["buildings"][1]["recipeId"], 0);
    assert!(
        collect_building_rewards(&mut account, Some(&catalog), None, None, 161, 1.0, 1.0)
            .is_empty()
    );
}

#[test]
fn building_item_claim_preserves_unfinished_queue() {
    let mut account = serde_json::json!({
        "building": {"buildings": [{
            "id": 8, "tid": 61, "status": 3, "recipeId": 1,
            "itemCount": 3, "productCount": 0, "lastUpdateTime": 100
        }]}
    });
    let catalog = BuildingCatalog {
        capacities: std::collections::BTreeMap::new(),
        building_configs: [(61, serde_json::json!({"type": 7, "productmax": 100}))]
            .into_iter()
            .collect(),
        recipe_configs: [(1, serde_json::json!({"time": 30, "item": [1, 14001, 1]}))]
            .into_iter()
            .collect(),
        resource_time_seconds: std::collections::BTreeMap::new(),
    };

    let rewards =
        collect_building_rewards(&mut account, Some(&catalog), Some(8), None, 165, 1.0, 1.0);
    assert_eq!(rewards[0].num, 2);
    assert_eq!(account["building"]["buildings"][0]["itemCount"], 1);
    assert_eq!(account["building"]["buildings"][0]["recipeId"], 1);
    assert_eq!(account["building"]["buildings"][0]["status"], 3);
    assert_eq!(account["building"]["buildings"][0]["lastUpdateTime"], 160);
}

#[test]
fn strategy_state_apply_updates_selected_fleet() {
    let mut account = default_account_snapshot("strategy", "Strategy", 100);
    let mut args = Vec::new();
    append_varint_field(&mut args, 1, 17);
    append_varint_field(&mut args, 2, 2);
    append_varint_field(&mut args, 3, 3);
    append_varint_field(&mut args, 4, 1);

    assert!(apply_strategy_state(&mut account, "strategy.Learn", &args));
    assert!(apply_strategy_state(&mut account, "strategy.Apply", &args));
    assert_eq!(account["strategy"]["list"][0]["id"], 17);
    assert_eq!(account["strategy"]["list"][0]["level"], 2);
    assert_eq!(account["fleet"]["tactics"][2]["strategyId"], 17);
}

#[test]
fn support_state_round_trip_is_idempotent() {
    let mut account = default_account_snapshot("support", "Support", 100);
    let mut start = Vec::new();
    append_varint_field(&mut start, 1, 7001);
    append_varint_field(&mut start, 2, 1);

    let id = start_support_state(&mut account, &start, 200).expect("support starts");
    assert_eq!(id, 1);
    assert!(complete_support_state(&mut account, id));
    assert!(account["support"]["items"].as_array().unwrap().is_empty());
    assert!(!complete_support_state(&mut account, id));
}

#[test]
fn support_completion_obeys_type_and_duration_and_returns_rewards() {
    let mut account = default_account_snapshot("support-settle", "Support", 100);
    let mut start = Vec::new();
    append_varint_field(&mut start, 1, 7001);
    append_varint_field(&mut start, 2, 1);
    let id = start_support_state(&mut account, &start, 200).expect("support starts");
    let mut catalog = SupportCatalog::default();
    catalog.items.insert(
        7001,
        SupportFleetItem {
            duration_seconds: 100,
            base_rewards: vec![(5, 6, 300)],
            ..SupportFleetItem::default()
        },
    );

    assert!(settle_support_state(&mut account, id, 1, 250, &catalog).is_none());
    let result = settle_support_state(&mut account, id, 1, 300, &catalog).expect("completed");
    assert_eq!(result.reward_type, 1);
    assert_eq!(result.hero_ids, vec![1]);
    assert_eq!(result.base_rewards, vec![(5, 6, 300)]);
    assert!(result.random_rewards.is_empty());
    assert!(account["support"]["items"].as_array().unwrap().is_empty());
}

#[test]
fn preset_fleet_state_round_trips_through_account_projection() {
    let mut account = default_account_snapshot("preset", "Preset", 100);
    let value = PresetFleetInfo {
        fleets: vec![PresetFleet {
            name: "Boss fleet".to_owned(),
            hero_ids: vec![1, 2],
            ex_hero_ids: vec![9, 10],
            mode_id: 3,
            strategy_id: 17,
        }],
        name_num: 4,
        red_dot: 1,
    };

    set_preset_fleet_from_account(&mut account, &value);

    assert_eq!(preset_fleet_info_from_account(&account), value);
}

#[tokio::test]
async fn preset_fleet_route_persists_and_pushes_updated_snapshot() {
    let mut account = default_account_snapshot("preset-route", "Preset", 100);
    let value = PresetFleetInfo {
        fleets: vec![PresetFleet {
            name: "Route fleet".to_owned(),
            hero_ids: vec![1],
            ex_hero_ids: vec![8],
            mode_id: 2,
            strategy_id: 3,
        }],
        name_num: 5,
        red_dot: 1,
    };
    let (mut client, mut server) = duplex(1_048_576);
    let request = TMessageCodec::encode_request(&TRequest {
        method: "presetfleet.SetPresetFleets".to_owned(),
        args: Some(PresetFleetCodec::encode(&value)),
        callback_handler: 72,
        ..TRequest::default()
    });
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();
    process_game_login_frame_with_catalog_mut(
        &mut server,
        &ServerState::new("preset-route", "Preset", "1.0.0"),
        Some(&mut account),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    drop(server);

    let response = TMessageCodec::decode_response(
        &NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap()
            .payload,
    )
    .unwrap();
    let push = TMessageCodec::decode_response(
        &NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap()
            .payload,
    )
    .unwrap();
    assert_eq!(response.err, 0);
    assert_eq!(response.method, "presetfleet.SetPresetFleets");
    assert_eq!(push.method, "presetfleet.PresetFleetsInfo");
    assert_eq!(preset_fleet_info_from_account(&account), value);
}

#[tokio::test]
async fn copy_history_fleet_routes_return_and_delete_records() {
    let mut account = default_account_snapshot("history-route", "Captain", 42);
    account["copyRecords"] = json!([{
        "copyId": 2001,
        "passTime": 37,
        "recTime": 99,
        "strategyId": 17,
        "heroIds": [1]
    }]);
    let state = ServerState::new("history-route", "Captain", "1.0.0");
    let request = TMessageCodec::encode_request(&TRequest {
        method: "copy.GetRecord".to_owned(),
        args: Some(vec![0x08, 0xD1, 0x0F]),
        callback_handler: 73,
        ..TRequest::default()
    });
    let (mut client, mut server) = duplex(1_048_576);
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();
    process_game_login_frame_with_catalog_mut(
        &mut server,
        &state,
        Some(&mut account),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let response = TMessageCodec::decode_response(
        &NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap()
            .payload,
    )
    .unwrap();
    assert_eq!(response.err, 0);
    assert_eq!(response.method, "copy.GetRecord");
    assert!(CopyRecordListCodec::decode_copy_id(response.ret.as_deref().unwrap()).is_some());

    let delete = TMessageCodec::encode_request(&TRequest {
        method: "copy.DeleteRecord".to_owned(),
        args: Some(vec![0x08, 0xD1, 0x0F, 0x10, 0x00]),
        callback_handler: 74,
        ..TRequest::default()
    });
    let (mut client, mut server) = duplex(1_048_576);
    NetSocketFrameCodec::write(&mut client, 0, &delete)
        .await
        .unwrap();
    process_game_login_frame_with_catalog_mut(
        &mut server,
        &state,
        Some(&mut account),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let response = TMessageCodec::decode_response(
        &NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap()
            .payload,
    )
    .unwrap();
    assert_eq!(response.err, 0);
    assert!(account["copyRecords"].as_array().unwrap().is_empty());
}

#[test]
fn bootstrap_routes_return_client_login_payloads() {
    let state = ServerState::new("local-player", "Captain", "1.4.0");
    let (status, _, _, body) = bootstrap_response(
        "GET /phone/serverlist/1 HTTP/1.1",
        Some("sdk.example"),
        &state,
        Some(7201),
    );
    assert_eq!(status, 200);
    let payload: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(payload["errornu"], "0");
    assert_eq!(payload["root"]["item"][0]["port"], 7201);

    let mut state = state;
    state.level = 42;
    let (_, _, _, body) =
        bootstrap_response("GET /phone/loginrole/1 HTTP/1.1", None, &state, Some(7201));
    let payload: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(payload["root"]["role"][0]["level"], 42);

    let (_, _, _, body) = bootstrap_response("GET /login?x=1 HTTP/1.1", None, &state, None);
    let payload: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(payload["Pid"], "local-player");

    let (_, _, content_type, body) =
        bootstrap_response("GET /sdk/gettime HTTP/1.1", None, &state, None);
    assert_eq!(content_type, "application/json; charset=utf-8");
    let payload: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(payload["time"].is_number());

    state.version = "1.4.0\"\\unsafe".to_owned();
    let (_, _, _, body) =
        bootstrap_response("GET /phone/getversion/jp HTTP/1.1", None, &state, None);
    let payload: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(payload["script"][0]["src_version"], "1.4.0\"\\unsafe");

    let (status, _, _, body) = bootstrap_response(
        "GET /phone/platform/getPlatformExt/ HTTP/1.1",
        None,
        &state,
        None,
    );
    assert_eq!(status, 200);
    let payload: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(payload["errornu"], "0");
    assert!(payload["data"].is_object());
}

#[test]
fn preparing_mutation_does_not_change_original_state() {
    let state = ServerState::new("slot-a", "Captain", "1.4.0");
    let request = br#"{"type":"battle_result","requestId":"1","payload":{"stageId":1,"win":true}}"#;

    let (_response, candidate) = prepare_local_request(&state, request).unwrap();

    assert_eq!(state.fuel, 100);
    assert_eq!(state.coins, 0);
    assert_eq!(candidate.fuel, 90);
    assert_eq!(candidate.coins, 100);
}

#[test]
fn bundled_battle_catalog_stage_reference_audit() {
    let config_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("catalog/config");
    let chapters = load_chapter_catalog(Some(&config_dir));
    let battle = load_battle_catalog(Some(&config_dir));
    let families = [
        ("sea", &chapters.sea),
        ("mubar", &chapters.mubar),
        ("daily", &chapters.daily),
        ("goods", &chapters.goods_copy),
        ("equip_test", &chapters.equip_new_test),
    ];
    let mut missing_stage_refs = Vec::new();
    for (family, copy_ids) in families {
        for copy_id in copy_ids {
            if !battle.copies.contains_key(copy_id) {
                missing_stage_refs.push((family, *copy_id));
            }
        }
    }
    assert!(
        missing_stage_refs.is_empty(),
        "chapter stage ids missing config_copy rows: {missing_stage_refs:?}"
    );

    let mut missing_fleets = Vec::new();
    let mut missing_enemies = Vec::new();
    for (copy_id, copy) in &battle.copies {
        let mut fleet_ids = copy.fleet_ids.clone();
        let mut index = 0;
        while index < fleet_ids.len() {
            let fleet_id = fleet_ids[index];
            if let Some(attached) = battle.attached_fleet_ids.get(&fleet_id) {
                fleet_ids.extend(attached.iter().copied());
            }
            index += 1;
        }
        fleet_ids.sort_unstable();
        fleet_ids.dedup();
        for fleet_id in fleet_ids {
            let Some(enemy_ids) = battle.fleet_enemies.get(&fleet_id) else {
                missing_fleets.push((*copy_id, fleet_id));
                continue;
            };
            for enemy_id in enemy_ids {
                if !battle.enemies.contains_key(enemy_id) {
                    missing_enemies.push((*copy_id, fleet_id, *enemy_id));
                }
            }
        }
    }
    assert!(
        missing_fleets.is_empty(),
        "battle stage fleets missing config_fleet.copy_enemys: {missing_fleets:?}"
    );
    assert!(
        missing_enemies.is_empty(),
        "battle fleets reference missing config_ship_enemy rows: {missing_enemies:?}"
    );
    eprintln!(
        "battle-stage-audit copies={} plot={} sea={} mubar={} daily={} goods={} tower={} equip_test={} fleets={} enemies={} daily_groups={} search_3d={}",
        battle.copies.len(),
        chapters.plot.len(),
        chapters.sea.len(),
        chapters.mubar.len(),
        chapters.daily.len(),
        chapters.goods_copy.len(),
        chapters.tower.len(),
        chapters.equip_new_test.len(),
        battle.fleet_enemies.len(),
        battle.enemies.len(),
        battle.daily_group_by_copy.len(),
        battle.search_3d.len(),
    );
}

#[test]
fn bundled_all_chapter_battle_nodes_have_complete_start_payloads() {
    let catalog_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/config");
    let chapters = load_chapter_catalog(Some(&catalog_dir));
    let battle = load_battle_catalog(Some(&catalog_dir));
    let account = json!({
        "character": {"uid": 9, "level": 60, "name": "A"},
        "dock": {"heroes": [{"heroId": 1, "templateId": 100, "level": 10}]}
    });
    let families = [
        ("plot", &chapters.plot),
        ("sea", &chapters.sea),
        ("mubar", &chapters.mubar),
        ("daily", &chapters.daily),
        ("goods", &chapters.goods_copy),
        ("tower", &chapters.tower),
        ("equip_test", &chapters.equip_new_test),
    ];
    let mut checked = std::collections::BTreeSet::new();
    let mut missing_combat_refs: Vec<(&str, i32)> = Vec::new();
    let mut noncombat_refs = std::collections::BTreeMap::<&str, Vec<i32>>::new();

    for (family, copy_ids) in families {
        for copy_id in copy_ids {
            let Some(_copy) = battle.copies.get(copy_id) else {
                if matches!(family, "sea" | "mubar" | "daily" | "goods" | "equip_test") {
                    missing_combat_refs.push((family, *copy_id));
                } else {
                    noncombat_refs.entry(family).or_default().push(*copy_id);
                }
                continue;
            };
            if !checked.insert(*copy_id) {
                continue;
            }

            let session_fleet_ids = battle_session_fleet_ids(*copy_id, Some(&battle));
            assert!(
                !session_fleet_ids.is_empty(),
                "{family} copy {copy_id} has no battle fleets"
            );
            assert!(
                session_fleet_ids
                    .iter()
                    .any(|fleet_id| { battle.fleet_is_last.get(fleet_id).copied() == Some(true) }),
                "{family} copy {copy_id} has no final fleet: {session_fleet_ids:?}"
            );

            for fleet_id in &session_fleet_ids {
                let enemy_ids = battle
                    .fleet_enemies
                    .get(fleet_id)
                    .unwrap_or_else(|| panic!("{family} copy {copy_id} fleet {fleet_id} missing"));
                assert!(!enemy_ids.is_empty());
                for enemy_id in enemy_ids {
                    assert!(
                        battle.enemies.contains_key(enemy_id),
                        "{family} copy {copy_id} fleet {fleet_id} enemy {enemy_id} missing stats"
                    );
                }
            }

            let payload = battle_start_payload(&account, *copy_id, &[1], Some(&battle));
            assert_eq!(decode_repeated_varint_field(&payload, 5), session_fleet_ids);
            let enemy_fleets = decode_repeated_message_field(&payload, 24);
            assert_eq!(enemy_fleets.len(), session_fleet_ids.len());
            for (index, fleet) in session_fleet_ids.iter().enumerate() {
                let wire_fleet_id =
                    battle_position_fleet_id(*copy_id, *fleet, index, Some(&battle));
                assert_eq!(decode_varint_field(&enemy_fleets[index], 1), wire_fleet_id);
                assert_eq!(
                    decode_repeated_message_field(&enemy_fleets[index], 3).len(),
                    battle.fleet_enemies[fleet].len(),
                    "{family} copy {copy_id} fleet {fleet} enemy payload count mismatch"
                );
            }
        }
    }

    assert!(
        missing_combat_refs.is_empty(),
        "combat chapter nodes missing battle configs: {missing_combat_refs:?}"
    );
    eprintln!(
        "chapter-battle-audit checked={} missing_combat={} noncombat_plot={} noncombat_tower={}",
        checked.len(),
        missing_combat_refs.len(),
        noncombat_refs.get("plot").map_or(0, Vec::len),
        noncombat_refs.get("tower").map_or(0, Vec::len),
    );
}
