use std::net::SocketAddr;

mod handshake;

use futures_util::StreamExt;
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let try_socket = TcpListener::bind("127.0.0.1:9000").await;
    let listener = try_socket.expect("Failed to bind");
    info!("Listening on: {}", listener.local_addr().unwrap());

    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(stream, addr));
    }

    Ok(())
}

async fn handle_connection(raw_stream: TcpStream, addr: SocketAddr) -> anyhow::Result<()> {
    info!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    info!("WebSocket connection established: {}", addr);

    let (outgoing, incoming) = ws_stream.split();

    // Insert the write part of this peer to the peer map.
    //let (tx, rx) = unbounded();
    //peer_map.lock().unwrap().insert(addr, tx);

    // ------ do handshake
    let (raw_peer_id, peer_id) = handshake::handshake(incoming).await?;

    // create peer
    /*peer := NewPeer(r.metrics, peerID, conn, r.store)
    peer.log.Infof("peer connected from: %s", conn.RemoteAddr())
    storeTime := time.Now()
    r.store.AddPeer(peer)
    r.metrics.RecordPeerStoreTime(time.Since(storeTime))
    r.metrics.PeerConnected(peer.String())
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
    r.metrics.RecordAuthenticationTime(time.Since(acceptTime))
    */

    info!("{} disconnected", &addr);
    Ok(())
}
