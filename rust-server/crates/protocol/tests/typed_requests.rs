use blueoath_protocol::{
    BuildShipRequest, BuildShipRewardRequest, ChangeNameRequest, ChangeWorldChannelRequest,
    CopyAttackRequest, CopyPassRequest, CopyRecordRequest, CopyStartRequest, DailyCopyEnterRequest,
    DailyCopySelectExRequest, Decode, FriendSearchRequest, FriendTargetRequest,
    GetBarrageByIdRequest, ProtocolError, SeaDifficultyRequest, SendBarrageRequest,
    SendMessageRequest, SetHeadFrameRequest, SetHeadRequest, SetMessageRequest,
    SetSecretaryRequest, TaskAllRewardRequest, TaskRewardRequest,
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
}
