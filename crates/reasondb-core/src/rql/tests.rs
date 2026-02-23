//! Tests for RQL

use tempfile::tempdir;

use crate::model::{Document, Table};
use crate::store::NodeStore;

use super::*;

fn create_test_store() -> (NodeStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let store = NodeStore::open(&db_path).unwrap();
    (store, dir)
}

fn setup_test_data(store: &NodeStore) {
    // Create table
    let table = Table::with_id("legal".to_string(), "Legal Contracts".to_string());
    store.insert_table(&table).unwrap();

    // Create documents
    let mut doc1 = Document::new("Contract A".to_string(), "legal");
    doc1.tags = vec!["nda".to_string(), "active".to_string()];
    doc1.set_metadata("author", serde_json::json!("Alice"));
    doc1.set_metadata("status", serde_json::json!("active"));
    doc1.set_metadata("value", serde_json::json!(50000));
    // Add nested metadata for testing deep queries
    doc1.set_metadata(
        "employee",
        serde_json::json!({
            "name": "John Smith",
            "department": "Engineering",
            "manager": {
                "name": "Jane Doe",
                "email": "jane@company.com"
            }
        }),
    );
    doc1.set_metadata(
        "parties",
        serde_json::json!([
            {"name": "Acme Corp", "role": "client"},
            {"name": "Beta Inc", "role": "vendor"}
        ]),
    );
    store.insert_document(&doc1).unwrap();

    let mut doc2 = Document::new("Contract B".to_string(), "legal");
    doc2.tags = vec!["service".to_string(), "draft".to_string()];
    doc2.set_metadata("author", serde_json::json!("Bob"));
    doc2.set_metadata("status", serde_json::json!("draft"));
    doc2.set_metadata("value", serde_json::json!(25000));
    doc2.set_metadata(
        "employee",
        serde_json::json!({
            "name": "Alice Johnson",
            "department": "Legal",
            "manager": {
                "name": "Bob Wilson",
                "email": "bob@company.com"
            }
        }),
    );
    store.insert_document(&doc2).unwrap();

    let mut doc3 = Document::new("Contract C".to_string(), "legal");
    doc3.tags = vec!["nda".to_string(), "expired".to_string()];
    doc3.set_metadata("author", serde_json::json!("Alice"));
    doc3.set_metadata("status", serde_json::json!("expired"));
    doc3.set_metadata("value", serde_json::json!(100000));
    doc3.set_metadata(
        "employee",
        serde_json::json!({
            "name": "Charlie Brown",
            "department": "Engineering",
            "manager": {
                "name": "Jane Doe",
                "email": "jane@company.com"
            }
        }),
    );
    store.insert_document(&doc3).unwrap();
}

// ==================== Query Parsing Tests ====================

#[test]
fn test_parse_simple_select() {
    let query = Query::parse("SELECT * FROM legal").unwrap();
    assert_eq!(query.from.table, "legal");
    assert!(matches!(query.select, SelectClause::All));
}

#[test]
fn test_parse_select_count() {
    let query = Query::parse("SELECT COUNT(*) FROM legal").unwrap();
    match &query.select {
        SelectClause::Aggregates(aggs) => {
            assert_eq!(aggs.len(), 1);
            assert!(matches!(aggs[0].function, AggregateFunction::Count(None)));
        }
        _ => panic!("Expected Aggregates"),
    }
}

#[test]
fn test_parse_where_string() {
    let query = Query::parse("SELECT * FROM legal WHERE metadata.author = 'Alice'").unwrap();
    assert!(query.where_clause.is_some());
}

#[test]
fn test_parse_where_number() {
    let query = Query::parse("SELECT * FROM legal WHERE metadata.value > 30000").unwrap();
    assert!(query.where_clause.is_some());
}

#[test]
fn test_parse_where_and_or() {
    let query = Query::parse(
        "SELECT * FROM legal WHERE status = 'active' AND value > 1000 OR metadata.author = 'Bob'",
    )
    .unwrap();
    assert!(query.where_clause.is_some());
}

#[test]
fn test_parse_tags_contains() {
    let query =
        Query::parse("SELECT * FROM legal WHERE tags CONTAINS ALL ('nda', 'active')").unwrap();
    assert!(query.where_clause.is_some());
}

#[test]
fn test_parse_search() {
    let query = Query::parse("SELECT * FROM legal SEARCH 'liability clause'").unwrap();
    let search = query.search.expect("Expected search clause");
    assert_eq!(search.query, "liability clause");
}

#[test]
fn test_parse_reason() {
    let query =
        Query::parse("SELECT * FROM legal REASON 'What are the penalties?' WITH CONFIDENCE > 0.7")
            .unwrap();
    let reason = query.reason.expect("Expected reason clause");
    assert_eq!(reason.query, "What are the penalties?");
    assert_eq!(reason.min_confidence, Some(0.7));
}

#[test]
fn test_parse_order_limit() {
    let query =
        Query::parse("SELECT * FROM legal ORDER BY created_at DESC LIMIT 10 OFFSET 5").unwrap();
    assert!(query.order_by.is_some());
    assert_eq!(query.limit.as_ref().unwrap().count, 10);
    assert_eq!(query.limit.as_ref().unwrap().offset, Some(5));
}

// ==================== Query Builder Tests ====================

#[test]
fn test_builder_simple() {
    let query = QueryBuilder::new().from("legal").build().unwrap();
    assert_eq!(query.from.table, "legal");
}

#[test]
fn test_builder_with_conditions() {
    let query = QueryBuilder::new()
        .from("legal")
        .where_eq("status", "active")
        .where_gt("value", 1000.0)
        .build()
        .unwrap();

    assert!(query.where_clause.is_some());
}

#[test]
fn test_builder_with_tags() {
    let query = QueryBuilder::new()
        .from("legal")
        .where_in_tags(&["nda", "active"])
        .build()
        .unwrap();

    assert!(query.where_clause.is_some());
}

#[test]
fn test_builder_with_search() {
    let query = QueryBuilder::new()
        .from("legal")
        .search("liability")
        .limit(10)
        .build()
        .unwrap();

    let search = query.search.expect("Expected search clause");
    assert_eq!(search.query, "liability");
    assert_eq!(query.limit.unwrap().count, 10);
}

#[test]
fn test_builder_with_reason() {
    let query = QueryBuilder::new()
        .from("legal")
        .reason("What are the penalties?")
        .limit(5)
        .build()
        .unwrap();

    let reason = query.reason.expect("Expected reason clause");
    assert_eq!(reason.query, "What are the penalties?");
    assert_eq!(query.limit.unwrap().count, 5);
}

#[test]
fn test_builder_hybrid() {
    let query = QueryBuilder::new()
        .from("legal")
        .search("payment")
        .reason("What are the fees?")
        .build()
        .unwrap();

    let search = query.search.expect("Expected search clause");
    assert_eq!(search.query, "payment");
    let reason = query.reason.expect("Expected reason clause");
    assert_eq!(reason.query, "What are the fees?");
}

// ==================== Query Execution Tests ====================

#[test]
fn test_execute_select_all() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let query = Query::parse("SELECT * FROM legal").unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert_eq!(result.total_count, 3);
    assert_eq!(result.documents.len(), 3);
}

#[test]
fn test_execute_where_metadata_author() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let query = Query::parse("SELECT * FROM legal WHERE metadata.author = 'Alice'").unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert_eq!(result.total_count, 2);
    for doc_match in &result.documents {
        assert_eq!(
            doc_match.document.metadata.get("author"),
            Some(&serde_json::json!("Alice"))
        );
    }
}

#[test]
fn test_execute_where_metadata() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let query = Query::parse("SELECT * FROM legal WHERE metadata.status = 'active'").unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert_eq!(result.total_count, 1);
    assert_eq!(result.documents[0].document.title, "Contract A");
}

#[test]
fn test_execute_where_numeric() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let query = Query::parse("SELECT * FROM legal WHERE metadata.value > 30000").unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert_eq!(result.total_count, 2); // Contract A (50000) and Contract C (100000)
}

#[test]
fn test_execute_where_nested_metadata() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    // Test nested object access: metadata.employee.department
    let query =
        Query::parse("SELECT * FROM legal WHERE metadata.employee.department = 'Engineering'")
            .unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert_eq!(result.total_count, 2); // Contract A and Contract C have Engineering department
}

#[test]
fn test_execute_where_deeply_nested_metadata() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    // Test deeply nested object access: metadata.employee.manager.name
    let query =
        Query::parse("SELECT * FROM legal WHERE metadata.employee.manager.name = 'Jane Doe'")
            .unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert_eq!(result.total_count, 2); // Contract A and Contract C have Jane Doe as manager
}

#[test]
fn test_execute_where_array_index_metadata() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    // Test array access: metadata.parties[0].name
    let query =
        Query::parse("SELECT * FROM legal WHERE metadata.parties[0].name = 'Acme Corp'").unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert_eq!(result.total_count, 1); // Only Contract A has parties array
    assert_eq!(result.documents[0].document.title, "Contract A");
}

#[test]
fn test_execute_where_array_second_element() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    // Test array access: metadata.parties[1].role
    let query =
        Query::parse("SELECT * FROM legal WHERE metadata.parties[1].role = 'vendor'").unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert_eq!(result.total_count, 1); // Only Contract A has parties[1]
}

#[test]
fn test_execute_tags_contains_all() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let query =
        Query::parse("SELECT * FROM legal WHERE tags CONTAINS ALL ('nda', 'active')").unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert_eq!(result.total_count, 1);
    assert_eq!(result.documents[0].document.title, "Contract A");
}

#[test]
fn test_execute_tags_contains_any() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let query =
        Query::parse("SELECT * FROM legal WHERE tags CONTAINS ANY ('draft', 'expired')").unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert_eq!(result.total_count, 2); // Contract B and Contract C
}

#[test]
fn test_execute_order_by() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let query = Query::parse("SELECT * FROM legal ORDER BY title ASC").unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert_eq!(result.documents[0].document.title, "Contract A");
    assert_eq!(result.documents[1].document.title, "Contract B");
    assert_eq!(result.documents[2].document.title, "Contract C");
}

#[test]
fn test_execute_limit_offset() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let query = Query::parse("SELECT * FROM legal ORDER BY title ASC LIMIT 2 OFFSET 1").unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert_eq!(result.total_count, 3); // Total before pagination
    assert_eq!(result.documents.len(), 2); // After pagination
    assert_eq!(result.documents[0].document.title, "Contract B");
    assert_eq!(result.documents[1].document.title, "Contract C");
}

#[test]
fn test_execute_complex_query() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let query = Query::parse(
        "SELECT * FROM legal \
         WHERE metadata.author = 'Alice' AND metadata.value > 40000 \
         ORDER BY title DESC \
         LIMIT 10",
    )
    .unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert_eq!(result.total_count, 2); // Contract A (50000) and Contract C (100000)
    assert_eq!(result.documents[0].document.title, "Contract C"); // DESC order
}

// ==================== Error Handling Tests ====================

#[test]
fn test_parse_error_missing_from() {
    let result = Query::parse("SELECT *");
    assert!(result.is_err());
}

#[test]
fn test_parse_error_invalid_operator() {
    let result = Query::parse("SELECT * FROM t WHERE x == 1");
    assert!(result.is_err());
}

#[test]
fn test_builder_error_missing_from() {
    let result = QueryBuilder::new().where_eq("x", "y").build();
    assert!(result.is_err());
}

// ==================== UPDATE Parsing Tests ====================

#[test]
fn test_parse_update_single_field() {
    let stmt = Statement::parse(
        "UPDATE legal SET metadata.status = 'archived' WHERE metadata.status = 'expired'",
    )
    .unwrap();
    match stmt {
        Statement::Update(uq) => {
            assert_eq!(uq.table.table, "legal");
            assert_eq!(uq.assignments.len(), 1);
            assert_eq!(uq.assignments[0].field.to_string(), "metadata.status");
            assert_eq!(
                uq.assignments[0].value,
                Value::String("archived".to_string())
            );
            assert!(uq.where_clause.is_some());
        }
        _ => panic!("Expected Update statement"),
    }
}

#[test]
fn test_parse_update_multiple_fields() {
    let stmt = Statement::parse(
        "UPDATE legal SET metadata.status = 'active', title = 'New Title' WHERE metadata.author = 'Alice'"
    ).unwrap();
    match stmt {
        Statement::Update(uq) => {
            assert_eq!(uq.assignments.len(), 2);
            assert_eq!(uq.assignments[0].field.to_string(), "metadata.status");
            assert_eq!(uq.assignments[1].field.to_string(), "title");
            assert_eq!(
                uq.assignments[1].value,
                Value::String("New Title".to_string())
            );
        }
        _ => panic!("Expected Update statement"),
    }
}

#[test]
fn test_parse_update_no_where() {
    let stmt = Statement::parse("UPDATE legal SET metadata.status = 'archived'").unwrap();
    match stmt {
        Statement::Update(uq) => {
            assert!(uq.where_clause.is_none());
        }
        _ => panic!("Expected Update statement"),
    }
}

#[test]
fn test_parse_update_with_tags_array() {
    let stmt = Statement::parse("UPDATE legal SET tags = ('important', 'reviewed')").unwrap();
    match stmt {
        Statement::Update(uq) => {
            assert_eq!(uq.assignments.len(), 1);
            assert_eq!(
                uq.assignments[0].value,
                Value::Array(vec![
                    Value::String("important".to_string()),
                    Value::String("reviewed".to_string()),
                ])
            );
        }
        _ => panic!("Expected Update statement"),
    }
}

// ==================== DELETE Parsing Tests ====================

#[test]
fn test_parse_delete_with_where() {
    let stmt = Statement::parse("DELETE FROM legal WHERE metadata.status = 'expired'").unwrap();
    match stmt {
        Statement::Delete(dq) => {
            assert_eq!(dq.table.table, "legal");
            assert!(dq.where_clause.is_some());
        }
        _ => panic!("Expected Delete statement"),
    }
}

#[test]
fn test_parse_delete_all() {
    let stmt = Statement::parse("DELETE FROM legal").unwrap();
    match stmt {
        Statement::Delete(dq) => {
            assert_eq!(dq.table.table, "legal");
            assert!(dq.where_clause.is_none());
        }
        _ => panic!("Expected Delete statement"),
    }
}

#[test]
fn test_parse_delete_complex_where() {
    let stmt = Statement::parse(
        "DELETE FROM legal WHERE metadata.author = 'Alice' AND metadata.value < 30000",
    )
    .unwrap();
    match stmt {
        Statement::Delete(dq) => {
            assert!(dq.where_clause.is_some());
            match dq.where_clause.unwrap().condition {
                Condition::And(_, _) => {}
                _ => panic!("Expected AND condition"),
            }
        }
        _ => panic!("Expected Delete statement"),
    }
}

// ==================== Statement::parse backward compat ====================

#[test]
fn test_statement_parse_select() {
    let stmt = Statement::parse("SELECT * FROM legal WHERE metadata.status = 'active'").unwrap();
    match stmt {
        Statement::Select(q) => {
            assert_eq!(q.from.table, "legal");
            assert!(q.where_clause.is_some());
        }
        _ => panic!("Expected Select statement"),
    }
}

// ==================== UPDATE Execution Tests ====================

#[test]
fn test_execute_update_metadata() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let stmt = Statement::parse(
        "UPDATE legal SET metadata.status = 'archived' WHERE metadata.status = 'expired'",
    )
    .unwrap();
    match stmt {
        Statement::Update(ref uq) => {
            let result = store.execute_update(uq).unwrap();
            assert_eq!(result.rows_affected, 1);
        }
        _ => panic!("Expected Update"),
    }

    // Verify the update persisted
    let query = Query::parse("SELECT * FROM legal WHERE metadata.status = 'archived'").unwrap();
    let result = store.execute_rql(&query).unwrap();
    assert_eq!(result.total_count, 1);
    assert_eq!(result.documents[0].document.title, "Contract C");
}

#[test]
fn test_execute_update_title() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let stmt = Statement::parse(
        "UPDATE legal SET title = 'Renamed Contract' WHERE metadata.author = 'Bob'",
    )
    .unwrap();
    match stmt {
        Statement::Update(ref uq) => {
            let result = store.execute_update(uq).unwrap();
            assert_eq!(result.rows_affected, 1);
        }
        _ => panic!("Expected Update"),
    }

    let query = Query::parse("SELECT * FROM legal WHERE metadata.author = 'Bob'").unwrap();
    let result = store.execute_rql(&query).unwrap();
    assert_eq!(result.documents[0].document.title, "Renamed Contract");
}

#[test]
fn test_execute_update_multiple_rows() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let stmt = Statement::parse(
        "UPDATE legal SET metadata.reviewed = true WHERE metadata.author = 'Alice'",
    )
    .unwrap();
    match stmt {
        Statement::Update(ref uq) => {
            let result = store.execute_update(uq).unwrap();
            assert_eq!(result.rows_affected, 2);
        }
        _ => panic!("Expected Update"),
    }
}

#[test]
fn test_execute_update_no_match() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let stmt =
        Statement::parse("UPDATE legal SET metadata.status = 'x' WHERE metadata.author = 'Nobody'")
            .unwrap();
    match stmt {
        Statement::Update(ref uq) => {
            let result = store.execute_update(uq).unwrap();
            assert_eq!(result.rows_affected, 0);
        }
        _ => panic!("Expected Update"),
    }
}

// ==================== DELETE Execution Tests ====================

#[test]
fn test_execute_delete_with_where() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let stmt = Statement::parse("DELETE FROM legal WHERE metadata.status = 'expired'").unwrap();
    match stmt {
        Statement::Delete(ref dq) => {
            let result = store.execute_delete(dq).unwrap();
            assert_eq!(result.rows_affected, 1);
        }
        _ => panic!("Expected Delete"),
    }

    // Verify only 2 documents remain
    let query = Query::parse("SELECT * FROM legal").unwrap();
    let result = store.execute_rql(&query).unwrap();
    assert_eq!(result.total_count, 2);
}

#[test]
fn test_execute_delete_multiple() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let stmt = Statement::parse("DELETE FROM legal WHERE metadata.author = 'Alice'").unwrap();
    match stmt {
        Statement::Delete(ref dq) => {
            let result = store.execute_delete(dq).unwrap();
            assert_eq!(result.rows_affected, 2);
        }
        _ => panic!("Expected Delete"),
    }

    let query = Query::parse("SELECT * FROM legal").unwrap();
    let result = store.execute_rql(&query).unwrap();
    assert_eq!(result.total_count, 1);
    assert_eq!(result.documents[0].document.title, "Contract B");
}

#[test]
fn test_execute_delete_all() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let stmt = Statement::parse("DELETE FROM legal").unwrap();
    match stmt {
        Statement::Delete(ref dq) => {
            let result = store.execute_delete(dq).unwrap();
            assert_eq!(result.rows_affected, 3);
        }
        _ => panic!("Expected Delete"),
    }

    let query = Query::parse("SELECT * FROM legal").unwrap();
    let result = store.execute_rql(&query).unwrap();
    assert_eq!(result.total_count, 0);
}

#[test]
fn test_execute_delete_no_match() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let stmt = Statement::parse("DELETE FROM legal WHERE metadata.author = 'Nobody'").unwrap();
    match stmt {
        Statement::Delete(ref dq) => {
            let result = store.execute_delete(dq).unwrap();
            assert_eq!(result.rows_affected, 0);
        }
        _ => panic!("Expected Delete"),
    }

    // All 3 should remain
    let query = Query::parse("SELECT * FROM legal").unwrap();
    let result = store.execute_rql(&query).unwrap();
    assert_eq!(result.total_count, 3);
}

// ==================== BM25 Search Tests ====================

#[test]
fn test_execute_search_with_bm25() {
    use crate::text_index::TextIndex;

    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    // Get the actual document IDs
    let docs = store.list_documents().unwrap();
    let doc_a = docs.iter().find(|d| d.title == "Contract A").unwrap();
    let doc_b = docs.iter().find(|d| d.title == "Contract B").unwrap();
    let doc_c = docs.iter().find(|d| d.title == "Contract C").unwrap();

    // Create text index
    let text_index = TextIndex::in_memory().unwrap();

    // Index document content with correct table_id ("legal")
    text_index
        .index_node(
            &doc_a.id,
            "node_a",
            "legal", // Must match the actual table_id
            "Contract A",
            "This agreement covers payment terms of fifty thousand dollars.",
            &["nda".to_string()],
        )
        .unwrap();
    text_index
        .index_node(
            &doc_b.id,
            "node_b",
            "legal",
            "Contract B",
            "Service agreement with monthly maintenance fee.",
            &["service".to_string()],
        )
        .unwrap();
    text_index
        .index_node(
            &doc_c.id,
            "node_c",
            "legal",
            "Contract C",
            "Employment contract with salary and payment schedule.",
            &["employment".to_string()],
        )
        .unwrap();
    text_index.commit().unwrap();

    // Search for "payment"
    let query = Query::parse("SELECT * FROM legal SEARCH 'payment'").unwrap();
    let result = store
        .execute_rql_with_search(&query, Some(&text_index))
        .unwrap();

    // Should find documents with "payment" - Contract A and Contract C
    assert!(result.stats.search_executed);
    assert_eq!(result.stats.index_used, Some("bm25_full_text".to_string()));
    assert!(
        !result.documents.is_empty(),
        "Expected at least 1 match, got {}",
        result.documents.len()
    );

    // Results should have scores
    assert!(result.documents[0].score.is_some());
}

#[test]
fn test_execute_search_no_index() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    // Search without text index - should fall back to filter
    let query = Query::parse("SELECT * FROM legal SEARCH 'payment'").unwrap();
    let result = store.execute_rql_with_search(&query, None).unwrap();

    // Should return all documents (no search filtering without index)
    assert!(!result.stats.search_executed);
    assert_eq!(result.total_count, 3);
}

// ==================== Aggregate Tests ====================

#[test]
fn test_parse_aggregates() {
    // COUNT(*)
    let query = Query::parse("SELECT COUNT(*) FROM legal").unwrap();
    match &query.select {
        SelectClause::Aggregates(aggs) => {
            assert_eq!(aggs.len(), 1);
            assert!(matches!(aggs[0].function, AggregateFunction::Count(None)));
        }
        _ => panic!("Expected Aggregates"),
    }

    // Multiple aggregates
    let query =
        Query::parse("SELECT COUNT(*), MIN(metadata.value), MAX(metadata.value) FROM legal")
            .unwrap();
    match &query.select {
        SelectClause::Aggregates(aggs) => {
            assert_eq!(aggs.len(), 3);
            assert!(matches!(aggs[0].function, AggregateFunction::Count(None)));
            assert!(matches!(aggs[1].function, AggregateFunction::Min(_)));
            assert!(matches!(aggs[2].function, AggregateFunction::Max(_)));
        }
        _ => panic!("Expected Aggregates"),
    }

    // Aggregate with alias
    let query = Query::parse("SELECT COUNT(*) AS total FROM legal").unwrap();
    match &query.select {
        SelectClause::Aggregates(aggs) => {
            assert_eq!(aggs.len(), 1);
            assert_eq!(aggs[0].alias, Some("total".to_string()));
        }
        _ => panic!("Expected Aggregates"),
    }
}

#[test]
fn test_parse_group_by() {
    let query = Query::parse("SELECT COUNT(*) FROM legal GROUP BY metadata.author").unwrap();
    assert!(query.group_by.is_some());
    let group_by = query.group_by.unwrap();
    assert_eq!(group_by.fields.len(), 1);
    assert_eq!(group_by.fields[0].first_field(), Some("metadata"));
}

#[test]
fn test_parse_explain() {
    let query = Query::parse("EXPLAIN SELECT * FROM legal").unwrap();
    assert!(query.explain);
    assert!(matches!(query.select, SelectClause::All));

    let query = Query::parse("SELECT * FROM legal").unwrap();
    assert!(!query.explain);
}

#[test]
fn test_execute_count() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let query = Query::parse("SELECT COUNT(*) FROM legal").unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert!(result.aggregates.is_some());
    let aggs = result.aggregates.unwrap();
    assert_eq!(aggs.len(), 1);
    match &aggs[0].value {
        AggregateValue::Count(count) => assert_eq!(*count, 3),
        _ => panic!("Expected Count"),
    }
}

#[test]
fn test_execute_count_with_filter() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    // Note: Author names are case-sensitive ("Alice" not "alice")
    let query = Query::parse("SELECT COUNT(*) FROM legal WHERE metadata.author = 'Alice'").unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert!(result.aggregates.is_some());
    let aggs = result.aggregates.unwrap();
    match &aggs[0].value {
        AggregateValue::Count(count) => assert_eq!(*count, 2), // Alice has 2 documents
        _ => panic!("Expected Count"),
    }
}

#[test]
fn test_execute_explain() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let query =
        Query::parse("EXPLAIN SELECT * FROM legal WHERE metadata.author = 'alice'").unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert!(result.explain.is_some());
    let plan = result.explain.unwrap();

    // Should have multiple steps
    assert!(!plan.steps.is_empty());

    // Should include TableScan
    assert!(plan.steps.iter().any(|s| s.step_type == "TableScan"));

    // Should include Filter for WHERE clause
    assert!(plan.steps.iter().any(|s| s.step_type == "Filter"));

    // Should list indexes used
    assert!(!plan.indexes_used.is_empty());
}

#[test]
fn test_execute_group_by() {
    let (store, _dir) = create_test_store();
    setup_test_data(&store);

    let query = Query::parse("SELECT COUNT(*) FROM legal GROUP BY metadata.author").unwrap();
    let result = store.execute_rql(&query).unwrap();

    assert!(result.aggregates.is_some());
    let aggs = result.aggregates.unwrap();

    // Should have groups - one for Alice (2 docs), one for Bob (1 doc)
    assert!(aggs.len() >= 2);

    // Each result should have a group_key
    for agg in &aggs {
        assert!(agg.group_key.is_some());
    }
}
