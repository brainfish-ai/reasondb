//! Tests for model types

use serde_json::Value;

use super::*;

// ==================== PageNode Tests ====================

#[test]
fn test_page_node_creation() {
    let node = PageNode::new(
        "doc_1".to_string(),
        "Test Node".to_string(),
        Some("A test summary".to_string()),
        1,
    );

    assert!(!node.id.is_empty());
    assert_eq!(node.document_id, "doc_1");
    assert_eq!(node.title, "Test Node");
    assert_eq!(node.summary, "A test summary");
    assert_eq!(node.depth, 1);
    assert!(node.is_leaf()); // No children yet
}

#[test]
fn test_page_node_leaf() {
    let node = PageNode::new_leaf(
        "doc_1".to_string(),
        "Leaf Node".to_string(),
        "This is the content".to_string(),
        "Summary of content".to_string(),
        2,
    );

    assert!(node.is_leaf());
    assert_eq!(node.content, Some("This is the content".to_string()));
}

#[test]
fn test_page_node_hierarchy() {
    let mut parent = PageNode::new_root("doc_1".to_string(), "Root".to_string());
    let mut child = PageNode::new("doc_1".to_string(), "Child".to_string(), None, 1);

    child.set_parent(parent.id.clone());
    parent.add_child(child.id.clone());

    assert!(parent.is_root());
    assert!(!child.is_root());
    assert_eq!(parent.children_ids.len(), 1);
    assert_eq!(child.parent_id, Some(parent.id.clone()));
}

#[test]
fn test_llm_context() {
    let node = PageNode::new(
        "doc_1".to_string(),
        "Introduction".to_string(),
        Some("This chapter covers basics".to_string()),
        0,
    );

    let context = node.to_llm_context();
    assert!(context.contains("Introduction"));
    assert!(context.contains("This chapter covers basics"));
}

#[test]
fn test_node_serialization() {
    let node = PageNode::new(
        "doc_1".to_string(),
        "Test".to_string(),
        Some("Summary".to_string()),
        0,
    );

    // Test bincode serialization
    let encoded = bincode::serialize(&node).unwrap();
    let decoded: PageNode = bincode::deserialize(&encoded).unwrap();

    assert_eq!(node, decoded);
}

// ==================== Document Tests ====================

#[test]
fn test_document_creation() {
    let doc = Document::new("Test Document".to_string(), "test-table");

    assert!(!doc.id.is_empty());
    assert_eq!(doc.title, "Test Document");
    assert_eq!(doc.total_nodes, 0);
    assert_eq!(doc.table_id, "test-table");
    assert!(doc.tags.is_empty());
}

#[test]
fn test_document_in_table() {
    let doc = Document::in_table("Contract".to_string(), "legal-contracts");

    assert_eq!(doc.table_id, "legal-contracts");
    assert_eq!(doc.get_table_id(), "legal-contracts");
}

#[test]
fn test_document_tags() {
    let mut doc = Document::new("Test".to_string(), "test-table");

    doc.add_tag("nda");
    doc.add_tag("confidential");
    doc.add_tag("nda"); // Duplicate - should not add

    assert_eq!(doc.tags.len(), 2);
    assert!(doc.has_tag("nda"));
    assert!(doc.has_tag("confidential"));
    assert!(!doc.has_tag("other"));

    doc.remove_tag("nda");
    assert!(!doc.has_tag("nda"));
    assert_eq!(doc.tags.len(), 1);
}

#[test]
fn test_document_metadata() {
    let mut doc = Document::new("Test".to_string(), "test-table");

    doc.set_metadata("contract_type", Value::String("nda".to_string()));
    doc.set_metadata("value_usd", Value::Number(50000.into()));
    doc.set_metadata("signed", Value::Bool(true));

    assert_eq!(
        doc.get_metadata("contract_type"),
        Some(&Value::String("nda".to_string()))
    );
    assert_eq!(
        doc.get_metadata("value_usd"),
        Some(&Value::Number(50000.into()))
    );
    assert_eq!(doc.get_metadata("signed"), Some(&Value::Bool(true)));
    assert_eq!(doc.get_metadata("nonexistent"), None);
}

#[test]
fn test_document_serialization() {
    let mut doc = Document::new("Test".to_string(), "legal");
    doc.tags = vec!["nda".to_string()];
    doc.set_metadata("value", Value::Number(1000.into()));

    // Test bincode serialization
    let encoded = bincode::serialize(&doc).unwrap();
    let decoded: Document = bincode::deserialize(&encoded).unwrap();

    assert_eq!(doc, decoded);
}

// ==================== Table Tests ====================

#[test]
fn test_table_creation() {
    let mut table = Table::new("Legal Contracts".to_string());

    assert!(table.id.starts_with("tbl_"));
    assert_eq!(table.name, "Legal Contracts");
    assert_eq!(table.document_count, 0);

    table.set_metadata("department", Value::String("legal".to_string()));
    table.increment_documents(5);

    assert_eq!(table.document_count, 1);
    assert_eq!(table.total_nodes, 5);
    assert_eq!(
        table.get_metadata("department"),
        Some(&Value::String("legal".to_string()))
    );
}

#[test]
fn test_table_with_id() {
    let table = Table::with_id("custom-id".to_string(), "Custom Table".to_string());

    assert_eq!(table.id, "custom-id");
    assert_eq!(table.name, "Custom Table");
}

#[test]
fn test_table_serialization() {
    let mut table = Table::new("Test Table".to_string());
    table.set_metadata("department", Value::String("engineering".to_string()));

    // Test bincode serialization
    let encoded = bincode::serialize(&table).unwrap();
    let decoded: Table = bincode::deserialize(&encoded).unwrap();

    assert_eq!(table, decoded);
}

// ==================== SearchFilter Tests ====================

#[test]
fn test_search_filter() {
    let filter = SearchFilter::new()
        .with_table_id("legal")
        .with_tags(vec!["nda", "active"])
        .with_metadata("author", Value::String("Legal Team".to_string()))
        .with_metadata("signed", Value::Bool(true))
        .with_limit(10);

    assert_eq!(filter.table_id, Some("legal".to_string()));
    assert_eq!(
        filter.tags,
        Some(vec!["nda".to_string(), "active".to_string()])
    );
    assert_eq!(filter.limit, Some(10));
}

#[test]
fn test_search_filter_matches() {
    let mut doc = Document::new("Test".to_string(), "test-table");
    doc.tags = vec!["nda".to_string(), "active".to_string()];
    doc.set_metadata("author", serde_json::json!("Legal Team"));
    doc.set_metadata("signed", Value::Bool(true));

    // Should match
    let filter1 = SearchFilter::new().with_tags(vec!["nda"]);
    assert!(filter1.matches_tags(&doc));

    // Should match (all tags)
    let filter2 = SearchFilter::new().with_tags_all(vec!["nda", "active"]);
    assert!(filter2.matches_tags(&doc));

    // Should not match (missing tag)
    let filter3 = SearchFilter::new().with_tags_all(vec!["nda", "pending"]);
    assert!(!filter3.matches_tags(&doc));

    // Should match metadata (author is now in metadata)
    let filter4 =
        SearchFilter::new().with_metadata("author", Value::String("Legal Team".to_string()));
    assert!(filter4.matches_metadata(&doc));

    // Should match metadata
    let filter5 = SearchFilter::new().with_metadata("signed", Value::Bool(true));
    assert!(filter5.matches_metadata(&doc));

    // Should not match metadata
    let filter6 = SearchFilter::new().with_metadata("signed", Value::Bool(false));
    assert!(!filter6.matches_metadata(&doc));
}
