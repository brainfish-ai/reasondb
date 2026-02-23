//! Search and filter operations
//!
//! This module provides filtered document queries using secondary indexes.

use std::collections::HashSet;

use super::indexes::{format_metadata_key, get_doc_ids_from_index};
use super::{NodeStore, IDX_METADATA, IDX_TABLE_DOCS, IDX_TAG_DOCS};
use crate::error::{Result, StorageError};
use crate::model::{Document, SearchFilter};

impl NodeStore {
    /// Find documents matching the given filter.
    ///
    /// Uses secondary indexes for fast lookups. Filters are combined with AND logic.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use reasondb_core::{NodeStore, SearchFilter};
    /// use serde_json::Value;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let store = NodeStore::open("./db")?;
    ///
    /// // Find active NDA documents in the legal table
    /// let filter = SearchFilter::new()
    ///     .with_table_id("legal")
    ///     .with_tags(vec!["nda", "active"])
    ///     .with_metadata("signed", Value::Bool(true));
    ///
    /// let docs = store.find_documents(&filter)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn find_documents(&self, filter: &SearchFilter) -> Result<Vec<Document>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;

        // Start with all document IDs or filtered set
        let mut candidate_ids: Option<HashSet<String>> = None;

        // Filter by table
        if let Some(table_id) = &filter.table_id {
            let ids = get_doc_ids_from_index(&read_txn, IDX_TABLE_DOCS, table_id)?;
            candidate_ids = Some(ids.into_iter().collect());
        }

        // Filter by tags (match ANY)
        if let Some(tags) = &filter.tags {
            let mut tag_ids = HashSet::new();
            for tag in tags {
                let ids = get_doc_ids_from_index(&read_txn, IDX_TAG_DOCS, &tag.to_lowercase())?;
                tag_ids.extend(ids);
            }
            candidate_ids = intersect_or_replace(candidate_ids, tag_ids);
        }

        // Filter by tags_all (match ALL)
        if let Some(tags) = &filter.tags_all {
            for tag in tags {
                let ids = get_doc_ids_from_index(&read_txn, IDX_TAG_DOCS, &tag.to_lowercase())?;
                let tag_set: HashSet<String> = ids.into_iter().collect();
                candidate_ids = intersect_or_replace(candidate_ids, tag_set);
            }
        }

        // Filter by metadata
        if let Some(metadata) = &filter.document_metadata {
            for (key, value) in metadata {
                let index_key = format_metadata_key(key, value);
                let ids = get_doc_ids_from_index(&read_txn, IDX_METADATA, &index_key)?;
                candidate_ids = intersect_or_replace(candidate_ids, ids.into_iter().collect());
            }
        }

        // If no filters, get all documents
        let doc_ids: Vec<String> = match candidate_ids {
            Some(ids) => ids.into_iter().collect(),
            None => self
                .list_documents()?
                .iter()
                .map(|d| d.id.clone())
                .collect(),
        };

        // Load documents and apply date filters
        let mut results = Vec::new();
        for id in doc_ids {
            if let Some(doc) = self.get_document(&id)? {
                // Apply date filters if specified
                if matches_date_filters(&doc, filter) {
                    results.push(doc);
                }
            }
        }

        // Apply pagination
        let results = apply_pagination(results, filter.offset, filter.limit);

        Ok(results)
    }

    /// Get all documents in a specific table.
    pub fn get_documents_in_table(&self, table_id: &str) -> Result<Vec<Document>> {
        self.find_documents(&SearchFilter::new().with_table_id(table_id))
    }

    /// Get all documents with a specific tag.
    pub fn get_documents_by_tag(&self, tag: &str) -> Result<Vec<Document>> {
        self.find_documents(&SearchFilter::new().with_tags(vec![tag]))
    }
}

/// Intersect two sets, or replace if first is None.
fn intersect_or_replace(
    existing: Option<HashSet<String>>,
    new: HashSet<String>,
) -> Option<HashSet<String>> {
    match existing {
        Some(set) => Some(set.intersection(&new).cloned().collect()),
        None => Some(new),
    }
}

/// Apply pagination to results.
fn apply_pagination<T>(mut items: Vec<T>, offset: Option<usize>, limit: Option<usize>) -> Vec<T> {
    if let Some(off) = offset {
        if off >= items.len() {
            return vec![];
        }
        items = items.into_iter().skip(off).collect();
    }
    if let Some(lim) = limit {
        items.truncate(lim);
    }
    items
}

/// Check if a document matches the date filters.
fn matches_date_filters(doc: &Document, filter: &SearchFilter) -> bool {
    if let Some(after) = filter.created_after {
        if doc.created_at < after {
            return false;
        }
    }
    if let Some(before) = filter.created_before {
        if doc.created_at > before {
            return false;
        }
    }
    if let Some(after) = filter.updated_after {
        if doc.updated_at < after {
            return false;
        }
    }
    if let Some(before) = filter.updated_before {
        if doc.updated_at > before {
            return false;
        }
    }
    true
}
