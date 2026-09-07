use blueoath_server::{process_frame, ServerState};
use blueoath_transport::FrameCodec;
use serde_json::json;
use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn framed_login_returns_csharp_compatible_envelope() {
    let (mut client, mut server) = duplex(4096);
    let mut state = ServerState::new("slot-a", "Captain", "1.4.0");

    let request = serde_json::to_vec(&json!({
        "type": "login",
        "requestId": "req-1",
        "payload": {"profileId": "ignored"}
    }))
    .unwrap();
    FrameCodec::write(&mut client, &request).await.unwrap();

    assert!(process_frame(&mut server, &mut state).await.unwrap());

    let response = FrameCodec::read(&mut client).await.unwrap().unwrap();
    let response: serde_json::Value = serde_json::from_slice(&response).unwrap();
    assert_eq!(response["ok"], true);
    assert_eq!(response["requestId"], "req-1");
    assert_eq!(response["type"], "login");
    assert_eq!(
        response["payload"],
        json!({"profileId": "slot-a", "version": "1.4.0"})
    );

    client.shutdown().await.unwrap();
    let mut discarded = [0; 1];
    assert_eq!(server.read(&mut discarded).await.unwrap(), 0);
}
