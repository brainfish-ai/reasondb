//! Document - Root-level document metadata
//!
//! A Document represents a complete ingested document with metadata
//! about its tree structure, table assignment, tags, and custom attributes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

use super::{json_metadata, DocumentId, NodeId, TableId};

/// Root-level document metadata.
///
/// A `Document` represents a complete ingested document and contains
/// metadata about the tree structure without holding the actual nodes.
/// Every document MUST belong to a table.
///
/// # Example
///
/// ```rust
/// use reasondb_core::Document;
///
/// // Create a document - table_id is required
/// let mut doc = Document::new("NDA Agreement".to_string(), "legal-contracts");
/// doc.tags = vec!["nda".to_string(), "confidential".to_string()];
/// doc.set_metadata("author", serde_json::json!("Legal Team"));
/// doc.set_metadata("contract_type", serde_json::json!("nda"));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Document {
    /// Unique identifier for this document
    pub id: DocumentId,

    /// Human-readable title
    pub title: String,

    /// ID of the root node in the tree
    pub root_node_id: NodeId,

    /// Total number of nodes in the tree
    pub total_nodes: usize,

    /// Maximum depth of the tree
    pub max_depth: u8,

    /// Original source file path or URL
    pub source_path: String,

    /// MIME type of the source document
    pub mime_type: Option<String>,

    /// File size in bytes (if applicable)
    pub file_size: Option<u64>,

    /// When this document was ingested
    pub created_at: DateTime<Utc>,

    /// When this document was last updated
    pub updated_at: DateTime<Utc>,

    // === Table & Metadata Fields ===
    /// Table this document belongs to (REQUIRED)
    pub table_id: TableId,

    /// Custom metadata (key-value pairs with JSON values)
    ///
    /// Examples: `{"contract_type": "nda", "value_usd": 50000, "signed": true}`
    #[serde(with = "json_metadata")]
    pub metadata: HashMap<String, Value>,

    /// Tags for quick filtering
    pub tags: Vec<String>,

    /// Original source URL (if ingested from web)
    pub source_url: Option<String>,

    /// Document language (e.g., "en", "es", "fr")
    pub language: Option<String>,

    /// Document version
    pub version: Option<String>,
}

impl Document {
    /// Create a new Document with generated ID in a specific table.
    ///
    /// Table ID is REQUIRED - the table must be created first.
    pub fn new(title: String, table_id: &str) -> Self {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        Self {
            id,
            title,
            root_node_id: String::new(),
            total_nodes: 0,
            max_depth: 0,
            source_path: String::new(),
            mime_type: None,
            file_size: None,
            created_at: now,
            updated_at: now,
            table_id: table_id.to_string(),
            metadata: HashMap::new(),
            tags: Vec::new(),
            source_url: None,
            language: None,
            version: None,
        }
    }

    /// Create a document from a file path in a specific table.
    pub fn from_path(title: String, path: &str, table_id: &str) -> Self {
        let mut doc = Self::new(title, table_id);
        doc.source_path = path.to_string();
        doc
    }

    /// Create a document assigned to a table (alias for new).
    pub fn in_table(title: String, table_id: &str) -> Self {
        Self::new(title, table_id)
    }

    // ==================== Tree Operations ====================

    /// Set the root node ID.
    pub fn set_root_node(&mut self, root_id: NodeId) {
        self.root_node_id = root_id;
        self.updated_at = Utc::now();
    }

    /// Update tree statistics.
    pub fn update_stats(&mut self, total_nodes: usize, max_depth: u8) {
        self.total_nodes = total_nodes;
        self.max_depth = max_depth;
        self.updated_at = Utc::now();
    }

    // ==================== Metadata Operations ====================

    /// Set a metadata value (JSON).
    pub fn set_metadata(&mut self, key: &str, value: Value) {
        self.metadata.insert(key.to_string(), value);
        self.updated_at = Utc::now();
    }

    /// Get a metadata value.
    pub fn get_metadata(&self, key: &str) -> Option<&Value> {
        self.metadata.get(key)
    }

    /// Add custom metadata (legacy compatibility - string value).
    pub fn add_metadata(&mut self, key: &str, value: &str) {
        self.metadata
            .insert(key.to_string(), Value::String(value.to_string()));
        self.updated_at = Utc::now();
    }

    // ==================== Tag Operations ====================

    /// Add a tag.
    pub fn add_tag(&mut self, tag: &str) {
        if !self.tags.contains(&tag.to_string()) {
            self.tags.push(tag.to_string());
            self.updated_at = Utc::now();
        }
    }

    /// Remove a tag.
    pub fn remove_tag(&mut self, tag: &str) {
        if let Some(pos) = self.tags.iter().position(|t| t == tag) {
            self.tags.remove(pos);
            self.updated_at = Utc::now();
        }
    }

    /// Check if document has a tag.
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains(&tag.to_string())
    }

    // ==================== Table Operations ====================

    /// Set the table ID.
    pub fn set_table(&mut self, table_id: &str) {
        self.table_id = table_id.to_string();
        self.updated_at = Utc::now();
    }

    /// Get the table ID.
    pub fn get_table_id(&self) -> &str {
        &self.table_id
    }
}
