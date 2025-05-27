use std::fmt;

use bytes::Bytes;
use futures_util::SinkExt; // Add this import for the send method
use futures_util::stream::{SplitSink, SplitStream, StreamExt};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

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

    rx_chan: tokio::sync::mpsc::Receiver<bytes::Bytes>,

    /// Reference to the store that manages peers
    store: Arc<Store>,

    /// Buffer to accumulate data before flushing
    pending_bytes: usize,
}

// Buffer size constants
const BUFFER_FLUSH_SIZE: usize = 4096; // 8KB - optimal for most network applications

impl Peer {
    /// Creates a new Peer instance
    pub fn new(
        raw_peer_id: Vec<u8>,
        peer_id: String,
        rx_conn: SplitStream<WebSocketStream<TcpStream>>,
        tx_conn: SplitSink<WebSocketStream<TcpStream>, Message>,
        rx_chan: tokio::sync::mpsc::Receiver<bytes::Bytes>,
        store: Arc<Store>,
    ) -> Self {
        Peer {
            raw_peer_id,
            peer_id,
            rx_conn,
            tx_conn,
            rx_chan,
            store,
            pending_bytes: 0, // Initialize buffer counter
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
                            if let Err(e) = self.handle_websocket_message(msg).await {
                                error!("Error handling WebSocket message: {}", e);
                                break; // Exit the loop on error
                            }
                        }
                        Some(Err(e)) => {
                            error!("Error receiving WebSocket message: {}", e);
                            break; // Exit the loop on WebSocket error
                        }
                        None => {
                            info!("WebSocket stream ended for peer {}", self.peer_id);
                            break; // Exit the loop when WebSocket stream ends
                        }
                    }
                },

                // Handle messages from other peers via the channel with immediate batching
                message = self.rx_chan.recv() => {
                    match message {
                        Some(data) => {
                            if let Err(e) = self.handle_channel_message_with_batching(data).await {
                                error!("Error handling channel message: {}", e);
                                break;
                            }
                        }
                        None => {
                            info!("Channel for peer {} closed", self.peer_id);
                            break; // Exit the loop when channel is closed
                        }
                    }
                },

                // Periodic 25-second status update
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(25)) => {
                    let health_check_bytes = Bytes::from_static(&message::HEALTH_CHECK_MSG);

                    self.tx_conn.send(Message::Binary(health_check_bytes)).await
                        .unwrap_or_else(|e| error!("Failed to send ping: {}", e));
                }
            }
        }

        // Ensure any remaining data is flushed before closing
        if self.pending_bytes > 0 {
            let _ = self.flush_buffer().await;
        }

        info!("Peer {} disconnected", self.peer_id);
    }

    /// Handles different types of WebSocket messages
    async fn handle_websocket_message(&mut self, msg: Message) -> anyhow::Result<()> {
        match msg {
            Message::Binary(data) => {
                debug!(
                    "Received binary message from peer {}: {} bytes",
                    self.peer_id,
                    data.len()
                );
                // Handle the binary message (e.g., forward it to other peers)
                self.handle_net_binary_messsage(data).await?;
            }
            Message::Text(text) => {
                error!("Received text message from peer {}: {}", self.peer_id, text);
                // Handle the text message if needed
            }
            Message::Close(_) => {
                info!("Peer {} requested to close the connection", self.peer_id);
                return Err(anyhow::anyhow!("Peer requested connection close"));
            }
            _ => {
                warn!(
                    "Received unsupported message type from peer {}",
                    self.peer_id
                );
            }
        }
        Ok(())
    }

    async fn handle_net_binary_messsage(&mut self, data: bytes::Bytes) -> anyhow::Result<()> {
        // validate version and message type
        debug!("Handling network message of size: {} bytes", data.len());
        let msg_type = message::determine_client_message_type(&data);
        match msg_type {
            Ok(message::MessageType::Transport) => {
                // Handle transport message
                debug!("Received transport message from peer {}", self.peer_id);
                self.handle_transport_message(data).await?
            }
            Ok(message::MessageType::Close) => {
                info!("Received close message from peer {}", self.peer_id);
                self.close();
            }
            Ok(message::MessageType::HealthCheck) => {
                info!("Received health check from peer {}", self.peer_id);
                // Respond to health check if needed
            }
            Ok(_) => {
                warn!(
                    "Received unsupported message type from peer {}",
                    self.peer_id
                );
            }
            Err(e) => {
                error!("Error determining message type: {}", e);
            }
        }
        Ok(())
    }

    async fn handle_transport_message(&mut self, data: bytes::Bytes) -> anyhow::Result<()> {
        // Handle transport messages here
        debug!("Handling transport message of size: {} bytes", data.len());
        let raw_dst_peer_id = message::unmarshal_transport_id(&data).map_err(|e| {
            error!("Failed to unmarshal transport ID: {}", e);
            e
        })?;
        let dst_peer_id = message::hash_id_to_string(&raw_dst_peer_id);
        let dst_peer_tx = self.store.get_peer(&dst_peer_id).ok_or_else(|| {
            error!("Destination peer {} not found in store", dst_peer_id);
            anyhow::anyhow!("Destination peer not found")
        })?;

        // Create a BytesMut with the same data
        let mut data_mut = bytes::BytesMut::with_capacity(data.len());
        data_mut.extend_from_slice(&data);

        message::update_transport_msg(&mut data_mut, &self.raw_peer_id)?;

        // Convert back to Bytes (this is zero-copy)
        let updated_data = data_mut.freeze();

        dst_peer_tx.send(updated_data).await.map_err(|e| {
            error!("Failed to send message to peer {}: {}", dst_peer_id, e);
            e
        })?;

        Ok(())
    }

    /// Handle channel message with smart batching using try_recv()
    /// This approach is more efficient than timeout-based batching
    async fn handle_channel_message_with_batching(
        &mut self,
        first_data: bytes::Bytes,
    ) -> anyhow::Result<()> {
        // Start with the first message
        self.tx_conn
            .feed(Message::Binary(first_data.clone()))
            .await?;
        self.pending_bytes += first_data.len();

        // Batch additional messages using try_recv() - zero-copy approach
        loop {
            match self.rx_chan.try_recv() {
                Ok(data) => {
                    // We got more data, add it to the buffer
                    self.tx_conn.feed(Message::Binary(data.clone())).await?;
                    self.pending_bytes += data.len();

                    // Check if we should flush (buffer size limit reached)
                    if self.pending_bytes >= BUFFER_FLUSH_SIZE {
                        break; // Exit loop to flush
                    }
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    // No more messages immediately available
                    break;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    // Channel closed, flush and return error
                    self.flush_buffer().await?;
                    return Err(anyhow::anyhow!("Channel disconnected"));
                }
            }
        }

        // Flush all buffered data once after the loop
        self.flush_buffer().await?;

        Ok(())
    }

    /// Flushes the buffered WebSocket messages
    async fn flush_buffer(&mut self) -> anyhow::Result<()> {
        if let Err(e) = self.tx_conn.flush().await {
            error!("Failed to flush WebSocket buffer: {}", e);
            return Err(anyhow::anyhow!("WebSocket flush failed: {}", e));
        }
        debug!("Flushed {} bytes from buffer", self.pending_bytes);
        self.pending_bytes = 0; // Reset buffer counter

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
