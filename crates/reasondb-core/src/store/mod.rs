//! Storage engine for ReasonDB
//!
//! This module provides persistent storage using redb, a fast embedded database.
//! It handles serialization with bincode and provides CRUD operations for
//! tables, documents, and nodes. Includes secondary indexes for fast filtering.
//!
//! # Module Structure
//!
//! - `tables` - Table CRUD operations
//! - `documents` - Document CRUD operations
//! - `nodes` - Node CRUD operations
//! - `indexes` - Secondary index management
//! - `queries` - Search and filter operations

mod documents;
mod indexes;
mod nodes;
mod queries;
mod tables;

#[cfg(test)]
mod tests;

use std::path::Path;

use redb::{Database, MultimapTableDefinition, TableDefinition};

use crate::error::{Result, StorageError};

// ==================== Table Definitions ====================

/// Primary table for nodes (NodeId -> bincode bytes)
pub(crate) const NODES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("nodes");

/// Primary table for documents (DocumentId -> bincode bytes)
pub(crate) const DOCUMENTS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("documents");

/// Primary table for tables (TableId -> bincode bytes)
pub(crate) const TABLES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("tables");

/// Document-to-nodes index (DocumentId -> node IDs as JSON)
pub(crate) const DOC_NODES_INDEX: TableDefinition<&str, &[u8]> =
    TableDefinition::new("doc_nodes_index");

/// Table-to-documents index (TableId -> DocumentIds)
pub(crate) const IDX_TABLE_DOCS: MultimapTableDefinition<&str, &str> =
    MultimapTableDefinition::new("idx_table_docs");

/// Tag-to-documents index (Tag -> DocumentIds)
pub(crate) const IDX_TAG_DOCS: MultimapTableDefinition<&str, &str> =
    MultimapTableDefinition::new("idx_tag_docs");

/// Author-to-documents index (Author -> DocumentIds)
pub(crate) const IDX_AUTHOR_DOCS: MultimapTableDefinition<&str, &str> =
    MultimapTableDefinition::new("idx_author_docs");

/// Metadata value index (field:value -> DocumentIds)
pub(crate) const IDX_METADATA: MultimapTableDefinition<&str, &str> =
    MultimapTableDefinition::new("idx_metadata");

/// Table slug-to-ID index (slug -> TableId) for unique name enforcement
pub(crate) const IDX_TABLE_SLUG: TableDefinition<&str, &str> =
    TableDefinition::new("idx_table_slug");

// ==================== NodeStore ====================

/// Storage engine for ReasonDB.
///
/// Provides persistent storage for `Table`, `Document`, and `PageNode` objects using redb.
/// All data is serialized using bincode for efficient binary encoding.
/// Includes secondary indexes for fast filtered queries.
///
/// # Example
///
/// ```rust,no_run
/// use reasondb_core::{NodeStore, PageNode, Document, Table, SearchFilter};
///
/// # fn main() -> anyhow::Result<()> {
/// let store = NodeStore::open("./my_database")?;
///
/// // Create a table first (required for documents)
/// let table = Table::new("Legal".to_string());
/// store.insert_table(&table)?;
///
/// // Insert a document in the table
/// let mut doc = Document::new("Contract".to_string(), &table.id);
/// doc.tags = vec!["nda".to_string()];
/// store.insert_document(&doc)?;
///
/// // Filter documents
/// let filter = SearchFilter::new().with_table_id(&table.id);
/// let docs = store.find_documents(&filter)?;
/// # Ok(())
/// # }
/// ```
pub struct NodeStore {
    pub(crate) db: Database,
}

impl NodeStore {
    /// Open or create a database at the given path.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the database file
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or created.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = Database::create(path).map_err(StorageError::from)?;
        Self::initialize_tables(&db)?;
        Ok(Self { db })
    }

    /// Initialize all database tables.
    fn initialize_tables(db: &Database) -> Result<()> {
        let write_txn = db.begin_write().map_err(StorageError::from)?;
        {
            // Primary tables
            let _ = write_txn.open_table(NODES_TABLE).map_err(StorageError::from)?;
            let _ = write_txn
                .open_table(DOCUMENTS_TABLE)
                .map_err(StorageError::from)?;
            let _ = write_txn
                .open_table(TABLES_TABLE)
                .map_err(StorageError::from)?;

            // Index tables
            let _ = write_txn
                .open_table(DOC_NODES_INDEX)
                .map_err(StorageError::from)?;
            let _ = write_txn
                .open_multimap_table(IDX_TABLE_DOCS)
                .map_err(StorageError::from)?;
            let _ = write_txn
                .open_multimap_table(IDX_TAG_DOCS)
                .map_err(StorageError::from)?;
            let _ = write_txn
                .open_multimap_table(IDX_AUTHOR_DOCS)
                .map_err(StorageError::from)?;
            let _ = write_txn
                .open_multimap_table(IDX_METADATA)
                .map_err(StorageError::from)?;
            let _ = write_txn
                .open_table(IDX_TABLE_SLUG)
                .map_err(StorageError::from)?;
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(())
    }

    /// Get database statistics.
    pub fn stats(&self) -> Result<StoreStats> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let nodes_table = read_txn
            .open_table(NODES_TABLE)
            .map_err(StorageError::from)?;
        let docs_table = read_txn
            .open_table(DOCUMENTS_TABLE)
            .map_err(StorageError::from)?;
        let tables_table = read_txn
            .open_table(TABLES_TABLE)
            .map_err(StorageError::from)?;

        use redb::ReadableTableMetadata;
        Ok(StoreStats {
            total_nodes: nodes_table
                .len()
                .map_err(|e| StorageError::TableError(e.to_string()))? as usize,
            total_documents: docs_table
                .len()
                .map_err(|e| StorageError::TableError(e.to_string()))? as usize,
            total_tables: tables_table
                .len()
                .map_err(|e| StorageError::TableError(e.to_string()))? as usize,
        })
    }
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct StoreStats {
    pub total_nodes: usize,
    pub total_documents: usize,
    pub total_tables: usize,
}
