use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use tokio::sync::mpsc::Sender;

/// Store is a thread-safe store of peers
/// It is used to store the peers that are connected to the relay server
pub struct Store {
    peers_tx: RwLock<HashMap<String, Arc<Sender<bytes::Bytes>>>>,
}

impl Store {
    /// Creates a new Store instance
    pub fn new() -> Self {
        Store {
            peers_tx: RwLock::new(HashMap::new()),
        }
    }

    /// Adds a peer to the store
    pub fn add_peer(&self, peer_id: String, peer_tx: Arc<Sender<bytes::Bytes>>) {
        let mut peers = self.peers_tx.write().unwrap();

        // If a peer with the same ID already exists, close it
        if let Some(_) = peers.get(peer_id.as_str()) {
            //old_peer.close();
            tracing::info!("Closing existing peer with ID: {}", peer_id);
        }

        peers.insert(peer_id, peer_tx);
    }

    /// Deletes a peer from the store
    pub fn delete_peer(&self, peer_id: &str, peer_tx: &Arc<Sender<bytes::Bytes>>) {
        let mut peers = self.peers_tx.write().unwrap();

        // Check if the peer exists in the store
        if let Some(stored_peer) = peers.get(peer_id) {
            // Check if it's the same peer (by reference comparison)
            if !Arc::ptr_eq(stored_peer, peer_tx) {
                return;
            }

            peers.remove(peer_id);
        }
    }

    /// Returns a peer by its ID
    pub fn get_peer(&self, id: &str) -> Option<Arc<Sender<bytes::Bytes>>> {
        let peers = self.peers_tx.read().unwrap();
        peers.get(id).cloned()
    }

    /// Returns all the peers in the store
    pub fn get_all_peers(&self) -> Vec<Arc<Sender<bytes::Bytes>>> {
        let peers = self.peers_tx.read().unwrap();
        peers.values().cloned().collect()
    }
}
