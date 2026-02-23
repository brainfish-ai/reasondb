//! Document relationship storage operations
//!
//! This module provides CRUD operations for document relationships.

use redb::{MultimapTableDefinition, ReadableTable, TableDefinition};

use crate::error::{ReasonError, Result, StorageError};
use crate::model::{DocumentRelation, RelationType};

use super::NodeStore;

// ==================== Table Definitions ====================

/// Primary table: relation_id → DocumentRelation
pub(crate) const RELATIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("relations");

/// Index: from_document_id → [relation_ids]
pub(crate) const IDX_FROM_DOC: MultimapTableDefinition<&str, &str> =
    MultimapTableDefinition::new("idx_relation_from_doc");

/// Index: to_document_id → [relation_ids]
pub(crate) const IDX_TO_DOC: MultimapTableDefinition<&str, &str> =
    MultimapTableDefinition::new("idx_relation_to_doc");

/// Index: (from_doc, to_doc) → relation_id (for duplicate checking)
pub(crate) const IDX_DOC_PAIR: TableDefinition<&str, &str> =
    TableDefinition::new("idx_relation_doc_pair");

impl NodeStore {
    // ==================== Create ====================

    /// Insert a new document relationship.
    ///
    /// If the relationship type is not symmetric, also creates the inverse relationship.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Either document doesn't exist
    /// - The relationship already exists
    /// - Database write fails
    pub fn insert_relation(&self, relation: &DocumentRelation) -> Result<()> {
        // Verify both documents exist
        self.get_document(&relation.from_document_id)?
            .ok_or_else(|| ReasonError::DocumentNotFound(relation.from_document_id.clone()))?;
        self.get_document(&relation.to_document_id)?
            .ok_or_else(|| ReasonError::DocumentNotFound(relation.to_document_id.clone()))?;

        let txn = self.db.begin_write().map_err(StorageError::from)?;

        {
            let mut relations = txn.open_table(RELATIONS).map_err(StorageError::from)?;
            let mut idx_from = txn
                .open_multimap_table(IDX_FROM_DOC)
                .map_err(StorageError::from)?;
            let mut idx_to = txn
                .open_multimap_table(IDX_TO_DOC)
                .map_err(StorageError::from)?;
            let mut idx_pair = txn.open_table(IDX_DOC_PAIR).map_err(StorageError::from)?;

            // Check for duplicate
            let pair_key = format!("{}:{}", relation.from_document_id, relation.to_document_id);
            if idx_pair
                .get(pair_key.as_str())
                .map_err(StorageError::from)?
                .is_some()
            {
                return Err(StorageError::RelationAlreadyExists(
                    relation.from_document_id.clone(),
                    relation.to_document_id.clone(),
                )
                .into());
            }

            // Serialize and store the relation
            let data = bincode::serialize(relation)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            relations
                .insert(relation.id.as_str(), data.as_slice())
                .map_err(StorageError::from)?;

            // Update indexes
            idx_from
                .insert(relation.from_document_id.as_str(), relation.id.as_str())
                .map_err(StorageError::from)?;
            idx_to
                .insert(relation.to_document_id.as_str(), relation.id.as_str())
                .map_err(StorageError::from)?;
            idx_pair
                .insert(pair_key.as_str(), relation.id.as_str())
                .map_err(StorageError::from)?;

            // If not symmetric, create inverse relationship
            if !relation.relation_type.is_symmetric() {
                let inverse = relation.inverse();
                let inv_data = bincode::serialize(&inverse)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                relations
                    .insert(inverse.id.as_str(), inv_data.as_slice())
                    .map_err(StorageError::from)?;

                let inv_pair_key =
                    format!("{}:{}", inverse.from_document_id, inverse.to_document_id);
                idx_from
                    .insert(inverse.from_document_id.as_str(), inverse.id.as_str())
                    .map_err(StorageError::from)?;
                idx_to
                    .insert(inverse.to_document_id.as_str(), inverse.id.as_str())
                    .map_err(StorageError::from)?;
                idx_pair
                    .insert(inv_pair_key.as_str(), inverse.id.as_str())
                    .map_err(StorageError::from)?;
            }
        }

        txn.commit().map_err(StorageError::from)?;
        Ok(())
    }

    // ==================== Read ====================

    /// Get a relationship by ID.
    pub fn get_relation(&self, relation_id: &str) -> Result<Option<DocumentRelation>> {
        let txn = self.db.begin_read().map_err(StorageError::from)?;
        let relations = txn.open_table(RELATIONS).map_err(StorageError::from)?;

        if let Some(data) = relations.get(relation_id).map_err(StorageError::from)? {
            let relation: DocumentRelation = bincode::deserialize(data.value())
                .map_err(|e| StorageError::Deserialization(e.to_string()))?;
            Ok(Some(relation))
        } else {
            Ok(None)
        }
    }

    /// Get all relationships FROM a document.
    ///
    /// Returns relationships where the given document is the source.
    pub fn get_relations_from(&self, document_id: &str) -> Result<Vec<DocumentRelation>> {
        let txn = self.db.begin_read().map_err(StorageError::from)?;
        let relations = txn.open_table(RELATIONS).map_err(StorageError::from)?;
        let idx_from = txn
            .open_multimap_table(IDX_FROM_DOC)
            .map_err(StorageError::from)?;

        let mut result = Vec::new();
        let iter = idx_from.get(document_id).map_err(StorageError::from)?;

        for rel_id_result in iter {
            let rel_id = rel_id_result.map_err(StorageError::from)?;
            if let Some(data) = relations.get(rel_id.value()).map_err(StorageError::from)? {
                let relation: DocumentRelation = bincode::deserialize(data.value())
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                result.push(relation);
            }
        }

        Ok(result)
    }

    /// Get all relationships TO a document.
    ///
    /// Returns relationships where the given document is the target.
    pub fn get_relations_to(&self, document_id: &str) -> Result<Vec<DocumentRelation>> {
        let txn = self.db.begin_read().map_err(StorageError::from)?;
        let relations = txn.open_table(RELATIONS).map_err(StorageError::from)?;
        let idx_to = txn
            .open_multimap_table(IDX_TO_DOC)
            .map_err(StorageError::from)?;

        let mut result = Vec::new();
        let iter = idx_to.get(document_id).map_err(StorageError::from)?;

        for rel_id_result in iter {
            let rel_id = rel_id_result.map_err(StorageError::from)?;
            if let Some(data) = relations.get(rel_id.value()).map_err(StorageError::from)? {
                let relation: DocumentRelation = bincode::deserialize(data.value())
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                result.push(relation);
            }
        }

        Ok(result)
    }

    /// Get all relationships for a document (both directions).
    pub fn get_all_relations(&self, document_id: &str) -> Result<Vec<DocumentRelation>> {
        let mut relations = self.get_relations_from(document_id)?;
        let to_relations = self.get_relations_to(document_id)?;

        // Merge, avoiding duplicates (for symmetric relations)
        let from_ids: std::collections::HashSet<String> =
            relations.iter().map(|r| r.id.clone()).collect();
        for rel in to_relations {
            if !from_ids.contains(&rel.id) {
                relations.push(rel);
            }
        }

        Ok(relations)
    }

    /// Get all documents related to a given document.
    ///
    /// Optionally filter by relationship type.
    pub fn get_related_documents(
        &self,
        document_id: &str,
        relation_type: Option<&RelationType>,
    ) -> Result<Vec<String>> {
        let relations = self.get_all_relations(document_id)?;

        // Use a HashSet to deduplicate
        let related_ids: std::collections::HashSet<String> = relations
            .into_iter()
            .filter(|r| {
                if let Some(rt) = relation_type {
                    &r.relation_type == rt
                } else {
                    true
                }
            })
            .map(|r| {
                if r.from_document_id == document_id {
                    r.to_document_id
                } else {
                    r.from_document_id
                }
            })
            .collect();

        Ok(related_ids.into_iter().collect())
    }

    // ==================== Delete ====================

    /// Delete a relationship by ID.
    pub fn delete_relation(&self, relation_id: &str) -> Result<bool> {
        // First get the relation to know the doc IDs
        let relation = match self.get_relation(relation_id)? {
            Some(r) => r,
            None => return Ok(false),
        };

        let txn = self.db.begin_write().map_err(StorageError::from)?;

        {
            let mut relations = txn.open_table(RELATIONS).map_err(StorageError::from)?;
            let mut idx_from = txn
                .open_multimap_table(IDX_FROM_DOC)
                .map_err(StorageError::from)?;
            let mut idx_to = txn
                .open_multimap_table(IDX_TO_DOC)
                .map_err(StorageError::from)?;
            let mut idx_pair = txn.open_table(IDX_DOC_PAIR).map_err(StorageError::from)?;

            // Remove from primary table
            relations.remove(relation_id).map_err(StorageError::from)?;

            // Remove from indexes
            idx_from
                .remove(relation.from_document_id.as_str(), relation_id)
                .map_err(StorageError::from)?;
            idx_to
                .remove(relation.to_document_id.as_str(), relation_id)
                .map_err(StorageError::from)?;

            let pair_key = format!("{}:{}", relation.from_document_id, relation.to_document_id);
            idx_pair
                .remove(pair_key.as_str())
                .map_err(StorageError::from)?;
        }

        txn.commit().map_err(StorageError::from)?;
        Ok(true)
    }

    /// Delete all relationships for a document.
    ///
    /// Called automatically when a document is deleted.
    pub fn delete_relations_for_document(&self, document_id: &str) -> Result<usize> {
        let relations = self.get_all_relations(document_id)?;
        let count = relations.len();

        for relation in relations {
            self.delete_relation(&relation.id)?;
        }

        Ok(count)
    }

    /// Check if two documents are related.
    pub fn are_documents_related(&self, doc_a: &str, doc_b: &str) -> Result<bool> {
        let txn = self.db.begin_read().map_err(StorageError::from)?;
        let idx_pair = txn.open_table(IDX_DOC_PAIR).map_err(StorageError::from)?;

        let pair_key_ab = format!("{}:{}", doc_a, doc_b);
        let pair_key_ba = format!("{}:{}", doc_b, doc_a);

        Ok(idx_pair
            .get(pair_key_ab.as_str())
            .map_err(StorageError::from)?
            .is_some()
            || idx_pair
                .get(pair_key_ba.as_str())
                .map_err(StorageError::from)?
                .is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Document, Table};
    use tempfile::tempdir;

    fn create_test_store() -> (NodeStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let store = NodeStore::open(&db_path).unwrap();
        (store, dir)
    }

    fn setup_documents(store: &NodeStore) -> (String, String) {
        let table = Table::new("Test".to_string());
        store.insert_table(&table).unwrap();

        let doc_a = Document::new("Document A".to_string(), &table.id);
        let doc_b = Document::new("Document B".to_string(), &table.id);

        store.insert_document(&doc_a).unwrap();
        store.insert_document(&doc_b).unwrap();

        (doc_a.id, doc_b.id)
    }

    #[test]
    fn test_insert_and_get_relation() {
        let (store, _dir) = create_test_store();
        let (doc_a, doc_b) = setup_documents(&store);

        let rel = DocumentRelation::new(&doc_a, &doc_b, RelationType::References);
        store.insert_relation(&rel).unwrap();

        let retrieved = store.get_relation(&rel.id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.from_document_id, doc_a);
        assert_eq!(retrieved.to_document_id, doc_b);
        assert_eq!(retrieved.relation_type, RelationType::References);
    }

    #[test]
    fn test_inverse_relation_created() {
        let (store, _dir) = create_test_store();
        let (doc_a, doc_b) = setup_documents(&store);

        let rel = DocumentRelation::new(&doc_a, &doc_b, RelationType::References);
        store.insert_relation(&rel).unwrap();

        // Check that inverse relation exists
        let relations_to_a = store.get_relations_to(&doc_a).unwrap();
        assert_eq!(relations_to_a.len(), 1);
        assert_eq!(relations_to_a[0].relation_type, RelationType::ReferencedBy);
        assert_eq!(relations_to_a[0].from_document_id, doc_b);
    }

    #[test]
    fn test_symmetric_relation_no_inverse() {
        let (store, _dir) = create_test_store();
        let (doc_a, doc_b) = setup_documents(&store);

        let rel = DocumentRelation::new(&doc_a, &doc_b, RelationType::RelatedTo);
        store.insert_relation(&rel).unwrap();

        // For symmetric relations, only one relation should exist
        let relations_from_a = store.get_relations_from(&doc_a).unwrap();
        let relations_to_a = store.get_relations_to(&doc_a).unwrap();

        assert_eq!(relations_from_a.len(), 1);
        assert_eq!(relations_to_a.len(), 0); // No inverse for symmetric
    }

    #[test]
    fn test_get_related_documents() {
        let (store, _dir) = create_test_store();
        let (doc_a, doc_b) = setup_documents(&store);

        let rel = DocumentRelation::new(&doc_a, &doc_b, RelationType::References);
        store.insert_relation(&rel).unwrap();

        let related = store.get_related_documents(&doc_a, None).unwrap();
        assert!(related.contains(&doc_b));
    }

    #[test]
    fn test_duplicate_relation_error() {
        let (store, _dir) = create_test_store();
        let (doc_a, doc_b) = setup_documents(&store);

        let rel1 = DocumentRelation::new(&doc_a, &doc_b, RelationType::References);
        store.insert_relation(&rel1).unwrap();

        let rel2 = DocumentRelation::new(&doc_a, &doc_b, RelationType::References);
        let result = store.insert_relation(&rel2);
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_relation() {
        let (store, _dir) = create_test_store();
        let (doc_a, doc_b) = setup_documents(&store);

        let rel = DocumentRelation::new(&doc_a, &doc_b, RelationType::References);
        store.insert_relation(&rel).unwrap();

        let deleted = store.delete_relation(&rel.id).unwrap();
        assert!(deleted);

        let retrieved = store.get_relation(&rel.id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_are_documents_related() {
        let (store, _dir) = create_test_store();
        let (doc_a, doc_b) = setup_documents(&store);

        assert!(!store.are_documents_related(&doc_a, &doc_b).unwrap());

        let rel = DocumentRelation::new(&doc_a, &doc_b, RelationType::References);
        store.insert_relation(&rel).unwrap();

        assert!(store.are_documents_related(&doc_a, &doc_b).unwrap());
        assert!(store.are_documents_related(&doc_b, &doc_a).unwrap()); // Check reverse too
    }
}
