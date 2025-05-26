use std::fmt;

use futures_util::SinkExt; // Add this import for the send method
use futures_util::stream::{SplitSink, SplitStream, StreamExt};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

use crate::message;
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
                            match msg {
                                Message::Binary(data) => {
                                    tracing::info!("Received binary message from peer {}: {} bytes", self.peer_id, data.len());
                                    // Handle the binary message (e.g., forward it to other peers)
                                    if let Err(e) = self.handle_net_messsage(data.to_vec()).await {
                                        tracing::error!("Error handling network message: {}", e);
                                        break; // Exit the loop on message handling error
                                    }
                                }
                                Message::Text(text) => {
                                    tracing::info!("Received text message from peer {}: {}", self.peer_id, text);
                                    // Handle the text message if needed
                                }
                                Message::Close(_) => {
                                    tracing::info!("Peer {} requested to close the connection", self.peer_id);
                                    break; // Exit the loop on close message
                                }
                                _ => {
                                    tracing::warn!("Received unsupported message type from peer {}", self.peer_id);
                                }
                            }
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

    async fn handle_net_messsage(&mut self, data: Vec<u8>) -> anyhow::Result<()> {
        // validate version and message type
        tracing::info!("Handling network message of size: {} bytes", data.len());
        let msg_type = message::determine_client_message_type(&data);
        match msg_type {
            Ok(message::MessageType::Transport) => {
                // Handle transport message
                tracing::info!("Received transport message from peer {}", self.peer_id);
                self.handle_transport_message(data).await?
            }
            Ok(message::MessageType::Close) => {
                tracing::info!("Received close message from peer {}", self.peer_id);
                self.close();
            }
            Ok(message::MessageType::HealthCheck) => {
                tracing::info!("Received health check from peer {}", self.peer_id);
                // Respond to health check if needed
            }
            Ok(_) => {
                tracing::warn!(
                    "Received unsupported message type from peer {}",
                    self.peer_id
                );
            }
            Err(e) => {
                tracing::error!("Error determining message type: {}", e);
            }
        }
        Ok(())
    }

    async fn handle_transport_message(&mut self, mut data: Vec<u8>) -> anyhow::Result<()> {
        // Handle transport messages here
        tracing::info!("Handling transport message of size: {} bytes", data.len());
        let raw_dst_peer_id = message::unmarshal_transport_id(&data).map_err(|e| {
            tracing::error!("Failed to unmarshal transport ID: {}", e);
            e
        })?;
        let dst_peer_id = message::hash_id_to_string(&raw_dst_peer_id);
        let dst_peer_tx = self.store.get_peer(&dst_peer_id).ok_or_else(|| {
            tracing::error!("Destination peer {} not found in store", dst_peer_id);
            anyhow::anyhow!("Destination peer not found")
        })?;

        message::update_transport_msg(&mut data, &self.raw_peer_id)?;

        dst_peer_tx.send(data).await.map_err(|e| {
            tracing::error!("Failed to send message to peer {}: {}", dst_peer_id, e);
            e
        })?;

        Ok(())
    }

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
