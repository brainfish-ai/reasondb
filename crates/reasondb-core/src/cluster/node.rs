//! Cluster node identity and metadata

use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::SocketAddr;

/// Unique identifier for a cluster node
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl NodeId {
    /// Create a new node ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a random node ID
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Get the ID as a string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Role of a node in the cluster
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NodeRole {
    /// Leader node - handles all writes
    Leader,
    /// Follower node - replicates from leader, can serve reads
    #[default]
    Follower,
    /// Candidate - participating in leader election
    Candidate,
    /// Learner - non-voting member catching up
    Learner,
}

impl fmt::Display for NodeRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeRole::Leader => write!(f, "Leader"),
            NodeRole::Follower => write!(f, "Follower"),
            NodeRole::Candidate => write!(f, "Candidate"),
            NodeRole::Learner => write!(f, "Learner"),
        }
    }
}

/// Status of a cluster node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Node is healthy and responding
    #[default]
    Healthy,
    /// Node is suspected to be down
    Suspect,
    /// Node is confirmed down
    Down,
    /// Node is joining the cluster
    Joining,
    /// Node is leaving the cluster
    Leaving,
}

impl fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeStatus::Healthy => write!(f, "Healthy"),
            NodeStatus::Suspect => write!(f, "Suspect"),
            NodeStatus::Down => write!(f, "Down"),
            NodeStatus::Joining => write!(f, "Joining"),
            NodeStatus::Leaving => write!(f, "Leaving"),
        }
    }
}

/// Information about a cluster node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterNode {
    /// Unique identifier
    pub id: NodeId,
    /// Human-readable name
    pub name: String,
    /// Address for client connections (HTTP API)
    pub api_addr: SocketAddr,
    /// Address for cluster communication (Raft)
    pub raft_addr: SocketAddr,
    /// Current role in the cluster
    pub role: NodeRole,
    /// Current status
    pub status: NodeStatus,
    /// When the node joined the cluster
    pub joined_at: i64,
    /// Last heartbeat timestamp
    pub last_heartbeat: i64,
    /// Raft term when this node was last seen
    pub last_term: u64,
    /// Last applied log index
    pub last_applied: u64,
    /// Additional metadata
    pub metadata: NodeMetadata,
}

impl ClusterNode {
    /// Create a new cluster node
    pub fn new(id: NodeId, name: String, api_addr: SocketAddr, raft_addr: SocketAddr) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id,
            name,
            api_addr,
            raft_addr,
            role: NodeRole::Follower,
            status: NodeStatus::Joining,
            joined_at: now,
            last_heartbeat: now,
            last_term: 0,
            last_applied: 0,
            metadata: NodeMetadata::default(),
        }
    }

    /// Check if this node is the leader
    pub fn is_leader(&self) -> bool {
        self.role == NodeRole::Leader
    }

    /// Check if this node is healthy
    pub fn is_healthy(&self) -> bool {
        self.status == NodeStatus::Healthy
    }

    /// Check if this node can serve reads
    pub fn can_serve_reads(&self) -> bool {
        matches!(self.status, NodeStatus::Healthy)
            && matches!(self.role, NodeRole::Leader | NodeRole::Follower)
    }

    /// Check if this node can accept writes
    pub fn can_accept_writes(&self) -> bool {
        self.is_leader() && self.is_healthy()
    }

    /// Update the last heartbeat timestamp
    pub fn touch(&mut self) {
        self.last_heartbeat = chrono::Utc::now().timestamp();
    }
}

/// Additional node metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeMetadata {
    /// ReasonDB version
    pub version: Option<String>,
    /// Operating system
    pub os: Option<String>,
    /// Available storage bytes
    pub storage_available: Option<u64>,
    /// Total documents stored
    pub document_count: Option<u64>,
    /// Total nodes in storage
    pub node_count: Option<u64>,
    /// Region/datacenter
    pub region: Option<String>,
    /// Custom tags
    pub tags: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_generation() {
        let id1 = NodeId::generate();
        let id2 = NodeId::generate();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_cluster_node_creation() {
        let node = ClusterNode::new(
            NodeId::new("node-1"),
            "Primary".to_string(),
            "127.0.0.1:4444".parse().unwrap(),
            "127.0.0.1:4445".parse().unwrap(),
        );

        assert_eq!(node.id.as_str(), "node-1");
        assert_eq!(node.role, NodeRole::Follower);
        assert_eq!(node.status, NodeStatus::Joining);
        assert!(!node.is_leader());
    }

    #[test]
    fn test_node_capabilities() {
        let mut node = ClusterNode::new(
            NodeId::generate(),
            "Test".to_string(),
            "127.0.0.1:4444".parse().unwrap(),
            "127.0.0.1:4445".parse().unwrap(),
        );

        node.status = NodeStatus::Healthy;
        node.role = NodeRole::Follower;
        assert!(node.can_serve_reads());
        assert!(!node.can_accept_writes());

        node.role = NodeRole::Leader;
        assert!(node.can_serve_reads());
        assert!(node.can_accept_writes());
    }
}
