//! Backup management functionality

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

use super::snapshot::{Snapshot, SnapshotMetadata};
use crate::error::{ReasonError, Result};
use crate::store::NodeStore;

/// Type of backup
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackupType {
    /// Full database backup
    Full,
    /// Incremental backup (changes since last backup)
    Incremental,
}

/// Options for creating a backup
#[derive(Debug, Clone)]
pub struct BackupOptions {
    /// Type of backup
    pub backup_type: BackupType,
    /// Optional description
    pub description: Option<String>,
    /// Compression level (0-9, default 6)
    pub compression_level: u32,
    /// Whether to verify after creation
    pub verify: bool,
}

impl BackupOptions {
    /// Create options for a full backup
    pub fn full() -> Self {
        Self {
            backup_type: BackupType::Full,
            description: None,
            compression_level: 6,
            verify: true,
        }
    }

    /// Create options for an incremental backup
    pub fn incremental() -> Self {
        Self {
            backup_type: BackupType::Incremental,
            description: None,
            compression_level: 6,
            verify: true,
        }
    }

    /// Set the description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set compression level
    pub fn with_compression(mut self, level: u32) -> Self {
        self.compression_level = level.min(9);
        self
    }

    /// Disable verification
    pub fn without_verify(mut self) -> Self {
        self.verify = false;
        self
    }
}

impl Default for BackupOptions {
    fn default() -> Self {
        Self::full()
    }
}

/// Options for restoring a backup
#[derive(Debug, Clone)]
pub struct RestoreOptions {
    /// Whether to overwrite existing data
    pub overwrite: bool,
    /// Whether to verify backup before restoring
    pub verify_first: bool,
}

impl Default for RestoreOptions {
    fn default() -> Self {
        Self {
            overwrite: false,
            verify_first: true,
        }
    }
}

/// Information about a backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    /// Backup ID
    pub id: String,
    /// Backup file path
    pub path: PathBuf,
    /// When the backup was created
    pub created_at: DateTime<Utc>,
    /// Backup type
    pub backup_type: BackupType,
    /// Size in bytes
    pub size_bytes: u64,
    /// Number of tables
    pub table_count: usize,
    /// Number of documents
    pub document_count: usize,
    /// Number of nodes
    pub node_count: usize,
    /// Optional description
    pub description: Option<String>,
    /// Parent backup ID (for incremental)
    pub parent_id: Option<String>,
}

impl From<&SnapshotMetadata> for BackupInfo {
    fn from(meta: &SnapshotMetadata) -> Self {
        Self {
            id: meta.id.clone(),
            path: PathBuf::new(),
            created_at: meta.created_at,
            backup_type: if meta.is_incremental {
                BackupType::Incremental
            } else {
                BackupType::Full
            },
            size_bytes: meta.size_bytes,
            table_count: meta.table_count,
            document_count: meta.document_count,
            node_count: meta.node_count,
            description: meta.description.clone(),
            parent_id: meta.parent_id.clone(),
        }
    }
}

/// Backup index stored in the backup directory
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct BackupIndex {
    /// List of all backups
    backups: Vec<BackupInfo>,
    /// Last full backup ID
    last_full_backup: Option<String>,
}

/// Manages database backups
pub struct BackupManager {
    /// Backup directory
    backup_dir: PathBuf,
    /// Database path (for reference)
    #[allow(dead_code)]
    db_path: PathBuf,
}

impl BackupManager {
    /// Create a new backup manager
    pub fn new<P: AsRef<Path>>(_store: &NodeStore, backup_dir: P) -> Result<Self> {
        let backup_dir = backup_dir.as_ref().to_path_buf();

        // Create backup directory if it doesn't exist
        fs::create_dir_all(&backup_dir).map_err(|e| {
            ReasonError::Backup(format!("Failed to create backup directory: {}", e))
        })?;

        Ok(Self {
            backup_dir,
            db_path: PathBuf::new(), // We don't need the actual path
        })
    }

    /// Create a backup from a NodeStore
    pub fn create_backup(&self, store: &NodeStore, options: BackupOptions) -> Result<BackupInfo> {
        let mut index = self.load_index()?;

        let snapshot = match options.backup_type {
            BackupType::Full => Snapshot::from_store(store, options.description.clone())?,
            BackupType::Incremental => {
                // Find the last full backup
                let parent = index
                    .last_full_backup
                    .as_ref()
                    .and_then(|id| index.backups.iter().find(|b| &b.id == id))
                    .ok_or_else(|| {
                        ReasonError::Backup(
                            "No full backup found for incremental backup".to_string(),
                        )
                    })?;

                let parent_meta = SnapshotMetadata {
                    id: parent.id.clone(),
                    created_at: parent.created_at,
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    table_count: parent.table_count,
                    document_count: parent.document_count,
                    node_count: parent.node_count,
                    size_bytes: parent.size_bytes,
                    checksum: String::new(),
                    description: parent.description.clone(),
                    parent_id: parent.parent_id.clone(),
                    is_incremental: parent.backup_type == BackupType::Incremental,
                };

                Snapshot::incremental_from_store(store, &parent_meta, options.description.clone())?
            }
        };

        // Generate filename
        let timestamp = snapshot.metadata.created_at.format("%Y%m%d_%H%M%S");
        let suffix = match options.backup_type {
            BackupType::Full => "full",
            BackupType::Incremental => "incr",
        };
        let filename = format!("{}_{}.snap.gz", timestamp, suffix);
        let backup_path = self.backup_dir.join(&filename);

        // Save snapshot
        let mut snapshot = snapshot;
        snapshot.save_to_file(&backup_path)?;

        // Verify if requested
        if options.verify {
            let _ = Snapshot::load_from_file(&backup_path)?;
        }

        // Get file size
        let file_size = fs::metadata(&backup_path).map(|m| m.len()).unwrap_or(0);

        // Create backup info
        let mut info = BackupInfo::from(&snapshot.metadata);
        info.path = backup_path;
        info.size_bytes = file_size;

        // Update index
        if options.backup_type == BackupType::Full {
            index.last_full_backup = Some(info.id.clone());
        }
        index.backups.push(info.clone());
        self.save_index(&index)?;

        Ok(info)
    }

    /// List all backups
    pub fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        let index = self.load_index()?;
        Ok(index.backups)
    }

    /// Get backup by ID
    pub fn get_backup(&self, id: &str) -> Result<Option<BackupInfo>> {
        let index = self.load_index()?;
        Ok(index.backups.into_iter().find(|b| b.id == id))
    }

    /// Get the latest backup
    pub fn get_latest_backup(&self) -> Result<Option<BackupInfo>> {
        let index = self.load_index()?;
        Ok(index.backups.into_iter().max_by_key(|b| b.created_at))
    }

    /// Get the latest full backup
    pub fn get_latest_full_backup(&self) -> Result<Option<BackupInfo>> {
        let index = self.load_index()?;
        Ok(index
            .backups
            .into_iter()
            .filter(|b| b.backup_type == BackupType::Full)
            .max_by_key(|b| b.created_at))
    }

    /// Restore a backup to a new database
    pub fn restore<P: AsRef<Path>>(
        &self,
        backup_id: &str,
        target_path: P,
        options: RestoreOptions,
    ) -> Result<()> {
        let target_path = target_path.as_ref();

        // Check if target exists
        if target_path.exists() && !options.overwrite {
            return Err(ReasonError::Backup(
                "Target database already exists. Use overwrite option to replace.".to_string(),
            ));
        }

        // Find the backup
        let backup_info = self
            .get_backup(backup_id)?
            .ok_or_else(|| ReasonError::Backup(format!("Backup not found: {}", backup_id)))?;

        // Load snapshot
        let snapshot = Snapshot::load_from_file(&backup_info.path)?;

        // If incremental, we need to apply all backups in chain
        // For now, our incremental backups are actually full snapshots
        // A more sophisticated implementation would merge them

        // Remove existing database if overwriting
        if target_path.exists() && options.overwrite {
            fs::remove_file(target_path).map_err(|e| {
                ReasonError::Backup(format!("Failed to remove existing database: {}", e))
            })?;
        }

        // Create new store and restore
        let store = NodeStore::open(target_path)?;
        snapshot.restore_to_store(&store)?;

        Ok(())
    }

    /// Delete a backup
    pub fn delete_backup(&self, backup_id: &str) -> Result<()> {
        let mut index = self.load_index()?;

        // Find and remove from index
        let pos = index
            .backups
            .iter()
            .position(|b| b.id == backup_id)
            .ok_or_else(|| ReasonError::Backup(format!("Backup not found: {}", backup_id)))?;

        let info = index.backups.remove(pos);

        // Delete the file
        if info.path.exists() {
            fs::remove_file(&info.path)
                .map_err(|e| ReasonError::Backup(format!("Failed to delete backup file: {}", e)))?;
        }

        // Update last_full_backup if needed
        if index.last_full_backup.as_ref() == Some(&backup_id.to_string()) {
            index.last_full_backup = index
                .backups
                .iter()
                .filter(|b| b.backup_type == BackupType::Full)
                .max_by_key(|b| b.created_at)
                .map(|b| b.id.clone());
        }

        self.save_index(&index)?;
        Ok(())
    }

    /// Prune old backups, keeping only the specified number
    pub fn prune(&self, keep_full: usize, keep_incremental: usize) -> Result<Vec<BackupInfo>> {
        let mut index = self.load_index()?;
        let mut deleted = Vec::new();

        // Sort by date descending
        index
            .backups
            .sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Separate full and incremental
        let mut full_count = 0;
        let mut incr_count = 0;
        let mut to_keep: Vec<BackupInfo> = Vec::new();

        for backup in index.backups {
            let keep = match backup.backup_type {
                BackupType::Full => {
                    full_count += 1;
                    full_count <= keep_full
                }
                BackupType::Incremental => {
                    incr_count += 1;
                    incr_count <= keep_incremental
                }
            };

            if keep {
                to_keep.push(backup);
            } else {
                // Delete file
                if backup.path.exists() {
                    let _ = fs::remove_file(&backup.path);
                }
                deleted.push(backup);
            }
        }

        // Update index
        index.backups = to_keep;
        index.last_full_backup = index
            .backups
            .iter()
            .filter(|b| b.backup_type == BackupType::Full)
            .max_by_key(|b| b.created_at)
            .map(|b| b.id.clone());

        self.save_index(&index)?;
        Ok(deleted)
    }

    /// Verify a backup's integrity
    pub fn verify(&self, backup_id: &str) -> Result<bool> {
        let backup_info = self
            .get_backup(backup_id)?
            .ok_or_else(|| ReasonError::Backup(format!("Backup not found: {}", backup_id)))?;

        // Try to load the snapshot (this verifies checksum)
        match Snapshot::load_from_file(&backup_info.path) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    // Internal: Load backup index
    fn load_index(&self) -> Result<BackupIndex> {
        let index_path = self.backup_dir.join("index.json");

        if !index_path.exists() {
            return Ok(BackupIndex::default());
        }

        let file = File::open(&index_path)
            .map_err(|e| ReasonError::Backup(format!("Failed to open backup index: {}", e)))?;

        let index: BackupIndex = serde_json::from_reader(BufReader::new(file))
            .map_err(|e| ReasonError::Backup(format!("Failed to parse backup index: {}", e)))?;

        Ok(index)
    }

    // Internal: Save backup index
    fn save_index(&self, index: &BackupIndex) -> Result<()> {
        let index_path = self.backup_dir.join("index.json");

        let file = File::create(&index_path)
            .map_err(|e| ReasonError::Backup(format!("Failed to create backup index: {}", e)))?;

        serde_json::to_writer_pretty(BufWriter::new(file), index)
            .map_err(|e| ReasonError::Backup(format!("Failed to write backup index: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Table;
    use tempfile::tempdir;

    #[test]
    fn test_backup_options() {
        let opts = BackupOptions::full()
            .with_description("Test backup")
            .with_compression(9);

        assert_eq!(opts.backup_type, BackupType::Full);
        assert_eq!(opts.description, Some("Test backup".to_string()));
        assert_eq!(opts.compression_level, 9);
    }

    #[test]
    fn test_backup_manager_create_list() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.redb");
        let backup_dir = dir.path().join("backups");

        // Create store with some data
        let store = NodeStore::open(&db_path).unwrap();
        let table = Table::new("Test".to_string());
        store.insert_table(&table).unwrap();

        // Create backup manager
        let manager = BackupManager::new(&store, &backup_dir).unwrap();

        // Create backup
        let backup = manager
            .create_backup(&store, BackupOptions::full())
            .unwrap();
        assert!(backup.id.starts_with("snap_"));
        assert_eq!(backup.table_count, 1);

        // List backups
        let backups = manager.list_backups().unwrap();
        assert_eq!(backups.len(), 1);

        // Get latest
        let latest = manager.get_latest_backup().unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().id, backup.id);
    }

    #[test]
    fn test_backup_restore() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.redb");
        let backup_dir = dir.path().join("backups");
        let restore_path = dir.path().join("restored.redb");

        // Create store with some data
        let store = NodeStore::open(&db_path).unwrap();
        let table = Table::new("Test Table".to_string());
        store.insert_table(&table).unwrap();

        // Create backup
        let manager = BackupManager::new(&store, &backup_dir).unwrap();
        let backup = manager
            .create_backup(&store, BackupOptions::full())
            .unwrap();

        // Restore
        manager
            .restore(&backup.id, &restore_path, RestoreOptions::default())
            .unwrap();

        // Verify restored data
        let restored_store = NodeStore::open(&restore_path).unwrap();
        let tables = restored_store.list_tables().unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "Test Table");
    }
}
