use blueoath_protocol::{
    AttrIntensify, BagGrid, BagInfo, BagInfoCodec, BuildingInfo, BuildingLandInfo, BuildingTactic,
    ClientGameWireCodec, CopyInfoCodec, CopyRecord, CopyRecordEquip, CopyRecordHero,
    CopyRecordList, CopyRecordListCodec, DailyCopyCodec, DailyCopyGroupProgress, DailyCopyProgress,
    EquipInfo, EquipList, EquipListCodec, EquipPSkill, FashionInfo, FashionList, FashionListCodec,
    FleetInfo, FleetInfoCodec, FleetTactic, GameLoginCodec, GameLoginFrame, GameLoginFrameCodec,
    GuideInfoCodec, HeroBag, HeroBagCodec, HeroGrid, MedalAcquiredTime, PSkillEntry, PresetFleet,
    PresetFleetCodec, PresetFleetInfo, TArgLogin, TMessageCodec, TRequest, TResponse, TRetLogin,
    UserBuildingInfo, UserBuildingInfoCodec, UserInfo, UserInfoCodec, UserListCodec,
    UserLoginCodec,
};

#[test]
fn initial_guide_progress_skips_startup_but_keeps_feature_guides() {
    let payload = GuideInfoCodec::encode_initial_progress_completed();

    assert!(payload
        .windows(b"GUIDE_DONE_STAGES".len())
        .any(|window| window == b"GUIDE_DONE_STAGES"));
    for stage_id in ["10000", "100000", "1000000", "99995", "99998", "99992"] {
        let stage_id = stage_id.as_bytes();
        assert!(payload
            .windows(stage_id.len())
            .any(|window| window == stage_id));
    }
    assert!(!payload
        .windows(b"1200000".len())
        .any(|window| window == b"1200000"));
    assert!(!payload
        .windows(b"160000".len())
        .any(|window| window == b"160000"));
}

#[test]
fn request_encoding_matches_csharp_wire() {
    let request = TRequest {
        method: "player.Login".to_owned(),
        args: Some(vec![0x0A, 0x03, b'a', b'b', b'c']),
        callback_handler: 42,
        token: "tok".to_owned(),
    };
    assert_eq!(
        TMessageCodec::encode_request(&request),
        vec![
            0x0A, 0x0C, b'p', b'l', b'a', b'y', b'e', b'r', b'.', b'L', b'o', b'g', b'i', b'n',
            0x12, 0x05, 0x0A, 0x03, b'a', b'b', b'c', 0x18, 0x2A, 0x22, 0x03, b't', b'o', b'k'
        ]
    );
}

#[test]
fn request_decode_skips_unknown_fields() {
    let mut wire = TMessageCodec::encode_request(&TRequest {
        method: "user.UserLogin".to_owned(),
        ..TRequest::default()
    });
    wire.extend_from_slice(&[0x28, 0x7B]);
    let decoded = TMessageCodec::decode_request(&wire).unwrap();
    assert_eq!(decoded.method, "user.UserLogin");
    assert_eq!(decoded.callback_handler, 0);
}

#[test]
fn response_round_trip_preserves_all_fields() {
    let response = TResponse {
        err: 3,
        err_msg: "bad".to_owned(),
        method: "player.Login".to_owned(),
        ret: Some(vec![1, 2, 3]),
        callback_handler: 9,
        time: 123,
        token: "tok".to_owned(),
        seq: 4,
        is_response: 1,
    };
    assert_eq!(
        TMessageCodec::decode_response(&TMessageCodec::encode_response(&response)).unwrap(),
        response
    );
}

#[test]
fn copy_info_progress_marks_only_passed_stages() {
    let payload = CopyInfoCodec::encode_with_progress(2, &[1001, 1002], 1001, &[1001]);

    // First stage: StarLevel=7 and FirstPassTime=1.
    assert!(payload.windows(2).any(|window| window == [0x18, 0x07]));
    assert!(payload.windows(2).any(|window| window == [0x30, 0x01]));
    // Second stage: StarLevel=0 and FirstPassTime=0.
    assert!(payload.windows(2).any(|window| window == [0x18, 0x00]));
    assert!(payload.windows(2).any(|window| window == [0x30, 0x00]));
    // MaxCopyId follows progress, not catalog tail.
    assert!(payload.ends_with(&[0x10, 0xE9, 0x07, 0x18, 0x02]));
}

#[test]
fn copy_info_progress_emits_saved_star_level() {
    let payload =
        CopyInfoCodec::encode_with_progress_and_stars(2, &[1001], 1001, &[1001], &[(1001, 2)]);

    assert!(payload.windows(2).any(|window| window == [0x18, 0x02]));
    assert!(!payload.windows(2).any(|window| window == [0x18, 0x07]));
}

#[test]
fn sea_copy_info_encodes_selected_difficulty() {
    let payload = CopyInfoCodec::encode_with_progress_and_difficulty(&[1001], 1001, &[1001], 6);

    // Sea difficulty choice is TBaseInfo.SfLvChoose (field 12).
    assert!(payload.windows(2).any(|window| window == [0x60, 0x06]));
}

#[test]
fn sea_copy_info_encodes_repeat_clear_count() {
    let payload = CopyInfoCodec::encode_with_progress_and_difficulty_and_counts(
        &[1001],
        1001,
        &[1001],
        &[(1001, 3)],
        1,
    );

    // TUserCopyInfo.PassCopyCount -> copy id 1001, count 3.
    assert!(payload.windows(2).any(|window| window == [0x10, 0x03]));
}

#[test]
fn game_login_frame_uses_payload_plus_four_length() {
    let frame = GameLoginFrame {
        operation: 2,
        payload: vec![0xAA, 0xBB],
    };
    assert_eq!(
        GameLoginFrameCodec::encode(&frame).unwrap(),
        vec![0, 0, 0, 6, 0, 0, 0, 2, 0xAA, 0xBB]
    );
}

#[test]
fn login_payload_and_response_match_csharp_wire() {
    let login = TArgLogin {
        pid: "frame-player".to_owned(),
        timestamp: 1,
        open_date_time: "open".to_owned(),
        hash: "hash".to_owned(),
        sample_info: None,
    };
    assert_eq!(
        GameLoginCodec::encode_login(&login),
        vec![
            0x0A, 0x0C, b'f', b'r', b'a', b'm', b'e', b'-', b'p', b'l', b'a', b'y', b'e', b'r',
            0x10, 0x01, 0x1A, 0x04, b'o', b'p', b'e', b'n', 0x22, 0x04, b'h', b'a', b's', b'h'
        ]
    );
    let response = GameLoginCodec::encode_response(&TRetLogin {
        ret: "ok".to_owned(),
        feign_role_id: "frame-player".to_owned(),
        err_code: 0,
    });
    assert_eq!(response.last(), Some(&0));
    assert_eq!(
        GameLoginCodec::decode_login_response(&response)
            .unwrap()
            .ret,
        "ok"
    );
}

#[test]
fn user_login_response_matches_csharp_wire() {
    assert_eq!(
        UserLoginCodec::encode_response("ok", "", 0),
        vec![0x0A, 0x02, b'o', b'k']
    );
}

#[test]
fn player_user_list_uses_tuser_info_field_layout() {
    let user = UserInfo {
        uid: 1,
        uname: "Captain".to_owned(),
        level: 2,
        class_id: 3,
        ..UserInfo::default()
    };
    assert_eq!(
        UserListCodec::encode(&[user]),
        vec![
            0x0A, 0x0F, 0x08, 0x01, 0x12, 0x07, b'C', b'a', b'p', b't', b'a', b'i', b'n', 0x18,
            0x02, 0x20, 0x03
        ]
    );
}

#[test]
fn daily_copy_codec_uses_dynamic_chapter_and_group_ids() {
    let payload = DailyCopyCodec::encode(&[(20001, 7), (20101, 8)], &[7, 8]);
    assert!(payload
        .windows(3)
        .any(|window| window == [0x08, 0xA1, 0x9C]));
    assert!(payload
        .windows(3)
        .any(|window| window == [0x08, 0x85, 0x9D]));
    assert!(payload.windows(2).any(|window| window == [0x08, 0x07]));
    assert!(payload.windows(2).any(|window| window == [0x08, 0x08]));
}

#[test]
fn daily_copy_codec_preserves_existing_progress() {
    let payload = DailyCopyCodec::encode_with_progress(
        &[(20001, 7)],
        &[7],
        &[DailyCopyProgress {
            chapter_id: 20001,
            challenge_times: 2,
            pass_copy: vec![20101, 20102],
            select_ex: true,
            ex_star: 5,
        }],
        &[DailyCopyGroupProgress {
            group_id: 7,
            success_times: 3,
        }],
        &[DailyCopyGroupProgress {
            group_id: 7,
            success_times: 4,
        }],
    );
    assert!(payload.windows(2).any(|window| window == [0x10, 0x02]));
    assert!(payload
        .windows(3)
        .any(|window| window == [0x18, 0x85, 0x9D]));
    assert!(payload.windows(2).any(|window| window == [0x20, 0x01]));
    assert!(payload.windows(2).any(|window| window == [0x28, 0x05]));
    assert!(payload.windows(2).any(|window| window == [0x10, 0x03]));
}

#[test]
fn client_game_wire_preserves_little_endian_session_header() {
    let packet = ClientGameWireCodec::encode_client_request(2, &[1, 2], 42, 3);
    assert_eq!(&packet[..11], &[0, 2, 42, 0, 0, 0, 0, 0, 0, 0, 3]);
    let decoded = ClientGameWireCodec::decode_client_request(&packet).unwrap();
    assert_eq!(decoded.channel, 0);
    assert_eq!(decoded.operation, 2);
    assert_eq!(decoded.session_id, 42);
    assert_eq!(decoded.state, 3);
    assert_eq!(decoded.payload, vec![1, 2]);
}

#[test]
fn client_game_wire_encodes_server_operation_envelope() {
    let packet = ClientGameWireCodec::encode_server_response(2, b"ret");
    assert_eq!(&packet[..2], &[0, 5]);
    assert!(packet.windows(2).any(|window| window == [0x18, 0x02]));
    assert!(packet.windows(3).any(|window| window == [0x22, 0x03, b'r']));
}

#[test]
fn user_info_codec_preserves_csharp_account_fields() {
    let value = UserInfo {
        uid: 1,
        uname: "Alice".to_owned(),
        level: 80,
        class_id: 1,
        secretary_id: 1,
        create_time: 123,
        gold: 99,
        diamond: 88,
        supply: 77,
        pve_pt: 100,
        new_task_stage: 7,
        server_id: 1,
        medal_acquired_times: vec![MedalAcquiredTime {
            medal_id: 168226,
            time: 1_725_000_000,
        }],
        ..UserInfo::default()
    };

    let decoded = UserInfoCodec::decode(&UserInfoCodec::encode(&value)).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn hero_bag_codec_emits_client_safe_grid_fields() {
    let payload = HeroBagCodec::encode(&HeroBag {
        heroes: vec![HeroGrid {
            hero_id: 1,
            template_id: 10210511,
            level: 1,
            equip_slots: vec![1, 0, 2, 0, 0, 0],
            cur_hp: 10000000000,
            lock: true,
            ..HeroGrid::default()
        }],
        bag_size: 200,
    });

    assert!(payload.starts_with(&[0x0A]));
    assert!(payload
        .windows(3)
        .any(|window| window == [0x10, 0xC8, 0x01]));
    assert!(payload.contains(&0x12));
    assert!(payload.contains(&0x6A));
}

#[test]
fn hero_bag_codec_emits_empty_name_field() {
    let payload = HeroBagCodec::encode(&HeroBag {
        heroes: vec![HeroGrid::default()],
        ..HeroBag::default()
    });
    assert!(payload.windows(2).any(|window| window == [0x7A, 0x00]));
}

#[test]
fn hero_bag_codec_preserves_skill_and_intensify_entries() {
    let payload = HeroBagCodec::encode(&HeroBag {
        heroes: vec![HeroGrid {
            pskills: vec![PSkillEntry {
                pskill_id: 50001,
                pskill_exp: 4,
                level: 2,
                replace: 1,
            }],
            intensify: vec![AttrIntensify {
                attr_type: 8,
                intensify_level: 3,
                cur_exp: 1500,
            }],
            ..HeroGrid::default()
        }],
        ..HeroBag::default()
    });

    assert!(payload.contains(&0x3A));
    assert!(payload.contains(&0x6A));
    assert!(payload
        .windows(3)
        .any(|window| window == [0x08, 0xD1, 0x86]));
}

#[test]
fn bag_info_codec_always_emits_item_count() {
    let payload = BagInfoCodec::encode(&BagInfo {
        bag_type: 1,
        bag_size: 100,
        items: vec![BagGrid {
            template_id: 3001,
            num: 0,
        }],
    });

    assert!(payload.windows(2).any(|window| window == [0x08, 0x01]));
    assert!(payload.contains(&0x64));
    assert!(payload.windows(2).any(|window| window == [0x08, 0xB9]));
    assert!(payload.windows(2).any(|window| window == [0x10, 0x64]));
    assert!(payload.windows(2).any(|window| window == [0x10, 0x00]));
}

#[test]
fn fashion_list_codec_emits_repeated_fashion_ids() {
    let payload = FashionListCodec::encode(&FashionList {
        items: vec![FashionInfo {
            sf_id: 1021051,
            fashion_tids: vec![1021051, 1021052],
        }],
    });
    assert!(payload.starts_with(&[0x0A]));
    assert!(payload
        .windows(3)
        .any(|window| window == [0x08, 0xFB, 0xA8]));
    assert!(payload
        .windows(3)
        .any(|window| window == [0x10, 0xFB, 0xA8]));
}

#[test]
fn equip_list_codec_emits_required_zero_fields_and_skills() {
    let payload = EquipListCodec::encode(&EquipList {
        bag_size: 2000,
        items: vec![EquipInfo {
            equip_id: 1,
            template_id: 30091,
            hero_id: 1,
            pskills: vec![EquipPSkill {
                pskill_id: 50001,
                level: 2,
            }],
            ..EquipInfo::default()
        }],
        ..EquipList::default()
    });
    assert!(payload
        .windows(3)
        .any(|window| window == [0x08, 0xD0, 0x0F]));
    assert!(payload.contains(&0x12));
    assert!(payload
        .windows(3)
        .any(|window| window == [0x08, 0xD1, 0x86]));
}

#[test]
fn building_and_fleet_codecs_emit_required_snapshots() {
    let building = UserBuildingInfoCodec::encode(&UserBuildingInfo {
        buildings: vec![BuildingInfo {
            id: 1,
            template_id: 2,
            level: 3,
            hero_ids: vec![7],
            status: 1,
            ..BuildingInfo::default()
        }],
        lands: vec![BuildingLandInfo {
            index: 1,
            building_id: 1,
        }],
        worker_strength: 1_000_000,
        worker_recover: 10,
        food_max: 100,
        electric_max: 100,
        ..UserBuildingInfo::default()
    });
    assert!(building.contains(&0x0A));
    assert!(building.contains(&0x12));
    assert!(building.contains(&0x18));

    let fleet = FleetInfoCodec::encode(&FleetInfo {
        tactics: vec![FleetTactic {
            tactic_name: "Fleet".to_owned(),
            hero_ids: vec![1, 2],
            mode_id: 1,
            strategy_id: 2,
            formation_id: 2,
            tactic_type: 1,
            ..FleetTactic::default()
        }],
        ..FleetInfo::default()
    });
    assert!(fleet.starts_with(&[0x0A]));
    assert!(fleet.contains(&0x0A));
    assert!(fleet.contains(&0x10));
}

#[test]
fn preset_fleet_codec_round_trips_client_shape() {
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

    assert_eq!(
        PresetFleetCodec::decode(&PresetFleetCodec::encode(&value)).unwrap(),
        value
    );
}

#[test]
fn copy_record_list_codec_matches_history_fleet_shape() {
    let value = CopyRecordList {
        copy_id: 2001,
        records: vec![CopyRecord {
            uid: 42,
            user_name: "Captain".to_owned(),
            level: 80,
            pass_time: 37,
            secret_id: 0,
            strategy_id: 17,
            tactics: vec![CopyRecordHero {
                template_id: 10210511,
                level: 80,
                advance_level: 3,
                cur_hp: 9_000,
                equips: vec![CopyRecordEquip {
                    template_id: 30091,
                    level: 5,
                    star_level: 2,
                }],
                point: 0,
            }],
            power: 1234,
            record_time: 99,
            ex_buff: vec![7001, 7002],
        }],
    };
    let payload = CopyRecordListCodec::encode(&value);
    assert!(payload.starts_with(&[0x08, 0xD1, 0x0F]));
    assert!(payload
        .windows(b"Captain".len())
        .any(|window| window == b"Captain"));
    assert!(payload.contains(&0x3A));
}

#[test]
fn building_codec_preserves_production_and_tactic_state() {
    let payload = UserBuildingInfoCodec::encode(&UserBuildingInfo {
        buildings: vec![BuildingInfo {
            id: 7,
            template_id: 41,
            level: 2,
            productivity: 12,
            produce_speed: 13,
            product_count: 14,
            recipe_id: 15,
            item_count: 16,
            last_mood_update_time: 17,
            recipe_time: 18,
            float_count: 19,
            tactic_list: vec![BuildingTactic {
                building_id: 7,
                name: "Plan A".to_owned(),
                hero_ids: vec![101, 102],
                index: 1,
            }],
            ..BuildingInfo::default()
        }],
        ..UserBuildingInfo::default()
    });

    assert!(payload.windows(2).any(|window| window == [0x28, 12]));
    assert!(payload.windows(2).any(|window| window == [0x30, 13]));
    assert!(payload.windows(2).any(|window| window == [0x38, 14]));
    assert!(payload.windows(2).any(|window| window == [0x50, 15]));
    assert!(payload.windows(2).any(|window| window == [0x58, 16]));
    assert!(payload.windows(2).any(|window| window == [0x60, 17]));
    assert!(payload.windows(2).any(|window| window == [0x78, 18]));
    assert!(payload.windows(3).any(|window| window == [0x80, 0x01, 19]));
    assert!(payload.contains(&0x8A));
    assert!(payload
        .windows(b"Plan A".len())
        .any(|window| window == b"Plan A"));
}
