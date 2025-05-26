use std::{error::Error, fmt};

use anyhow::Result;
use base64::prelude::*;
use tracing::error;

#[derive(Debug)]
pub enum MessageError {
    InvalidMsgLength,
    InvalidMsgType,
}

impl fmt::Display for MessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageError::InvalidMsgLength => write!(f, "Invalid message length"),
            MessageError::InvalidMsgType => write!(f, "Invalid message type"),
        }
    }
}

impl Error for MessageError {}

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

pub const MAX_HANDSHAKE_SIZE: usize = 212;
pub const MAX_HANDSHAKE_RESP_SIZE: usize = 8192;
pub const SIZE_OF_VERSION_BYTE: usize = 1;
pub const SIZE_OF_MSG_TYPE: usize = 1;
pub const PROTO_HEADER_SIZE: usize = SIZE_OF_VERSION_BYTE + SIZE_OF_MSG_TYPE;
pub const CURRENT_PROTO_VERSION: i64 = 1;
pub const OFFSET_TRANSPORT_ID: usize = PROTO_HEADER_SIZE;

/// Creates a response message to the auth.
///
/// In case of successful connection, the server responds with an AuthResponse message.
/// This message contains the server's instance URL. This URL will be used to choose
/// the common Relay server in case the peers are in different Relay servers.
///
/// This is a Rust implementation of the Go function:
/// https://github.com/netbirdio/netbird/blob/5bed6777d568f1e0e96a4c3dbd290e175502eb81/relay/messages/message.go#L234-L248
pub fn marshal_auth_response(address: &str) -> Result<Vec<u8>, MessageError> {
    let address_bytes = address.as_bytes();

    // Pre-allocate with capacity for the header plus the address
    let mut msg = Vec::with_capacity(PROTO_HEADER_SIZE + address_bytes.len());

    // Set protocol version and message type
    msg.push(CURRENT_PROTO_VERSION as u8);
    msg.push(MessageType::AuthResponse as u8);

    // Append the address bytes
    msg.extend_from_slice(address_bytes);

    // Check if the message length exceeds the maximum allowed size
    if msg.len() > MAX_HANDSHAKE_RESP_SIZE {
        return Err(MessageError::InvalidMsgLength);
    }

    Ok(msg)
}

// Constants needed for unmarshal_auth_msg
const SIZE_OF_MAGIC_BYTE: usize = 4;
const MAGIC_HEADER: [u8; 4] = [0x21, 0x12, 0xA4, 0x42];
const HEADER_TOTAL_SIZE_AUTH: usize = PROTO_HEADER_SIZE + SIZE_OF_MAGIC_BYTE + 36; // 36 is ID_SIZE from Go code
const OFFSET_MAGIC_BYTE: usize = PROTO_HEADER_SIZE;
const OFFSET_AUTH_PEER_ID: usize = PROTO_HEADER_SIZE + SIZE_OF_MAGIC_BYTE;

const HEADER_SIZE_TRANSPORT: usize = 36; // IDSize, which is 36 from the Go code
const HEADER_TOTAL_SIZE_TRANSPORT: usize = PROTO_HEADER_SIZE + HEADER_SIZE_TRANSPORT;

pub fn determine_client_message_type(data: &[u8]) -> Result<MessageType, MessageError> {
    if data.len() < PROTO_HEADER_SIZE {
        return Err(MessageError::InvalidMsgLength);
    }

    let msg_type = data[1];

    match msg_type {
        1 => Ok(MessageType::Hello),
        3 => Ok(MessageType::Transport),
        4 => Ok(MessageType::Close),
        5 => Ok(MessageType::HealthCheck),
        6 => Ok(MessageType::Auth),
        _ => Err(MessageError::InvalidMsgType),
    }
}

/// Extracts peerID and the auth payload from the message
///
/// This is a Rust implementation of the Go function in:
/// https://github.com/netbirdio/netbird/blob/2a89d6e47a4c144dd1e1162f3e5d3cb73525ae77/relay/messages/message.go#L219-L228
pub fn unmarshal_auth_msg(msg: &[u8]) -> Result<(Vec<u8>, Vec<u8>), MessageError> {
    if msg.len() < HEADER_TOTAL_SIZE_AUTH {
        error!("Invalid message length: {}", msg.len());
        return Err(MessageError::InvalidMsgLength);
    }

    // Check magic header
    if msg[OFFSET_MAGIC_BYTE..OFFSET_MAGIC_BYTE + SIZE_OF_MAGIC_BYTE] != MAGIC_HEADER {
        error!("Invalid magic header");
        return Err(MessageError::InvalidMsgType);
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

/// Extracts the peerID from the transport message.
///
/// This is a Rust implementation of the Go function:
/// https://github.com/netbirdio/netbird/blob/670446d42e385397b8be87b13c5fd504c303ce86/relay/messages/message.go#L295-L300
pub fn unmarshal_transport_id(buf: &[u8]) -> Result<Vec<u8>, MessageError> {
    if buf.len() < HEADER_TOTAL_SIZE_TRANSPORT {
        return Err(MessageError::InvalidMsgLength);
    }

    Ok(buf[OFFSET_TRANSPORT_ID..OFFSET_TRANSPORT_ID + HEADER_SIZE_TRANSPORT].to_vec())
}

/// Updates the peerID in the transport message.
///
/// This is a Rust implementation of the Go function:
/// https://github.com/netbirdio/netbird/blob/main/relay/messages/message.go#L306-L310
pub fn update_transport_msg(msg: &mut [u8], peer_id: &[u8]) -> Result<(), MessageError> {
    if msg.len() < OFFSET_TRANSPORT_ID + peer_id.len() {
        return Err(MessageError::InvalidMsgLength);
    }

    // Copy the peer_id into the message at the transport ID offset
    msg[OFFSET_TRANSPORT_ID..OFFSET_TRANSPORT_ID + peer_id.len()].copy_from_slice(peer_id);

    Ok(())
}
