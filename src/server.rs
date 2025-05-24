use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use futures_util::StreamExt;
use tokio::net::TcpStream;
use tracing::info;
use url::Url;

use crate::handshake;
use crate::message::marshal_auth_response;
use crate::peer::Peer;
use crate::store::Store;

/// The main server that handles incoming WebSocket connections
#[derive(Clone)]
pub struct Server {
    /// Store that manages all peer connections
    store: Arc<Store>,

    prepared_auth_response: Vec<u8>,
}

impl Server {
    /// Create a new Server instance
    pub fn new(exposed_address: String, tls_supported: bool) -> Result<Self> {
        let store = Arc::new(Store::new());
        let instance_url = get_instance_url(&exposed_address, tls_supported)?;
        let prepared_auth_response = marshal_auth_response(&instance_url)?;
        Ok(Server {
            store,
            prepared_auth_response,
        })
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

        let (mut outgoing, mut incoming) = ws_stream.split();

        let (raw_peer_id, peer_id) = handshake::handshake(
            &mut outgoing,
            &mut incoming,
            self.prepared_auth_response.clone(),
        )
        .await?;
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

/// Checks if user supplied a URL scheme otherwise adds to the
/// provided address according to TLS definition and parses the address before returning it
///
/// # Arguments
/// * `exposed_address` - A string representing the address that the relay server is exposed on
/// * `tls_supported` - A boolean indicating whether the relay server supports TLS
///
/// # Returns
/// * `Result<String>` - The parsed URL as a string or an error
fn get_instance_url(exposed_address: &str, tls_supported: bool) -> Result<String> {
    let addr = if exposed_address.contains("://") {
        // Address already has a scheme
        let parts: Vec<&str> = exposed_address.split("://").collect();
        if parts.len() > 2 {
            return Err(anyhow!("invalid exposed address: {}", exposed_address));
        }
        exposed_address.to_string()
    } else {
        // Add scheme based on TLS support
        if tls_supported {
            format!("rels://{}", exposed_address)
        } else {
            format!("rel://{}", exposed_address)
        }
    };

    // Parse the URL to validate it
    let parsed_url = Url::parse(&addr).map_err(|e| anyhow!("invalid exposed address: {}", e))?;

    // Validate the scheme
    if parsed_url.scheme() != "rel" && parsed_url.scheme() != "rels" {
        return Err(anyhow!("invalid scheme: {}", parsed_url.scheme()));
    }

    Ok(parsed_url.to_string())
}
