//! Write replication via Raft
//!
//! When clustering is enabled, write operations go through the Raft log
//! for replication. Reads can be served from any node.

use reasondb_core::{
    cluster::{LogEntry, LogEntryType},
    cluster::ApplyResult,
    error::ReasonError,
    model::{Document, DocumentRelation, PageNode, RelationType, Table},
    store::NodeStore,
};
use std::sync::Arc;
use tracing::debug;

/// Create the apply callback that processes Raft log entries against the local NodeStore.
/// This is called by the state machine when entries are committed (on all nodes).
pub fn create_apply_callback(
    store: Arc<NodeStore>,
) -> impl Fn(&LogEntry) -> Result<ApplyResult, ReasonError> + Send + Sync {
    move |entry: &LogEntry| -> Result<ApplyResult, ReasonError> {
        match &entry.entry_type {
            LogEntryType::UpsertTable { table_id, name, description } => {
                debug!("Replicating UpsertTable: {}", table_id);
                let mut table = Table::new(name.clone());
                table.id = table_id.clone();
                if let Some(desc) = description {
                    table.description = Some(desc.clone());
                }
                // Try update first, then insert
                if store.get_table(table_id)?.is_some() {
                    store.update_table(&table)?;
                } else {
                    store.insert_table(&table)?;
                }
                Ok(ApplyResult::Success)
            }

            LogEntryType::DeleteTable { table_id } => {
                debug!("Replicating DeleteTable: {}", table_id);
                store.delete_table(table_id, true)?;
                Ok(ApplyResult::Success)
            }

            LogEntryType::UpsertDocument { document_id, table_id: _, title: _, metadata } => {
                debug!("Replicating UpsertDocument: {}", document_id);
                let doc: Document = bincode::deserialize(metadata)
                    .map_err(|e| ReasonError::Serialization(e.to_string()))?;
                if store.get_document(document_id)?.is_some() {
                    store.update_document(&doc)?;
                } else {
                    store.insert_document(&doc)?;
                }
                Ok(ApplyResult::Success)
            }

            LogEntryType::DeleteDocument { document_id } => {
                debug!("Replicating DeleteDocument: {}", document_id);
                store.delete_document(document_id)?;
                Ok(ApplyResult::Success)
            }

            LogEntryType::UpsertNode { node_id, document_id: _, data } => {
                debug!("Replicating UpsertNode: {}", node_id);
                let node: PageNode = bincode::deserialize(data)
                    .map_err(|e| ReasonError::Serialization(e.to_string()))?;
                if store.get_node(node_id)?.is_some() {
                    store.update_node(&node)?;
                } else {
                    store.insert_node(&node)?;
                }
                Ok(ApplyResult::Success)
            }

            LogEntryType::DeleteNode { node_id } => {
                debug!("Replicating DeleteNode: {}", node_id);
                store.delete_node(node_id)?;
                Ok(ApplyResult::Success)
            }

            LogEntryType::CreateRelation { from_doc, to_doc, relation_type, note } => {
                debug!("Replicating CreateRelation: {} -> {}", from_doc, to_doc);
                let rt = match relation_type.as_str() {
                    "references" => RelationType::References,
                    "referenced_by" => RelationType::ReferencedBy,
                    "follows_up" => RelationType::FollowsUp,
                    "followed_up_by" => RelationType::FollowedUpBy,
                    "supersedes" => RelationType::Supersedes,
                    "superseded_by" => RelationType::SupersededBy,
                    "related_to" => RelationType::RelatedTo,
                    "parent_of" => RelationType::ParentOf,
                    "child_of" => RelationType::ChildOf,
                    other => RelationType::Custom(other.to_string()),
                };
                let mut relation = DocumentRelation::new(from_doc.clone(), to_doc.clone(), rt);
                relation.note = note.clone();
                let _ = store.insert_relation(&relation); // Ignore duplicate errors
                Ok(ApplyResult::Success)
            }

            LogEntryType::DeleteRelation { from_doc, to_doc, relation_type: _ } => {
                debug!("Replicating DeleteRelation: {} -> {}", from_doc, to_doc);
                // Find relations from from_doc and delete matching ones
                if let Ok(relations) = store.get_all_relations(from_doc) {
                    for rel in relations {
                        if rel.to_document_id == *to_doc {
                            let _ = store.delete_relation(&rel.id);
                        }
                    }
                }
                Ok(ApplyResult::Success)
            }

            LogEntryType::CreateApiKey { .. } | LogEntryType::RevokeApiKey { .. } => {
                // API key replication handled separately via ApiKeyStore
                Ok(ApplyResult::Success)
            }

            LogEntryType::Noop | LogEntryType::MembershipChange { .. } => {
                Ok(ApplyResult::Success)
            }
        }
    }
}

/// Helper: propose a write operation through Raft if clustering is enabled.
/// Returns Ok(true) if replicated via Raft, Ok(false) if single-node mode (caller should apply directly).
pub async fn propose_write<R: reasondb_core::llm::ReasoningEngine>(
    state: &crate::state::AppState<R>,
    entry_type: LogEntryType,
) -> Result<bool, ReasonError> {
    match &state.cluster_node {
        Some(raft_node) => {
            if !raft_node.is_leader().await {
                return Err(ReasonError::InvalidOperation(
                    "Not the leader — forward write to leader node".to_string(),
                ));
            }
            let entry = LogEntry::new(entry_type);
            raft_node.propose(entry).await?;
            Ok(true)
        }
        None => Ok(false), // Single-node mode, caller applies directly
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reasondb_core::model::{Document, PageNode, Table};
    use tempfile::tempdir;

    fn create_test_store() -> (Arc<NodeStore>, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_repl.db");
        let store = Arc::new(NodeStore::open(&db_path).unwrap());
        (store, dir)
    }

    fn make_entry(entry_type: LogEntryType) -> LogEntry {
        LogEntry::new(entry_type)
    }

    fn create_test_table(store: &NodeStore, id: &str, name: &str) {
        let mut table = Table::new(name.to_string());
        table.id = id.to_string();
        store.insert_table(&table).unwrap();
    }

    fn create_test_document(store: &NodeStore, table_id: &str, title: &str) -> Document {
        let doc = Document::new(title.to_string(), table_id);
        store.insert_document(&doc).unwrap();
        doc
    }

    #[test]
    fn test_apply_upsert_table_insert() {
        let (store, _dir) = create_test_store();
        let callback = create_apply_callback(store.clone());

        let entry = make_entry(LogEntryType::UpsertTable {
            table_id: "tbl_1".to_string(),
            name: "Test Table".to_string(),
            description: Some("A test table".to_string()),
        });

        let result = callback(&entry).unwrap();
        assert!(matches!(result, ApplyResult::Success));

        let table = store.get_table("tbl_1").unwrap().unwrap();
        assert_eq!(table.name, "Test Table");
        assert_eq!(table.description, Some("A test table".to_string()));
    }

    #[test]
    fn test_apply_upsert_table_update() {
        let (store, _dir) = create_test_store();
        let callback = create_apply_callback(store.clone());

        create_test_table(&store, "tbl_1", "Original");

        let entry = make_entry(LogEntryType::UpsertTable {
            table_id: "tbl_1".to_string(),
            name: "Updated".to_string(),
            description: None,
        });

        callback(&entry).unwrap();
        let updated = store.get_table("tbl_1").unwrap().unwrap();
        assert_eq!(updated.name, "Updated");
    }

    #[test]
    fn test_apply_delete_table() {
        let (store, _dir) = create_test_store();
        let callback = create_apply_callback(store.clone());

        create_test_table(&store, "tbl_del", "To Delete");

        let entry = make_entry(LogEntryType::DeleteTable {
            table_id: "tbl_del".to_string(),
        });

        callback(&entry).unwrap();
        assert!(store.get_table("tbl_del").unwrap().is_none());
    }

    #[test]
    fn test_apply_upsert_document() {
        let (store, _dir) = create_test_store();
        let callback = create_apply_callback(store.clone());

        create_test_table(&store, "tbl_docs", "Docs");

        let doc = Document::new("Test Doc".to_string(), "tbl_docs");
        let doc_id = doc.id.clone();
        let metadata = bincode::serialize(&doc).unwrap();

        let entry = make_entry(LogEntryType::UpsertDocument {
            document_id: doc_id.clone(),
            table_id: "tbl_docs".to_string(),
            title: "Test Doc".to_string(),
            metadata,
        });

        callback(&entry).unwrap();
        let stored_doc = store.get_document(&doc_id).unwrap().unwrap();
        assert_eq!(stored_doc.title, "Test Doc");
    }

    #[test]
    fn test_apply_delete_document() {
        let (store, _dir) = create_test_store();
        let callback = create_apply_callback(store.clone());

        create_test_table(&store, "tbl_docs", "Docs");
        let doc = create_test_document(&store, "tbl_docs", "To Delete");

        let entry = make_entry(LogEntryType::DeleteDocument {
            document_id: doc.id.clone(),
        });

        callback(&entry).unwrap();
        assert!(store.get_document(&doc.id).unwrap().is_none());
    }

    #[test]
    fn test_apply_upsert_node() {
        let (store, _dir) = create_test_store();
        let callback = create_apply_callback(store.clone());

        let node = PageNode::new("doc_1".to_string(), "Node Title".to_string(), None, 0);
        let data = bincode::serialize(&node).unwrap();

        let entry = make_entry(LogEntryType::UpsertNode {
            node_id: node.id.clone(),
            document_id: "doc_1".to_string(),
            data,
        });

        callback(&entry).unwrap();
        let stored = store.get_node(&node.id).unwrap().unwrap();
        assert_eq!(stored.title, "Node Title");
    }

    #[test]
    fn test_apply_noop() {
        let (store, _dir) = create_test_store();
        let callback = create_apply_callback(store);

        let entry = make_entry(LogEntryType::Noop);
        let result = callback(&entry).unwrap();
        assert!(matches!(result, ApplyResult::Success));
    }

    #[test]
    fn test_apply_create_relation() {
        let (store, _dir) = create_test_store();
        let callback = create_apply_callback(store.clone());

        create_test_table(&store, "tbl_rels", "Rels");
        let doc1 = create_test_document(&store, "tbl_rels", "From");
        let doc2 = create_test_document(&store, "tbl_rels", "To");

        let entry = make_entry(LogEntryType::CreateRelation {
            from_doc: doc1.id.clone(),
            to_doc: doc2.id.clone(),
            relation_type: "references".to_string(),
            note: Some("A note".to_string()),
        });

        let result = callback(&entry).unwrap();
        assert!(matches!(result, ApplyResult::Success));

        let relations = store.get_all_relations(&doc1.id).unwrap();
        assert!(!relations.is_empty(), "Should have at least one relation");
        assert!(relations.iter().any(|r| r.to_document_id == doc2.id));
    }

    #[test]
    fn test_apply_api_key_noop() {
        let (store, _dir) = create_test_store();
        let callback = create_apply_callback(store);

        let entry = make_entry(LogEntryType::CreateApiKey {
            key_id: "key_1".to_string(),
            key_hash: "hash".to_string(),
            metadata: vec![],
        });

        let result = callback(&entry).unwrap();
        assert!(matches!(result, ApplyResult::Success));
    }
}
