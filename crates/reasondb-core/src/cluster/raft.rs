//! Raft consensus integration
//!
//! Implements a simplified Raft consensus protocol for leader election
//! and log replication.

use super::config::ClusterConfig;
use super::log::LogEntry;
use super::network::NetworkClient;
use super::node::{ClusterNode, NodeId, NodeRole};
use super::state::{ApplyResult, ClusterStateMachine};
use crate::error::ReasonError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Type alias for Raft node ID (numeric)
pub type RaftId = u64;

/// Type configuration marker for the cluster
pub struct RaftTypeConfig;

/// Node information stored in Raft
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaftNodeInfo {
    /// Node's API address
    pub api_addr: String,
    /// Node's Raft address
    pub raft_addr: String,
}

impl std::fmt::Display for RaftNodeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RaftNode(api={}, raft={})",
            self.api_addr, self.raft_addr
        )
    }
}

/// A Raft node in the ReasonDB cluster
pub struct RaftNode {
    /// Node ID (string format)
    node_id: NodeId,
    /// Numeric ID for Raft
    raft_id: RaftId,
    /// Cluster configuration
    config: ClusterConfig,
    /// State machine
    state_machine: Arc<ClusterStateMachine>,
    /// Network client
    network: NetworkClient,
    /// Current role
    role: RwLock<NodeRole>,
    /// Current term
    current_term: RwLock<u64>,
    /// Voted for in current term
    voted_for: RwLock<Option<RaftId>>,
    /// Is running
    running: RwLock<bool>,
}

impl RaftNode {
    /// Create a new Raft node
    pub fn new(
        node_id: NodeId,
        config: ClusterConfig,
        state_machine: Arc<ClusterStateMachine>,
    ) -> Self {
        // Generate a numeric ID from the node ID string
        let raft_id = Self::node_id_to_raft_id(&node_id);

        Self {
            node_id,
            raft_id,
            config,
            state_machine,
            network: NetworkClient::new(),
            role: RwLock::new(NodeRole::Follower),
            current_term: RwLock::new(0),
            voted_for: RwLock::new(None),
            running: RwLock::new(false),
        }
    }

    /// Convert string node ID to numeric Raft ID
    fn node_id_to_raft_id(node_id: &NodeId) -> RaftId {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        node_id.0.hash(&mut hasher);
        hasher.finish()
    }

    /// Get the node ID
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// Get the Raft numeric ID
    pub fn raft_id(&self) -> RaftId {
        self.raft_id
    }

    /// Get the current role
    pub async fn role(&self) -> NodeRole {
        *self.role.read().await
    }

    /// Check if this node is the leader
    pub async fn is_leader(&self) -> bool {
        *self.role.read().await == NodeRole::Leader
    }

    /// Get the current term
    pub async fn current_term(&self) -> u64 {
        *self.current_term.read().await
    }

    /// Get the state machine
    pub fn state_machine(&self) -> &Arc<ClusterStateMachine> {
        &self.state_machine
    }

    /// Get the cluster configuration
    pub fn config(&self) -> &ClusterConfig {
        &self.config
    }

    /// Start the Raft node
    pub async fn start(&self) -> Result<(), ReasonError> {
        let mut running = self.running.write().await;
        if *running {
            return Err(ReasonError::InvalidOperation(
                "Node already running".to_string(),
            ));
        }
        *running = true;

        tracing::info!(
            node_id = %self.node_id,
            raft_id = self.raft_id,
            "Starting Raft node"
        );

        // Initialize cluster state
        let state = self.state_machine.state();
        let mut cluster_state = state
            .write()
            .map_err(|_| ReasonError::Internal("Failed to acquire state lock".to_string()))?;

        // Add self to cluster state
        let self_node = ClusterNode::new(
            self.node_id.clone(),
            self.node_id.to_string(),
            "127.0.0.1:4444".parse().unwrap(), // TODO: Use actual config
            "127.0.0.1:4445".parse().unwrap(),
        );
        cluster_state.upsert_node(self_node);

        Ok(())
    }

    /// Stop the Raft node
    pub async fn stop(&self) -> Result<(), ReasonError> {
        let mut running = self.running.write().await;
        *running = false;

        tracing::info!(
            node_id = %self.node_id,
            "Stopping Raft node"
        );

        Ok(())
    }

    /// Check if the node is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Propose a log entry (leader only)
    pub async fn propose(&self, entry: LogEntry) -> Result<ApplyResult, ReasonError> {
        // Check if we're the leader
        if !self.is_leader().await {
            return Err(ReasonError::InvalidOperation(
                "Not the leader - forward to leader".to_string(),
            ));
        }

        // Apply to state machine
        self.state_machine.apply(&entry)
    }

    /// Become the leader (for single-node or testing)
    pub async fn become_leader(&self) {
        *self.role.write().await = NodeRole::Leader;

        // Update cluster state
        let state = self.state_machine.state();
        if let Ok(mut cluster_state) = state.write() {
            cluster_state.set_leader(Some(self.node_id.clone()));
        }

        tracing::info!(
            node_id = %self.node_id,
            term = self.current_term().await,
            "Became leader"
        );
    }

    /// Step down from leader
    pub async fn step_down(&self) {
        *self.role.write().await = NodeRole::Follower;

        tracing::info!(
            node_id = %self.node_id,
            "Stepped down from leader"
        );
    }

    /// Handle a vote request
    pub async fn handle_vote_request(
        &self,
        term: u64,
        candidate_id: RaftId,
        _last_log_index: u64,
        _last_log_term: u64,
    ) -> (u64, bool) {
        let current_term = *self.current_term.read().await;

        // If term < currentTerm, reject
        if term < current_term {
            return (current_term, false);
        }

        // If term > currentTerm, update term and become follower
        if term > current_term {
            *self.current_term.write().await = term;
            *self.role.write().await = NodeRole::Follower;
            *self.voted_for.write().await = None;
        }

        // Check if we already voted for someone else
        let voted_for = *self.voted_for.read().await;
        if voted_for.is_some() && voted_for != Some(candidate_id) {
            return (term, false);
        }

        // Grant vote
        *self.voted_for.write().await = Some(candidate_id);
        (term, true)
    }

    /// Handle an append entries request
    pub async fn handle_append_entries(
        &self,
        term: u64,
        leader_id: String,
        _prev_log_index: u64,
        _prev_log_term: u64,
        entries: Vec<LogEntry>,
        leader_commit: u64,
    ) -> (u64, bool) {
        let current_term = *self.current_term.read().await;

        // If term < currentTerm, reject
        if term < current_term {
            return (current_term, false);
        }

        // Update term if needed
        if term > current_term {
            *self.current_term.write().await = term;
            *self.voted_for.write().await = None;
        }

        // Become follower
        *self.role.write().await = NodeRole::Follower;

        // Update leader in state
        let state = self.state_machine.state();
        if let Ok(mut cluster_state) = state.write() {
            cluster_state.leader_id = Some(NodeId::new(leader_id));
            cluster_state.commit_index = leader_commit;
            drop(cluster_state);
        }

        // Apply entries
        for entry in entries {
            if let Err(e) = self.state_machine.apply(&entry) {
                tracing::error!(error = %e, "Failed to apply log entry");
                return (term, false);
            }
        }

        (term, true)
    }

    /// Get cluster status
    pub async fn status(&self) -> ClusterStatus {
        // Read role and term first (these are tokio RwLock which is Send)
        let role = *self.role.read().await;
        let term = *self.current_term.read().await;

        // Then read cluster state (this is std RwLock, don't hold across await)
        let (leader_id, nodes, last_applied, commit_index, has_quorum) = {
            let state = self.state_machine.state();
            let cluster_state = state.read().unwrap();
            (
                cluster_state.leader_id.clone(),
                cluster_state.nodes.len(),
                cluster_state.last_applied,
                cluster_state.commit_index,
                cluster_state.has_quorum(),
            )
        };

        ClusterStatus {
            node_id: self.node_id.clone(),
            raft_id: self.raft_id,
            role,
            term,
            leader_id,
            nodes,
            last_applied,
            commit_index,
            has_quorum,
        }
    }

    /// Add a peer to the network
    pub async fn add_peer(&self, node_id: &NodeId, addr: std::net::SocketAddr) {
        self.network.add_peer(node_id, addr).await;
    }

    /// Remove a peer from the network
    pub async fn remove_peer(&self, node_id: &NodeId) {
        self.network.remove_peer(node_id).await;
    }

    /// Get all peers
    pub async fn get_peers(&self) -> Vec<(NodeId, std::net::SocketAddr)> {
        self.network.get_peers().await
    }
}

/// Cluster status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatus {
    /// This node's ID
    pub node_id: NodeId,
    /// This node's Raft ID
    pub raft_id: RaftId,
    /// Current role
    pub role: NodeRole,
    /// Current term
    pub term: u64,
    /// Leader ID (if known)
    pub leader_id: Option<NodeId>,
    /// Number of nodes in cluster
    pub nodes: usize,
    /// Last applied log index
    pub last_applied: u64,
    /// Commit index
    pub commit_index: u64,
    /// Whether we have quorum
    pub has_quorum: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_raft_node_creation() {
        let node_id = NodeId::new("test-node");
        let config = ClusterConfig::default();
        let state_machine = Arc::new(ClusterStateMachine::new());

        let raft_node = RaftNode::new(node_id.clone(), config, state_machine);

        assert_eq!(raft_node.node_id().as_str(), "test-node");
        assert!(!raft_node.is_leader().await);
        assert_eq!(raft_node.current_term().await, 0);
    }

    #[tokio::test]
    async fn test_raft_node_start_stop() {
        let node_id = NodeId::new("test-node");
        let config = ClusterConfig::default();
        let state_machine = Arc::new(ClusterStateMachine::new());

        let raft_node = RaftNode::new(node_id, config, state_machine);

        // Start
        raft_node.start().await.unwrap();
        assert!(raft_node.is_running().await);

        // Starting again should fail
        assert!(raft_node.start().await.is_err());

        // Stop
        raft_node.stop().await.unwrap();
        assert!(!raft_node.is_running().await);
    }

    #[tokio::test]
    async fn test_become_leader() {
        let node_id = NodeId::new("leader-node");
        let config = ClusterConfig::default();
        let state_machine = Arc::new(ClusterStateMachine::new());

        let raft_node = RaftNode::new(node_id.clone(), config, state_machine);
        raft_node.start().await.unwrap();

        assert!(!raft_node.is_leader().await);

        raft_node.become_leader().await;
        assert!(raft_node.is_leader().await);

        let status = raft_node.status().await;
        assert_eq!(status.role, NodeRole::Leader);
        assert_eq!(status.leader_id, Some(node_id));
    }

    #[tokio::test]
    async fn test_vote_request_handling() {
        let node_id = NodeId::new("voter");
        let config = ClusterConfig::default();
        let state_machine = Arc::new(ClusterStateMachine::new());

        let raft_node = RaftNode::new(node_id, config, state_machine);

        // Vote for candidate with higher term
        let (term, granted) = raft_node.handle_vote_request(1, 12345, 0, 0).await;
        assert_eq!(term, 1);
        assert!(granted);

        // Reject vote for same term (already voted)
        let (term, granted) = raft_node.handle_vote_request(1, 99999, 0, 0).await;
        assert_eq!(term, 1);
        assert!(!granted);

        // Accept vote for higher term
        let (term, granted) = raft_node.handle_vote_request(2, 99999, 0, 0).await;
        assert_eq!(term, 2);
        assert!(granted);
    }

    #[tokio::test]
    async fn test_append_entries() {
        let node_id = NodeId::new("follower");
        let config = ClusterConfig::default();
        let state_machine = Arc::new(ClusterStateMachine::new());

        let raft_node = RaftNode::new(node_id, config, state_machine);
        raft_node.start().await.unwrap();

        // Receive append entries from leader
        let (term, success) = raft_node
            .handle_append_entries(1, "leader-1".to_string(), 0, 0, vec![], 0)
            .await;

        assert_eq!(term, 1);
        assert!(success);
        assert_eq!(raft_node.role().await, NodeRole::Follower);
    }
}
