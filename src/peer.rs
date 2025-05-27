use std::fmt;

use bytes::Bytes;
use fastwebsockets::{FragmentCollector, Frame, OpCode, WebSocket};
use std::sync::Arc;
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

    /// The WebSocket stream
    ws_stream: WebSocket<tokio::net::TcpStream>,

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
        ws_stream: WebSocket<tokio::net::TcpStream>,
        rx_chan: tokio::sync::mpsc::Receiver<bytes::Bytes>,
        store: Arc<Store>,
    ) -> Self {
        Peer {
            raw_peer_id,
            peer_id,
            ws_stream,
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
                ws_result = self.ws_stream.read_frame() => {
                    match ws_result {
                        Ok(frame) => {
                            // Process the incoming WebSocket frame
                            if let Err(e) = self.handle_websocket_frame(frame).await {
                                error!("Error handling WebSocket frame: {}", e);
                                break; // Exit the loop on error
                            }
                        }
                        Err(e) => {
                            error!("Error receiving WebSocket frame: {}", e);
                            break; // Exit the loop on WebSocket error
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
                    // Gunakan referensi langsung ke data statis
                    let health_check_frame = Frame::new(true, OpCode::Binary, None, fastwebsockets::Payload::Borrowed(&message::HEALTH_CHECK_MSG));

                    if let Err(e) = self.ws_stream.write_frame(health_check_frame).await {
                        error!("Failed to send health check: {}", e);
                    }
                }
            }
        }

        // Tidak perlu flush lagi dengan fastwebsockets
        info!("Peer {} disconnected", self.peer_id);
    }

    /// Handles different types of WebSocket frames
    async fn handle_websocket_frame(&mut self, frame: Frame<'_>) -> anyhow::Result<()> {
        match frame.opcode {
            OpCode::Binary => {
                // Menggunakan referensi ke payload langsung tanpa alokasi tambahan
                let payload_ref = &frame.payload[..];
                debug!(
                    "Received binary frame from peer {}: {} bytes",
                    self.peer_id,
                    payload_ref.len()
                );
                // Determine message type
                match message::determine_client_message_type(payload_ref) {
                    Ok(message::MessageType::Transport) => {
                        // Cek jenis pesan transport berdasarkan header atau konten
                        if message::is_net_message(payload_ref) {
                            // Jika ini adalah pesan net, gunakan handler khusus
                            self.handle_net_binary_messsage(payload_ref).await?
                        } else {
                            // Jika ini adalah pesan transport biasa
                            self.handle_transport_message(payload_ref).await?
                        }
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
            }
            OpCode::Text => {
                error!(
                    "Received text frame from peer {}, not supported",
                    self.peer_id
                );
            }
            OpCode::Close => {
                info!("Peer {} requested to close the connection", self.peer_id);
                return Err(anyhow::anyhow!("Peer requested connection close"));
            }
            _ => {
                warn!("Received unsupported frame type from peer {}", self.peer_id);
            }
        }
        Ok(())
    }

    async fn handle_net_binary_messsage(&mut self, data: &[u8]) -> anyhow::Result<()> {
        // validate version and message type
        debug!("Handling network message of size: {} bytes", data.len());
        let msg_type = message::determine_client_message_type(data);
        match msg_type {
            Ok(message::MessageType::Transport) => {
                // Handle transport message
                debug!("Received transport message from peer {}", self.peer_id);
                self.handle_transport_message(data).await?
            }
            Ok(message::MessageType::Close) => {
                info!("Received close message from peer {}", self.peer_id);
                // Kirim frame close
                let close_frame = Frame::new(
                    true,
                    OpCode::Close,
                    None,
                    fastwebsockets::Payload::Owned(Vec::new()),
                );
                if let Err(e) = self.ws_stream.write_frame(close_frame).await {
                    error!("Failed to send close frame: {}", e);
                }
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

    async fn handle_transport_message(&mut self, data: &[u8]) -> anyhow::Result<()> {
        // Handle transport messages here
        debug!("Handling transport message of size: {} bytes", data.len());
        let raw_dst_peer_id = message::unmarshal_transport_id(data).map_err(|e| {
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
        // Start with the first message - konversi ke Vec untuk Payload::Owned
        let data_vec = first_data.to_vec();
        let first_frame = Frame::new(
            true,
            OpCode::Binary,
            None,
            fastwebsockets::Payload::Owned(data_vec),
        );
        self.ws_stream.write_frame(first_frame).await?;
        self.pending_bytes += first_data.len();

        // Batch additional messages using try_recv() - zero-copy approach
        loop {
            match self.rx_chan.try_recv() {
                Ok(data) => {
                    // We got more data, create a new frame dan konversi ke Vec untuk Payload::Owned
                    let data_vec = data.to_vec();
                    let frame = Frame::new(
                        true,
                        OpCode::Binary,
                        None,
                        fastwebsockets::Payload::Owned(data_vec),
                    );
                    self.ws_stream.write_frame(frame).await?;
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
                    // Channel closed, return error
                    return Err(anyhow::anyhow!("Channel disconnected"));
                }
            }
        }

        // Reset buffer counter after sending all messages
        debug!("Sent {} bytes in batch", self.pending_bytes);
        self.pending_bytes = 0;

        Ok(())
    }

    // Dengan fastwebsockets, kita tidak perlu fungsi flush_buffer lagi karena
    // write_frame langsung mengirim data tanpa buffering

    pub async fn close(&mut self) -> anyhow::Result<()> {
        // Kirim frame close untuk menutup koneksi WebSocket
        let close_frame = Frame::new(
            true,
            OpCode::Close,
            None,
            fastwebsockets::Payload::Owned(Vec::new()),
        );
        self.ws_stream.write_frame(close_frame).await?;
        info!("Closed WebSocket connection for peer {}", self.peer_id);
        Ok(())
    }
}

impl fmt::Display for Peer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.peer_id)
    }
}
