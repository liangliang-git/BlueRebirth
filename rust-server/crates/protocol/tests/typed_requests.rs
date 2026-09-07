use blueoath_protocol::{
    ChangeNameRequest, ChangeWorldChannelRequest, CopyRecordRequest, CopyStartRequest,
    DailyCopyEnterRequest, Decode, ProtocolError, SeaDifficultyRequest, SendBarrageRequest,
    SendMessageRequest, SetHeadFrameRequest, SetHeadRequest, SetMessageRequest,
    SetSecretaryRequest,
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
}
