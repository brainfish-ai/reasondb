//! Search Engine for ReasonDB
//!
//! This module implements the core search algorithm using LLM-guided
//! tree traversal with beam search.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{debug, info, instrument, warn};

use crate::error::Result;
use crate::llm::{NodeSummary, ReasoningEngine};
use crate::model::PageNode;
use crate::store::NodeStore;

/// Configuration for the search engine
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// Maximum branches to explore at each level (beam width)
    pub beam_width: usize,
    /// Maximum tree depth to traverse
    pub max_depth: u8,
    /// Maximum leaf nodes to return as results
    pub max_results: usize,
    /// Minimum confidence threshold for branch selection
    pub min_confidence: f32,
    /// Enable parallel branch exploration
    pub parallel_branches: bool,
    /// Include intermediate summaries in results
    pub include_path: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            beam_width: 2,
            max_depth: 5,
            max_results: 5,
            min_confidence: 0.3,
            parallel_branches: true,
            include_path: true,
        }
    }
}

/// A search result from the reasoning engine
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// ID of the leaf node containing the result
    pub node_id: String,
    /// Title of the leaf node
    pub title: String,
    /// The actual content of the result
    pub content: String,
    /// Confidence score for this result
    pub confidence: f32,
    /// Path from root to this node (titles)
    pub path: Vec<String>,
    /// The reasoning trace showing decisions made
    pub reasoning_trace: Vec<ReasoningStep>,
}

/// A step in the reasoning trace
#[derive(Debug, Clone)]
pub struct ReasoningStep {
    /// Node title at this step
    pub node_title: String,
    /// Decision made (which child was chosen)
    pub decision: String,
    /// Confidence at this step
    pub confidence: f32,
}

/// Statistics about the search traversal
#[derive(Debug, Clone, Default)]
pub struct TraversalStats {
    /// Total nodes visited during search
    pub nodes_visited: usize,
    /// Nodes pruned (not explored)
    pub nodes_pruned: usize,
    /// Maximum depth reached
    pub depth_reached: u8,
    /// Number of LLM calls made
    pub llm_calls: usize,
}

/// The core search engine that performs LLM-guided tree traversal
pub struct SearchEngine<R: ReasoningEngine + 'static> {
    store: Arc<NodeStore>,
    reasoner: Arc<R>,
    config: SearchConfig,
}

impl<R: ReasoningEngine + 'static> SearchEngine<R> {
    /// Create a new search engine
    pub fn new(store: Arc<NodeStore>, reasoner: Arc<R>) -> Self {
        Self {
            store,
            reasoner,
            config: SearchConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(store: Arc<NodeStore>, reasoner: Arc<R>, config: SearchConfig) -> Self {
        Self {
            store,
            reasoner,
            config,
        }
    }

    /// Execute a search query starting from a root node
    #[instrument(skip(self), fields(query = %query, root_id = %root_id))]
    pub async fn search(&self, query: &str, root_id: &str) -> Result<SearchResponse> {
        info!("Starting search");

        let results = Arc::new(Mutex::new(Vec::new()));
        let stats = Arc::new(Mutex::new(TraversalStats::default()));

        // Get the root node
        let root = match self.store.get_node(root_id)? {
            Some(node) => node,
            None => {
                warn!("Root node not found: {}", root_id);
                return Ok(SearchResponse {
                    results: Vec::new(),
                    stats: TraversalStats::default(),
                });
            }
        };

        // Start traversal
        let cancel = Arc::new(AtomicBool::new(false));
        self.traverse(
            query,
            &root,
            Vec::new(),
            Vec::new(),
            results.clone(),
            stats.clone(),
            0,
            cancel,
        )
        .await?;

        let mut final_results = results.lock().await.clone();
        let final_stats = stats.lock().await.clone();

        // Sort by confidence and truncate
        final_results.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        final_results.truncate(self.config.max_results);

        info!(
            "Search complete: {} results, {} nodes visited",
            final_results.len(),
            final_stats.nodes_visited
        );

        Ok(SearchResponse {
            results: final_results,
            stats: final_stats,
        })
    }

    /// Search starting from a document's root node
    pub async fn search_document(&self, query: &str, document_id: &str) -> Result<SearchResponse> {
        let cancel = Arc::new(AtomicBool::new(false));
        self.search_document_with_cancel(query, document_id, cancel)
            .await
    }

    /// Search a document with a shared cancellation flag.
    /// When `cancel` is set to true, the traversal stops early.
    pub async fn search_document_with_cancel(
        &self,
        query: &str,
        document_id: &str,
        cancel: Arc<AtomicBool>,
    ) -> Result<SearchResponse> {
        if cancel.load(Ordering::Relaxed) {
            return Ok(SearchResponse {
                results: Vec::new(),
                stats: TraversalStats::default(),
            });
        }

        let root = self.store.get_root_node(document_id)?;

        match root {
            Some(node) => {
                let results = Arc::new(Mutex::new(Vec::new()));
                let stats = Arc::new(Mutex::new(TraversalStats::default()));

                self.traverse(
                    query,
                    &node,
                    Vec::new(),
                    Vec::new(),
                    results.clone(),
                    stats.clone(),
                    0,
                    cancel.clone(),
                )
                .await?;

                let mut final_results = results.lock().await.clone();
                let final_stats = stats.lock().await.clone();

                final_results.sort_by(|a, b| {
                    b.confidence
                        .partial_cmp(&a.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                final_results.truncate(self.config.max_results);

                Ok(SearchResponse {
                    results: final_results,
                    stats: final_stats,
                })
            }
            None => Ok(SearchResponse {
                results: Vec::new(),
                stats: TraversalStats::default(),
            }),
        }
    }

    /// Recursive traversal with beam search
    #[allow(clippy::too_many_arguments)]
    fn traverse<'a>(
        &'a self,
        query: &'a str,
        node: &'a PageNode,
        path: Vec<String>,
        reasoning_trace: Vec<ReasoningStep>,
        results: Arc<Mutex<Vec<SearchResult>>>,
        stats: Arc<Mutex<TraversalStats>>,
        depth: u8,
        cancel: Arc<AtomicBool>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            // Check cancellation
            if cancel.load(Ordering::Relaxed) {
                return Ok(());
            }
            // Update stats
            {
                let mut s = stats.lock().await;
                s.nodes_visited += 1;
                s.depth_reached = s.depth_reached.max(depth);
            }

            debug!(
                "Traversing node: {} (depth: {}, is_leaf: {})",
                node.title,
                depth,
                node.is_leaf()
            );

            // Check depth limit
            if depth > self.config.max_depth {
                debug!("Max depth reached, stopping traversal");
                return Ok(());
            }

            // Update path
            let mut current_path = path;
            current_path.push(node.title.clone());

            // Base case: leaf node
            if node.is_leaf() {
                return self
                    .verify_leaf(
                        query,
                        node,
                        current_path,
                        reasoning_trace,
                        results,
                        stats,
                        cancel,
                    )
                    .await;
            }

            // Get children
            let children = self.store.get_children(node)?;
            if children.is_empty() {
                debug!("No children found for node: {}", node.title);
                return Ok(());
            }

            // Ask LLM which branches to explore
            let candidates: Vec<NodeSummary> = children.iter().map(NodeSummary::from).collect();

            let context = if self.config.include_path {
                current_path.join(" > ")
            } else {
                node.summary.clone()
            };

            // Increment LLM call count
            {
                let mut s = stats.lock().await;
                s.llm_calls += 1;
            }

            let decisions = self
                .reasoner
                .decide_next_step(query, &context, &candidates)
                .await?;

            // Filter by confidence threshold
            let selected: Vec<_> = decisions
                .into_iter()
                .filter(|d| d.confidence >= self.config.min_confidence)
                .take(self.config.beam_width)
                .collect();

            // Track pruned nodes
            {
                let mut s = stats.lock().await;
                s.nodes_pruned += children.len() - selected.len();
            }

            if selected.is_empty() {
                debug!("No branches selected, stopping at: {}", node.title);
                return Ok(());
            }

            // Build reasoning trace
            let mut traces: Vec<(PageNode, Vec<ReasoningStep>)> = Vec::new();
            for decision in &selected {
                if let Some(child) = children.iter().find(|c| c.id == decision.node_id) {
                    let mut trace = reasoning_trace.clone();
                    trace.push(ReasoningStep {
                        node_title: child.title.clone(),
                        decision: decision.reasoning.clone(),
                        confidence: decision.confidence,
                    });
                    traces.push((child.clone(), trace));
                }
            }

            // Early exit: skip traversal if we already have enough results
            {
                let current_results = results.lock().await;
                if current_results.len() >= self.config.max_results {
                    debug!(
                        "Already have {} results, skipping further traversal",
                        current_results.len()
                    );
                    return Ok(());
                }
            }

            if self.config.parallel_branches && traces.len() > 1 {
                let children: Vec<PageNode> = traces.iter().map(|(c, _)| c.clone()).collect();
                let trace_list: Vec<Vec<ReasoningStep>> =
                    traces.into_iter().map(|(_, t)| t).collect();

                let futures: Vec<_> = children
                    .iter()
                    .zip(trace_list)
                    .map(|(child, trace)| {
                        self.traverse(
                            query,
                            child,
                            current_path.clone(),
                            trace,
                            results.clone(),
                            stats.clone(),
                            depth + 1,
                            cancel.clone(),
                        )
                    })
                    .collect();

                let outcomes = futures::future::join_all(futures).await;
                for outcome in outcomes {
                    outcome?;
                }
            } else {
                for (child, trace) in traces {
                    self.traverse(
                        query,
                        &child,
                        current_path.clone(),
                        trace,
                        results.clone(),
                        stats.clone(),
                        depth + 1,
                        cancel.clone(),
                    )
                    .await?;
                }
            }

            Ok(())
        }) // End Box::pin(async move)
    }

    /// Verify a leaf node and potentially add it to results
    #[allow(clippy::too_many_arguments)]
    async fn verify_leaf(
        &self,
        query: &str,
        node: &PageNode,
        path: Vec<String>,
        reasoning_trace: Vec<ReasoningStep>,
        results: Arc<Mutex<Vec<SearchResult>>>,
        stats: Arc<Mutex<TraversalStats>>,
        cancel: Arc<AtomicBool>,
    ) -> Result<()> {
        // Skip if cancelled or we already have enough results
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        {
            let current_results = results.lock().await;
            if current_results.len() >= self.config.max_results {
                return Ok(());
            }
        }

        let content = node.get_content();

        // Increment LLM call count
        {
            let mut s = stats.lock().await;
            s.llm_calls += 1;
        }

        let verification = self.reasoner.verify_answer(query, content).await?;

        debug!(
            "Leaf verification for '{}': relevant={}, confidence={}",
            node.title, verification.is_relevant, verification.confidence
        );

        if verification.is_relevant && verification.confidence >= self.config.min_confidence {
            let result = SearchResult {
                node_id: node.id.clone(),
                title: node.title.clone(),
                content: content.to_string(),
                confidence: verification.confidence,
                path,
                reasoning_trace,
            };

            results.lock().await.push(result);
        }

        Ok(())
    }
}

/// Response from a search operation
#[derive(Debug)]
pub struct SearchResponse {
    /// The search results (relevant leaf nodes)
    pub results: Vec<SearchResult>,
    /// Statistics about the traversal
    pub stats: TraversalStats,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::MockReasoner;
    use crate::model::{Document, Table};
    use tempfile::tempdir;

    async fn setup_test_tree() -> (Arc<NodeStore>, String) {
        let dir = tempdir().unwrap();
        let store = Arc::new(NodeStore::open(dir.path().join("test.db")).unwrap());

        // Create a table first (required for documents)
        let table = Table::new("Test Table".to_string());
        store.insert_table(&table).unwrap();

        // Create a document in the table
        let doc = Document::new("Test Document".to_string(), &table.id);
        store.insert_document(&doc).unwrap();

        // Create a tree structure:
        // Root
        // ├── Chapter 1 (Finance)
        // │   ├── Section 1.1 (Revenue) - LEAF
        // │   └── Section 1.2 (Costs) - LEAF
        // └── Chapter 2 (Technology)
        //     └── Section 2.1 (Cloud) - LEAF

        let mut root = PageNode::new_root(doc.id.clone(), "Document Root".to_string());
        root.set_summary("Overview of the entire document".to_string());

        let mut ch1 = PageNode::new(
            doc.id.clone(),
            "Chapter 1: Finance".to_string(),
            Some("Financial information including revenue and costs".to_string()),
            1,
        );

        let mut ch2 = PageNode::new(
            doc.id.clone(),
            "Chapter 2: Technology".to_string(),
            Some("Technology and infrastructure details".to_string()),
            1,
        );

        let mut s11 = PageNode::new_leaf(
            doc.id.clone(),
            "Section 1.1: Revenue".to_string(),
            "Q3 2024 revenue was $4.2 billion, up 15% YoY.".to_string(),
            "Revenue figures and growth metrics".to_string(),
            2,
        );

        let mut s12 = PageNode::new_leaf(
            doc.id.clone(),
            "Section 1.2: Costs".to_string(),
            "Operating costs decreased by 8% due to efficiency improvements.".to_string(),
            "Cost structure and efficiency data".to_string(),
            2,
        );

        let mut s21 = PageNode::new_leaf(
            doc.id.clone(),
            "Section 2.1: Cloud Services".to_string(),
            "Cloud revenue grew 25% with 1 million new subscribers.".to_string(),
            "Cloud infrastructure and services".to_string(),
            2,
        );

        // Build relationships
        s11.set_parent(ch1.id.clone());
        s12.set_parent(ch1.id.clone());
        ch1.add_child(s11.id.clone());
        ch1.add_child(s12.id.clone());

        s21.set_parent(ch2.id.clone());
        ch2.add_child(s21.id.clone());

        ch1.set_parent(root.id.clone());
        ch2.set_parent(root.id.clone());
        root.add_child(ch1.id.clone());
        root.add_child(ch2.id.clone());

        let root_id = root.id.clone();

        // Store all nodes
        store.insert_node(&root).unwrap();
        store.insert_node(&ch1).unwrap();
        store.insert_node(&ch2).unwrap();
        store.insert_node(&s11).unwrap();
        store.insert_node(&s12).unwrap();
        store.insert_node(&s21).unwrap();

        (store, root_id)
    }

    #[tokio::test]
    async fn test_basic_search() {
        let (store, root_id) = setup_test_tree().await;

        let reasoner = Arc::new(
            MockReasoner::new()
                .with_keywords(vec!["finance".to_string(), "revenue".to_string()])
                .with_always_relevant(true),
        );

        let engine = SearchEngine::new(store, reasoner);

        let response = engine
            .search("What is the revenue?", &root_id)
            .await
            .unwrap();

        assert!(!response.results.is_empty());
        assert!(response.stats.nodes_visited > 0);

        // Should find the revenue section
        let found_revenue = response
            .results
            .iter()
            .any(|r| r.content.contains("$4.2 billion"));
        assert!(found_revenue, "Should find revenue information");
    }

    #[tokio::test]
    async fn test_search_with_path() {
        let (store, root_id) = setup_test_tree().await;

        let reasoner = Arc::new(MockReasoner::new().with_select_count(1));

        let config = SearchConfig {
            include_path: true,
            ..Default::default()
        };

        let engine = SearchEngine::with_config(store, reasoner, config);

        let response = engine.search("query", &root_id).await.unwrap();

        if !response.results.is_empty() {
            let result = &response.results[0];
            assert!(!result.path.is_empty(), "Path should be populated");
            assert!(
                result.path[0].contains("Document Root"),
                "Should start at root"
            );
        }
    }

    #[tokio::test]
    async fn test_reasoning_trace() {
        let (store, root_id) = setup_test_tree().await;

        let reasoner = Arc::new(MockReasoner::new().with_select_count(1));

        let engine = SearchEngine::new(store, reasoner);

        let response = engine.search("query", &root_id).await.unwrap();

        if !response.results.is_empty() {
            let result = &response.results[0];
            assert!(
                !result.reasoning_trace.is_empty(),
                "Should have reasoning trace"
            );
        }
    }

    #[tokio::test]
    async fn test_stats_tracking() {
        let (store, root_id) = setup_test_tree().await;

        let reasoner = Arc::new(MockReasoner::new());

        let engine = SearchEngine::new(store, reasoner);

        let response = engine.search("query", &root_id).await.unwrap();

        assert!(response.stats.nodes_visited > 0, "Should visit nodes");
        assert!(response.stats.llm_calls > 0, "Should make LLM calls");
    }

    #[tokio::test]
    async fn test_confidence_filtering() {
        let (store, root_id) = setup_test_tree().await;

        // Low confidence mock
        let reasoner = Arc::new(MockReasoner::new().with_confidence(0.1));

        let config = SearchConfig {
            min_confidence: 0.5, // Higher than mock's 0.1
            ..Default::default()
        };

        let engine = SearchEngine::with_config(store, reasoner, config);

        let response = engine.search("query", &root_id).await.unwrap();

        // Should have no results due to low confidence
        assert!(
            response.results.is_empty(),
            "Low confidence should be filtered"
        );
    }

    #[tokio::test]
    async fn test_max_depth() {
        let (store, root_id) = setup_test_tree().await;

        let reasoner = Arc::new(MockReasoner::new());

        let config = SearchConfig {
            max_depth: 1, // Only go one level deep
            ..Default::default()
        };

        let engine = SearchEngine::with_config(store, reasoner, config);

        let response = engine.search("query", &root_id).await.unwrap();

        // With max_depth=1, we won't reach leaf nodes (depth 2)
        assert!(
            response.results.is_empty(),
            "Should not reach leaves with max_depth=1"
        );
    }

    #[tokio::test]
    async fn test_parallel_vs_sequential() {
        let (store, root_id) = setup_test_tree().await;

        let reasoner = Arc::new(MockReasoner::new());

        // Test sequential
        let seq_config = SearchConfig {
            parallel_branches: false,
            ..Default::default()
        };
        let seq_engine = SearchEngine::with_config(store.clone(), reasoner.clone(), seq_config);
        let seq_response = seq_engine.search("query", &root_id).await.unwrap();

        // Test parallel
        let par_config = SearchConfig {
            parallel_branches: true,
            ..Default::default()
        };
        let par_engine = SearchEngine::with_config(store.clone(), reasoner.clone(), par_config);
        let par_response = par_engine.search("query", &root_id).await.unwrap();

        // Both should visit similar number of nodes
        assert_eq!(
            seq_response.stats.nodes_visited, par_response.stats.nodes_visited,
            "Sequential and parallel should visit same nodes"
        );
    }
}
