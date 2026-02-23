//! SearchFilter - Criteria for filtering documents
//!
//! This module provides flexible filtering capabilities for document searches.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use super::{json_metadata_option, Document, DocumentId, TableId};

/// Search filter criteria for documents.
///
/// Allows filtering by table, tags, metadata, and date ranges.
/// All filters are combined with AND logic.
///
/// # Example
///
/// ```rust
/// use reasondb_core::SearchFilter;
/// use serde_json::json;
///
/// let filter = SearchFilter::new()
///     .with_table_id("legal-contracts")
///     .with_tags(vec!["nda", "active"])
///     .with_metadata("author", json!("Legal Team"));
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchFilter {
    // === Table Filters ===
    /// Filter by table ID (exact match)
    pub table_id: Option<TableId>,

    /// Filter by table name (contains match)
    pub table_name: Option<String>,

    /// Filter by table metadata
    #[serde(with = "json_metadata_option", default)]
    pub table_metadata: Option<HashMap<String, Value>>,

    // === Document Filters ===
    /// Filter by document ID
    pub document_id: Option<DocumentId>,

    /// Filter by document tags (any match - OR)
    pub tags: Option<Vec<String>>,

    /// Filter by document tags (all must match - AND)
    pub tags_all: Option<Vec<String>>,

    /// Filter by document metadata (exact match)
    #[serde(with = "json_metadata_option", default)]
    pub document_metadata: Option<HashMap<String, Value>>,

    // === Date Filters ===
    /// Only include documents created after this date
    pub created_after: Option<DateTime<Utc>>,

    /// Only include documents created before this date
    pub created_before: Option<DateTime<Utc>>,

    /// Only include documents updated after this date
    pub updated_after: Option<DateTime<Utc>>,

    /// Only include documents updated before this date
    pub updated_before: Option<DateTime<Utc>>,

    // === Pagination ===
    /// Maximum number of results
    pub limit: Option<usize>,

    /// Offset for pagination
    pub offset: Option<usize>,
}

impl SearchFilter {
    /// Create a new empty filter.
    pub fn new() -> Self {
        Self::default()
    }

    // ==================== Builder Methods ====================

    /// Filter by table ID.
    pub fn with_table_id(mut self, table_id: &str) -> Self {
        self.table_id = Some(table_id.to_string());
        self
    }

    /// Filter by tags (any match - OR logic).
    pub fn with_tags(mut self, tags: Vec<&str>) -> Self {
        self.tags = Some(tags.into_iter().map(|s| s.to_string()).collect());
        self
    }

    /// Filter by tags (all must match - AND logic).
    pub fn with_tags_all(mut self, tags: Vec<&str>) -> Self {
        self.tags_all = Some(tags.into_iter().map(|s| s.to_string()).collect());
        self
    }

    /// Filter by document metadata (exact match).
    pub fn with_metadata(mut self, key: &str, value: Value) -> Self {
        self.document_metadata
            .get_or_insert_with(HashMap::new)
            .insert(key.to_string(), value);
        self
    }

    /// Filter by creation date (after).
    pub fn with_created_after(mut self, date: DateTime<Utc>) -> Self {
        self.created_after = Some(date);
        self
    }

    /// Filter by creation date (before).
    pub fn with_created_before(mut self, date: DateTime<Utc>) -> Self {
        self.created_before = Some(date);
        self
    }

    /// Set maximum number of results.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    // ==================== Matching Methods ====================

    /// Check if a document matches date filters.
    pub fn matches_date_range(&self, doc: &Document) -> bool {
        if let Some(after) = self.created_after {
            if doc.created_at < after {
                return false;
            }
        }
        if let Some(before) = self.created_before {
            if doc.created_at > before {
                return false;
            }
        }
        if let Some(after) = self.updated_after {
            if doc.updated_at < after {
                return false;
            }
        }
        true
    }

    /// Check if a document matches tag filters.
    pub fn matches_tags(&self, doc: &Document) -> bool {
        // Check "any" tags (OR)
        if let Some(filter_tags) = &self.tags {
            let has_any = filter_tags.iter().any(|t| doc.tags.contains(t));
            if !has_any {
                return false;
            }
        }

        // Check "all" tags (AND)
        if let Some(filter_tags) = &self.tags_all {
            let has_all = filter_tags.iter().all(|t| doc.tags.contains(t));
            if !has_all {
                return false;
            }
        }

        true
    }

    /// Check if a document matches metadata filters.
    pub fn matches_metadata(&self, doc: &Document) -> bool {
        if let Some(filter_meta) = &self.document_metadata {
            for (key, value) in filter_meta {
                match doc.metadata.get(key) {
                    Some(doc_value) if doc_value == value => continue,
                    _ => return false,
                }
            }
        }
        true
    }

    /// Check if a document matches all non-indexed filters.
    pub fn matches_document(&self, doc: &Document) -> bool {
        self.matches_date_range(doc) && self.matches_tags(doc) && self.matches_metadata(doc)
    }
}
