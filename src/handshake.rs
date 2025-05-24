use std::{error::Error, fmt};

use anyhow::Result;
use futures_util::StreamExt;
use futures_util::stream::SplitStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info};

use crate::message::{CURRENT_PROTO_VERSION, MAX_HANDSHAKE_SIZE, PROTO_HEADER_SIZE};
use crate::message::{
    MessageError, MessageType, determine_client_message_type, hash_id_to_string, unmarshal_auth_msg,
};

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

impl From<MessageError> for HandshakeError {
    fn from(error: MessageError) -> Self {
        match error {
            MessageError::InvalidMsgLength => HandshakeError::InvalidMsgLength,
            MessageError::InvalidMsgType => HandshakeError::InvalidMsgType,
        }
    }
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

pub fn handle_auth_message(data: &[u8]) -> Result<(Vec<u8>, String), HandshakeError> {
    // Handle the authentication message here
    // For now, just print the data
    info!("Handling auth message: {:?}", data);

    // MessageError will be automatically converted to HandshakeError via the From trait
    let (raw_peer_id, _auth_payload) = unmarshal_auth_msg(data)?;

    // TODO : validate auth_payload
    let peer_id = hash_id_to_string(&raw_peer_id);

    Ok((raw_peer_id, peer_id))
}
