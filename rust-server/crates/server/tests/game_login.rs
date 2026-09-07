use blueoath_protocol::{
    GameLoginCodec, TMessageCodec, TRequest, TRetLogin, UserInfo, UserInfoCodec,
};
use blueoath_server::process_game_login_frame_with_account;
use blueoath_server::{process_game_login_frame, ServerState};
use blueoath_transport::NetSocketFrameCodec;
use serde_json::json;
use tokio::io::duplex;

#[tokio::test]
async fn game_login_session_returns_protobuf_login_response() {
    let (mut client, mut server) = duplex(16384);
    let state = ServerState::new("slot-a", "Captain", "1.4.0");
    let request = TMessageCodec::encode_request(&TRequest {
        method: "player.Login".to_owned(),
        args: Some(GameLoginCodec::encode_login(&Default::default())),
        callback_handler: 7,
        token: "token".to_owned(),
    });
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();

    assert!(process_game_login_frame(&mut server, &state).await.unwrap());

    let frame = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    let response = TMessageCodec::decode_response(&frame.payload).unwrap();
    assert_eq!(response.method, "player.Login");
    assert_eq!(response.callback_handler, 7);
    assert_eq!(response.token, "token");
    assert_eq!(response.is_response, 1);
    assert_eq!(
        GameLoginCodec::decode_login_response(response.ret.as_deref().unwrap()).unwrap(),
        TRetLogin {
            ret: "ok".to_owned(),
            feign_role_id: "slot-a".to_owned(),
            err_code: 0,
        }
    );
}

#[tokio::test]
async fn game_login_unknown_method_returns_explicit_error_response() {
    let (mut client, mut server) = duplex(4096);
    let state = ServerState::new("slot-a", "Captain", "1.4.0");
    let request = TMessageCodec::encode_request(&TRequest {
        method: "unknown.NotRegistered".to_owned(),
        callback_handler: 11,
        ..TRequest::default()
    });
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();

    assert!(process_game_login_frame(&mut server, &state).await.unwrap());
    let response = TMessageCodec::decode_response(
        &NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap()
            .payload,
    )
    .unwrap();
    assert_eq!(response.err, 1);
    assert_eq!(response.method, "unknown.NotRegistered");
    assert_eq!(response.callback_handler, 11);
    assert_eq!(response.ret, Some(Vec::new()));
}

#[tokio::test]
async fn game_login_session_returns_player_bootstrap_responses() {
    for method in ["player.GetUserList", "player.CreateUser"] {
        let (mut client, mut server) = duplex(4096);
        let state = ServerState::new("slot-a", "Captain", "1.4.0");
        let request = TMessageCodec::encode_request(&TRequest {
            method: method.to_owned(),
            callback_handler: 3,
            ..TRequest::default()
        });
        NetSocketFrameCodec::write(&mut client, 0, &request)
            .await
            .unwrap();

        assert!(process_game_login_frame(&mut server, &state).await.unwrap());
        let frame = NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap();
        let response = TMessageCodec::decode_response(&frame.payload).unwrap();
        assert_eq!(response.method, method);
        assert_eq!(response.callback_handler, 3);
        assert_eq!(response.is_response, 1);
        assert!(response.ret.as_deref().is_some_and(|ret| !ret.is_empty()));
    }
}

#[tokio::test]
async fn game_login_session_returns_user_info_payload() {
    let (mut client, mut server) = duplex(4096);
    let state = ServerState::new("slot-a", "Captain", "1.4.0");
    let request = TMessageCodec::encode_request(&TRequest {
        method: "user.GetUserInfo".to_owned(),
        callback_handler: 9,
        ..TRequest::default()
    });
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();

    assert!(process_game_login_frame(&mut server, &state).await.unwrap());

    let frame = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    let response = TMessageCodec::decode_response(&frame.payload).unwrap();
    assert_eq!(response.method, "user.GetUserInfo");
    assert_eq!(response.callback_handler, 9);
    assert_eq!(
        UserInfoCodec::decode(response.ret.as_deref().unwrap()).unwrap(),
        UserInfo {
            uid: 1,
            uname: "Captain".to_owned(),
            level: 1,
            class_id: 1,
            secretary_id: 1,
            supply: 100,
            head: 1021051,
            pve_pt: 100,
            new_task_stage: 7,
            server_id: 1,
            ..UserInfo::default()
        }
    );
}

#[tokio::test]
async fn bathroom_info_request_emits_post_snapshot_after_response() {
    let (mut client, mut server) = duplex(4096);
    let state = ServerState::new("slot-a", "Captain", "1.4.0");
    let request = TMessageCodec::encode_request(&TRequest {
        method: "bathroom.GetBathroomInfo".to_owned(),
        callback_handler: 4,
        ..TRequest::default()
    });
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();

    assert!(process_game_login_frame(&mut server, &state).await.unwrap());
    let response = TMessageCodec::decode_response(
        &NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap()
            .payload,
    )
    .unwrap();
    assert_eq!(response.method, "bathroom.GetBathroomInfo");
    assert_eq!(response.callback_handler, 4);
    assert_eq!(response.is_response, 1);

    let push = TMessageCodec::decode_response(
        &NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap()
            .payload,
    )
    .unwrap();
    assert_eq!(push.method, "bathroom.BathroomInfo");
    assert_eq!(push.is_response, 0);
}

#[tokio::test]
async fn user_info_reads_character_values_from_account_snapshot() {
    let (mut client, mut server) = duplex(4096);
    let state = ServerState::new("slot-a", "Fallback", "1.4.0");
    let account = json!({
        "character": {
            "uid": 42,
            "name": "Stored Captain",
            "level": 80,
            "class": 3,
            "secretaryId": 9,
            "createTime": 123,
            "gold": 99999999,
            "diamond": 999999,
            "supply": 9999,
            "pvePt": 100
        },
        "bag": {
            "bagSize": 77,
            "items": [{"templateId": 3001, "num": 0}]
        },
        "fashion": {
            "entries": [{"sfId": 1021051, "fashionTids": [1021051]}]
        },
        "equip": {
            "equipBagSize": 2000,
            "items": [{"equipId": 9, "templateId": 30091, "heroId": 1, "enhanceLv": 2, "star": 1, "enhanceExp": 3}]
        }
    });
    let request = TMessageCodec::encode_request(&TRequest {
        method: "user.GetUserInfo".to_owned(),
        ..TRequest::default()
    });
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();

    assert!(
        process_game_login_frame_with_account(&mut server, &state, Some(&account))
            .await
            .unwrap()
    );

    let frame = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    let response = TMessageCodec::decode_response(&frame.payload).unwrap();
    let info = UserInfoCodec::decode(response.ret.as_deref().unwrap()).unwrap();
    assert_eq!(info.uid, 42);
    assert_eq!(info.uname, "Stored Captain");
    assert_eq!(info.level, 80);
    assert_eq!(info.class_id, 3);
    assert_eq!(info.secretary_id, 9);
    assert_eq!(info.diamond, 999999);
    assert_eq!(info.create_time, 123);

    for method in [
        "user.UpdateLoginTime",
        "user.UpdateSvrTime",
        "user.GetUserInfo",
        "build.BuildsInfo",
        "bathroom.BathroomInfo",
        "study.GetStudyInfo",
        "task.TaskInfo",
    ] {
        let frame = NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap();
        let push = TMessageCodec::decode_response(&frame.payload).unwrap();
        assert_eq!(push.method, method);
        assert_eq!(push.is_response, 0);
        assert!(push.ret.is_some());
    }

    let bag_frame = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    let bag_push = TMessageCodec::decode_response(&bag_frame.payload).unwrap();
    assert_eq!(bag_push.method, "bag.UpdateBagData");
    assert_eq!(bag_push.is_response, 0);
    let bag_payload = bag_push.ret.as_deref().unwrap();
    assert!(bag_payload.windows(2).any(|window| window == [0x08, 0x01]));
    assert!(bag_payload.windows(2).any(|window| window == [0x10, 0x4D]));
    assert!(bag_payload.contains(&0x1A));

    let fashion_frame = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    let fashion_push = TMessageCodec::decode_response(&fashion_frame.payload).unwrap();
    assert_eq!(fashion_push.method, "fashion.updateData");
    let fashion_payload = fashion_push.ret.as_deref().unwrap();
    assert!(fashion_payload.starts_with(&[0x0A]));
    assert!(fashion_payload
        .windows(3)
        .any(|window| window == [0x08, 0xFB, 0xA8]));

    let equip_frame = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    let equip_push = TMessageCodec::decode_response(&equip_frame.payload).unwrap();
    assert_eq!(equip_push.method, "equip.UpdateEquipBagData");
    let equip_payload = equip_push.ret.as_deref().unwrap();
    assert!(equip_payload
        .windows(3)
        .any(|window| window == [0x08, 0xD0, 0x0F]));
    assert!(equip_payload.contains(&0x12));

    let hero_frame = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    let hero_push = TMessageCodec::decode_response(&hero_frame.payload).unwrap();
    assert_eq!(hero_push.method, "hero.UpdateHeroBagData");
    assert!(hero_push.ret.is_some());

    let building_frame = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    let building_push = TMessageCodec::decode_response(&building_frame.payload).unwrap();
    assert_eq!(building_push.method, "building.UpdateBuildingInfo");
    assert!(building_push.ret.is_some());

    let fleet_frame = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    let fleet_push = TMessageCodec::decode_response(&fleet_frame.payload).unwrap();
    assert_eq!(fleet_push.method, "tactic.GetHerosTactic");
    assert!(fleet_push.ret.is_some());

    let shop_frame = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    let shop_push = TMessageCodec::decode_response(&shop_frame.payload).unwrap();
    assert_eq!(shop_push.method, "shop.UpdateShopInfo");
    let shop_payload = shop_push.ret.as_deref().unwrap();
    assert!(shop_payload.len() > 100);
    assert!(shop_payload.starts_with(&[0x0A, 0x0A, 0x08, 0x01, 0x1A, 0x00]));
    assert!(shop_payload.ends_with(&[
        0x0A, 0x0B, 0x08, 0xB2, 0x09, 0x1A, 0x00, 0x20, 0x00, 0x28, 0x00, 0x30, 0x00
    ]));

    let recharge_frame = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    let recharge_push = TMessageCodec::decode_response(&recharge_frame.payload).unwrap();
    assert_eq!(recharge_push.method, "recharge.RechargeInfo");
    assert_eq!(recharge_push.ret, Some(vec![0x1A, 0x00]));

    let buildship_frame = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    let buildship_push = TMessageCodec::decode_response(&buildship_frame.payload).unwrap();
    assert_eq!(buildship_push.method, "buildship.BuildShipInfo");
    assert!(buildship_push
        .ret
        .as_deref()
        .is_some_and(|ret| ret.contains(&0x52)));
}

#[tokio::test]
async fn game_login_session_returns_user_login_response_and_user_update_push() {
    let (mut client, mut server) = duplex(4096);
    let state = ServerState::new("slot-a", "Captain", "1.4.0");
    let request = TMessageCodec::encode_request(&TRequest {
        method: "user.UserLogin".to_owned(),
        callback_handler: 11,
        ..TRequest::default()
    });
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();

    assert!(process_game_login_frame(&mut server, &state).await.unwrap());

    let expected_pushes = [
        "user.UpdateUserInfo",
        "guide.GuideInfo",
        "copy.GetCopy",
        "copy.GetCopy",
        "copy.GetCopy",
        "copy.GetCopy",
        "dailycopy.UpdateDailyCopyData",
        "illustrate.IllustrateInfo",
        "illustrate.OldIllustrateInfo",
        "illustrate.Memory",
    ];
    for method in expected_pushes {
        let push_frame = NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap();
        let push = TMessageCodec::decode_response(&push_frame.payload).unwrap();
        assert_eq!(push.method, method);
        assert_eq!(push.is_response, 0);
        assert!(push.ret.is_some());
    }

    let response_frame = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    let response = TMessageCodec::decode_response(&response_frame.payload).unwrap();
    assert_eq!(response.method, "user.UserLogin");
    assert_eq!(response.callback_handler, 11);
    assert_eq!(response.is_response, 1);
    assert_eq!(response.ret, Some(vec![0x0A, 0x02, b'o', b'k']));
}

#[tokio::test]
async fn game_login_session_handles_fleet_save_and_copy_reads() {
    let state = ServerState::new("slot-a", "Captain", "1.4.0");
    let fleet_args = {
        let mut tactic = Vec::new();
        tactic.extend_from_slice(&[0x10, 0x2A]); // heroInfo=[42]
        tactic.extend_from_slice(&[0x18, 0x01, 0x20, 0x07, 0x28, 0x03, 0x30, 0x01]);
        vec![0x0A, tactic.len() as u8]
            .into_iter()
            .chain(tactic)
            .collect::<Vec<_>>()
    };
    for (method, args) in [
        ("tactic.SetHerosTactic", fleet_args),
        ("copy.GetCopy", vec![]),
    ] {
        let (mut client, mut server) = duplex(4096);
        let request = TMessageCodec::encode_request(&TRequest {
            method: method.to_owned(),
            args: Some(args),
            callback_handler: 4,
            ..TRequest::default()
        });
        NetSocketFrameCodec::write(&mut client, 0, &request)
            .await
            .unwrap();
        assert!(process_game_login_frame(&mut server, &state).await.unwrap());
        let frame = NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap();
        let response = TMessageCodec::decode_response(&frame.payload).unwrap();
        assert_eq!(response.method, method);
        assert_eq!(response.callback_handler, 4);
        assert!(response.ret.as_deref().is_some_and(|ret| !ret.is_empty()));
    }
}

#[tokio::test]
async fn user_login_preserves_daily_copy_progress_from_account_snapshot() {
    let (mut client, mut server) = duplex(4096);
    let state = ServerState::new("slot-a", "Captain", "1.4.0");
    let reset_day = (u64::from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32,
    ) + 8 * 60 * 60)
        / 86_400;
    let account = json!({
        "dailyCopy": {
            "resetDay": reset_day,
            "chapters": [{
                "chapterId": 1,
                "challengeTimes": 2,
                "passCopy": [101],
                "selectEx": true,
                "exStar": 5
            }],
            "groups": [{"dailyGroupId": 1, "successTimes": 3}],
            "extraGroups": [{"dailyGroupId": 1, "successTimes": 4}]
        }
    });
    let request = TMessageCodec::encode_request(&TRequest {
        method: "user.UserLogin".to_owned(),
        ..TRequest::default()
    });
    NetSocketFrameCodec::write(&mut client, 0, &request)
        .await
        .unwrap();

    assert!(
        process_game_login_frame_with_account(&mut server, &state, Some(&account))
            .await
            .unwrap()
    );
    for _ in 0..6 {
        NetSocketFrameCodec::read(&mut client)
            .await
            .unwrap()
            .unwrap();
    }
    let daily_frame = NetSocketFrameCodec::read(&mut client)
        .await
        .unwrap()
        .unwrap();
    let daily_push = TMessageCodec::decode_response(&daily_frame.payload).unwrap();
    let payload = daily_push.ret.unwrap();
    assert!(payload.windows(2).any(|window| window == [0x10, 0x02]));
    assert!(payload.windows(2).any(|window| window == [0x20, 0x01]));
    assert!(payload.windows(2).any(|window| window == [0x28, 0x05]));
    assert!(payload.windows(2).any(|window| window == [0x10, 0x03]));
    assert!(payload.windows(2).any(|window| window == [0x10, 0x04]));
}
