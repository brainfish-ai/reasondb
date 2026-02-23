//! Database snapshot functionality

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use crate::error::{ReasonError, Result};
use crate::model::{Document, PageNode, Table};
use crate::store::NodeStore;

/// Snapshot metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    /// Unique snapshot ID
    pub id: String,
    /// When the snapshot was created
    pub created_at: DateTime<Utc>,
    /// Database version
    pub version: String,
    /// Number of tables
    pub table_count: usize,
    /// Number of documents
    pub document_count: usize,
    /// Number of nodes
    pub node_count: usize,
    /// Size in bytes
    pub size_bytes: u64,
    /// Checksum for integrity verification
    pub checksum: String,
    /// Optional description
    pub description: Option<String>,
    /// Parent snapshot ID (for incremental backups)
    pub parent_id: Option<String>,
    /// Whether this is an incremental snapshot
    pub is_incremental: bool,
}

/// A database snapshot containing all data
#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    /// Snapshot metadata
    pub metadata: SnapshotMetadata,
    /// All tables
    pub tables: Vec<Table>,
    /// All documents
    pub documents: Vec<Document>,
    /// All nodes
    pub nodes: Vec<PageNode>,
    /// Document-to-nodes mapping
    pub doc_nodes: HashMap<String, Vec<String>>,
}

impl Snapshot {
    /// Create a new snapshot from a NodeStore
    pub fn from_store(store: &NodeStore, description: Option<String>) -> Result<Self> {
        let tables = store.list_tables()?;
        let documents = store.list_all_documents()?;

        let mut nodes = Vec::new();
        let mut doc_nodes: HashMap<String, Vec<String>> = HashMap::new();

        for doc in &documents {
            let doc_node_list = store.get_nodes_for_document(&doc.id)?;
            let node_ids: Vec<String> = doc_node_list.iter().map(|n| n.id.clone()).collect();
            doc_nodes.insert(doc.id.clone(), node_ids);
            nodes.extend(doc_node_list);
        }

        let id = format!(
            "snap_{}",
            &uuid::Uuid::new_v4().to_string().replace("-", "")[..12]
        );

        let metadata = SnapshotMetadata {
            id: id.clone(),
            created_at: Utc::now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            table_count: tables.len(),
            document_count: documents.len(),
            node_count: nodes.len(),
            size_bytes: 0,           // Will be calculated after serialization
            checksum: String::new(), // Will be calculated after serialization
            description,
            parent_id: None,
            is_incremental: false,
        };

        Ok(Self {
            metadata,
            tables,
            documents,
            nodes,
            doc_nodes,
        })
    }

    /// Create an incremental snapshot (only changes since parent)
    pub fn incremental_from_store(
        store: &NodeStore,
        parent: &SnapshotMetadata,
        description: Option<String>,
    ) -> Result<Self> {
        // For now, we'll create a full snapshot but mark it as incremental
        // A more sophisticated implementation would track changes
        let mut snapshot = Self::from_store(store, description)?;
        snapshot.metadata.parent_id = Some(parent.id.clone());
        snapshot.metadata.is_incremental = true;
        snapshot.metadata.id = format!(
            "snap_inc_{}",
            &uuid::Uuid::new_v4().to_string().replace("-", "")[..8]
        );
        Ok(snapshot)
    }

    /// Save snapshot to a file
    pub fn save_to_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                ReasonError::Backup(format!("Failed to create backup directory: {}", e))
            })?;
        }

        // Reset checksum and size for consistent hashing
        self.metadata.checksum = String::new();
        self.metadata.size_bytes = 0;

        // Serialize to JSON to calculate checksum
        let json_for_checksum = serde_json::to_vec_pretty(self)
            .map_err(|e| ReasonError::Backup(format!("Failed to serialize snapshot: {}", e)))?;

        // Calculate checksum and size from the base content
        self.metadata.checksum = format!("{:x}", md5::compute(&json_for_checksum));
        self.metadata.size_bytes = json_for_checksum.len() as u64;

        // Re-serialize with checksum and size included for storage
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| ReasonError::Backup(format!("Failed to serialize snapshot: {}", e)))?;

        // Compress with gzip
        let file = File::create(path)
            .map_err(|e| ReasonError::Backup(format!("Failed to create backup file: {}", e)))?;
        let mut encoder =
            flate2::write::GzEncoder::new(BufWriter::new(file), flate2::Compression::default());
        encoder
            .write_all(&json)
            .map_err(|e| ReasonError::Backup(format!("Failed to write backup: {}", e)))?;
        encoder
            .finish()
            .map_err(|e| ReasonError::Backup(format!("Failed to finalize backup: {}", e)))?;

        Ok(())
    }

    /// Load snapshot from a file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        let file = File::open(path)
            .map_err(|e| ReasonError::Backup(format!("Failed to open backup file: {}", e)))?;

        // Decompress
        let mut decoder = flate2::read::GzDecoder::new(BufReader::new(file));
        let mut json = Vec::new();
        decoder
            .read_to_end(&mut json)
            .map_err(|e| ReasonError::Backup(format!("Failed to decompress backup: {}", e)))?;

        // Deserialize
        let snapshot: Snapshot = serde_json::from_slice(&json)
            .map_err(|e| ReasonError::Backup(format!("Failed to deserialize snapshot: {}", e)))?;

        // Verify checksum
        let expected_checksum = snapshot.metadata.checksum.clone();

        // Recreate the state used during checksum calculation
        let mut verify_snapshot = snapshot.clone();
        verify_snapshot.metadata.checksum = String::new();
        verify_snapshot.metadata.size_bytes = 0;

        let verify_json = serde_json::to_vec_pretty(&verify_snapshot)
            .map_err(|e| ReasonError::Backup(format!("Failed to verify snapshot: {}", e)))?;
        let actual_checksum = format!("{:x}", md5::compute(&verify_json));

        if actual_checksum != expected_checksum {
            return Err(ReasonError::Backup(format!(
                "Checksum mismatch: expected {}, got {}",
                expected_checksum, actual_checksum
            )));
        }

        Ok(snapshot)
    }

    /// Restore snapshot to a NodeStore
    pub fn restore_to_store(&self, store: &NodeStore) -> Result<()> {
        // Insert tables first
        for table in &self.tables {
            store.insert_table(table)?;
        }

        // Insert documents
        for doc in &self.documents {
            store.insert_document(doc)?;
        }

        // Insert nodes
        for node in &self.nodes {
            store.insert_node(node)?;
        }

        Ok(())
    }
}

impl Clone for Snapshot {
    fn clone(&self) -> Self {
        Self {
            metadata: self.metadata.clone(),
            tables: self.tables.clone(),
            documents: self.documents.clone(),
            nodes: self.nodes.clone(),
            doc_nodes: self.doc_nodes.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_snapshot_metadata() {
        let metadata = SnapshotMetadata {
            id: "snap_test123".to_string(),
            created_at: Utc::now(),
            version: "0.1.0".to_string(),
            table_count: 2,
            document_count: 10,
            node_count: 50,
            size_bytes: 1024,
            checksum: "abc123".to_string(),
            description: Some("Test backup".to_string()),
            parent_id: None,
            is_incremental: false,
        };

        assert_eq!(metadata.id, "snap_test123");
        assert_eq!(metadata.table_count, 2);
        assert!(!metadata.is_incremental);
    }

    #[test]
    fn test_snapshot_save_load() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.redb");
        let backup_path = dir.path().join("backup.snap.gz");

        // Create a store and add some data
        let store = NodeStore::open(&db_path).unwrap();
        let table = Table::new("Test Table".to_string());
        store.insert_table(&table).unwrap();

        // Create snapshot
        let mut snapshot = Snapshot::from_store(&store, Some("Test".to_string())).unwrap();
        assert_eq!(snapshot.metadata.table_count, 1);

        // Save and load
        snapshot.save_to_file(&backup_path).unwrap();
        let loaded = Snapshot::load_from_file(&backup_path).unwrap();

        assert_eq!(loaded.metadata.table_count, snapshot.metadata.table_count);
        assert_eq!(loaded.tables.len(), 1);
    }
}
