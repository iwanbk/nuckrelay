use std::{error::Error, fmt};

use anyhow::Result;
use base64::prelude::*;
use futures_util::StreamExt;
use futures_util::stream::SplitStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info};

#[derive(Debug)]
pub enum HandshakeError {
    InvalidMsgLength,
    WebSocketError(tokio_tungstenite::tungstenite::Error),
    UnsupportedVersion,
    StreamClosed,
    InvalidMsgType,
}

impl fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HandshakeError::InvalidMsgLength => write!(f, "Invalid handshake message length"),
            HandshakeError::WebSocketError(e) => write!(f, "WebSocket error: {}", e),
            HandshakeError::StreamClosed => write!(f, "WebSocket stream closed before handshake"),
            HandshakeError::UnsupportedVersion => write!(f, "Unsupported protocol version"),
            HandshakeError::InvalidMsgType => write!(f, "Invalid message type"),
        }
    }
}

impl Error for HandshakeError {}

impl From<tokio_tungstenite::tungstenite::Error> for HandshakeError {
    fn from(error: tokio_tungstenite::tungstenite::Error) -> Self {
        HandshakeError::WebSocketError(error)
    }
}

#[derive(PartialEq, Debug)]
#[allow(dead_code)]
pub enum MessageType {
    Unknown = 0,
    // Deprecated: Use MsgTypeAuth instead.
    Hello = 1,
    // Deprecated: Use MsgTypeAuthResponse instead.
    HelloResponse = 2,
    Transport = 3,
    Close = 4,
    HealthCheck = 5,
    Auth = 6,
    AuthResponse = 7,
}

/// Reads the handshake data from the incoming WebSocket stream
pub async fn handshake<S>(
    incoming: &mut SplitStream<WebSocketStream<S>>,
) -> Result<(Vec<u8>, String), HandshakeError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    // Get the next message from the stream
    let message = incoming.next().await.ok_or_else(|| {
        error!("WebSocket stream closed before handshake");
        HandshakeError::StreamClosed
    })??;

    // Extract data from the message
    let data = match message {
        Message::Binary(data) => data,
        other => {
            error!("Expected binary message for handshake, got: {:?}", other);
            return Err(HandshakeError::InvalidMsgLength);
        }
    };

    // Validate message size
    if data.len() > MAX_HANDSHAKE_SIZE {
        error!("Handshake too large: {} bytes", data.len());
        return Err(HandshakeError::InvalidMsgLength);
    }

    info!("Received handshake data of size: {} bytes", data.len());

    // make sure the data length is less that PROTO_HEADER_SIZE
    if data.len() < PROTO_HEADER_SIZE {
        error!(
            "Handshake too small: {} bytes, expected at least {} bytes",
            data.len(),
            PROTO_HEADER_SIZE
        );
        return Err(HandshakeError::InvalidMsgLength);
    }

    // Check protocol version
    if data[0] as i64 != CURRENT_PROTO_VERSION {
        error!(
            "Unsupported protocol version: {}, expected: {}",
            data[0], CURRENT_PROTO_VERSION
        );
        return Err(HandshakeError::UnsupportedVersion);
    }

    // Now we can use data here and in subsequent code
    let msg_type = determine_client_message_type(&data)?;

    if msg_type != MessageType::Auth {
        error!("Invalid message type: {:?}", msg_type);
        return Err(HandshakeError::InvalidMsgType);
    }

    // handle auth message
    let (raw_peer_id, peer_id) = handle_auth_message(&data)?;

    Ok((raw_peer_id, peer_id))
}

pub fn determine_client_message_type(data: &[u8]) -> Result<MessageType, HandshakeError> {
    if data.len() < PROTO_HEADER_SIZE {
        return Err(HandshakeError::InvalidMsgLength);
    }

    let msg_type = data[1];

    match msg_type {
        1 => Ok(MessageType::Hello),
        3 => Ok(MessageType::Transport),
        4 => Ok(MessageType::Close),
        5 => Ok(MessageType::HealthCheck),
        6 => Ok(MessageType::Auth),
        _ => Err(HandshakeError::InvalidMsgType),
    }
}

pub fn handle_auth_message(data: &[u8]) -> Result<(Vec<u8>, String), HandshakeError> {
    // Handle the authentication message here
    // For now, just print the data
    info!("Handling auth message: {:?}", data);
    let (raw_peer_id, _auth_payload) = unmarshal_auth_msg(data)?;

    // TODO : validate auth_payload
    let peer_id = hash_id_to_string(&raw_peer_id);

    Ok((raw_peer_id, peer_id))
}

/// Extracts peerID and the auth payload from the message
///
/// This is a Rust implementation of the Go function in:
/// https://github.com/netbirdio/netbird/blob/2a89d6e47a4c144dd1e1162f3e5d3cb73525ae77/relay/messages/message.go#L219-L228
pub fn unmarshal_auth_msg(msg: &[u8]) -> Result<(Vec<u8>, Vec<u8>), HandshakeError> {
    const HEADER_TOTAL_SIZE_AUTH: usize = PROTO_HEADER_SIZE + SIZE_OF_MAGIC_BYTE + 36; // 36 is ID_SIZE from Go code
    const OFFSET_MAGIC_BYTE: usize = PROTO_HEADER_SIZE;
    const OFFSET_AUTH_PEER_ID: usize = PROTO_HEADER_SIZE + SIZE_OF_MAGIC_BYTE;
    const SIZE_OF_MAGIC_BYTE: usize = 4;

    const MAGIC_HEADER: [u8; 4] = [0x21, 0x12, 0xA4, 0x42];

    if msg.len() < HEADER_TOTAL_SIZE_AUTH {
        error!("Invalid message length: {}", msg.len());
        return Err(HandshakeError::InvalidMsgLength);
    }

    // Check magic header
    if msg[OFFSET_MAGIC_BYTE..OFFSET_MAGIC_BYTE + SIZE_OF_MAGIC_BYTE] != MAGIC_HEADER {
        error!("Invalid magic header");
        return Err(HandshakeError::InvalidMsgType);
    }

    // Extract peer ID and auth payload
    let peer_id = msg[OFFSET_AUTH_PEER_ID..HEADER_TOTAL_SIZE_AUTH].to_vec();
    let auth_payload = msg[HEADER_TOTAL_SIZE_AUTH..].to_vec();

    Ok((peer_id, auth_payload))
}

/// Converts a hash ID to a human-readable string
///
/// This is a Rust implementation of the Go function:
/// https://github.com/netbirdio/netbird/blob/main/relay/messages/id.go#L29-L31
pub fn hash_id_to_string(id_hash: &[u8]) -> String {
    const PREFIX_LENGTH: usize = 4;

    if id_hash.len() <= PREFIX_LENGTH {
        return String::from_utf8_lossy(id_hash).to_string();
    }

    // Take the first PREFIX_LENGTH bytes as is (typically "sha-")
    let prefix = String::from_utf8_lossy(&id_hash[..PREFIX_LENGTH]);

    // Base64 encode the rest of the bytes
    let encoded = BASE64_STANDARD.encode(&id_hash[PREFIX_LENGTH..]);

    // Combine prefix and encoded data
    format!("{}{}", prefix, encoded)
}

pub const MAX_HANDSHAKE_SIZE: usize = 212;
pub const SIZE_OF_VERSION_BYTE: usize = 1;
pub const SIZE_OF_MSG_TYPE: usize = 1;
pub const PROTO_HEADER_SIZE: usize = SIZE_OF_VERSION_BYTE + SIZE_OF_MSG_TYPE;
pub const CURRENT_PROTO_VERSION: i64 = 1;
