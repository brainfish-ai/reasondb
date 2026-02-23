//! Cluster state machine
//!
//! The state machine applies log entries to the local database.

use super::log::{LogEntry, LogEntryType};
use super::node::{ClusterNode, NodeId, NodeRole, NodeStatus};
use crate::error::ReasonError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

type ApplyCallback = Box<dyn Fn(&LogEntry) -> Result<ApplyResult, ReasonError> + Send + Sync>;

/// Result of applying a log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApplyResult {
    /// Entry applied successfully
    Success,
    /// Entry applied, returning data
    Data(Vec<u8>),
    /// Entry failed to apply
    Error(String),
}

/// The cluster state - tracks all nodes in the cluster
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClusterState {
    /// All known nodes
    pub nodes: HashMap<String, ClusterNode>,
    /// Current leader ID
    pub leader_id: Option<NodeId>,
    /// Current Raft term
    pub current_term: u64,
    /// This node's vote in current term
    pub voted_for: Option<NodeId>,
    /// Last applied log index
    pub last_applied: u64,
    /// Commit index
    pub commit_index: u64,
}

impl ClusterState {
    /// Create a new cluster state
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or update a node
    pub fn upsert_node(&mut self, node: ClusterNode) {
        self.nodes.insert(node.id.0.clone(), node);
    }

    /// Remove a node
    pub fn remove_node(&mut self, node_id: &NodeId) {
        self.nodes.remove(&node_id.0);
    }

    /// Get a node by ID
    pub fn get_node(&self, node_id: &NodeId) -> Option<&ClusterNode> {
        self.nodes.get(&node_id.0)
    }

    /// Get a mutable node by ID
    pub fn get_node_mut(&mut self, node_id: &NodeId) -> Option<&mut ClusterNode> {
        self.nodes.get_mut(&node_id.0)
    }

    /// Get the current leader
    pub fn get_leader(&self) -> Option<&ClusterNode> {
        self.leader_id.as_ref().and_then(|id| self.get_node(id))
    }

    /// Set the leader
    pub fn set_leader(&mut self, node_id: Option<NodeId>) {
        // Update roles
        for node in self.nodes.values_mut() {
            node.role = if Some(&node.id) == node_id.as_ref() {
                NodeRole::Leader
            } else {
                NodeRole::Follower
            };
        }
        self.leader_id = node_id;
    }

    /// Get all healthy nodes
    pub fn healthy_nodes(&self) -> Vec<&ClusterNode> {
        self.nodes.values().filter(|n| n.is_healthy()).collect()
    }

    /// Get all nodes that can serve reads
    pub fn read_capable_nodes(&self) -> Vec<&ClusterNode> {
        self.nodes
            .values()
            .filter(|n| n.can_serve_reads())
            .collect()
    }

    /// Get the number of voting members
    pub fn voting_members(&self) -> usize {
        self.nodes
            .values()
            .filter(|n| {
                matches!(
                    n.role,
                    NodeRole::Leader | NodeRole::Follower | NodeRole::Candidate
                )
            })
            .count()
    }

    /// Check if we have quorum
    pub fn has_quorum(&self) -> bool {
        let healthy = self.healthy_nodes().len();
        let total = self.voting_members();
        healthy > total / 2
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap_or_default()
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bincode::deserialize(bytes).ok()
    }
}

/// The cluster state machine - applies log entries
pub struct ClusterStateMachine {
    /// Cluster state
    state: Arc<RwLock<ClusterState>>,
    /// Callback for applying entries to the database
    apply_callback: Option<ApplyCallback>,
}

impl ClusterStateMachine {
    /// Create a new state machine
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(ClusterState::new())),
            apply_callback: None,
        }
    }

    /// Create with an apply callback
    pub fn with_callback<F>(callback: F) -> Self
    where
        F: Fn(&LogEntry) -> Result<ApplyResult, ReasonError> + Send + Sync + 'static,
    {
        Self {
            state: Arc::new(RwLock::new(ClusterState::new())),
            apply_callback: Some(Box::new(callback)),
        }
    }

    /// Get the cluster state
    pub fn state(&self) -> Arc<RwLock<ClusterState>> {
        self.state.clone()
    }

    /// Apply a log entry to the state machine
    pub fn apply(&self, entry: &LogEntry) -> Result<ApplyResult, ReasonError> {
        // Handle membership changes internally
        if let LogEntryType::MembershipChange {
            change_type,
            node_id,
            raft_addr,
        } = &entry.entry_type
        {
            return self.apply_membership_change(*change_type, node_id, raft_addr.as_deref());
        }

        // Apply through callback if set
        if let Some(callback) = &self.apply_callback {
            let result = callback(entry)?;

            // Update last applied
            let mut state = self
                .state
                .write()
                .map_err(|_| ReasonError::Internal("Failed to acquire state lock".to_string()))?;
            state.last_applied = entry.index;

            return Ok(result);
        }

        // No callback, just update index
        let mut state = self
            .state
            .write()
            .map_err(|_| ReasonError::Internal("Failed to acquire state lock".to_string()))?;
        state.last_applied = entry.index;

        Ok(ApplyResult::Success)
    }

    /// Apply a membership change
    fn apply_membership_change(
        &self,
        change_type: super::log::MembershipChangeType,
        node_id: &str,
        raft_addr: Option<&str>,
    ) -> Result<ApplyResult, ReasonError> {
        use super::log::MembershipChangeType::*;

        let mut state = self
            .state
            .write()
            .map_err(|_| ReasonError::Internal("Failed to acquire state lock".to_string()))?;

        match change_type {
            AddVoter | AddLearner => {
                if let Some(addr_str) = raft_addr {
                    if let Ok(raft_addr) = addr_str.parse() {
                        let mut node = ClusterNode::new(
                            NodeId::new(node_id),
                            node_id.to_string(),
                            raft_addr, // Use raft_addr for both temporarily
                            raft_addr,
                        );
                        node.role = if change_type == AddLearner {
                            NodeRole::Learner
                        } else {
                            NodeRole::Follower
                        };
                        node.status = NodeStatus::Joining;
                        state.upsert_node(node);
                    }
                }
            }
            RemoveMember => {
                state.remove_node(&NodeId::new(node_id));
            }
            PromoteLearner => {
                if let Some(node) = state.get_node_mut(&NodeId::new(node_id)) {
                    node.role = NodeRole::Follower;
                }
            }
        }

        Ok(ApplyResult::Success)
    }

    /// Get a snapshot of the state
    pub fn snapshot(&self) -> Result<Vec<u8>, ReasonError> {
        let state = self
            .state
            .read()
            .map_err(|_| ReasonError::Internal("Failed to acquire state lock".to_string()))?;
        Ok(state.to_bytes())
    }

    /// Restore from a snapshot
    pub fn restore(&self, snapshot: &[u8]) -> Result<(), ReasonError> {
        let new_state = ClusterState::from_bytes(snapshot)
            .ok_or_else(|| ReasonError::Internal("Failed to deserialize snapshot".to_string()))?;

        let mut state = self
            .state
            .write()
            .map_err(|_| ReasonError::Internal("Failed to acquire state lock".to_string()))?;
        *state = new_state;

        Ok(())
    }
}

impl Default for ClusterStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_state_basic() {
        let mut state = ClusterState::new();

        let node = ClusterNode::new(
            NodeId::new("node-1"),
            "Primary".to_string(),
            "127.0.0.1:4444".parse().unwrap(),
            "127.0.0.1:4445".parse().unwrap(),
        );

        state.upsert_node(node);
        assert!(state.get_node(&NodeId::new("node-1")).is_some());
    }

    #[test]
    fn test_leader_election() {
        let mut state = ClusterState::new();

        // Add three nodes
        for i in 1..=3 {
            let mut node = ClusterNode::new(
                NodeId::new(format!("node-{}", i)),
                format!("Node {}", i),
                format!("127.0.0.1:{}", 4444 + i).parse().unwrap(),
                format!("127.0.0.1:{}", 4544 + i).parse().unwrap(),
            );
            node.status = NodeStatus::Healthy;
            state.upsert_node(node);
        }

        // Elect node-2 as leader
        state.set_leader(Some(NodeId::new("node-2")));

        let leader = state.get_leader().unwrap();
        assert_eq!(leader.id.as_str(), "node-2");
        assert!(leader.is_leader());

        // Other nodes should be followers
        let node1 = state.get_node(&NodeId::new("node-1")).unwrap();
        assert_eq!(node1.role, NodeRole::Follower);
    }

    #[test]
    fn test_quorum() {
        let mut state = ClusterState::new();

        // Add 3 nodes, 2 healthy
        for i in 1..=3 {
            let mut node = ClusterNode::new(
                NodeId::new(format!("node-{}", i)),
                format!("Node {}", i),
                format!("127.0.0.1:{}", 4444 + i).parse().unwrap(),
                format!("127.0.0.1:{}", 4544 + i).parse().unwrap(),
            );
            node.status = if i <= 2 {
                NodeStatus::Healthy
            } else {
                NodeStatus::Down
            };
            state.upsert_node(node);
        }

        assert!(state.has_quorum()); // 2 out of 3 healthy

        // Mark another node down
        state.get_node_mut(&NodeId::new("node-2")).unwrap().status = NodeStatus::Down;
        assert!(!state.has_quorum()); // Only 1 out of 3 healthy
    }
}
