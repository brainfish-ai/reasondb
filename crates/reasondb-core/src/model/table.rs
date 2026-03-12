//! Table - A collection of related documents
//!
//! Tables are the primary organizational unit for documents in ReasonDB.
//! Every document MUST belong to a table.
//!
//! # Name Uniqueness
//!
//! Table names must be unique. Names are normalized to a "slug" format for
//! uniqueness checks:
//! - Lowercased
//! - Spaces and special characters replaced with underscores
//! - Consecutive underscores collapsed
//!
//! Example: "Legal Contracts!" → "legal_contracts"

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

use super::{json_metadata, TableId};

/// A collection of related documents with metadata.
///
/// Tables allow organizing documents into logical groups (like folders or namespaces)
/// with custom metadata for filtering and access control.
///
/// # Name Uniqueness
///
/// Table names must be unique (case-insensitive). The `slug` field contains
/// the normalized name used for uniqueness checks and lookups.
///
/// # Example
///
/// ```rust
/// use reasondb_core::Table;
///
/// let mut table = Table::new("Legal Contracts".to_string());
/// assert_eq!(table.slug, "legal_contracts"); // Normalized for uniqueness
/// table.description = Some("All legal documents and contracts".to_string());
/// table.set_metadata("department", serde_json::json!("legal"));
/// table.set_metadata("confidential", serde_json::json!(true));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Table {
    /// Unique identifier for this table (primary key)
    pub id: TableId,

    /// Human-readable name (display name)
    pub name: String,

    /// Normalized name for uniqueness checks and lookups
    ///
    /// - Lowercased
    /// - Spaces/special chars → underscores
    /// - Used for lookups and uniqueness checks
    pub slug: String,

    /// Optional description
    pub description: Option<String>,

    /// Custom metadata (key-value pairs)
    ///
    /// Examples: `{"department": "legal", "region": "us-west", "confidential": true}`
    #[serde(with = "json_metadata")]
    pub metadata: HashMap<String, Value>,

    /// Domain-specific vocabulary and terminology for this table.
    ///
    /// Used by the LLM reasoning engine to understand domain-specific terms
    /// when traversing and interpreting documents in this table.
    ///
    /// Examples: `["contract", "indemnification", "arbitration"]`
    #[serde(default)]
    pub domain_vocab: Option<Vec<String>>,

    /// Contextual description for LLM reasoning.
    ///
    /// Provides background context about what this table contains and how
    /// documents should be interpreted during semantic search and reasoning.
    #[serde(default)]
    pub context: Option<String>,

    /// Instructions for LLM query and reasoning behavior on this table.
    ///
    /// Custom directives that guide how the LLM should answer questions
    /// or traverse documents within this table.
    #[serde(default)]
    pub instructions: Option<String>,

    /// Classification tags for this table.
    ///
    /// Used for grouping, filtering, and discovery of tables.
    #[serde(default)]
    pub tags: Option<Vec<String>>,

    /// Number of documents in this table
    pub document_count: usize,

    /// Total nodes across all documents
    pub total_nodes: usize,

    /// When this table was created
    pub created_at: DateTime<Utc>,

    /// When this table was last updated
    pub updated_at: DateTime<Utc>,
}

impl Table {
    /// Create a new table with a generated ID.
    ///
    /// The ID will be in the format `tbl_<uuid>`.
    /// The slug will be auto-generated from the name.
    pub fn new(name: String) -> Self {
        let slug = Self::slugify(&name);
        let now = Utc::now();
        Self {
            id: format!(
                "tbl_{}",
                Uuid::new_v4().to_string().split('-').next().unwrap()
            ),
            name,
            slug,
            description: None,
            metadata: HashMap::new(),
            domain_vocab: None,
            context: None,
            instructions: None,
            tags: None,
            document_count: 0,
            total_nodes: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new table with a specific ID.
    pub fn with_id(id: String, name: String) -> Self {
        let slug = Self::slugify(&name);
        let now = Utc::now();
        Self {
            id,
            name,
            slug,
            description: None,
            metadata: HashMap::new(),
            domain_vocab: None,
            context: None,
            instructions: None,
            tags: None,
            document_count: 0,
            total_nodes: 0,
            created_at: now,
            updated_at: now,
        }
    }

    // ==================== Slug Generation ====================

    /// Convert a name to a normalized slug.
    ///
    /// - Lowercase
    /// - Replace non-alphanumeric with underscores
    /// - Collapse consecutive underscores
    /// - Trim leading/trailing underscores
    ///
    /// # Examples
    ///
    /// ```rust
    /// use reasondb_core::Table;
    ///
    /// assert_eq!(Table::slugify("Legal Contracts"), "legal_contracts");
    /// assert_eq!(Table::slugify("My-Table!"), "my_table");
    /// assert_eq!(Table::slugify("  Multiple   Spaces  "), "multiple_spaces");
    /// assert_eq!(Table::slugify("CamelCaseTable"), "camelcasetable");
    /// ```
    pub fn slugify(name: &str) -> String {
        let slug: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();

        // Collapse consecutive underscores and trim
        let mut result = String::new();
        let mut prev_underscore = true; // Start true to trim leading underscores

        for c in slug.chars() {
            if c == '_' {
                if !prev_underscore {
                    result.push(c);
                }
                prev_underscore = true;
            } else {
                result.push(c);
                prev_underscore = false;
            }
        }

        // Trim trailing underscore
        if result.ends_with('_') {
            result.pop();
        }

        result
    }

    /// Update the name and regenerate the slug.
    pub fn set_name(&mut self, name: String) {
        self.slug = Self::slugify(&name);
        self.name = name;
        self.updated_at = Utc::now();
    }

    /// Set a metadata value.
    pub fn set_metadata(&mut self, key: &str, value: Value) {
        self.metadata.insert(key.to_string(), value);
        self.updated_at = Utc::now();
    }

    /// Get a metadata value.
    pub fn get_metadata(&self, key: &str) -> Option<&Value> {
        self.metadata.get(key)
    }

    /// Remove a metadata key.
    pub fn remove_metadata(&mut self, key: &str) -> Option<Value> {
        let result = self.metadata.remove(key);
        if result.is_some() {
            self.updated_at = Utc::now();
        }
        result
    }

    /// Increment document count.
    pub fn increment_documents(&mut self, nodes: usize) {
        self.document_count += 1;
        self.total_nodes += nodes;
        self.updated_at = Utc::now();
    }

    /// Decrement document count.
    pub fn decrement_documents(&mut self, nodes: usize) {
        self.document_count = self.document_count.saturating_sub(1);
        self.total_nodes = self.total_nodes.saturating_sub(nodes);
        self.updated_at = Utc::now();
    }
}
