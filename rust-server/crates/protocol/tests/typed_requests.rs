use blueoath_protocol::{
    ActivityCodeExchangeRequest, ActivityExchangeRewardRequest, ActivityExtractDrawRequest,
    ActivityFormulaRequest, ActivityItemIdRequest, ActivityRewardIndexRequest,
    ActivitySelectShipRequest, ActivitySelectTeamRequest, AlchemyRequest, BathroomAutoRequest,
    BathroomRequest, BathroomServiceRequest, BathroomStartAllRequest, BathroomStartEntry,
    BattleAutoMessageRequest, BattlePassLevelRequest, BattlePassRefreshRequest,
    BattlePassRewardRequest, BattlePassTaskRewardRequest, BattlePassTypeRequest,
    BigActivityRankRequest, BirthdayFeedRequest, BuildShipRequest, BuildShipRewardRequest,
    ChangeNameRequest, ChangeWorldChannelRequest, ChristmasBuyBlindBoxRequest,
    ChristmasBuyItemRequest, CoopChangeChapterRequest, CoopCreateRoomRequest, CoopKickRequest,
    CoopMatchTypeRequest, CoopPasswordRequest, CoopRoomHeroesRequest, CoopRoomIdRequest,
    CopyAttackRequest, CopyIdRequest, CopyPassBaseRequest, CopyPassRequest, CopyRecordRequest,
    CopyRewardCountRequest, CopyStartRequest, DailyCopyEnterRequest, DailyCopySelectExRequest,
    Decode, DiscussRequest, EquipRiseStarRequest, ExchangeRequest, FashionEquipRequest,
    FashionPurchaseRequest, FleetInfo, FleetTactic, FoodComposeRequest, FriendSearchRequest,
    FriendTargetRequest, GetBarrageByIdRequest, GuideSettingEntry, GuideSettingRequest,
    GuildActivityPresentRequest, GuildBoxAnonymousRequest, GuildBoxIdRequest, GuildCreateRequest,
    GuildIdRequest, GuildListRequest, GuildModifyRequest, GuildOfferRequest, GuildSearchRequest,
    GuildTaskDonateRequest, GuildTaskIdRequest, GuildTaskMemberRequest, GuildWarBaseRequest,
    GuildWarScoreRequest, HeroAwakenFinishRequest, HeroAwakenRewardRequest, HeroChangeEquipRequest,
    InviteRecordVersionRequest, InviteStateTypeRequest, OutpostBuildingRequest,
    OutpostSetHeroRequest, PaperCutRequest, ProtocolError, SeaDifficultyRequest,
    SendBarrageRequest, SendMessageRequest, SetHeadFrameRequest, SetHeadRequest, SetMessageRequest,
    SetSecretaryRequest, ShipTaskCurrentShipRequest, ShipTaskRewardRequest, SignDayRequest,
    SportsMeetPointsRequest, StudyProgressRequest, StudySpeedupItem, StudySpeedupRequest,
    StudyStartRequest, TaskAllRewardRequest, TaskRewardRequest, TeachingUserRequest,
    ValentineRewardRequest, WorldEventStageRequest,
};

#[test]
fn decodes_copy_start_request_with_typed_fields_and_repeated_buffs() {
    let payload = vec![
        0x10, 0x2a, // copy_id = 42
        0x18, 0x01, // is_running_fight
        0x48, 0x03, // battle_mode = 3
        0x50, 0x02, // anim_mode = 2
        0x60, 0x07, // ex_buff = 7
        0x60, 0x09, // ex_buff = 9
        0x88, 0x01, 0x01, // is_pve_pt_mode
        0x7a, 0x01, 0x01, // unknown length-delimited field
    ];

    let request = CopyStartRequest::decode(&payload).unwrap();
    assert_eq!(request.copy_id, 42);
    assert!(request.is_running_fight);
    assert_eq!(request.battle_mode, 3);
    assert_eq!(request.anim_mode, 2);
    assert_eq!(request.ex_buffs, vec![7, 9]);
    assert!(request.is_pve_pt_mode);
}

#[test]
fn rejects_missing_or_duplicate_required_copy_id() {
    assert!(matches!(
        CopyStartRequest::decode(&[]),
        Err(ProtocolError::Invalid(
            "copy start request is missing copy id"
        ))
    ));
    assert!(matches!(
        CopyStartRequest::decode(&[0x10, 1, 0x10, 2]),
        Err(ProtocolError::Invalid(
            "copy start request has duplicate copy id"
        ))
    ));
}

#[test]
fn decodes_typed_user_profile_updates() {
    assert_eq!(
        SetSecretaryRequest::decode(&[0x08, 0x2a])
            .unwrap()
            .secretary_id,
        42
    );
    assert_eq!(SetHeadRequest::decode(&[0x10, 0x09]).unwrap().head, 9);
    assert_eq!(
        SetHeadFrameRequest::decode(&[0x08, 0x0b])
            .unwrap()
            .head_frame,
        11
    );
    assert_eq!(
        ChangeNameRequest::decode(&[0x0a, 0x03, b'C', b'a', b'p'])
            .unwrap()
            .name,
        "Cap"
    );
    assert_eq!(
        SetMessageRequest::decode(&[0x0a, 0x02, b'h', b'i'])
            .unwrap()
            .message,
        "hi"
    );
}

#[test]
fn rejects_missing_duplicate_and_oversized_user_fields() {
    assert!(matches!(
        SetSecretaryRequest::decode(&[]),
        Err(ProtocolError::Invalid("secretary request is missing id"))
    ));
    assert!(matches!(
        SetSecretaryRequest::decode(&[0x08, 1, 0x08, 2]),
        Err(ProtocolError::Invalid("secretary request has duplicate id"))
    ));

    let mut oversized = vec![0x0a, 65];
    oversized.extend(std::iter::repeat_n(b'x', 65));
    assert!(matches!(
        ChangeNameRequest::decode(&oversized),
        Err(ProtocolError::Invalid("name is too long"))
    ));
}

#[test]
fn decodes_typed_copy_and_daily_requests() {
    let daily = DailyCopyEnterRequest::decode(&[0x08, 1, 0x10, 2, 0x18, 3]).unwrap();
    assert_eq!(
        (daily.chapter_id, daily.copy_id, daily.tactic_id),
        (1, 2, 3)
    );

    let record = CopyRecordRequest::decode(&[0x08, 7, 0x10, 2]).unwrap();
    assert_eq!(record.index, 2);

    let sea = SeaDifficultyRequest::decode(&[0x08, 9, 0x10, 3]).unwrap();
    assert_eq!((sea.copy_id, sea.difficulty), (9, 3));
}

#[test]
fn rejects_invalid_typed_copy_requests() {
    assert!(matches!(
        DailyCopyEnterRequest::decode(&[0x08, 1, 0x10, 2]),
        Err(ProtocolError::Invalid(
            "daily copy request is missing tactic id"
        ))
    ));
    assert!(matches!(
        SeaDifficultyRequest::decode(&[0x08, 9, 0x10, 8]),
        Err(ProtocolError::Invalid("sea request has invalid value"))
    ));
}

#[test]
fn decodes_typed_battle_attack_and_pass_requests() {
    let attack = CopyAttackRequest::decode(&[0x08, 1, 0x10, 9, 0x18, 2, 0x18, 3, 0x20, 7]).unwrap();
    assert_eq!(
        (
            attack.attack_type,
            attack.copy_id,
            attack.hero_ids,
            attack.enemy_id
        ),
        (1, 9, vec![2, 3], 7)
    );

    let mut pass = vec![0x40, 3, 0x60, 12, 0x48, 2];
    pass.extend([0x92, 0x01, 0x02, 0x08, 0x01]);
    pass.extend([0xa2, 0x01, 0x02, 0x08, 0x09]);
    let decoded = CopyPassRequest::decode(&pass).unwrap();
    assert_eq!((decoded.grade, decoded.battle_time), (3, 12));
    assert_eq!(decoded.mvp_hero_id, Some(2));
    assert_eq!(decoded.heroes[0].hero_id, 1);
    assert_eq!(decoded.passed_fleet_ids, vec![9]);
}

#[test]
fn rejects_invalid_typed_battle_requests() {
    assert!(matches!(
        CopyAttackRequest::decode(&[]),
        Err(ProtocolError::Invalid("copy attack is missing attack type"))
    ));
    assert!(matches!(
        CopyAttackRequest::decode(&[0x08, 1, 0x10, 9, 0x18, 2, 0x18, 2, 0x20, 7]),
        Err(ProtocolError::Invalid(
            "copy attack hero ids are duplicated"
        ))
    ));
}

#[test]
fn decodes_typed_task_reward_request() {
    let request = TaskRewardRequest::decode(&[0x08, 101, 0x10, 1]).unwrap();
    assert_eq!(request.task_id, 101);
    assert_eq!(request.task_type, 1);
    assert!(TaskRewardRequest::decode(&[]).is_err());
    assert_eq!(
        TaskAllRewardRequest::decode(&[0x08, 2])
            .unwrap()
            .reward_type,
        2
    );
}

#[test]
fn decodes_typed_social_requests_and_daily_selection() {
    let target = FriendTargetRequest::decode(&[0x08, 42]).unwrap();
    assert_eq!(target.uid, 42);
    let search = FriendSearchRequest::decode(&[0x08, 42]).unwrap();
    assert_eq!((search.uid, search.name), (42, String::new()));
    let select = DailyCopySelectExRequest::decode(&[0x08, 1, 0x10, 1]).unwrap();
    assert_eq!((select.chapter_id, select.select_ex), (1, true));
}

#[test]
fn rejects_invalid_typed_social_requests() {
    assert!(matches!(
        FriendTargetRequest::decode(&[]),
        Err(ProtocolError::Invalid("friend request is missing target"))
    ));
    assert!(matches!(
        FriendSearchRequest::decode(&[]),
        Err(ProtocolError::Invalid("friend search requires uid or name"))
    ));
    assert!(matches!(
        DailyCopySelectExRequest::decode(&[0x08, 0]),
        Err(ProtocolError::Invalid(
            "daily copy select chapter is invalid"
        ))
    ));
}

#[test]
fn decodes_typed_chat_requests_with_length_limits() {
    let message = SendMessageRequest::decode(&[
        0x08, 3, 0x10, 7, 0x1a, 2, b'h', b'i', 0x20, 1, 0x2a, 1, b'v',
    ])
    .unwrap();
    assert_eq!(message.channel, 3);
    assert_eq!(message.receive_uid, 7);
    assert_eq!(message.message, "hi");
    assert_eq!(message.message_type, 1);
    assert_eq!(message.voice, "v");
    assert_eq!(
        ChangeWorldChannelRequest::decode(&[0x08, 2])
            .unwrap()
            .channel,
        2
    );
    assert_eq!(
        SendBarrageRequest::decode(&[0x08, 9, 0x1a, 3, b'w', b'a', b'v'])
            .unwrap()
            .id,
        9
    );
    assert_eq!(
        GetBarrageByIdRequest::decode(&[0x08, 9, 0x10, 2, 0x18, 10]).unwrap(),
        GetBarrageByIdRequest {
            id: 9,
            begin: 2,
            len: 10,
        }
    );
    assert_eq!(
        BuildShipRequest::decode(&[0x08, 2, 0x10, 10]).unwrap(),
        BuildShipRequest {
            pool_id: 2,
            pulls: 10,
        }
    );
    assert_eq!(
        BuildShipRewardRequest::decode(&[0x08, 2, 0x10, 10]).unwrap(),
        BuildShipRewardRequest {
            pool_id: 2,
            milestone: 10,
        }
    );
    assert_eq!(
        GuildBoxIdRequest::decode(&[0x08, 77]).unwrap(),
        GuildBoxIdRequest { box_id: 77 }
    );
    assert_eq!(
        GuildBoxAnonymousRequest::decode(&[0x08, 1]).unwrap(),
        GuildBoxAnonymousRequest { anonymous: true }
    );
    assert_eq!(
        OutpostSetHeroRequest::decode(&[0x08, 1, 0x10, 2, 0x10, 3]).unwrap(),
        OutpostSetHeroRequest {
            building_id: 1,
            hero_ids: vec![2, 3],
        }
    );
    assert_eq!(
        SportsMeetPointsRequest::decode(&[0x08, 99]).unwrap(),
        SportsMeetPointsRequest { points: 99 }
    );
    assert_eq!(
        TeachingUserRequest::decode(&[0x08, 42]).unwrap(),
        TeachingUserRequest { uid: 42 }
    );
    assert_eq!(
        InviteStateTypeRequest::decode(&[0x08, 2]).unwrap(),
        InviteStateTypeRequest { state_type: 2 }
    );
    assert_eq!(
        InviteRecordVersionRequest::decode(&[0x08, 7]).unwrap(),
        InviteRecordVersionRequest { version: 7 }
    );
    assert_eq!(
        ShipTaskRewardRequest::decode(&[0x08, 12, 0x10, 3]).unwrap(),
        ShipTaskRewardRequest {
            ship_tid: 12,
            task_id: 3,
        }
    );
    assert_eq!(
        ShipTaskCurrentShipRequest::decode(&[0x08, 12, 0x10, 0x8B, 0xEB, 0x01]).unwrap(),
        ShipTaskCurrentShipRequest {
            ship_tid: 12,
            hero_template_id: 30091,
        }
    );
}

#[test]
fn decodes_typed_guild_task_requests() {
    assert_eq!(GuildTaskIdRequest::decode(&[0x08, 7]).unwrap().task_id, 7);
    assert_eq!(
        GuildTaskMemberRequest::decode(&[0x10, 8]).unwrap().task_id,
        8
    );

    let donation = [0x10, 8, 0x1a, 6, 0x08, 1, 0x10, 2, 0x18, 3, 0x20, 9];
    let request = GuildTaskDonateRequest::decode(&donation).unwrap();
    assert_eq!((request.task_id, request.contribute), (8, 9));
    assert_eq!(
        request.items[0],
        blueoath_protocol::GuildTaskDonationItem {
            goods_type: 1,
            item_id: 2,
            amount: 3,
        }
    );
}

#[test]
fn decodes_typed_activity_requests() {
    assert_eq!(
        BigActivityRankRequest::decode(&[0x08, 3]).unwrap(),
        BigActivityRankRequest { start: 3 }
    );
    assert_eq!(
        GuildActivityPresentRequest::decode(&[0x08, 7, 0x10, 2]).unwrap(),
        GuildActivityPresentRequest {
            item_id: 7,
            count: 2,
        }
    );
    assert_eq!(
        HeroAwakenFinishRequest::decode(&[0x08, 1]).unwrap(),
        HeroAwakenFinishRequest { finished: true }
    );
    assert_eq!(
        HeroAwakenRewardRequest::decode(&[0x08, 5]).unwrap(),
        HeroAwakenRewardRequest { milestone: 5 }
    );
    assert_eq!(
        BirthdayFeedRequest::decode(&[0x08, 3, 0x10, 4]).unwrap(),
        BirthdayFeedRequest {
            team_id: 3,
            cake: 4,
        }
    );
    assert_eq!(
        ActivityExtractDrawRequest::decode(&[0x08, 7, 0x10, 2]).unwrap(),
        ActivityExtractDrawRequest { draw_id: 7, num: 2 }
    );
    assert_eq!(
        ActivitySelectShipRequest::decode(&[0x08, 3]).unwrap(),
        ActivitySelectShipRequest { ship_id: 3 }
    );
    assert_eq!(
        ActivitySelectTeamRequest::decode(&[0x08, 4]).unwrap(),
        ActivitySelectTeamRequest { team_id: 4 }
    );
    assert_eq!(
        ActivityRewardIndexRequest::decode(&[0x08, 2]).unwrap(),
        ActivityRewardIndexRequest { index: 2 }
    );
    assert_eq!(
        ActivityItemIdRequest::decode(&[0x08, 5]).unwrap(),
        ActivityItemIdRequest { item_id: 5 }
    );
    assert_eq!(
        ActivityCodeExchangeRequest::decode(&[0x08, 7, 0x18, 2]).unwrap(),
        ActivityCodeExchangeRequest { code: 7, number: 2 }
    );
    assert_eq!(
        ActivityExchangeRewardRequest::decode(&[0x08, 2, 0x10, 3]).unwrap(),
        ActivityExchangeRewardRequest {
            reward_index: 2,
            number: 3,
        }
    );
    assert_eq!(
        ChristmasBuyItemRequest::decode(&[0x08, 1, 0x10, 2]).unwrap(),
        ChristmasBuyItemRequest {
            buy_way: 1,
            buy_times: 2,
        }
    );
    assert_eq!(
        ChristmasBuyBlindBoxRequest::decode(&[0x08, 2]).unwrap(),
        ChristmasBuyBlindBoxRequest { buy_index: 2 }
    );
    assert_eq!(
        FashionPurchaseRequest::decode(&[0x08, 2, 0x10, 3]).unwrap(),
        FashionPurchaseRequest {
            requested: 2,
            group_id: 3,
        }
    );
    assert_eq!(
        FashionEquipRequest::decode(&[0x08, 0xe9, 0x07, 0x10, 1, 0x18, 10]).unwrap(),
        FashionEquipRequest {
            fashion_tid: 1001,
            equip_status: 1,
            hero_id: 10,
        }
    );
    assert_eq!(
        ActivityFormulaRequest::decode(&[0x08, 4]).unwrap(),
        ActivityFormulaRequest { formula: 4 }
    );
    assert_eq!(
        ValentineRewardRequest::decode(&[0x08, 2]).unwrap(),
        ValentineRewardRequest { index: 2 }
    );
    assert_eq!(
        CopyRewardCountRequest::decode(&[0x08, 7, 0x10, 3]).unwrap(),
        CopyRewardCountRequest {
            chapter_id: 7,
            reward_time: 3,
        }
    );
    assert_eq!(
        SignDayRequest::decode(&[0x08, 4]).unwrap(),
        SignDayRequest { day: 4 }
    );
    assert_eq!(
        AlchemyRequest::decode(&[0x08, 9, 0x10, 1, 0x10, 2]).unwrap(),
        AlchemyRequest {
            formula_id: 9,
            equip_ids: vec![1, 2],
        }
    );
    assert_eq!(
        CopyIdRequest::decode(&[0x08, 12]).unwrap(),
        CopyIdRequest { copy_id: 12 }
    );
    assert_eq!(
        BattlePassRewardRequest::decode(&[0x08, 3]).unwrap(),
        BattlePassRewardRequest { level: 3 }
    );
    assert_eq!(
        BattlePassRefreshRequest::decode(&[0x08, 4]).unwrap(),
        BattlePassRefreshRequest { task_id: 4 }
    );
    assert_eq!(
        BattlePassTypeRequest::decode(&[0x08, 2]).unwrap(),
        BattlePassTypeRequest { pass_type: 2 }
    );
    assert_eq!(
        BattlePassLevelRequest::decode(&[0x08, 5]).unwrap(),
        BattlePassLevelRequest { levels: 5 }
    );
    assert_eq!(
        BattlePassTaskRewardRequest::decode(&[0x08, 6]).unwrap(),
        BattlePassTaskRewardRequest { task_id: 6 }
    );
    assert_eq!(
        ExchangeRequest::decode(&[0x08, 8]).unwrap(),
        ExchangeRequest { exchange_id: 8 }
    );
    assert_eq!(
        WorldEventStageRequest::decode(&[0x08, 9]).unwrap(),
        WorldEventStageRequest { stage_id: 9 }
    );
    assert_eq!(
        PaperCutRequest::decode(&[0x08, 1, 0x08, 2]).unwrap(),
        PaperCutRequest {
            material_ids: vec![1, 2],
        }
    );
    assert_eq!(
        FoodComposeRequest::decode(&[0x08, 1, 0x08, 2, 0x10, 3]).unwrap(),
        FoodComposeRequest {
            material_ids: vec![1, 2],
            recipe_id: 3,
        }
    );
    assert_eq!(
        DiscussRequest::decode(&[0x08, 11]).unwrap(),
        DiscussRequest { discuss_id: 11 }
    );
}

#[test]
fn decodes_typed_coop_requests() {
    let create = [0x10, 9, 0x22, 4, 0x08, 1, 0x08, 2];
    assert_eq!(
        CoopCreateRoomRequest::decode(&create).unwrap(),
        CoopCreateRoomRequest {
            copy_id: 9,
            hero_ids: vec![1, 2],
        }
    );
    let room_heroes = [0x08, 7, 0x22, 2, 0x08, 1];
    assert_eq!(
        CoopRoomHeroesRequest::decode(&room_heroes).unwrap(),
        CoopRoomHeroesRequest {
            room_id: 7,
            hero_ids: vec![1],
        }
    );
    assert_eq!(
        CoopRoomIdRequest::decode(&[0x08, 7]).unwrap(),
        CoopRoomIdRequest { room_id: 7 }
    );
    assert_eq!(
        CoopKickRequest::decode(&[0x08, 7, 0x18, 9]).unwrap(),
        CoopKickRequest {
            room_id: 7,
            kicked_uid: 9,
        }
    );
    assert_eq!(
        CoopChangeChapterRequest::decode(&[0x08, 7, 0x10, 9]).unwrap(),
        CoopChangeChapterRequest {
            room_id: 7,
            copy_id: 9,
        }
    );
    assert_eq!(
        CoopPasswordRequest::decode(&[0x08, 7, 0x10, 3]).unwrap(),
        CoopPasswordRequest {
            room_id: 7,
            password: 3,
        }
    );
    assert_eq!(
        CoopMatchTypeRequest::decode(&[0x08, 2]).unwrap(),
        CoopMatchTypeRequest { match_type: 2 }
    );
    assert_eq!(
        BattleAutoMessageRequest::decode(&[0x08, 4]).unwrap(),
        BattleAutoMessageRequest { message_id: 4 }
    );
    assert_eq!(
        GuildWarScoreRequest::decode(&[0x08, 6]).unwrap(),
        GuildWarScoreRequest { score: 6 }
    );
    assert_eq!(
        GuildWarBaseRequest::decode(&[0x08, 2, 0x10, 3]).unwrap(),
        GuildWarBaseRequest {
            base_id: 2,
            stage_id: 3,
        }
    );
    assert_eq!(
        GuildOfferRequest::decode(&[0x08, 4, 0x10, 2]).unwrap(),
        GuildOfferRequest {
            task_id: 4,
            task_index: 2,
        }
    );
    assert_eq!(
        GuildCreateRequest::decode(&[
            0x0a, 6, 0xe8, 0x88, 0xb0, 0xe9, 0x98, 0x9f, 0x10, 0xe8, 0x07, 0x18, 2,
        ])
        .unwrap(),
        GuildCreateRequest {
            name: "舰队".to_owned(),
            emblem: 1000,
            frame: 2,
        }
    );
    assert_eq!(
        GuildListRequest::decode(&[0x08, 1, 0x10, 20]).unwrap(),
        GuildListRequest { start: 1, end: 20 }
    );
    assert_eq!(
        GuildSearchRequest::decode(&[0x08, 7, 0x12, 2, b'G', b'B']).unwrap(),
        GuildSearchRequest {
            guild_id: 7,
            name: "GB".to_owned(),
        }
    );
    assert_eq!(
        GuildIdRequest::decode(&[0x08, 7]).unwrap(),
        GuildIdRequest { guild_id: 7 }
    );
    assert_eq!(
        GuildModifyRequest::decode(&[0x0a, 2, b'X', b'Y', 0x30, 3, 0x3a, 1, b'c']).unwrap(),
        GuildModifyRequest {
            name: Some("XY".to_owned()),
            emblem: 0,
            enounce: None,
            notice: None,
            frame: 3,
            chat_room: Some("c".to_owned()),
        }
    );
    assert_eq!(
        HeroChangeEquipRequest::decode(&[0x08, 1, 0x10, 2, 0x18, 3, 0x20, 1]).unwrap(),
        HeroChangeEquipRequest {
            hero_id: 1,
            slot: 2,
            equip_id: 3,
            equip_type: 1,
        }
    );
}

#[test]
fn rejects_invalid_typed_chat_requests() {
    assert!(matches!(
        SendMessageRequest::decode(&[0x08, 1]),
        Err(ProtocolError::Invalid("chat message is missing content"))
    ));
    assert!(matches!(
        SendBarrageRequest::decode(&[0x1a, 0]),
        Err(ProtocolError::Invalid("barrage request is invalid"))
    ));
    assert!(matches!(
        GetBarrageByIdRequest::decode(&[0x18, 101]),
        Err(ProtocolError::Invalid("barrage query is invalid"))
    ));
    assert!(matches!(
        BuildShipRewardRequest::decode(&[0x08, 0, 0x10, 1]),
        Err(ProtocolError::Invalid("build reward request is invalid"))
    ));
    assert!(matches!(
        GuildBoxAnonymousRequest::decode(&[0x08, 2]),
        Err(ProtocolError::Invalid("guild box anonymous is invalid"))
    ));
    assert!(matches!(
        OutpostBuildingRequest::decode(&[0x08, 0]),
        Err(ProtocolError::Invalid("outpost building id is invalid"))
    ));
    assert!(matches!(
        SportsMeetPointsRequest::decode(&[0x08, 0]),
        Err(ProtocolError::Invalid("sports meet points are invalid"))
    ));
    assert!(matches!(
        TeachingUserRequest::decode(&[0x08, 0]),
        Err(ProtocolError::Invalid("teaching uid is invalid"))
    ));
    assert!(matches!(
        InviteStateTypeRequest::decode(&[0x08, 4]),
        Err(ProtocolError::Invalid("invite state type is invalid"))
    ));
    assert!(matches!(
        ShipTaskRewardRequest::decode(&[0x08, 0, 0x10, 1]),
        Err(ProtocolError::Invalid(
            "ship task reward request is invalid"
        ))
    ));
}

#[test]
fn decodes_typed_progression_requests() {
    assert_eq!(
        BathroomRequest::decode(&[0x08, 1, 0x10, 2, 0x18, 1]).unwrap(),
        BathroomRequest {
            hero_id: 1,
            position: 2,
            is_auto: true,
        }
    );
    assert_eq!(
        BathroomAutoRequest::decode(&[0x08, 9, 0x10, 1]).unwrap(),
        BathroomAutoRequest {
            hero_id: 9,
            is_auto: true,
        }
    );
    assert_eq!(
        BathroomServiceRequest::decode(&[0x08, 9, 0x10, 0xD1, 0xF7, 0x07]).unwrap(),
        BathroomServiceRequest {
            hero_id: 9,
            gift_id: 130001,
        }
    );
    assert_eq!(
        EquipRiseStarRequest::decode(&[0x08, 7])
            .unwrap()
            .consume_ids,
        Vec::<u64>::new()
    );
    assert_eq!(
        BathroomStartAllRequest::decode(&[0x0a, 4, 0x08, 1, 0x10, 2]).unwrap(),
        BathroomStartAllRequest {
            entries: vec![BathroomStartEntry {
                hero_id: 1,
                position: 2,
            }],
        }
    );
    assert_eq!(
        StudyStartRequest::decode(&[0x08, 1, 0x10, 2, 0x18, 3]).unwrap(),
        StudyStartRequest {
            hero_id: 1,
            skill_id: 2,
            textbook_id: 3,
        }
    );
    assert_eq!(
        StudyProgressRequest::decode(&[0x08, 1, 0x10, 2]).unwrap(),
        StudyProgressRequest {
            hero_id: 1,
            skill_id: 2,
        }
    );
    assert_eq!(
        StudySpeedupRequest::decode(&[0x08, 1, 0x10, 2, 0x1a, 4, 0x08, 3, 0x10, 4]).unwrap(),
        StudySpeedupRequest {
            hero_id: 1,
            skill_id: 2,
            items: vec![StudySpeedupItem {
                item_id: 3,
                count: 4,
            }],
        }
    );
}

#[test]
fn decodes_typed_battle_and_guide_requests() {
    assert_eq!(
        CopyPassBaseRequest::decode(&[]).unwrap(),
        CopyPassBaseRequest { copy_id: 0 }
    );
    assert_eq!(
        CopyPassBaseRequest::decode(&[0x08, 9]).unwrap(),
        CopyPassBaseRequest { copy_id: 9 }
    );
    assert_eq!(
        DailyCopySelectExRequest::decode(&[0x08, 2, 0x10, 1]).unwrap(),
        DailyCopySelectExRequest {
            chapter_id: 2,
            select_ex: true,
        }
    );
    assert_eq!(
        GuideSettingRequest::decode(&[
            0x0a, 6, 0x0a, 1, b'a', 0x12, 1, b'b', 0x0a, 6, 0x0a, 1, b'c', 0x12, 1, b'd',
        ])
        .unwrap(),
        GuideSettingRequest {
            entries: vec![
                GuideSettingEntry {
                    key: "a".to_owned(),
                    value: "b".to_owned(),
                },
                GuideSettingEntry {
                    key: "c".to_owned(),
                    value: "d".to_owned(),
                },
            ],
        }
    );
    assert_eq!(
        FleetInfo::decode(&[0x0a, 6, 0x10, 1, 0x28, 4, 0x30, 2, 0x10, 9, 0x18, 3,]).unwrap(),
        FleetInfo {
            tactics: vec![FleetTactic {
                tactic_name: String::new(),
                hero_ids: vec![1],
                mode_id: 0,
                strategy_id: 0,
                formation_id: 4,
                tactic_type: 2,
                ex_hero_ids: vec![],
            }],
            max_power: 9,
            min_power: 3,
        }
    );
}
