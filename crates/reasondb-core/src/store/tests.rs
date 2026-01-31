//! Tests for the storage engine

use serde_json::Value;
use tempfile::tempdir;

use super::NodeStore;
use crate::model::{Document, PageNode, SearchFilter, Table};

fn create_test_store() -> (NodeStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let store = NodeStore::open(&db_path).unwrap();
    (store, dir)
}

// ==================== Node Tests ====================

#[test]
fn test_node_crud() {
    let (store, _dir) = create_test_store();

    // Create
    let node = PageNode::new(
        "doc_1".to_string(),
        "Test Node".to_string(),
        Some("A summary".to_string()),
        0,
    );
    store.insert_node(&node).unwrap();

    // Read
    let retrieved = store.get_node(&node.id).unwrap().unwrap();
    assert_eq!(retrieved.title, "Test Node");

    // Update
    let mut updated = retrieved.clone();
    updated.set_summary("Updated summary".to_string());
    store.update_node(&updated).unwrap();

    let retrieved2 = store.get_node(&node.id).unwrap().unwrap();
    assert_eq!(retrieved2.summary, "Updated summary");

    // Delete
    let deleted = store.delete_node(&node.id).unwrap();
    assert!(deleted);

    let not_found = store.get_node(&node.id).unwrap();
    assert!(not_found.is_none());
}

#[test]
fn test_batch_insert_nodes() {
    let (store, _dir) = create_test_store();

    let nodes: Vec<PageNode> = (0..10)
        .map(|i| {
            PageNode::new(
                "doc_1".to_string(),
                format!("Node {}", i),
                Some(format!("Summary {}", i)),
                0,
            )
        })
        .collect();

    store.insert_nodes(&nodes).unwrap();

    let stats = store.stats().unwrap();
    assert_eq!(stats.total_nodes, 10);
}

#[test]
fn test_tree_traversal() {
    let (store, _dir) = create_test_store();

    // Create a table first
    let table = Table::new("Test Table".to_string());
    store.insert_table(&table).unwrap();

    // Create a tree: root -> child1, child2
    let doc = Document::new("Test".to_string(), &table.id);
    store.insert_document(&doc).unwrap();

    let mut root = PageNode::new_root(doc.id.clone(), "Root".to_string());
    let mut child1 = PageNode::new(doc.id.clone(), "Child 1".to_string(), None, 1);
    let mut child2 = PageNode::new(doc.id.clone(), "Child 2".to_string(), None, 1);

    child1.set_parent(root.id.clone());
    child2.set_parent(root.id.clone());
    root.add_child(child1.id.clone());
    root.add_child(child2.id.clone());

    store.insert_node(&root).unwrap();
    store.insert_node(&child1).unwrap();
    store.insert_node(&child2).unwrap();

    // Test get_children
    let children = store.get_children(&root).unwrap();
    assert_eq!(children.len(), 2);

    // Test get_parent
    let parent = store.get_parent(&child1).unwrap().unwrap();
    assert_eq!(parent.id, root.id);
}

// ==================== Table Tests ====================

#[test]
fn test_table_crud() {
    let (store, _dir) = create_test_store();

    // Create
    let mut table = Table::new("Legal Contracts".to_string());
    table.set_metadata("department", Value::String("legal".to_string()));
    store.insert_table(&table).unwrap();

    // Read
    let retrieved = store.get_table(&table.id).unwrap().unwrap();
    assert_eq!(retrieved.name, "Legal Contracts");
    assert_eq!(
        retrieved.get_metadata("department"),
        Some(&Value::String("legal".to_string()))
    );

    // Update
    let mut updated = retrieved.clone();
    updated.set_metadata("confidential", Value::Bool(true));
    store.update_table(&updated).unwrap();

    let retrieved2 = store.get_table(&table.id).unwrap().unwrap();
    assert_eq!(
        retrieved2.get_metadata("confidential"),
        Some(&Value::Bool(true))
    );

    // Delete
    let deleted = store.delete_table(&table.id, false).unwrap();
    assert!(deleted);

    let not_found = store.get_table(&table.id).unwrap();
    assert!(not_found.is_none());
}

#[test]
fn test_list_tables() {
    let (store, _dir) = create_test_store();

    store.insert_table(&Table::new("Table 1".to_string())).unwrap();
    store.insert_table(&Table::new("Table 2".to_string())).unwrap();

    let tables = store.list_tables().unwrap();
    assert_eq!(tables.len(), 2);
}

#[test]
fn test_table_name_uniqueness() {
    let (store, _dir) = create_test_store();

    // Create first table
    let table1 = Table::new("Legal Contracts".to_string());
    assert_eq!(table1.slug, "legal_contracts");
    store.insert_table(&table1).unwrap();

    // Try to create another table with same name (different case)
    let table2 = Table::new("legal contracts".to_string());
    assert_eq!(table2.slug, "legal_contracts"); // Same slug
    let result = store.insert_table(&table2);
    assert!(result.is_err()); // Should fail

    // Try to create with similar name (different chars)
    let table3 = Table::new("Legal-Contracts!".to_string());
    assert_eq!(table3.slug, "legal_contracts"); // Same slug
    let result = store.insert_table(&table3);
    assert!(result.is_err()); // Should fail

    // Create table with different name
    let table4 = Table::new("HR Documents".to_string());
    assert_eq!(table4.slug, "hr_documents");
    store.insert_table(&table4).unwrap(); // Should succeed
}

#[test]
fn test_get_table_by_name() {
    let (store, _dir) = create_test_store();

    // Create a table
    let table = Table::new("Legal Contracts".to_string());
    store.insert_table(&table).unwrap();

    // Look up by various name formats
    let found1 = store.get_table_by_name("Legal Contracts").unwrap().unwrap();
    assert_eq!(found1.id, table.id);

    let found2 = store.get_table_by_name("legal contracts").unwrap().unwrap();
    assert_eq!(found2.id, table.id);

    let found3 = store.get_table_by_name("LEGAL_CONTRACTS").unwrap().unwrap();
    assert_eq!(found3.id, table.id);

    let found4 = store.get_table_by_slug("legal_contracts").unwrap().unwrap();
    assert_eq!(found4.id, table.id);

    // Non-existent name
    let not_found = store.get_table_by_name("NonExistent").unwrap();
    assert!(not_found.is_none());
}

#[test]
fn test_table_rename_uniqueness() {
    let (store, _dir) = create_test_store();

    // Create two tables
    let table1 = Table::new("Legal Contracts".to_string());
    store.insert_table(&table1).unwrap();

    let table2 = Table::new("HR Documents".to_string());
    store.insert_table(&table2).unwrap();

    // Try to rename table2 to conflict with table1
    let mut renamed = table2.clone();
    renamed.set_name("Legal Contracts".to_string());
    let result = store.update_table(&renamed);
    assert!(result.is_err()); // Should fail

    // Rename to a unique name should work
    let mut renamed2 = table2.clone();
    renamed2.set_name("HR Files".to_string());
    store.update_table(&renamed2).unwrap();

    // Verify the rename worked
    let found = store.get_table_by_name("HR Files").unwrap().unwrap();
    assert_eq!(found.id, table2.id);

    // Old name should not work
    let not_found = store.get_table_by_name("HR Documents").unwrap();
    assert!(not_found.is_none());
}

#[test]
fn test_cannot_delete_table_with_docs() {
    let (store, _dir) = create_test_store();

    // Create table and document
    let table = Table::new("Test Table".to_string());
    store.insert_table(&table).unwrap();

    let doc = Document::new("Test Doc".to_string(), &table.id);
    store.insert_document(&doc).unwrap();

    // Should fail without cascade
    let result = store.delete_table(&table.id, false);
    assert!(result.is_err());

    // Should succeed with cascade
    let deleted = store.delete_table(&table.id, true).unwrap();
    assert!(deleted);

    // Document should be deleted too
    let doc_result = store.get_document(&doc.id).unwrap();
    assert!(doc_result.is_none());
}

// ==================== Document Tests ====================

#[test]
fn test_document_crud() {
    let (store, _dir) = create_test_store();

    // Create table first
    let table = Table::new("Test Table".to_string());
    store.insert_table(&table).unwrap();

    // Create document
    let mut doc = Document::new("Test Document".to_string(), &table.id);
    doc.add_metadata("key", "value");
    store.insert_document(&doc).unwrap();

    // Read
    let retrieved = store.get_document(&doc.id).unwrap().unwrap();
    assert_eq!(retrieved.title, "Test Document");
    assert_eq!(
        retrieved.metadata.get("key"),
        Some(&Value::String("value".to_string()))
    );

    // Delete
    let deleted = store.delete_document(&doc.id).unwrap();
    assert!(deleted);
}

#[test]
fn test_document_requires_table() {
    let (store, _dir) = create_test_store();

    // Try to insert document without creating table first
    let doc = Document::new("Test".to_string(), "nonexistent-table");
    let result = store.insert_document(&doc);

    assert!(result.is_err());
}

#[test]
fn test_document_with_nodes() {
    let (store, _dir) = create_test_store();

    let table = Table::new("Test Table".to_string());
    store.insert_table(&table).unwrap();

    let doc = Document::new("Test Doc".to_string(), &table.id);
    store.insert_document(&doc).unwrap();

    let nodes: Vec<PageNode> = (0..5)
        .map(|i| PageNode::new(doc.id.clone(), format!("Node {}", i), None, 0))
        .collect();

    store.insert_nodes(&nodes).unwrap();

    let doc_nodes = store.get_nodes_for_document(&doc.id).unwrap();
    assert_eq!(doc_nodes.len(), 5);
}

#[test]
fn test_delete_document_cascades_nodes() {
    let (store, _dir) = create_test_store();

    let table = Table::new("Test Table".to_string());
    store.insert_table(&table).unwrap();

    let doc = Document::new("Test".to_string(), &table.id);
    store.insert_document(&doc).unwrap();

    let nodes: Vec<PageNode> = (0..3)
        .map(|i| PageNode::new(doc.id.clone(), format!("Node {}", i), None, 0))
        .collect();
    store.insert_nodes(&nodes).unwrap();

    // Verify nodes exist
    let stats_before = store.stats().unwrap();
    assert_eq!(stats_before.total_nodes, 3);

    // Delete document (should cascade to nodes)
    store.delete_document(&doc.id).unwrap();

    // Verify nodes are deleted
    let stats_after = store.stats().unwrap();
    assert_eq!(stats_after.total_nodes, 0);
    assert_eq!(stats_after.total_documents, 0);
}

#[test]
fn test_list_documents() {
    let (store, _dir) = create_test_store();

    let table = Table::new("Test Table".to_string());
    store.insert_table(&table).unwrap();

    store.insert_document(&Document::new("Doc 1".to_string(), &table.id)).unwrap();
    store.insert_document(&Document::new("Doc 2".to_string(), &table.id)).unwrap();
    store.insert_document(&Document::new("Doc 3".to_string(), &table.id)).unwrap();

    let docs = store.list_documents().unwrap();
    assert_eq!(docs.len(), 3);
}

// ==================== Index/Query Tests ====================

#[test]
fn test_document_table_index() {
    let (store, _dir) = create_test_store();

    // Create tables
    let table1 = Table::new("Legal".to_string());
    let table2 = Table::new("HR".to_string());
    store.insert_table(&table1).unwrap();
    store.insert_table(&table2).unwrap();

    // Create documents in different tables
    store.insert_document(&Document::new("Doc 1".to_string(), &table1.id)).unwrap();
    store.insert_document(&Document::new("Doc 2".to_string(), &table1.id)).unwrap();
    store.insert_document(&Document::new("Doc 3".to_string(), &table2.id)).unwrap();

    // Query by table
    let legal_docs = store.get_documents_in_table(&table1.id).unwrap();
    assert_eq!(legal_docs.len(), 2);

    let hr_docs = store.get_documents_in_table(&table2.id).unwrap();
    assert_eq!(hr_docs.len(), 1);
}

#[test]
fn test_document_tag_index() {
    let (store, _dir) = create_test_store();

    let table = Table::new("Test".to_string());
    store.insert_table(&table).unwrap();

    // Create documents with tags
    let mut doc1 = Document::new("Doc 1".to_string(), &table.id);
    doc1.tags = vec!["nda".to_string(), "active".to_string()];
    store.insert_document(&doc1).unwrap();

    let mut doc2 = Document::new("Doc 2".to_string(), &table.id);
    doc2.tags = vec!["nda".to_string(), "pending".to_string()];
    store.insert_document(&doc2).unwrap();

    let mut doc3 = Document::new("Doc 3".to_string(), &table.id);
    doc3.tags = vec!["other".to_string()];
    store.insert_document(&doc3).unwrap();

    // Query by tag
    let nda_docs = store.get_documents_by_tag("nda").unwrap();
    assert_eq!(nda_docs.len(), 2);

    let active_docs = store.get_documents_by_tag("active").unwrap();
    assert_eq!(active_docs.len(), 1);
}

#[test]
fn test_find_documents_with_filter() {
    let (store, _dir) = create_test_store();

    // Create table
    let table = Table::new("Legal".to_string());
    store.insert_table(&table).unwrap();

    // Create documents
    let mut doc1 = Document::new("NDA Agreement".to_string(), &table.id);
    doc1.tags = vec!["nda".to_string(), "active".to_string()];
    doc1.set_metadata("author", serde_json::json!("Legal Team"));
    doc1.set_metadata("signed", Value::Bool(true));
    store.insert_document(&doc1).unwrap();

    let mut doc2 = Document::new("Service Agreement".to_string(), &table.id);
    doc2.tags = vec!["msa".to_string(), "active".to_string()];
    doc2.set_metadata("author", serde_json::json!("Legal Team"));
    doc2.set_metadata("signed", Value::Bool(false));
    store.insert_document(&doc2).unwrap();

    // Create another table with different doc
    let other_table = Table::new("Other".to_string());
    store.insert_table(&other_table).unwrap();

    let mut doc3 = Document::new("Other Doc".to_string(), &other_table.id);
    doc3.tags = vec!["other".to_string()];
    store.insert_document(&doc3).unwrap();

    // Filter by table
    let filter1 = SearchFilter::new().with_table_id(&table.id);
    let results1 = store.find_documents(&filter1).unwrap();
    assert_eq!(results1.len(), 2);

    // Filter by tag
    let filter2 = SearchFilter::new().with_tags(vec!["active"]);
    let results2 = store.find_documents(&filter2).unwrap();
    assert_eq!(results2.len(), 2);

    // Filter by metadata (author is now in metadata)
    let filter3 = SearchFilter::new().with_metadata("author", Value::String("Legal Team".to_string()));
    let results3 = store.find_documents(&filter3).unwrap();
    assert_eq!(results3.len(), 2);

    // Filter by metadata
    let filter4 = SearchFilter::new().with_metadata("signed", Value::Bool(true));
    let results4 = store.find_documents(&filter4).unwrap();
    assert_eq!(results4.len(), 1);
    assert_eq!(results4[0].title, "NDA Agreement");

    // Combined filter
    let filter5 = SearchFilter::new()
        .with_table_id(&table.id)
        .with_tags(vec!["active"])
        .with_metadata("signed", Value::Bool(false));
    let results5 = store.find_documents(&filter5).unwrap();
    assert_eq!(results5.len(), 1);
    assert_eq!(results5[0].title, "Service Agreement");
}

#[test]
fn test_move_document_to_table() {
    let (store, _dir) = create_test_store();

    // Create tables
    let table1 = Table::new("Table 1".to_string());
    let table2 = Table::new("Table 2".to_string());
    store.insert_table(&table1).unwrap();
    store.insert_table(&table2).unwrap();

    // Create document in table1
    let doc = Document::new("Test Doc".to_string(), &table1.id);
    store.insert_document(&doc).unwrap();

    // Verify in table1
    let t1_docs = store.get_documents_in_table(&table1.id).unwrap();
    assert_eq!(t1_docs.len(), 1);

    // Move to table2
    store.move_document_to_table(&doc.id, &table2.id).unwrap();

    // Verify moved
    let t1_docs_after = store.get_documents_in_table(&table1.id).unwrap();
    assert_eq!(t1_docs_after.len(), 0);

    let t2_docs = store.get_documents_in_table(&table2.id).unwrap();
    assert_eq!(t2_docs.len(), 1);

    // Verify document updated
    let updated_doc = store.get_document(&doc.id).unwrap().unwrap();
    assert_eq!(updated_doc.table_id, table2.id);
}
