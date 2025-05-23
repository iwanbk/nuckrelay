use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::peer::Peer;

/// Store is a thread-safe store of peers
/// It is used to store the peers that are connected to the relay server
pub struct Store {
    peers: RwLock<HashMap<String, Arc<Peer>>>,
}

impl Store {
    /// Creates a new Store instance
    pub fn new() -> Self {
        Store {
            peers: RwLock::new(HashMap::new()),
        }
    }

    /// Adds a peer to the store
    pub fn add_peer(&self, peer: Arc<Peer>) {
        let mut peers = self.peers.write().unwrap();

        // If a peer with the same ID already exists, close it
        if let Some(old_peer) = peers.get(&peer.to_string()) {
            old_peer.close();
        }

        peers.insert(peer.to_string(), peer);
    }

    /// Deletes a peer from the store
    pub fn delete_peer(&self, peer: &Arc<Peer>) {
        let mut peers = self.peers.write().unwrap();

        // Check if the peer exists in the store
        if let Some(stored_peer) = peers.get(&peer.to_string()) {
            // Check if it's the same peer (by reference comparison)
            if !Arc::ptr_eq(stored_peer, peer) {
                return;
            }

            peers.remove(&peer.to_string());
        }
    }

    /// Returns a peer by its ID
    pub fn get_peer(&self, id: &str) -> Option<Arc<Peer>> {
        let peers = self.peers.read().unwrap();
        peers.get(id).cloned()
    }

    /// Returns all the peers in the store
    pub fn get_all_peers(&self) -> Vec<Arc<Peer>> {
        let peers = self.peers.read().unwrap();
        peers.values().cloned().collect()
    }
}
