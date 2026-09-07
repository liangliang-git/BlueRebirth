use blueoath_transport::{KcpCommand, KcpConnection, KcpPacket};

#[test]
fn kcp_packet_round_trips_little_endian_wire() {
    let packet = KcpPacket {
        conv: 7,
        command: KcpCommand::Push,
        fragment: 0,
        window: 128,
        timestamp: 10,
        sequence_number: 3,
        unacknowledged: 2,
        data: b"hello".to_vec(),
    };
    let encoded = packet.encode().unwrap();
    let (decoded, consumed) = KcpPacket::decode(&encoded).unwrap().unwrap();
    assert_eq!(consumed, encoded.len());
    assert_eq!(decoded, packet);
}

#[test]
fn kcp_connection_reassembles_and_acks_fragments() {
    let mut client = KcpConnection::new(9);
    client.send(&vec![7_u8; 3000], 100).unwrap();
    let first = client.flush(100);
    assert_eq!(first.len(), 3);

    let mut server = KcpConnection::new(9);
    let mut messages = Vec::new();
    for datagram in first {
        let (packet, _) = KcpPacket::decode(&datagram).unwrap().unwrap();
        messages.extend(server.input(packet));
    }
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].len(), 3000);
    let acks = server.flush(100);
    assert_eq!(acks.len(), 1);
    let (ack, _) = KcpPacket::decode(&acks[0]).unwrap().unwrap();
    assert_eq!(ack.command, KcpCommand::Ack);
    client.input(ack);
    assert!(client.flush(1000).is_empty());
}
