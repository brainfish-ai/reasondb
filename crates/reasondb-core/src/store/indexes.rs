//! Secondary index management
//!
//! This module handles indexing documents for fast filtered queries.
//! Indexes are maintained automatically when documents are inserted/updated/deleted.

use redb::ReadableTable;
use serde_json::Value;

use super::{IDX_METADATA, IDX_TABLE_DOCS, IDX_TAG_DOCS, TABLES_TABLE};
use crate::error::{Result, StorageError};
use crate::model::{Document, Table};

/// Index a document in all secondary indexes.
pub(crate) fn index_document_in_txn(
    write_txn: &redb::WriteTransaction,
    doc: &Document,
) -> Result<()> {
    // Index by table
    {
        let mut idx = write_txn
            .open_multimap_table(IDX_TABLE_DOCS)
            .map_err(StorageError::from)?;
        idx.insert(doc.table_id.as_str(), doc.id.as_str())
            .map_err(|e| StorageError::TableError(e.to_string()))?;
    }

    // Index by tags
    {
        let mut idx = write_txn
            .open_multimap_table(IDX_TAG_DOCS)
            .map_err(StorageError::from)?;
        for tag in &doc.tags {
            idx.insert(tag.to_lowercase().as_str(), doc.id.as_str())
                .map_err(|e| StorageError::TableError(e.to_string()))?;
        }
    }

    // Index metadata values
    {
        let mut idx = write_txn
            .open_multimap_table(IDX_METADATA)
            .map_err(StorageError::from)?;
        for (key, value) in &doc.metadata {
            let index_key = format_metadata_key(key, value);
            idx.insert(index_key.as_str(), doc.id.as_str())
                .map_err(|e| StorageError::TableError(e.to_string()))?;
        }
    }

    Ok(())
}

/// Remove a document from all secondary indexes.
pub(crate) fn unindex_document_in_txn(
    write_txn: &redb::WriteTransaction,
    doc: &Document,
) -> Result<()> {
    // Unindex from table
    {
        let mut idx = write_txn
            .open_multimap_table(IDX_TABLE_DOCS)
            .map_err(StorageError::from)?;
        idx.remove(doc.table_id.as_str(), doc.id.as_str())
            .map_err(|e| StorageError::TableError(e.to_string()))?;
    }

    // Unindex from tags
    {
        let mut idx = write_txn
            .open_multimap_table(IDX_TAG_DOCS)
            .map_err(StorageError::from)?;
        for tag in &doc.tags {
            idx.remove(tag.to_lowercase().as_str(), doc.id.as_str())
                .map_err(|e| StorageError::TableError(e.to_string()))?;
        }
    }

    // Unindex metadata values
    {
        let mut idx = write_txn
            .open_multimap_table(IDX_METADATA)
            .map_err(StorageError::from)?;
        for (key, value) in &doc.metadata {
            let index_key = format_metadata_key(key, value);
            idx.remove(index_key.as_str(), doc.id.as_str())
                .map_err(|e| StorageError::TableError(e.to_string()))?;
        }
    }

    Ok(())
}

/// Update table document and node counts.
pub(crate) fn update_table_count_in_txn(
    write_txn: &redb::WriteTransaction,
    table_id: &str,
    doc_delta: i64,
    node_delta: i64,
) -> Result<()> {
    let mut tables = write_txn
        .open_table(TABLES_TABLE)
        .map_err(StorageError::from)?;

    // Read the table
    let table_data: Option<Table> = {
        let table_opt = tables
            .get(table_id)
            .map_err(|e| StorageError::TableError(e.to_string()))?;

        match table_opt {
            Some(value) => {
                let t: Table = bincode::deserialize(value.value())?;
                Some(t)
            }
            None => None,
        }
    };

    // Update and write back
    if let Some(mut table) = table_data {
        table.document_count = (table.document_count as i64 + doc_delta).max(0) as usize;
        table.total_nodes = (table.total_nodes as i64 + node_delta).max(0) as usize;

        let value = bincode::serialize(&table)?;
        tables
            .insert(table_id, value.as_slice())
            .map_err(|e| StorageError::TableError(e.to_string()))?;
    }

    Ok(())
}

/// Format a metadata key for indexing.
///
/// Creates a searchable key from field name and value.
pub(crate) fn format_metadata_key(field: &str, value: &Value) -> String {
    match value {
        Value::String(s) => format!("{}:s:{}", field, s),
        Value::Number(n) => format!("{}:n:{}", field, n),
        Value::Bool(b) => format!("{}:b:{}", field, b),
        Value::Null => format!("{}:null", field),
        // For complex types, use JSON string
        _ => format!("{}:j:{}", field, value),
    }
}

/// Get document IDs from a multimap index.
pub(crate) fn get_doc_ids_from_index(
    read_txn: &redb::ReadTransaction,
    index: MultimapTableDefinition<&str, &str>,
    key: &str,
) -> Result<Vec<String>>
where
{
    let table = read_txn
        .open_multimap_table(index)
        .map_err(StorageError::from)?;

    let values = table
        .get(key)
        .map_err(|e| StorageError::TableError(e.to_string()))?;

    let mut ids = Vec::new();
    for result in values {
        let value = result.map_err(|e| StorageError::TableError(e.to_string()))?;
        ids.push(value.value().to_string());
    }
    Ok(ids)
}

/// Type alias for multimap table definition used in queries
pub(crate) type MultimapTableDefinition<'a, K, V> = redb::MultimapTableDefinition<'a, K, V>;
