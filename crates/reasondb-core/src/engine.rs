//! Search Engine for ReasonDB
//!
//! This module implements the core search algorithm using LLM-guided
//! tree traversal with beam search.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{debug, info, instrument, warn};

use crate::error::Result;
use crate::llm::{BatchVerifyInput, NodeSummary, ReasoningEngine};
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
            beam_width: 4,
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
    /// Sibling sections that this node explicitly references inline
    /// (resolved from cross_ref_node_ids stored during ingestion).
    pub cross_ref_sections: Vec<CrossRefSection>,
}

/// A sibling section that was referenced inline by a search result node.
#[derive(Debug, Clone)]
pub struct CrossRefSection {
    /// Node ID of the referenced section
    pub node_id: String,
    /// Title of the referenced section
    pub title: String,
    /// Full content of the referenced section
    pub content: String,
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

// ==================== Keyword helpers ====================
//
// Delegate to the shared `query_filter` module which uses the `stop-words`
// crate (400+ English stopwords) and `rust_stemmers` for English stemming.
// This ensures the same term normalisation pipeline is used everywhere:
// tree-grep, BM25 candidate selection, and beam-search pre-filtering.

use crate::query_filter::{extract_query_terms, has_any_term};

// ==================== Search Engine ====================

/// The core search engine that performs LLM-guided tree traversal
pub struct SearchEngine<R: ReasoningEngine + 'static> {
    store: Arc<NodeStore>,
    reasoner: Arc<R>,
    config: SearchConfig,
}

impl<R: ReasoningEngine + 'static> Clone for SearchEngine<R> {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
            reasoner: Arc::clone(&self.reasoner),
            config: self.config.clone(),
        }
    }
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

    /// Fast-path: verify pre-selected BM25 node hits directly without tree traversal.
    ///
    /// `bm25_hits` is a list of `(node_id, bm25_score)` pairs already sorted
    /// descending by BM25 score from Phase 1.  We load the actual nodes, then
    /// run `batch_verify_nodes` on them.  This avoids the expensive
    /// `decide_next_step` call for flat documents where BM25 already identified
    /// the relevant leaf nodes.
    pub async fn verify_bm25_hits(
        &self,
        query: &str,
        document_id: &str,
        bm25_hits: Vec<(String, f32)>,
        cancel: Arc<AtomicBool>,
    ) -> Result<SearchResponse> {
        if cancel.load(Ordering::Relaxed) || bm25_hits.is_empty() {
            return Ok(SearchResponse {
                results: Vec::new(),
                stats: TraversalStats::default(),
            });
        }

        // Load the actual PageNodes for each BM25 hit.
        let mut nodes: Vec<PageNode> = Vec::with_capacity(bm25_hits.len());
        for (node_id, _score) in &bm25_hits {
            if let Ok(Some(node)) = self.store.get_node(node_id) {
                // Only include nodes belonging to this document.
                if node.document_id == document_id {
                    nodes.push(node);
                }
            }
        }

        if nodes.is_empty() {
            // BM25 hits don't map to loadable nodes — fall back to tree traversal.
            return self
                .search_document_with_cancel(query, document_id, cancel)
                .await;
        }

        let results = Arc::new(Mutex::new(Vec::new()));
        let stats = Arc::new(Mutex::new(TraversalStats {
            nodes_visited: nodes.len(),
            ..Default::default()
        }));

        self.batch_verify_nodes(
            query,
            &nodes,
            vec![],
            vec![],
            results.clone(),
            stats.clone(),
            cancel,
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

        info!(
            "BM25 fast-path: {} hits → {} relevant for doc {}",
            bm25_hits.len(),
            final_results.len(),
            document_id
        );

        Ok(SearchResponse {
            results: final_results,
            stats: final_stats,
        })
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
            let all_candidates: Vec<NodeSummary> = children.iter().map(NodeSummary::from).collect();

            let context = if self.config.include_path {
                current_path.join(" > ")
            } else {
                node.summary.clone()
            };

            // Widen the beam for flat/broad nodes so we explore more branches when
            // there are many children (e.g. XBRL financials with 100+ leaf nodes).
            let effective_beam = if children.len() > 50 {
                self.config.beam_width * 4
            } else if children.len() > 20 {
                self.config.beam_width * 2
            } else {
                self.config.beam_width
            };

            // Keyword pre-filter: remove candidates whose summaries share no
            // meaningful token with the query.  This shrinks the LLM prompt
            // (sometimes eliminating the call entirely) and avoids wasting
            // tokens on clearly irrelevant nodes.
            //
            // Uses the shared pipeline: tokenise → stop-words removal (400+
            // English words via the `stop-words` crate) → English stemming
            // (`rust_stemmers`).  "disability" → "disabl" matches "disabled",
            // "disabling", etc.
            let query_terms = extract_query_terms(query);
            let candidates: Vec<NodeSummary> = {
                let matched: Vec<_> = all_candidates
                    .iter()
                    .filter(|c| has_any_term(&c.summary, &query_terms))
                    .cloned()
                    .collect();
                // Fall back to all candidates when the filter is too aggressive
                // so we always have at least `effective_beam` candidates.
                if matched.len() >= effective_beam.min(all_candidates.len()) {
                    matched
                } else {
                    all_candidates.clone()
                }
            };

            // If every remaining candidate is a leaf node we can skip
            // decide_next_step entirely and verify them directly in one batch
            // call. batch_verify_nodes internally caps at 30 highest-signal
            // candidates, so prompt size stays bounded regardless of how many
            // candidates there are.
            //
            // Guard: leaf children live at depth+1, so only verify them when
            // that level is still within the configured max depth.
            if depth < self.config.max_depth
                && candidates.iter().all(|c| {
                    children
                        .iter()
                        .find(|n| n.id == c.id)
                        .map(|n| n.is_leaf())
                        .unwrap_or(false)
                })
            {
                debug!(
                    "All {} candidate(s) are leaves within beam — batch-verifying in one call",
                    candidates.len()
                );
                let leaf_nodes: Vec<PageNode> = candidates
                    .iter()
                    .filter_map(|c| children.iter().find(|n| n.id == c.id).cloned())
                    .collect();
                self.batch_verify_nodes(
                    query,
                    &leaf_nodes,
                    current_path,
                    reasoning_trace,
                    results,
                    stats,
                    cancel,
                )
                .await?;
                return Ok(());
            }

            // Cap candidates passed to decide_next_step to keep the prompt
            // small. Sort by keyword-match count (highest first) so the most
            // relevant nodes appear regardless of document order.  Non-leaf
            // nodes (tree branches) are given a bonus to ensure they are
            // included for deeper traversal.
            const MAX_DECIDE_CANDIDATES: usize = 30;
            let capped_candidates: Vec<NodeSummary> = if candidates.len() > MAX_DECIDE_CANDIDATES {
                let mut scored: Vec<(NodeSummary, usize)> = candidates
                    .into_iter()
                    .map(|c| {
                        let kw_score = if query_terms.is_empty() {
                            0
                        } else {
                            query_terms
                                .iter()
                                .filter(|t| c.summary.contains(t.as_str()))
                                .count()
                        };
                        // Give non-leaf nodes a +100 bonus so they're always
                        // preferred over leaf nodes in the decide_next_step list.
                        let is_leaf = children
                            .iter()
                            .find(|n| n.id == c.id)
                            .map(|n| n.is_leaf())
                            .unwrap_or(true);
                        let total_score = kw_score + if is_leaf { 0 } else { 100 };
                        (c, total_score)
                    })
                    .collect();
                scored.sort_by(|a, b| b.1.cmp(&a.1));
                scored
                    .into_iter()
                    .take(MAX_DECIDE_CANDIDATES)
                    .map(|(c, _)| c)
                    .collect()
            } else {
                candidates
            };

            // Increment LLM call count
            {
                let mut s = stats.lock().await;
                s.llm_calls += 1;
            }

            let decisions = self
                .reasoner
                .decide_next_step(query, &context, &capped_candidates, effective_beam)
                .await?;

            // Filter by confidence threshold
            let selected: Vec<_> = decisions
                .into_iter()
                .filter(|d| d.confidence >= self.config.min_confidence)
                .take(effective_beam)
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

            // If every selected node is a leaf, batch-verify them all in ONE
            // LLM call instead of recursing into verify_leaf individually.
            // This is the primary call-count reduction: N leaf verifications
            // collapse from N calls → 1 call per `decide_next_step` result.
            //
            // Guard: same depth constraint as above — leaf children are at
            // depth+1, so skip when that would exceed max_depth.
            let all_selected_are_leaves = traces.iter().all(|(node, _)| node.is_leaf());

            if depth < self.config.max_depth && all_selected_are_leaves && !traces.is_empty() {
                debug!(
                    "All {} selected node(s) are leaves — batch-verifying in one call",
                    traces.len()
                );
                let leaf_nodes: Vec<PageNode> = traces.iter().map(|(n, _)| n.clone()).collect();
                // Use the reasoning trace from the first entry; all leaves share
                // the same parent path so the traces are essentially the same.
                let rep_trace = traces
                    .into_iter()
                    .next()
                    .map(|(_, t)| t)
                    .unwrap_or_default();
                self.batch_verify_nodes(
                    query,
                    &leaf_nodes,
                    current_path,
                    rep_trace,
                    results,
                    stats,
                    cancel.clone(),
                )
                .await?;
                return Ok(());
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

        let raw_content = node.get_content();

        // Keyword pre-filter: skip the LLM call entirely when neither the
        // node's summary nor the first 500 chars of its content share any
        // stemmed keyword with the query.  This is ~1µs and saves a full
        // LLM round-trip for clearly irrelevant leaf nodes.
        let query_terms = extract_query_terms(query);
        if !query_terms.is_empty() {
            let content_head: String = raw_content.chars().take(500).collect();
            if !has_any_term(&node.summary, &query_terms)
                && !has_any_term(&content_head, &query_terms)
            {
                debug!(
                    "Leaf '{}' has no keyword overlap — skipping verify_answer",
                    node.title
                );
                return Ok(());
            }
        }

        // Prepend a truncated version of the node's human-readable summary so
        // the LLM can judge relevance even when the raw content is opaque (e.g.
        // XBRL, JSON).  Capped at 300 chars to keep the prompt short.
        let content_for_verification;
        let content = if !node.summary.is_empty() && node.summary != raw_content {
            let summary_prefix: String = node.summary.chars().take(300).collect();
            content_for_verification =
                format!("[Section summary: {}]\n\n{}", summary_prefix, raw_content);
            content_for_verification.as_str()
        } else {
            raw_content
        };

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
            let cross_ref_sections = self.resolve_cross_refs(node);
            let result = SearchResult {
                node_id: node.id.clone(),
                title: node.title.clone(),
                // Return the raw content (not the summary-prefixed verification text)
                content: raw_content.to_string(),
                confidence: verification.confidence,
                path,
                reasoning_trace,
                cross_ref_sections,
            };

            results.lock().await.push(result);
        }

        Ok(())
    }

    /// Verify a **batch** of leaf nodes with a single LLM call.
    ///
    /// Pre-filters each candidate with the keyword check, builds
    /// `BatchVerifyInput` entries, issues one `batch_verify_answers` call,
    /// then pushes relevant results into `results`.
    #[allow(clippy::too_many_arguments)]
    async fn batch_verify_nodes(
        &self,
        query: &str,
        nodes: &[PageNode],
        path: Vec<String>,
        reasoning_trace: Vec<ReasoningStep>,
        results: Arc<Mutex<Vec<SearchResult>>>,
        stats: Arc<Mutex<TraversalStats>>,
        cancel: Arc<AtomicBool>,
    ) -> Result<()> {
        if nodes.is_empty() || cancel.load(Ordering::Relaxed) {
            return Ok(());
        }

        // Nodes arrive pre-ranked by BM25 score (from verify_bm25_hits) or by
        // tree position (from the decide_next_step fallback path). We cap at
        // MAX_BATCH_INPUTS to bound prompt size; callers are responsible for
        // passing the most relevant nodes first.
        const MAX_BATCH_INPUTS: usize = 25;
        let top: Vec<&PageNode> = nodes.iter().take(MAX_BATCH_INPUTS).collect();

        // Build BatchVerifyInput using summary-only content (200 chars max).
        // Full content is still returned in SearchResult; only scoring uses the
        // condensed version to keep prompt tokens low.
        let inputs: Vec<BatchVerifyInput> = top
            .iter()
            .map(|n| {
                let raw = n.get_content();
                let content: String = if !n.summary.is_empty() {
                    n.summary.chars().take(200).collect()
                } else {
                    raw.chars().take(180).collect()
                };
                BatchVerifyInput {
                    node_id: n.id.clone(),
                    content,
                }
            })
            .collect();

        // Single LLM call for the whole batch (sparse-output format).
        {
            let mut s = stats.lock().await;
            s.llm_calls += 1;
        }

        let verifications = self.reasoner.batch_verify_answers(query, &inputs).await?;

        // Align results with the top-N node list.
        let min_conf = self.config.min_confidence;
        for (node, verification) in top.iter().zip(verifications.iter()) {
            debug!(
                "Batch-verify '{}': relevant={} conf={}",
                node.title, verification.is_relevant, verification.confidence
            );
            if verification.is_relevant && verification.confidence >= min_conf {
                let raw = node.get_content();
                let cross_ref_sections = self.resolve_cross_refs(node);
                let result = SearchResult {
                    node_id: node.id.clone(),
                    title: node.title.clone(),
                    content: raw.to_string(),
                    confidence: verification.confidence,
                    path: path.clone(),
                    reasoning_trace: reasoning_trace.clone(),
                    cross_ref_sections,
                };
                results.lock().await.push(result);
            }
        }

        Ok(())
    }

    /// Resolve `node.metadata.cross_ref_node_ids` to `CrossRefSection` values
    /// by fetching each referenced node from the store.
    ///
    /// IDs that no longer exist in the store are silently skipped so that
    /// stale references after a document update don't cause errors.
    fn resolve_cross_refs(&self, node: &PageNode) -> Vec<CrossRefSection> {
        node.metadata
            .cross_ref_node_ids
            .iter()
            .filter_map(|id| {
                self.store
                    .get_node(id)
                    .ok()
                    .flatten()
                    .map(|ref_node| CrossRefSection {
                        node_id: ref_node.id.clone(),
                        title: ref_node.title.clone(),
                        content: ref_node.get_content().to_string(),
                    })
            })
            .collect()
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
