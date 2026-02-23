//! Cluster configuration

use super::node::NodeId;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Configuration for a cluster node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Unique node identifier (auto-generated if not provided)
    pub node_id: Option<NodeId>,
    /// Human-readable node name
    pub name: String,
    /// Address for client API connections
    pub api_addr: SocketAddr,
    /// Address for Raft/cluster communication
    pub raft_addr: SocketAddr,
    /// Path to store Raft logs
    pub raft_log_path: PathBuf,
    /// Path to the main database
    pub db_path: PathBuf,
}

impl NodeConfig {
    /// Create a new node configuration
    pub fn new(
        name: impl Into<String>,
        api_addr: SocketAddr,
        raft_addr: SocketAddr,
        data_dir: impl Into<PathBuf>,
    ) -> Self {
        let data_dir = data_dir.into();
        Self {
            node_id: None,
            name: name.into(),
            api_addr,
            raft_addr,
            raft_log_path: data_dir.join("raft"),
            db_path: data_dir.join("reasondb.redb"),
        }
    }

    /// Get or generate the node ID
    pub fn get_or_create_id(&mut self) -> NodeId {
        self.node_id.clone().unwrap_or_else(|| {
            let id = NodeId::generate();
            self.node_id = Some(id.clone());
            id
        })
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            node_id: None,
            name: "reasondb-node".to_string(),
            api_addr: "127.0.0.1:4444".parse().unwrap(),
            raft_addr: "127.0.0.1:4445".parse().unwrap(),
            raft_log_path: PathBuf::from("data/raft"),
            db_path: PathBuf::from("data/reasondb.redb"),
        }
    }
}

/// Configuration for the entire cluster
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Cluster name/identifier
    pub cluster_name: String,
    /// Minimum number of nodes for quorum
    pub min_quorum: usize,
    /// Whether to enable automatic failover
    pub auto_failover: bool,
    /// How long before a node is considered suspect
    pub heartbeat_timeout: Duration,
    /// How long before a suspect node is marked down
    pub failure_timeout: Duration,
    /// Raft election timeout range (min)
    pub election_timeout_min: Duration,
    /// Raft election timeout range (max)
    pub election_timeout_max: Duration,
    /// Raft heartbeat interval
    pub raft_heartbeat_interval: Duration,
    /// Maximum number of entries per append request
    pub max_entries_per_request: u64,
    /// Snapshot threshold (entries before snapshot)
    pub snapshot_threshold: u64,
    /// Whether to enable read scaling (serve reads from followers)
    pub enable_read_scaling: bool,
    /// Maximum replication lag allowed for read replicas (in entries)
    pub max_read_lag: u64,
    /// Initial cluster members (for bootstrapping)
    pub initial_members: Vec<ClusterMember>,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            cluster_name: "reasondb-cluster".to_string(),
            min_quorum: 2,
            auto_failover: true,
            heartbeat_timeout: Duration::from_secs(5),
            failure_timeout: Duration::from_secs(30),
            election_timeout_min: Duration::from_millis(150),
            election_timeout_max: Duration::from_millis(300),
            raft_heartbeat_interval: Duration::from_millis(50),
            max_entries_per_request: 100,
            snapshot_threshold: 10000,
            enable_read_scaling: true,
            max_read_lag: 100,
            initial_members: Vec::new(),
        }
    }
}

impl ClusterConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(name) = std::env::var("REASONDB_CLUSTER_NAME") {
            config.cluster_name = name;
        }

        if let Ok(quorum) = std::env::var("REASONDB_MIN_QUORUM") {
            if let Ok(q) = quorum.parse() {
                config.min_quorum = q;
            }
        }

        if let Ok(auto) = std::env::var("REASONDB_AUTO_FAILOVER") {
            config.auto_failover = auto == "true" || auto == "1";
        }

        if let Ok(scale) = std::env::var("REASONDB_READ_SCALING") {
            config.enable_read_scaling = scale == "true" || scale == "1";
        }

        // Parse initial members: "node1@host1:4445,node2@host2:4445"
        if let Ok(members) = std::env::var("REASONDB_CLUSTER_MEMBERS") {
            config.initial_members = members
                .split(',')
                .filter_map(|s| ClusterMember::parse(s.trim()))
                .collect();
        }

        config
    }

    /// Check if this is a single-node configuration
    pub fn is_single_node(&self) -> bool {
        self.initial_members.len() <= 1
    }

    /// Calculate quorum size based on cluster size
    pub fn quorum_size(&self, total_nodes: usize) -> usize {
        (total_nodes / 2) + 1
    }
}

/// A cluster member entry for bootstrapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterMember {
    /// Node ID
    pub node_id: NodeId,
    /// Raft address
    pub raft_addr: SocketAddr,
}

impl ClusterMember {
    /// Create a new cluster member
    pub fn new(node_id: NodeId, raft_addr: SocketAddr) -> Self {
        Self { node_id, raft_addr }
    }

    /// Parse from string format: "node_id@host:port"
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('@').collect();
        if parts.len() != 2 {
            return None;
        }

        let node_id = NodeId::new(parts[0]);
        let raft_addr: SocketAddr = parts[1].parse().ok()?;

        Some(Self { node_id, raft_addr })
    }
}

impl std::fmt::Display for ClusterMember {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.node_id, self.raft_addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_member_parse() {
        let member = ClusterMember::parse("node-1@127.0.0.1:4445").unwrap();
        assert_eq!(member.node_id.as_str(), "node-1");
        assert_eq!(member.raft_addr.to_string(), "127.0.0.1:4445");
    }

    #[test]
    fn test_cluster_member_invalid() {
        assert!(ClusterMember::parse("invalid").is_none());
        assert!(ClusterMember::parse("node@invalid").is_none());
    }

    #[test]
    fn test_quorum_calculation() {
        let config = ClusterConfig::default();
        assert_eq!(config.quorum_size(3), 2);
        assert_eq!(config.quorum_size(5), 3);
        assert_eq!(config.quorum_size(7), 4);
    }
}
