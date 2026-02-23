//! Document CRUD operations
//!
//! Documents are the primary content units in ReasonDB.
//! Each document MUST belong to a table and can contain hierarchical nodes.

use redb::ReadableTable;

use super::indexes::{index_document_in_txn, unindex_document_in_txn, update_table_count_in_txn};
use super::{NodeStore, DOCUMENTS_TABLE};
use crate::error::{ReasonError, Result, StorageError};
use crate::model::Document;

impl NodeStore {
    /// Insert a new document into the database.
    ///
    /// The document's `table_id` MUST reference an existing table.
    ///
    /// # Errors
    ///
    /// Returns an error if the referenced table doesn't exist.
    pub fn insert_document(&self, doc: &Document) -> Result<()> {
        // Verify the table exists
        self.get_table_required(&doc.table_id)?;

        let key = doc.id.as_str();
        let value = bincode::serialize(doc)?;

        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        {
            let mut table = write_txn
                .open_table(DOCUMENTS_TABLE)
                .map_err(StorageError::from)?;
            table
                .insert(key, value.as_slice())
                .map_err(|e| StorageError::TableError(e.to_string()))?;

            // Index the document for fast filtering
            index_document_in_txn(&write_txn, doc)?;

            // Update table document count
            update_table_count_in_txn(&write_txn, &doc.table_id, 1, doc.total_nodes as i64)?;
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(())
    }

    /// Get a document by its ID.
    pub fn get_document(&self, id: &str) -> Result<Option<Document>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let table = read_txn
            .open_table(DOCUMENTS_TABLE)
            .map_err(StorageError::from)?;

        match table
            .get(id)
            .map_err(|e| StorageError::TableError(e.to_string()))?
        {
            Some(value) => {
                let doc: Document = bincode::deserialize(value.value())?;
                Ok(Some(doc))
            }
            None => Ok(None),
        }
    }

    /// Get a document, returning an error if not found.
    pub fn get_document_required(&self, id: &str) -> Result<Document> {
        self.get_document(id)?
            .ok_or_else(|| ReasonError::DocumentNotFound(id.to_string()))
    }

    /// Update an existing document.
    ///
    /// Automatically updates all indexes when document fields change.
    pub fn update_document(&self, doc: &Document) -> Result<()> {
        // Get the old document for index cleanup
        let old_doc = self.get_document_required(&doc.id)?;

        // If table changed, verify new table exists
        if old_doc.table_id != doc.table_id {
            self.get_table_required(&doc.table_id)?;
        }

        let key = doc.id.as_str();
        let value = bincode::serialize(doc)?;

        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        {
            let mut table = write_txn
                .open_table(DOCUMENTS_TABLE)
                .map_err(StorageError::from)?;
            table
                .insert(key, value.as_slice())
                .map_err(|e| StorageError::TableError(e.to_string()))?;

            // Unindex old values, index new values
            unindex_document_in_txn(&write_txn, &old_doc)?;
            index_document_in_txn(&write_txn, doc)?;

            // Update table counts if table changed
            if old_doc.table_id != doc.table_id {
                update_table_count_in_txn(
                    &write_txn,
                    &old_doc.table_id,
                    -1,
                    -(old_doc.total_nodes as i64),
                )?;
                update_table_count_in_txn(&write_txn, &doc.table_id, 1, doc.total_nodes as i64)?;
            }
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(())
    }

    /// Delete a document and all its nodes.
    pub fn delete_document(&self, id: &str) -> Result<bool> {
        let doc = match self.get_document(id)? {
            Some(d) => d,
            None => return Ok(false),
        };

        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        {
            // Delete all nodes for this document
            self.delete_document_nodes_in_txn(&write_txn, id)?;

            // Unindex the document
            unindex_document_in_txn(&write_txn, &doc)?;

            // Update table counts
            update_table_count_in_txn(&write_txn, &doc.table_id, -1, -(doc.total_nodes as i64))?;

            // Delete the document
            let mut table = write_txn
                .open_table(DOCUMENTS_TABLE)
                .map_err(StorageError::from)?;
            table
                .remove(id)
                .map_err(|e| StorageError::TableError(e.to_string()))?;
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(true)
    }

    /// List all documents.
    pub fn list_documents(&self) -> Result<Vec<Document>> {
        let read_txn = self.db.begin_read().map_err(StorageError::from)?;
        let table = read_txn
            .open_table(DOCUMENTS_TABLE)
            .map_err(StorageError::from)?;

        let mut docs = Vec::new();
        for result in table
            .iter()
            .map_err(|e| StorageError::TableError(e.to_string()))?
        {
            let (_, value) = result.map_err(|e| StorageError::TableError(e.to_string()))?;
            let doc: Document = bincode::deserialize(value.value())?;
            docs.push(doc);
        }
        Ok(docs)
    }

    /// List all documents in the database.
    ///
    /// This is an alias for `list_documents` for backup/export compatibility.
    pub fn list_all_documents(&self) -> Result<Vec<Document>> {
        self.list_documents()
    }

    /// Get all documents in a specific table.
    ///
    /// This is an alias for `get_documents_in_table` for backup/export compatibility.
    pub fn get_table_documents(&self, table_id: &str) -> Result<Vec<Document>> {
        self.get_documents_in_table(table_id)
    }

    /// Move a document to a different table.
    pub fn move_document_to_table(&self, doc_id: &str, new_table_id: &str) -> Result<()> {
        // Verify new table exists
        self.get_table_required(new_table_id)?;

        // Get the document
        let doc = self.get_document_required(doc_id)?;
        let old_table_id = doc.table_id.clone();

        // Skip if already in target table
        if old_table_id == new_table_id {
            return Ok(());
        }

        let write_txn = self.db.begin_write().map_err(StorageError::from)?;
        {
            // Update document
            let mut updated_doc = doc.clone();
            updated_doc.table_id = new_table_id.to_string();

            let value = bincode::serialize(&updated_doc)?;
            let mut table = write_txn
                .open_table(DOCUMENTS_TABLE)
                .map_err(StorageError::from)?;
            table
                .insert(doc_id, value.as_slice())
                .map_err(|e| StorageError::TableError(e.to_string()))?;

            // Update indexes
            unindex_document_in_txn(&write_txn, &doc)?;
            index_document_in_txn(&write_txn, &updated_doc)?;

            // Update table counts
            update_table_count_in_txn(&write_txn, &old_table_id, -1, -(doc.total_nodes as i64))?;
            update_table_count_in_txn(&write_txn, new_table_id, 1, doc.total_nodes as i64)?;
        }
        write_txn.commit().map_err(StorageError::from)?;
        Ok(())
    }
}
