use blueoath_transport::{NetSocketFrame, NetSocketFrameCodec, NetSocketFrameError};
use tokio::io::duplex;

#[tokio::test]
async fn data_frame_round_trip_has_big_endian_length_and_type() {
    let (mut writer, mut reader) = duplex(1024);
    let payload = b"protobuf";
    let write = tokio::spawn(async move {
        NetSocketFrameCodec::write(&mut writer, 0, payload)
            .await
            .unwrap();
    });

    let frame = NetSocketFrameCodec::read(&mut reader)
        .await
        .unwrap()
        .unwrap();
    write.await.unwrap();
    assert_eq!(
        frame,
        NetSocketFrame {
            frame_type: 0,
            payload: payload.to_vec()
        }
    );
}

#[tokio::test]
async fn ping_frame_allows_empty_payload() {
    let (mut writer, mut reader) = duplex(1024);
    NetSocketFrameCodec::write(&mut writer, 2, &[])
        .await
        .unwrap();
    assert_eq!(
        NetSocketFrameCodec::read(&mut reader)
            .await
            .unwrap()
            .unwrap(),
        NetSocketFrame {
            frame_type: 2,
            payload: Vec::new()
        }
    );
}

#[tokio::test]
async fn hash_frame_consumes_hash_before_payload() {
    let mut wire = vec![0, 0, 0, 1, 1];
    wire.extend_from_slice(&[0xAB; 16]);
    wire.push(0xCD);
    let mut reader = std::io::Cursor::new(wire);
    let frame = NetSocketFrameCodec::read(&mut reader)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame.frame_type, 1);
    assert_eq!(frame.payload, vec![0xCD]);
}

#[tokio::test]
async fn invalid_length_is_rejected() {
    let wire = [0xFF, 0xFF, 0xFF, 0xFF, 0];
    let error = NetSocketFrameCodec::read(&mut std::io::Cursor::new(wire))
        .await
        .unwrap_err();
    assert!(matches!(error, NetSocketFrameError::InvalidLength(-1)));
}
