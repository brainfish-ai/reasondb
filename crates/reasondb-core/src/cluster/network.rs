//! Network layer for cluster communication
//!
//! Handles node-to-node communication for Raft messages.

use super::node::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Message types for cluster communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    /// Raft AppendEntries request
    AppendEntries {
        term: u64,
        leader_id: String,
        prev_log_index: u64,
        prev_log_term: u64,
        entries: Vec<Vec<u8>>,
        leader_commit: u64,
    },
    /// Raft AppendEntries response
    AppendEntriesResponse {
        term: u64,
        success: bool,
        match_index: Option<u64>,
    },
    /// Raft RequestVote request
    RequestVote {
        term: u64,
        candidate_id: String,
        last_log_index: u64,
        last_log_term: u64,
    },
    /// Raft RequestVote response
    RequestVoteResponse { term: u64, vote_granted: bool },
    /// Install snapshot request
    InstallSnapshot {
        term: u64,
        leader_id: String,
        last_included_index: u64,
        last_included_term: u64,
        offset: u64,
        data: Vec<u8>,
        done: bool,
    },
    /// Install snapshot response
    InstallSnapshotResponse { term: u64, success: bool },
    /// Heartbeat (lightweight AppendEntries)
    Heartbeat {
        term: u64,
        leader_id: String,
        leader_commit: u64,
    },
    /// Heartbeat response
    HeartbeatResponse { term: u64, success: bool },
    /// Forward write request to leader
    ForwardWrite { request_id: String, entry: Vec<u8> },
    /// Forward write response
    ForwardWriteResponse {
        request_id: String,
        success: bool,
        error: Option<String>,
    },
    /// Cluster status request
    StatusRequest,
    /// Cluster status response
    StatusResponse {
        node_id: String,
        is_leader: bool,
        term: u64,
        last_applied: u64,
        commit_index: u64,
    },
}

impl NetworkMessage {
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap_or_default()
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bincode::deserialize(bytes).ok()
    }
}

/// Network client for sending messages to other nodes
#[derive(Debug, Clone)]
pub struct NetworkClient {
    /// Known peer addresses
    peers: Arc<RwLock<HashMap<String, SocketAddr>>>,
    /// Request timeout
    timeout: std::time::Duration,
}

impl NetworkClient {
    /// Create a new network client
    pub fn new() -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
            timeout: std::time::Duration::from_secs(5),
        }
    }

    /// Add a peer
    pub async fn add_peer(&self, node_id: &NodeId, addr: SocketAddr) {
        let mut peers = self.peers.write().await;
        peers.insert(node_id.0.clone(), addr);
    }

    /// Remove a peer
    pub async fn remove_peer(&self, node_id: &NodeId) {
        let mut peers = self.peers.write().await;
        peers.remove(&node_id.0);
    }

    /// Get all peers
    pub async fn get_peers(&self) -> Vec<(NodeId, SocketAddr)> {
        let peers = self.peers.read().await;
        peers
            .iter()
            .map(|(id, addr)| (NodeId::new(id.clone()), *addr))
            .collect()
    }

    /// Send a message to a specific node
    pub async fn send(
        &self,
        node_id: &NodeId,
        message: NetworkMessage,
    ) -> Result<NetworkMessage, NetworkError> {
        let peers = self.peers.read().await;
        let addr = peers.get(&node_id.0).ok_or(NetworkError::PeerNotFound)?;

        self.send_to_addr(*addr, message).await
    }

    /// Send a message to an address
    pub async fn send_to_addr(
        &self,
        addr: SocketAddr,
        message: NetworkMessage,
    ) -> Result<NetworkMessage, NetworkError> {
        // Create HTTP client and send message
        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| NetworkError::ConnectionFailed(e.to_string()))?;

        let url = format!("http://{}/raft", addr);
        let response = client
            .post(&url)
            .body(message.to_bytes())
            .header("Content-Type", "application/octet-stream")
            .send()
            .await
            .map_err(|e| NetworkError::ConnectionFailed(e.to_string()))?;

        if !response.status().is_success() {
            return Err(NetworkError::RequestFailed(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| NetworkError::RequestFailed(e.to_string()))?;

        NetworkMessage::from_bytes(&bytes).ok_or(NetworkError::InvalidResponse)
    }

    /// Broadcast a message to all peers
    pub async fn broadcast(
        &self,
        message: NetworkMessage,
    ) -> Vec<(NodeId, Result<NetworkMessage, NetworkError>)> {
        let peers: Vec<_> = {
            let peers = self.peers.read().await;
            peers
                .iter()
                .map(|(id, addr)| (NodeId::new(id.clone()), *addr))
                .collect()
        };

        let mut results = Vec::new();
        for (node_id, addr) in peers {
            let result = self.send_to_addr(addr, message.clone()).await;
            results.push((node_id, result));
        }
        results
    }
}

impl Default for NetworkClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Network server for receiving messages from other nodes
pub struct NetworkServer {
    /// Local node ID
    node_id: NodeId,
    /// Listen address
    addr: SocketAddr,
    /// Message handler
    handler: Option<Arc<dyn MessageHandler + Send + Sync>>,
}

/// Trait for handling incoming network messages
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    /// Handle an incoming message
    async fn handle(&self, message: NetworkMessage) -> NetworkMessage;
}

impl NetworkServer {
    /// Create a new network server
    pub fn new(node_id: NodeId, addr: SocketAddr) -> Self {
        Self {
            node_id,
            addr,
            handler: None,
        }
    }

    /// Set the message handler
    pub fn set_handler<H: MessageHandler + 'static>(&mut self, handler: H) {
        self.handler = Some(Arc::new(handler));
    }

    /// Get the listen address
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Get the node ID
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }
}

/// Network errors
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("Peer not found")]
    PeerNotFound,
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Request failed: {0}")]
    RequestFailed(String),
    #[error("Invalid response")]
    InvalidResponse,
    #[error("Timeout")]
    Timeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let msg = NetworkMessage::Heartbeat {
            term: 5,
            leader_id: "node-1".to_string(),
            leader_commit: 100,
        };

        let bytes = msg.to_bytes();
        let decoded = NetworkMessage::from_bytes(&bytes).unwrap();

        match decoded {
            NetworkMessage::Heartbeat {
                term,
                leader_id,
                leader_commit,
            } => {
                assert_eq!(term, 5);
                assert_eq!(leader_id, "node-1");
                assert_eq!(leader_commit, 100);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[tokio::test]
    async fn test_network_client_peers() {
        let client = NetworkClient::new();

        client
            .add_peer(&NodeId::new("node-1"), "127.0.0.1:4445".parse().unwrap())
            .await;
        client
            .add_peer(&NodeId::new("node-2"), "127.0.0.1:4446".parse().unwrap())
            .await;

        let peers = client.get_peers().await;
        assert_eq!(peers.len(), 2);

        client.remove_peer(&NodeId::new("node-1")).await;
        let peers = client.get_peers().await;
        assert_eq!(peers.len(), 1);
    }
}
