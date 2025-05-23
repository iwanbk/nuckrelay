use futures_util::stream::{SplitSink, SplitStream};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

use crate::store::Store;

/// Represents a peer connected to the relay server
#[allow(dead_code)]
pub struct Peer {
    /// The raw peer ID as bytes
    raw_peer_id: Vec<u8>,

    /// The peer ID as a string (for display and lookup)
    peer_id: String,

    /// The incoming part of the WebSocket stream
    incoming: SplitStream<WebSocketStream<TcpStream>>,

    /// The outgoing part of the WebSocket stream
    outgoing: SplitSink<WebSocketStream<TcpStream>, Message>,

    /// Reference to the store that manages peers
    store: Arc<Store>,
}

impl Peer {
    /// Creates a new Peer instance
    pub fn new(
        raw_peer_id: Vec<u8>,
        peer_id: String,
        incoming: SplitStream<WebSocketStream<TcpStream>>,
        outgoing: SplitSink<WebSocketStream<TcpStream>, Message>,
        store: Arc<Store>,
    ) -> Self {
        Peer {
            raw_peer_id,
            peer_id,
            incoming,
            outgoing,
            store,
        }
    }

    #[allow(dead_code)]
    pub fn close(&self) {
        // Implementation would go here
        // In the future, this would close the WebSocket connection
    }
}

impl ToString for Peer {
    fn to_string(&self) -> String {
        self.peer_id.clone()
    }
}
