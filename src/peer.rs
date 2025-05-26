use std::fmt;

use futures_util::SinkExt; // Add this import for the send method
use futures_util::stream::{SplitSink, SplitStream, StreamExt};
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
    rx_conn: SplitStream<WebSocketStream<TcpStream>>,

    /// The outgoing part of the WebSocket stream
    tx_conn: SplitSink<WebSocketStream<TcpStream>, Message>,

    rx_chan: tokio::sync::mpsc::Receiver<Vec<u8>>,

    /// Reference to the store that manages peers
    store: Arc<Store>,
}

impl Peer {
    /// Creates a new Peer instance
    pub fn new(
        raw_peer_id: Vec<u8>,
        peer_id: String,
        rx_conn: SplitStream<WebSocketStream<TcpStream>>,
        tx_conn: SplitSink<WebSocketStream<TcpStream>, Message>,
        rx_chan: tokio::sync::mpsc::Receiver<Vec<u8>>,
        store: Arc<Store>,
    ) -> Self {
        Peer {
            raw_peer_id,
            peer_id,
            rx_conn,
            tx_conn,
            rx_chan,
            store,
        }
    }

    pub async fn work(&mut self) {
        // Main event loop
        loop {
            // Use tokio::select! to concurrently wait on multiple async operations
            tokio::select! {
                // Handle incoming WebSocket messages
                message = self.rx_conn.next() => {
                    match message {
                        Some(Ok(msg)) => {
                            // Process the incoming WebSocket message
                            tracing::info!("Received WebSocket message: {:?}", msg);
                            // TODO: Process different message types
                        }
                        Some(Err(e)) => {
                            tracing::error!("Error receiving WebSocket message: {}", e);
                            break; // Exit the loop on WebSocket error
                        }
                        None => {
                            tracing::info!("WebSocket stream ended for peer {}", self.peer_id);
                            break; // Exit the loop when WebSocket stream ends
                        }
                    }
                },

                // Handle messages from other peers via the channel
                message = self.rx_chan.recv() => {
                    match message {
                        Some(data) => {
                            tracing::info!("Received message via channel, size: {} bytes", data.len());
                            // Forward messages to connected client
                            match self.tx_conn.send(Message::Binary(data.into())).await {
                                Ok(_) => tracing::debug!("Successfully forwarded message to peer"),
                                Err(e) => {
                                    tracing::error!("Failed to forward message: {}", e);
                                    break; // Exit the loop on send error
                                }
                            }
                        }
                        None => {
                            tracing::info!("Channel for peer {} closed", self.peer_id);
                            break; // Exit the loop when channel is closed
                        }
                    }
                }
            }
        }

        tracing::info!("Peer {} disconnected", self.peer_id);
    }

    #[allow(dead_code)]
    pub fn close(&self) {
        // Implementation would go here
        // In the future, this would close the WebSocket connection
    }
}

impl fmt::Display for Peer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.peer_id)
    }
}
