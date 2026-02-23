//! Replication log entries
//!
//! Defines the log entries that are replicated across the cluster.

use serde::{Deserialize, Serialize};

/// Types of operations that can be replicated
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LogEntryType {
    /// Create or update a table
    UpsertTable {
        table_id: String,
        name: String,
        description: Option<String>,
    },
    /// Delete a table
    DeleteTable { table_id: String },
    /// Create or update a document
    UpsertDocument {
        document_id: String,
        table_id: String,
        title: String,
        metadata: Vec<u8>, // Serialized document metadata
    },
    /// Delete a document
    DeleteDocument { document_id: String },
    /// Create or update a node
    UpsertNode {
        node_id: String,
        document_id: String,
        data: Vec<u8>, // Serialized PageNode
    },
    /// Delete a node
    DeleteNode { node_id: String },
    /// Create a relation
    CreateRelation {
        from_doc: String,
        to_doc: String,
        relation_type: String,
        note: Option<String>,
    },
    /// Delete a relation
    DeleteRelation {
        from_doc: String,
        to_doc: String,
        relation_type: String,
    },
    /// Create an API key
    CreateApiKey {
        key_id: String,
        key_hash: String,
        metadata: Vec<u8>,
    },
    /// Revoke an API key
    RevokeApiKey { key_id: String },
    /// No-op entry (used for leader election)
    Noop,
    /// Cluster membership change
    MembershipChange {
        change_type: MembershipChangeType,
        node_id: String,
        raft_addr: Option<String>,
    },
}

/// Types of membership changes
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum MembershipChangeType {
    /// Add a new voting member
    AddVoter,
    /// Add a non-voting learner
    AddLearner,
    /// Remove a member
    RemoveMember,
    /// Promote learner to voter
    PromoteLearner,
}

/// A single log entry for replication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Log index (assigned by Raft)
    pub index: u64,
    /// Term when entry was created
    pub term: u64,
    /// The operation to apply
    pub entry_type: LogEntryType,
    /// Timestamp when created
    pub timestamp: i64,
    /// Client request ID (for deduplication)
    pub request_id: Option<String>,
}

impl LogEntry {
    /// Create a new log entry
    pub fn new(entry_type: LogEntryType) -> Self {
        Self {
            index: 0, // Assigned by Raft
            term: 0,  // Assigned by Raft
            entry_type,
            timestamp: chrono::Utc::now().timestamp(),
            request_id: None,
        }
    }

    /// Create a new log entry with a request ID
    pub fn with_request_id(entry_type: LogEntryType, request_id: String) -> Self {
        Self {
            index: 0,
            term: 0,
            entry_type,
            timestamp: chrono::Utc::now().timestamp(),
            request_id: Some(request_id),
        }
    }

    /// Serialize the log entry
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap_or_default()
    }

    /// Deserialize a log entry
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bincode::deserialize(bytes).ok()
    }

    /// Check if this is a no-op entry
    pub fn is_noop(&self) -> bool {
        matches!(self.entry_type, LogEntryType::Noop)
    }

    /// Check if this is a membership change
    pub fn is_membership_change(&self) -> bool {
        matches!(self.entry_type, LogEntryType::MembershipChange { .. })
    }
}

/// The replication log - stores log entries locally
#[derive(Debug)]
pub struct ReplicationLog {
    /// Path to the log storage
    #[allow(dead_code)]
    path: std::path::PathBuf,
    /// First index in the log
    first_index: u64,
    /// Last index in the log
    last_index: u64,
    /// Last applied index
    last_applied: u64,
    /// Committed index
    commit_index: u64,
}

impl ReplicationLog {
    /// Create a new replication log
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            path: path.into(),
            first_index: 0,
            last_index: 0,
            last_applied: 0,
            commit_index: 0,
        }
    }

    /// Get the first index
    pub fn first_index(&self) -> u64 {
        self.first_index
    }

    /// Get the last index
    pub fn last_index(&self) -> u64 {
        self.last_index
    }

    /// Get the last applied index
    pub fn last_applied(&self) -> u64 {
        self.last_applied
    }

    /// Get the commit index
    pub fn commit_index(&self) -> u64 {
        self.commit_index
    }

    /// Set the commit index
    pub fn set_commit_index(&mut self, index: u64) {
        self.commit_index = index;
    }

    /// Set the last applied index
    pub fn set_last_applied(&mut self, index: u64) {
        self.last_applied = index;
    }

    /// Get the replication lag
    pub fn replication_lag(&self) -> u64 {
        self.commit_index.saturating_sub(self.last_applied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry_serialization() {
        let entry = LogEntry::new(LogEntryType::UpsertTable {
            table_id: "tbl_123".to_string(),
            name: "Test Table".to_string(),
            description: Some("A test table".to_string()),
        });

        let bytes = entry.to_bytes();
        let decoded = LogEntry::from_bytes(&bytes).unwrap();

        assert_eq!(decoded.timestamp, entry.timestamp);
        match decoded.entry_type {
            LogEntryType::UpsertTable {
                table_id,
                name,
                description,
            } => {
                assert_eq!(table_id, "tbl_123");
                assert_eq!(name, "Test Table");
                assert_eq!(description, Some("A test table".to_string()));
            }
            _ => panic!("Wrong entry type"),
        }
    }

    #[test]
    fn test_log_entry_types() {
        let noop = LogEntry::new(LogEntryType::Noop);
        assert!(noop.is_noop());
        assert!(!noop.is_membership_change());

        let membership = LogEntry::new(LogEntryType::MembershipChange {
            change_type: MembershipChangeType::AddVoter,
            node_id: "node-1".to_string(),
            raft_addr: Some("127.0.0.1:4445".to_string()),
        });
        assert!(!membership.is_noop());
        assert!(membership.is_membership_change());
    }
}
