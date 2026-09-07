use blueoath_transport::{FrameCodec, FrameError, MAX_FRAME_SIZE};
use std::io::Cursor;

#[tokio::test]
async fn frame_round_trip_uses_big_endian_length_prefix() {
    let payload = br#"{"type":"login","requestId":"1"}"#;
    let mut wire = Vec::new();

    FrameCodec::write(&mut wire, payload).await.unwrap();

    assert_eq!(&wire[..4], &(payload.len() as u32).to_be_bytes());
    let decoded = FrameCodec::read(&mut Cursor::new(wire)).await.unwrap();
    assert_eq!(decoded.as_deref(), Some(payload.as_slice()));
}

#[tokio::test]
async fn frame_reader_rejects_zero_and_oversized_payloads() {
    for length in [0_u32, (MAX_FRAME_SIZE as u32) + 1] {
        let mut wire = length.to_be_bytes().to_vec();
        wire.extend_from_slice(b"x");
        let error = FrameCodec::read(&mut Cursor::new(wire)).await.unwrap_err();
        assert!(matches!(error, FrameError::InvalidLength(_)));
    }
}

#[tokio::test]
async fn frame_reader_reports_truncated_payload() {
    let wire = [0, 0, 0, 4, b'x', b'y'];
    let error = FrameCodec::read(&mut Cursor::new(wire)).await.unwrap_err();
    assert!(matches!(error, FrameError::Truncated));
}
