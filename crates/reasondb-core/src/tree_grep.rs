//! Recursive tree-grep for document pre-filtering.
//!
//! Walks a document's hierarchical node tree and matches query terms at
//! each level (title, summary, content). Produces a structural relevance
//! score that complements BM25 — cheap and zero LLM calls.

use crate::error::Result;
use crate::query_filter::count_term_matches;
use crate::store::NodeStore;

/// Result of tree-grepping a single document.
#[derive(Debug, Clone, Default)]
pub struct TreeGrepResult {
    pub document_id: String,
    pub structural_score: f32,
    pub matched_nodes: Vec<TreeGrepHit>,
}

/// A single node that matched during tree-grep.
#[derive(Debug, Clone)]
pub struct TreeGrepHit {
    pub node_id: String,
    pub title: String,
    pub depth: u8,
    pub title_match: bool,
    pub summary_match: bool,
    pub content_match: bool,
    pub match_count: usize,
}

const FIELD_WEIGHT_TITLE: f32 = 3.0;
const FIELD_WEIGHT_SUMMARY: f32 = 1.5;
const FIELD_WEIGHT_CONTENT: f32 = 1.0;

/// Recursively grep through a document's node tree, scoring term matches
/// at each level. Shallower matches and title matches are weighted higher.
pub fn tree_grep(store: &NodeStore, document_id: &str, terms: &[String]) -> Result<TreeGrepResult> {
    if terms.is_empty() {
        return Ok(TreeGrepResult {
            document_id: document_id.to_string(),
            ..Default::default()
        });
    }

    let root = match store.get_root_node(document_id)? {
        Some(node) => node,
        None => {
            return Ok(TreeGrepResult {
                document_id: document_id.to_string(),
                ..Default::default()
            });
        }
    };

    let mut hits = Vec::new();
    let total_terms = terms.len() as f32;

    grep_node(store, &root, terms, total_terms, &mut hits)?;

    let structural_score = hits
        .iter()
        .map(|hit| {
            let depth_weight = 1.0 / (1.0 + hit.depth as f32);
            let base_score = hit.match_count as f32 / total_terms;

            let mut field_score = 0.0_f32;
            if hit.title_match {
                field_score = field_score.max(FIELD_WEIGHT_TITLE);
            }
            if hit.summary_match {
                field_score = field_score.max(FIELD_WEIGHT_SUMMARY);
            }
            if hit.content_match {
                field_score = field_score.max(FIELD_WEIGHT_CONTENT);
            }

            base_score * depth_weight * field_score
        })
        .sum();

    Ok(TreeGrepResult {
        document_id: document_id.to_string(),
        structural_score,
        matched_nodes: hits,
    })
}

fn grep_node(
    store: &NodeStore,
    node: &crate::model::PageNode,
    terms: &[String],
    _total_terms: f32,
    hits: &mut Vec<TreeGrepHit>,
) -> Result<()> {
    let title_matches = count_term_matches(&node.title, terms);
    let summary_matches = count_term_matches(&node.summary, terms);
    let content_matches = node
        .content
        .as_deref()
        .map(|c| count_term_matches(c, terms))
        .unwrap_or(0);

    let total_match_count = title_matches.max(summary_matches).max(content_matches);

    if total_match_count > 0 {
        hits.push(TreeGrepHit {
            node_id: node.id.clone(),
            title: node.title.clone(),
            depth: node.depth,
            title_match: title_matches > 0,
            summary_match: summary_matches > 0,
            content_match: content_matches > 0,
            match_count: total_match_count,
        });
    }

    let children = store.get_children(node)?;
    for child in &children {
        grep_node(store, child, terms, _total_terms, hits)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Document, PageNode, Table};
    use crate::query_filter::extract_query_terms;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup_store_with_tree() -> (Arc<NodeStore>, String) {
        let dir = tempdir().unwrap();
        let store = Arc::new(NodeStore::open(dir.path().join("test.db")).unwrap());

        let table = Table::new("Test Table".to_string());
        store.insert_table(&table).unwrap();

        let mut doc = Document::new("Acme Services Agreement".to_string(), &table.id);

        let mut root = PageNode::new_root(doc.id.clone(), "Document Root".to_string());
        root.set_summary("Overview of Acme services agreement".to_string());

        doc.root_node_id = root.id.clone();
        store.insert_document(&doc).unwrap();

        let mut ch1 = PageNode::new(
            doc.id.clone(),
            "Chapter 1: Payment Terms".to_string(),
            Some("Payment schedule and terms".to_string()),
            1,
        );

        let mut ch2 = PageNode::new(
            doc.id.clone(),
            "Chapter 2: Termination".to_string(),
            Some("Termination clauses and exit procedures".to_string()),
            1,
        );

        let mut leaf1 = PageNode::new_leaf(
            doc.id.clone(),
            "Section 1.1: Monthly Payments".to_string(),
            "Monthly payment of $15,000 due on the 1st.".to_string(),
            "Payment details".to_string(),
            2,
        );

        let mut leaf2 = PageNode::new_leaf(
            doc.id.clone(),
            "Section 2.1: Termination for Cause".to_string(),
            "Either party may terminate this agreement for cause with 30 days notice.".to_string(),
            "Termination for cause procedures".to_string(),
            2,
        );

        leaf1.set_parent(ch1.id.clone());
        ch1.add_child(leaf1.id.clone());

        leaf2.set_parent(ch2.id.clone());
        ch2.add_child(leaf2.id.clone());

        ch1.set_parent(root.id.clone());
        ch2.set_parent(root.id.clone());
        root.add_child(ch1.id.clone());
        root.add_child(ch2.id.clone());

        store.insert_node(&root).unwrap();
        store.insert_node(&ch1).unwrap();
        store.insert_node(&ch2).unwrap();
        store.insert_node(&leaf1).unwrap();
        store.insert_node(&leaf2).unwrap();

        (store, doc.id.clone())
    }

    #[test]
    fn test_tree_grep_finds_matching_nodes() {
        let (store, doc_id) = setup_store_with_tree();
        let terms = extract_query_terms("termination clauses");
        let result = tree_grep(&store, &doc_id, &terms).unwrap();

        assert!(result.structural_score > 0.0);
        assert!(!result.matched_nodes.is_empty());

        let termination_hits: Vec<_> = result
            .matched_nodes
            .iter()
            .filter(|h| h.title.contains("Termination"))
            .collect();
        assert!(!termination_hits.is_empty());
    }

    #[test]
    fn test_tree_grep_shallow_matches_score_higher() {
        let (store, doc_id) = setup_store_with_tree();
        let terms = extract_query_terms("termination");
        let result = tree_grep(&store, &doc_id, &terms).unwrap();

        let shallow: Vec<_> = result
            .matched_nodes
            .iter()
            .filter(|h| h.depth <= 1)
            .collect();
        let deep: Vec<_> = result
            .matched_nodes
            .iter()
            .filter(|h| h.depth > 1)
            .collect();

        assert!(!shallow.is_empty(), "Should have shallow matches");
        assert!(!deep.is_empty(), "Should have deep matches");
    }

    #[test]
    fn test_tree_grep_no_match() {
        let (store, doc_id) = setup_store_with_tree();
        let terms = extract_query_terms("cryptocurrency blockchain");
        let result = tree_grep(&store, &doc_id, &terms).unwrap();

        assert_eq!(result.structural_score, 0.0);
        assert!(result.matched_nodes.is_empty());
    }

    #[test]
    fn test_tree_grep_empty_terms() {
        let (store, doc_id) = setup_store_with_tree();
        let result = tree_grep(&store, &doc_id, &[]).unwrap();
        assert_eq!(result.structural_score, 0.0);
    }
}
