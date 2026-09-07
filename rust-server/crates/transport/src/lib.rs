use std::collections::BTreeMap;
use std::io;

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum KcpCommand {
    Push = 81,
    Ack = 82,
    Wask = 83,
    Wins = 84,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KcpPacket {
    pub conv: u32,
    pub command: KcpCommand,
    pub fragment: u8,
    pub window: u16,
    pub timestamp: u32,
    pub sequence_number: u32,
    pub unacknowledged: u32,
    pub data: Vec<u8>,
}

pub const KCP_HEADER_LENGTH: usize = 24;
pub const KCP_MAX_DATA_LENGTH: usize = 16 * 1024 * 1024;
const KCP_RECEIVE_WINDOW: u32 = 128;
const KCP_SEND_WINDOW: usize = 128;
pub const KCP_MAX_MESSAGE_LENGTH: usize = KCP_SEND_WINDOW * 1400;

impl KcpPacket {
    pub fn encode(&self) -> Result<Vec<u8>, KcpError> {
        if self.data.len() > KCP_MAX_DATA_LENGTH {
            return Err(KcpError::InvalidLength(self.data.len()));
        }
        let mut output = vec![0_u8; KCP_HEADER_LENGTH + self.data.len()];
        output[0..4].copy_from_slice(&self.conv.to_le_bytes());
        output[4] = self.command as u8;
        output[5] = self.fragment;
        output[6..8].copy_from_slice(&self.window.to_le_bytes());
        output[8..12].copy_from_slice(&self.timestamp.to_le_bytes());
        output[12..16].copy_from_slice(&self.sequence_number.to_le_bytes());
        output[16..20].copy_from_slice(&self.unacknowledged.to_le_bytes());
        output[20..24].copy_from_slice(&(self.data.len() as u32).to_le_bytes());
        output[KCP_HEADER_LENGTH..].copy_from_slice(&self.data);
        Ok(output)
    }

    pub fn decode(buffer: &[u8]) -> Result<Option<(Self, usize)>, KcpError> {
        if buffer.len() < KCP_HEADER_LENGTH {
            return Ok(None);
        }
        let Some(length) = read_u32_le(buffer, 20).map(|length| length as usize) else {
            return Ok(None);
        };
        if length > KCP_MAX_DATA_LENGTH {
            return Err(KcpError::InvalidLength(length));
        }
        let consumed = KCP_HEADER_LENGTH + length;
        if buffer.len() < consumed {
            return Ok(None);
        }
        let command = match buffer[4] {
            81 => KcpCommand::Push,
            82 => KcpCommand::Ack,
            83 => KcpCommand::Wask,
            84 => KcpCommand::Wins,
            value => return Err(KcpError::InvalidCommand(value)),
        };
        let Some(conv) = read_u32_le(buffer, 0) else {
            return Ok(None);
        };
        let Some(window) = read_u16_le(buffer, 6) else {
            return Ok(None);
        };
        let Some(timestamp) = read_u32_le(buffer, 8) else {
            return Ok(None);
        };
        let Some(sequence_number) = read_u32_le(buffer, 12) else {
            return Ok(None);
        };
        let Some(unacknowledged) = read_u32_le(buffer, 16) else {
            return Ok(None);
        };
        Ok(Some((
            Self {
                conv,
                command,
                fragment: buffer[5],
                window,
                timestamp,
                sequence_number,
                unacknowledged,
                data: buffer[KCP_HEADER_LENGTH..consumed].to_vec(),
            },
            consumed,
        )))
    }
}

fn read_u16_le(buffer: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        buffer
            .get(offset..offset.checked_add(2)?)?
            .try_into()
            .ok()?,
    ))
}

fn read_u32_le(buffer: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        buffer
            .get(offset..offset.checked_add(4)?)?
            .try_into()
            .ok()?,
    ))
}

#[derive(Debug, Error)]
pub enum KcpError {
    #[error("invalid KCP payload length: {0}")]
    InvalidLength(usize),
    #[error("invalid KCP command: {0}")]
    InvalidCommand(u8),
    #[error("KCP send window is full")]
    SendWindowFull,
}

pub struct KcpConnection {
    conv: u32,
    send_next: u32,
    send_una: u32,
    receive_next: u32,
    rto: u32,
    send_buffer: BTreeMap<u32, PendingKcpSend>,
    receive_buffer: BTreeMap<u32, KcpPacket>,
    fragments: Vec<KcpPacket>,
    ack_list: Vec<u32>,
    dead: bool,
}

struct PendingKcpSend {
    data: Vec<u8>,
    sent_at: Option<u32>,
    retransmits: u32,
}

impl KcpConnection {
    pub fn new(conv: u32) -> Self {
        Self {
            conv,
            send_next: 0,
            send_una: 0,
            receive_next: 0,
            rto: 200,
            send_buffer: BTreeMap::new(),
            receive_buffer: BTreeMap::new(),
            fragments: Vec::new(),
            ack_list: Vec::new(),
            dead: false,
        }
    }

    pub fn conv(&self) -> u32 {
        self.conv
    }
    pub fn dead(&self) -> bool {
        self.dead
    }

    pub fn input(&mut self, packet: KcpPacket) -> Vec<Vec<u8>> {
        if self.dead || packet.conv != self.conv {
            return Vec::new();
        }
        if packet.command == KcpCommand::Ack {
            let outstanding = self.send_next.wrapping_sub(self.send_una);
            let advance = packet.unacknowledged.wrapping_sub(self.send_una);
            if advance <= outstanding {
                self.send_una = packet.unacknowledged;
                self.send_buffer.retain(|sequence, _| {
                    let behind = self.send_una.wrapping_sub(*sequence);
                    behind == 0 || behind > outstanding
                });
            }
            if self.send_buffer.is_empty() {
                self.rto = 200;
            }
            return Vec::new();
        }
        if packet.command != KcpCommand::Push {
            return Vec::new();
        }
        if packet.sequence_number < self.receive_next {
            self.ack_list.push(packet.sequence_number);
            return Vec::new();
        }
        if packet.sequence_number.wrapping_sub(self.receive_next) >= KCP_RECEIVE_WINDOW {
            return Vec::new();
        }
        if self
            .receive_buffer
            .insert(packet.sequence_number, packet)
            .is_some()
        {
            return Vec::new();
        }
        let mut messages = Vec::new();
        while let Some(packet) = self.receive_buffer.remove(&self.receive_next) {
            let sequence = packet.sequence_number;
            if self.fragments.len() >= 255 {
                self.fragments.clear();
                self.ack_list.push(sequence);
                self.receive_next = self.receive_next.wrapping_add(1);
                continue;
            }
            self.fragments.push(packet);
            if self
                .fragments
                .last()
                .is_some_and(|packet| packet.fragment == 0)
            {
                let size = self.fragments.iter().map(|packet| packet.data.len()).sum();
                if size > KCP_MAX_MESSAGE_LENGTH {
                    self.fragments.clear();
                    self.ack_list.push(sequence);
                    self.receive_next = self.receive_next.wrapping_add(1);
                    continue;
                }
                let mut message = Vec::with_capacity(size);
                for fragment in self.fragments.drain(..) {
                    message.extend_from_slice(&fragment.data);
                }
                messages.push(message);
            }
            self.ack_list.push(sequence);
            self.receive_next = self.receive_next.wrapping_add(1);
        }
        messages
    }

    pub fn send(&mut self, message: &[u8], now_ms: u32) -> Result<(), KcpError> {
        if self.dead {
            return Ok(());
        }
        if message.len() > KCP_MAX_MESSAGE_LENGTH {
            return Err(KcpError::InvalidLength(message.len()));
        }
        let max_payload = 1400;
        let total = message.len().max(1).div_ceil(max_payload);
        if self.send_buffer.len() + total > KCP_SEND_WINDOW {
            return Err(KcpError::SendWindowFull);
        }
        for index in 0..total {
            let start = index * max_payload;
            let end = (start + max_payload).min(message.len());
            let data = if message.is_empty() {
                Vec::new()
            } else {
                message[start..end].to_vec()
            };
            let packet = KcpPacket {
                conv: self.conv,
                command: KcpCommand::Push,
                fragment: (total - index - 1) as u8,
                window: 128,
                timestamp: now_ms,
                sequence_number: self.send_next,
                unacknowledged: self.receive_next,
                data,
            };
            self.send_buffer.insert(
                self.send_next,
                PendingKcpSend {
                    data: packet.encode()?,
                    sent_at: None,
                    retransmits: 0,
                },
            );
            self.send_next = self.send_next.wrapping_add(1);
        }
        Ok(())
    }

    pub fn flush(&mut self, now_ms: u32) -> Vec<Vec<u8>> {
        let mut output = Vec::new();
        if !self.ack_list.is_empty() {
            let packet = KcpPacket {
                conv: self.conv,
                command: KcpCommand::Ack,
                fragment: 0,
                window: 128,
                timestamp: 0,
                sequence_number: self.receive_next.saturating_sub(1),
                unacknowledged: self.receive_next,
                data: Vec::new(),
            };
            if let Ok(bytes) = packet.encode() {
                output.push(bytes);
            }
            self.ack_list.clear();
        }
        for pending in self.send_buffer.values_mut() {
            let due = pending
                .sent_at
                .is_none_or(|sent| now_ms.wrapping_sub(sent) >= self.rto);
            if !due {
                continue;
            }
            let was_sent = pending.sent_at.is_some();
            pending.sent_at = Some(now_ms);
            if was_sent {
                pending.retransmits += 1;
            }
            if pending.retransmits >= 20 {
                self.dead = true;
                break;
            }
            if pending.retransmits > 0 {
                self.rto = (self.rto * 2).min(5000);
            }
            output.push(pending.data.clone());
        }
        output
    }
}

/// Maximum payload accepted by the C# `FrameCodec` (4 MiB).
pub const MAX_FRAME_SIZE: usize = 4 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid frame length: {0}")]
    InvalidLength(i64),
    #[error("truncated frame")]
    Truncated,
}

/// Big-endian length-prefixed frame used by the local JSON protocol.
pub struct FrameCodec;

impl FrameCodec {
    pub async fn write<W>(writer: &mut W, payload: &[u8]) -> Result<(), FrameError>
    where
        W: AsyncWrite + Unpin,
    {
        if payload.len() > MAX_FRAME_SIZE {
            return Err(FrameError::InvalidLength(payload.len() as i64));
        }

        let length = u32::try_from(payload.len())
            .map_err(|_| FrameError::InvalidLength(payload.len() as i64))?;
        writer.write_all(&length.to_be_bytes()).await?;
        writer.write_all(payload).await?;
        writer.flush().await?;
        Ok(())
    }

    /// Reads one frame. Clean EOF before any header byte returns `None`.
    pub async fn read<R>(reader: &mut R) -> Result<Option<Vec<u8>>, FrameError>
    where
        R: AsyncRead + Unpin,
    {
        let mut header = [0_u8; 4];
        match reader.read(&mut header[..1]).await? {
            0 => return Ok(None),
            1 => {}
            _ => unreachable!("single-byte read cannot return more than one byte"),
        }
        reader
            .read_exact(&mut header[1..])
            .await
            .map_err(|error| match error.kind() {
                io::ErrorKind::UnexpectedEof => FrameError::Truncated,
                _ => FrameError::Io(error),
            })?;

        let length = u32::from_be_bytes(header) as usize;
        if length == 0 || length > MAX_FRAME_SIZE {
            return Err(FrameError::InvalidLength(length as i64));
        }

        let mut payload = vec![0_u8; length];
        reader
            .read_exact(&mut payload)
            .await
            .map_err(|error| match error.kind() {
                io::ErrorKind::UnexpectedEof => FrameError::Truncated,
                _ => FrameError::Io(error),
            })?;
        Ok(Some(payload))
    }
}

/// NetSocket outer frame used by the game's protobuf login transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSocketFrame {
    pub frame_type: u8,
    pub payload: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum NetSocketFrameError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid NetSocket frame length: {0}")]
    InvalidLength(i64),
    #[error("truncated NetSocket frame")]
    Truncated,
}

/// Codec for the game's 5-byte `[length: i32 BE][type: u8]` frame header.
pub struct NetSocketFrameCodec;

impl NetSocketFrameCodec {
    pub async fn write<W>(
        writer: &mut W,
        frame_type: u8,
        payload: &[u8],
    ) -> Result<(), NetSocketFrameError>
    where
        W: AsyncWrite + Unpin,
    {
        if payload.len() > MAX_FRAME_SIZE {
            return Err(NetSocketFrameError::InvalidLength(payload.len() as i64));
        }
        let length = i32::try_from(payload.len())
            .map_err(|_| NetSocketFrameError::InvalidLength(payload.len() as i64))?;
        writer.write_all(&length.to_be_bytes()).await?;
        writer.write_all(&[frame_type]).await?;
        writer.write_all(payload).await?;
        writer.flush().await?;
        Ok(())
    }

    pub async fn read<R>(reader: &mut R) -> Result<Option<NetSocketFrame>, NetSocketFrameError>
    where
        R: AsyncRead + Unpin,
    {
        let mut header = [0_u8; 5];
        match reader.read(&mut header[..1]).await? {
            0 => return Ok(None),
            1 => {}
            _ => unreachable!("single-byte read cannot return more than one byte"),
        }
        reader
            .read_exact(&mut header[1..])
            .await
            .map_err(|error| match error.kind() {
                io::ErrorKind::UnexpectedEof => NetSocketFrameError::Truncated,
                _ => NetSocketFrameError::Io(error),
            })?;

        let length = i32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        if !(0..=MAX_FRAME_SIZE as i32).contains(&length) {
            return Err(NetSocketFrameError::InvalidLength(length as i64));
        }

        // TypeDataWithHash carries a 16-byte hash before payload. C# currently discards it.
        if header[4] == 1 {
            let mut hash = [0_u8; 16];
            reader
                .read_exact(&mut hash)
                .await
                .map_err(|error| match error.kind() {
                    io::ErrorKind::UnexpectedEof => NetSocketFrameError::Truncated,
                    _ => NetSocketFrameError::Io(error),
                })?;
        }

        let mut payload = vec![0_u8; length as usize];
        reader
            .read_exact(&mut payload)
            .await
            .map_err(|error| match error.kind() {
                io::ErrorKind::UnexpectedEof => NetSocketFrameError::Truncated,
                _ => NetSocketFrameError::Io(error),
            })?;
        Ok(Some(NetSocketFrame {
            frame_type: header[4],
            payload,
        }))
    }
}
