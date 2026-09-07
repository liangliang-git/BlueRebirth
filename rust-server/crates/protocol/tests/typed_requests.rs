use blueoath_protocol::{CopyStartRequest, Decode, ProtocolError};

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
