use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::StreamExt;
use tokio::net::TcpStream;
use tracing::info;

use crate::handshake;
use crate::peer::Peer;
use crate::store::Store;

/// The main server that handles incoming WebSocket connections
#[derive(Clone)]
pub struct Server {
    /// Store that manages all peer connections
    store: Arc<Store>,
}

impl Server {
    /// Create a new Server instance
    pub fn new() -> Self {
        let store = Arc::new(Store::new());
        Server { store }
    }

    /// Handle a new connection from a client
    pub async fn handle_connection(
        &self,
        raw_stream: TcpStream,
        addr: SocketAddr,
    ) -> anyhow::Result<()> {
        info!("Incoming TCP connection from: {}", addr);

        let ws_stream = tokio_tungstenite::accept_async(raw_stream)
            .await
            .expect("Error during the websocket handshake occurred");
        info!("WebSocket connection established: {}", addr);

        let (outgoing, mut incoming) = ws_stream.split();

        // ------ do handshake
        let (raw_peer_id, peer_id) = handshake::handshake(&mut incoming).await?;
        info!("Handshake successful for peer: {}", peer_id);

        // Create a new peer
        let peer = Arc::new(Peer::new(
            raw_peer_id,
            peer_id.clone(),
            incoming,
            outgoing,
            self.store.clone(),
        ));

        // Add the peer to the store
        self.store.add_peer(peer.clone());
        info!("Peer {} added to store", peer_id);

        // Wait for the peer to disconnect or for an error
        // TODO: Implement peer.work() to handle messages

        /*
        go func() {
            peer.Work()
            r.store.DeletePeer(peer)
            peer.log.Debugf("relay connection closed")
            r.metrics.PeerDisconnected(peer.String())
        }()

        if err := h.handshakeResponse(); err != nil {
            log.Errorf("failed to send handshake response, close peer: %s", err)
            peer.Close()
        }
        */

        // For now, just sleep for a bit to keep the connection alive
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;

        // Clean up by removing peer from store
        // self.store.delete_peer(&peer);
        // info!("Peer {} removed from store", peer_id);
        info!("{} disconnected", &addr);
        Ok(())
    }
}
