//! Clustering and replication for ReasonDB
//!
//! Provides distributed deployment with:
//! - Raft consensus for leader election
//! - Log replication for write operations
//! - Read replicas for scaling reads
//! - Automatic failover
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    ReasonDB Cluster                          │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │   ┌─────────┐     ┌─────────┐     ┌─────────┐              │
//! │   │ Node 1  │────▶│ Node 2  │────▶│ Node 3  │              │
//! │   │ LEADER  │◀────│FOLLOWER │◀────│FOLLOWER │              │
//! │   └─────────┘     └─────────┘     └─────────┘              │
//! │        │                │               │                   │
//! │        │  Raft Log Replication          │                   │
//! │        ▼                ▼               ▼                   │
//! │   ┌─────────┐     ┌─────────┐     ┌─────────┐              │
//! │   │  redb   │     │  redb   │     │  redb   │              │
//! │   │(primary)│     │(replica)│     │(replica)│              │
//! │   └─────────┘     └─────────┘     └─────────┘              │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//! ```

mod config;
mod log;
mod network;
mod node;
mod raft;
mod state;

pub use config::{ClusterConfig, NodeConfig};
pub use log::{LogEntry, LogEntryType, ReplicationLog};
pub use network::{NetworkClient, NetworkMessage, NetworkServer};
pub use node::{ClusterNode, NodeId, NodeRole, NodeStatus};
pub use raft::{ClusterStatus, RaftId, RaftNode, RaftNodeInfo, RaftTypeConfig};
pub use state::{ApplyResult, ClusterState, ClusterStateMachine};
