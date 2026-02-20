//! Table-level sharding for horizontal scaling
//!
//! Partitions data by table ID across cluster nodes. Each shard (node) owns
//! a set of tables. Requests are routed to the node that owns the target table.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                     Shard Router                              │
//! │                                                              │
//! │   table_id ──► hash(table_id) % N ──► shard_id ──► node_id  │
//! │                                                              │
//! │   Shard 0: [tbl_a, tbl_d, tbl_g]  → Node 1                  │
//! │   Shard 1: [tbl_b, tbl_e, tbl_h]  → Node 2                  │
//! │   Shard 2: [tbl_c, tbl_f, tbl_i]  → Node 3                  │
//! └──────────────────────────────────────────────────────────────┘
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

/// Identifies which shard (and therefore which node) owns a table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardAssignment {
    /// Table ID
    pub table_id: String,
    /// Shard number (0..num_shards)
    pub shard_id: u32,
    /// Node ID that owns this shard
    pub node_id: String,
}

/// Shard map: tracks table → shard → node assignments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardMap {
    /// Total number of shards
    pub num_shards: u32,
    /// Shard → Node ID mapping
    pub shard_to_node: HashMap<u32, String>,
    /// Explicit table overrides (for migration)
    pub table_overrides: HashMap<String, String>,
}

impl ShardMap {
    /// Create a new shard map with the given number of shards.
    pub fn new(num_shards: u32) -> Self {
        Self {
            num_shards: num_shards.max(1),
            shard_to_node: HashMap::new(),
            table_overrides: HashMap::new(),
        }
    }

    /// Single-node shard map (everything on one node).
    pub fn single_node(node_id: &str) -> Self {
        let mut map = Self::new(1);
        map.shard_to_node.insert(0, node_id.to_string());
        map
    }

    /// Assign a shard to a node.
    pub fn assign_shard(&mut self, shard_id: u32, node_id: &str) {
        self.shard_to_node.insert(shard_id, node_id.to_string());
    }

    /// Get the shard ID for a table ID (consistent hashing via FNV).
    pub fn shard_for_table(&self, table_id: &str) -> u32 {
        if let Some(node) = self.table_overrides.get(table_id) {
            // Find the shard for this specific node
            for (&shard_id, n) in &self.shard_to_node {
                if n == node {
                    return shard_id;
                }
            }
        }
        fnv_hash(table_id) % self.num_shards
    }

    /// Get the node ID that owns a table.
    pub fn node_for_table(&self, table_id: &str) -> Option<&str> {
        if let Some(node) = self.table_overrides.get(table_id) {
            return Some(node.as_str());
        }
        let shard = self.shard_for_table(table_id);
        self.shard_to_node.get(&shard).map(|s| s.as_str())
    }

    /// Check if a table is local to this node.
    pub fn is_local(&self, table_id: &str, local_node_id: &str) -> bool {
        self.node_for_table(table_id)
            .map(|n| n == local_node_id)
            .unwrap_or(true) // If no assignment, default to local
    }

    /// Get all shard IDs owned by a node.
    pub fn shards_for_node(&self, node_id: &str) -> Vec<u32> {
        self.shard_to_node
            .iter()
            .filter(|(_, n)| n.as_str() == node_id)
            .map(|(&shard_id, _)| shard_id)
            .collect()
    }

    /// Get all node IDs in the shard map (for scatter-gather).
    pub fn all_nodes(&self) -> Vec<String> {
        let mut nodes: Vec<String> = self.shard_to_node.values().cloned().collect();
        nodes.sort();
        nodes.dedup();
        nodes
    }

    /// Serialize to bytes for cluster replication.
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap_or_default()
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bincode::deserialize(bytes).ok()
    }
}

impl Default for ShardMap {
    fn default() -> Self {
        Self::new(1)
    }
}

/// Thread-safe shard router used by the server.
pub struct ShardRouter {
    /// Current shard map
    map: RwLock<ShardMap>,
    /// This node's ID
    local_node_id: String,
}

impl ShardRouter {
    /// Create a new shard router.
    pub fn new(local_node_id: &str, shard_map: ShardMap) -> Self {
        Self {
            map: RwLock::new(shard_map),
            local_node_id: local_node_id.to_string(),
        }
    }

    /// Create a single-node router (no sharding).
    pub fn single_node(node_id: &str) -> Self {
        Self::new(node_id, ShardMap::single_node(node_id))
    }

    /// Check if a table is local to this node.
    pub fn is_local(&self, table_id: &str) -> bool {
        let map = self.map.read().unwrap();
        map.is_local(table_id, &self.local_node_id)
    }

    /// Get the node that owns a table (None if local or unknown).
    pub fn owner_node(&self, table_id: &str) -> Option<String> {
        let map = self.map.read().unwrap();
        map.node_for_table(table_id).map(|s| s.to_string())
    }

    /// Get all remote nodes for scatter-gather operations.
    pub fn remote_nodes(&self) -> Vec<String> {
        let map = self.map.read().unwrap();
        map.all_nodes()
            .into_iter()
            .filter(|n| n != &self.local_node_id)
            .collect()
    }

    /// Update the shard map (e.g., after cluster membership change).
    pub fn update_map(&self, new_map: ShardMap) {
        let mut map = self.map.write().unwrap();
        *map = new_map;
    }

    /// Get a snapshot of the current shard map.
    pub fn snapshot(&self) -> ShardMap {
        self.map.read().unwrap().clone()
    }

    /// Get this node's ID.
    pub fn local_node_id(&self) -> &str {
        &self.local_node_id
    }
}

/// FNV-1a hash for consistent table → shard routing.
fn fnv_hash(key: &str) -> u32 {
    let mut hash: u32 = 2166136261;
    for byte in key.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

/// Result of a scatter-gather search across shards.
#[derive(Debug, Clone)]
pub struct ScatterGatherResult<T> {
    /// Results from each node
    pub results: Vec<(String, Vec<T>)>,
}

impl<T> ScatterGatherResult<T> {
    /// Merge all results into a flat list.
    pub fn flatten(self) -> Vec<T> {
        self.results.into_iter().flat_map(|(_, items)| items).collect()
    }

    /// Total count across all shards.
    pub fn total_count(&self) -> usize {
        self.results.iter().map(|(_, items)| items.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_node_shard_map() {
        let map = ShardMap::single_node("node-1");
        assert_eq!(map.node_for_table("tbl_abc"), Some("node-1"));
        assert!(map.is_local("tbl_abc", "node-1"));
    }

    #[test]
    fn test_multi_shard_distribution() {
        let mut map = ShardMap::new(3);
        map.assign_shard(0, "node-1");
        map.assign_shard(1, "node-2");
        map.assign_shard(2, "node-3");

        // Different tables should distribute across shards
        let mut shard_counts = HashMap::new();
        for i in 0..100 {
            let table_id = format!("tbl_{}", i);
            let shard = map.shard_for_table(&table_id);
            *shard_counts.entry(shard).or_insert(0) += 1;
        }

        // Each shard should have some tables (rough balance)
        for shard_id in 0..3 {
            assert!(
                *shard_counts.get(&shard_id).unwrap_or(&0) > 10,
                "Shard {} should have reasonable distribution",
                shard_id
            );
        }
    }

    #[test]
    fn test_table_override() {
        let mut map = ShardMap::new(3);
        map.assign_shard(0, "node-1");
        map.assign_shard(1, "node-2");
        map.assign_shard(2, "node-3");

        // Override a specific table to a specific node
        map.table_overrides
            .insert("tbl_special".to_string(), "node-3".to_string());

        assert_eq!(map.node_for_table("tbl_special"), Some("node-3"));
    }

    #[test]
    fn test_shard_router() {
        let mut map = ShardMap::new(2);
        map.assign_shard(0, "node-1");
        map.assign_shard(1, "node-2");

        let router = ShardRouter::new("node-1", map);

        assert_eq!(router.local_node_id(), "node-1");
        assert_eq!(router.remote_nodes(), vec!["node-2".to_string()]);
    }

    #[test]
    fn test_shard_map_serialization() {
        let mut map = ShardMap::new(3);
        map.assign_shard(0, "node-1");
        map.assign_shard(1, "node-2");
        map.assign_shard(2, "node-3");

        let bytes = map.to_bytes();
        let restored = ShardMap::from_bytes(&bytes).unwrap();

        assert_eq!(restored.num_shards, 3);
        assert_eq!(restored.shard_to_node.len(), 3);
    }

    #[test]
    fn test_fnv_hash_consistency() {
        // Same input should always produce same hash
        let h1 = fnv_hash("tbl_abc123");
        let h2 = fnv_hash("tbl_abc123");
        assert_eq!(h1, h2);

        // Different inputs should (usually) produce different hashes
        let h3 = fnv_hash("tbl_def456");
        assert_ne!(h1, h3);
    }
}
