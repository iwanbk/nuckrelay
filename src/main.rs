use std::net::SocketAddr;
use std::sync::Arc;

mod handshake;
mod peer;
mod store;

use futures_util::StreamExt;
use tokio::net::{TcpListener, TcpStream};
use tracing::info;

use crate::peer::Peer;
use crate::store::Store;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create the peer store
    let store = Arc::new(Store::new());

    let try_socket = TcpListener::bind("127.0.0.1:9000").await;
    let listener = try_socket.expect("Failed to bind");
    info!("Listening on: {}", listener.local_addr().unwrap());

    while let Ok((stream, addr)) = listener.accept().await {
        let store_clone = Arc::clone(&store);
        tokio::spawn(handle_connection(stream, addr, store_clone));
    }

    Ok(())
}

async fn handle_connection(
    raw_stream: TcpStream,
    addr: SocketAddr,
    _store: Arc<Store>, // Using underscore prefix to indicate it's unused
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
    let _peer = Peer::new(raw_peer_id, peer_id.clone(), incoming, outgoing);

    // We're not adding the peer to the store for now
    info!("Peer {} created", peer_id);

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

    // No need to call store.delete_peer since we're not adding it to store
    info!("Peer {} disconnected", peer_id);
    info!("{} disconnected", &addr);
    Ok(())
}
