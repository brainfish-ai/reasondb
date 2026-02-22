//! LLM-powered semantic search execution
//!
//! This module handles REASON clause execution using an agentic search pattern:
//! 1. BM25 pre-filter (if SEARCH clause) or table filter → get candidates
//! 2. Recursive tree-grep pre-filter → structural re-ranking (zero LLM calls)
//! 3. LLM scans document summaries → ranks top N most relevant
//! 4. LLM deep reasoning → only on top N documents (parallel execution)

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::engine::{SearchConfig, SearchEngine};
use crate::error::Result;
use crate::llm::{DocumentSummary, ReasoningEngine};
use crate::model::Document;
use crate::query_filter::extract_query_terms;
use crate::store::NodeStore;
use crate::text_index::TextIndex;
use crate::tree_grep;
use crate::rql::ast::Query;

use super::types::{
    DocumentMatch, MatchedNode, QueryResult, QueryStats,
    ReasonPhase, ReasonPhaseStatus, ReasonProgress,
};

/// Configuration constants
const MAX_CANDIDATES: usize = 100;
const SAFE_TABLE_SIZE: usize = 1000;
const MAX_CONCURRENT: usize = 5;

// ==================== Candidate Document ====================

/// A BM25 node-level hit preserved through the pipeline.
#[derive(Debug, Clone)]
pub struct NodeHit {
    pub node_id: String,
    pub title: String,
    pub score: f32,
    pub snippet: Option<String>,
}

/// A candidate document with BM25 node-level hit info and tree-grep signals.
#[derive(Debug, Clone)]
pub struct CandidateDocument {
    pub document: Document,
    pub bm25_score: f32,
    pub matched_nodes: Vec<NodeHit>,
    pub matched_sections: Vec<String>,
    pub best_snippet: Option<String>,
}

// ==================== Progress Helper ====================

/// Send a progress event, ignoring channel errors (receiver may have dropped).
async fn send_progress(tx: &Option<mpsc::Sender<ReasonProgress>>, event: ReasonProgress) {
    if let Some(tx) = tx {
        let _ = tx.send(event).await;
    }
}

// ==================== Public API ====================

/// Execute a REASON (semantic search) query using the LLM.
pub async fn execute_reason_query<R: ReasoningEngine + Send + Sync + 'static>(
    store: &Arc<NodeStore>,
    query: &Query,
    reason_query: &str,
    min_confidence: Option<f32>,
    text_index: Option<&TextIndex>,
    reasoner: Arc<R>,
) -> Result<QueryResult> {
    execute_reason_query_with_progress(
        store,
        query,
        reason_query,
        min_confidence,
        text_index,
        reasoner,
        None,
    )
    .await
}

/// Execute a REASON query with optional progress reporting via an mpsc channel.
///
/// When `progress_tx` is `Some`, progress events are emitted at each phase boundary
/// so callers (e.g. SSE endpoints) can stream them to clients.
pub async fn execute_reason_query_with_progress<R: ReasoningEngine + Send + Sync + 'static>(
    store: &Arc<NodeStore>,
    query: &Query,
    reason_query: &str,
    min_confidence: Option<f32>,
    text_index: Option<&TextIndex>,
    reasoner: Arc<R>,
    progress_tx: Option<mpsc::Sender<ReasonProgress>>,
) -> Result<QueryResult> {
    let start = std::time::Instant::now();

    // Resolve table name to ID
    let table_id = store.resolve_table_id(&query.from.table)?;

    // LIMIT controls how many results to return; reason over more to find the best ones
    let result_limit = query.limit.as_ref().map(|l| l.count).unwrap_or(10);
    let target_docs = (result_limit * 2).max(6).min(20);

    let config = SearchConfig {
        min_confidence: min_confidence.unwrap_or(0.3),
        max_results: result_limit,
        ..Default::default()
    };

    // Create search engine for deep reasoning
    let engine = SearchEngine::with_config(store.clone(), reasoner.clone(), config);

    // PHASE 1: BM25 Candidate Selection (preserving node-level hits)
    send_progress(&progress_tx, ReasonProgress {
        phase: ReasonPhase::Candidates,
        status: ReasonPhaseStatus::Started,
        message: "Searching for candidates...".to_string(),
        detail: None,
    }).await;

    let mut candidates = get_candidates(store, query, reason_query, text_index, &table_id)?;
    tracing::info!(
        candidate_count = candidates.len(),
        reason_query = %reason_query,
        "REASON Phase 1 (BM25): candidates retrieved"
    );

    send_progress(&progress_tx, ReasonProgress {
        phase: ReasonPhase::Candidates,
        status: ReasonPhaseStatus::Completed,
        message: format!("Found {} candidates", candidates.len()),
        detail: Some(serde_json::json!({ "count": candidates.len() })),
    }).await;

    // PHASE 2: Structural Filtering via recursive tree-grep (zero LLM calls)
    send_progress(&progress_tx, ReasonProgress {
        phase: ReasonPhase::Filtering,
        status: ReasonPhaseStatus::Started,
        message: "Analyzing document structure...".to_string(),
        detail: None,
    }).await;

    let terms = extract_query_terms(reason_query);
    if !terms.is_empty() {
        candidates = apply_tree_grep_filter(store, candidates, &terms, target_docs);
    }

    tracing::info!(
        filtered_count = candidates.len(),
        terms = ?terms,
        "REASON Phase 2 (tree-grep): structural filter applied"
    );

    send_progress(&progress_tx, ReasonProgress {
        phase: ReasonPhase::Filtering,
        status: ReasonPhaseStatus::Completed,
        message: format!("Structural analysis complete ({} terms)", terms.len()),
        detail: Some(serde_json::json!({ "terms": terms, "count": candidates.len() })),
    }).await;

    // PHASE 3: LLM Summary Ranking
    send_progress(&progress_tx, ReasonProgress {
        phase: ReasonPhase::Ranking,
        status: ReasonPhaseStatus::Started,
        message: "Ranking documents by relevance...".to_string(),
        detail: None,
    }).await;

    let documents = rank_documents_by_summary(
        store,
        candidates,
        reason_query,
        target_docs,
        &reasoner,
    ).await;
    tracing::info!(
        ranked_count = documents.len(),
        "REASON Phase 3 (LLM ranking): documents ranked"
    );

    send_progress(&progress_tx, ReasonProgress {
        phase: ReasonPhase::Ranking,
        status: ReasonPhaseStatus::Completed,
        message: format!("Selected top {} documents", documents.len()),
        detail: Some(serde_json::json!({ "count": documents.len() })),
    }).await;

    // PHASE 4: Deep LLM reasoning (parallel)
    send_progress(&progress_tx, ReasonProgress {
        phase: ReasonPhase::Reasoning,
        status: ReasonPhaseStatus::Started,
        message: format!("Deep reasoning on {} documents...", documents.len()),
        detail: Some(serde_json::json!({ "total": documents.len() })),
    }).await;

    let (all_matches, total_llm_calls, docs_processed) = execute_parallel_reasoning(
        &engine,
        documents,
        reason_query,
        min_confidence,
        query,
        &progress_tx,
    ).await;

    send_progress(&progress_tx, ReasonProgress {
        phase: ReasonPhase::Reasoning,
        status: ReasonPhaseStatus::Completed,
        message: "Reasoning complete".to_string(),
        detail: Some(serde_json::json!({ "matches": all_matches.len() })),
    }).await;

    // Sort by confidence
    let mut sorted_matches = all_matches;
    sorted_matches.sort_by(|a, b| {
        b.confidence
            .unwrap_or(0.0)
            .partial_cmp(&a.confidence.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply pagination
    let total_count = sorted_matches.len();
    let paginated = apply_pagination(sorted_matches, query);

    // Build stats
    let stats = QueryStats {
        index_used: if query.search.is_some() {
            Some("hybrid_bm25_treegrep_llm".to_string())
        } else {
            Some("treegrep_llm_semantic".to_string())
        },
        rows_scanned: docs_processed,
        rows_returned: paginated.len(),
        search_executed: query.search.is_some(),
        reason_executed: true,
        llm_calls: total_llm_calls,
    };

    Ok(QueryResult {
        documents: paginated,
        total_count,
        execution_time_ms: start.elapsed().as_millis() as u64,
        stats,
        aggregates: None,
        explain: None,
    })
}

// ==================== Phase 1: Get Candidates ====================

/// Get candidate documents using BM25 or table filter, preserving node-level hit info.
fn get_candidates(
    store: &NodeStore,
    query: &Query,
    reason_query: &str,
    text_index: Option<&TextIndex>,
    table_id: &str,
) -> Result<Vec<CandidateDocument>> {
    if let (Some(ref search_clause), Some(index)) = (&query.search, text_index) {
        return search_with_bm25(store, index, &search_clause.query, table_id);
    }

    if let Some(index) = text_index {
        let results = search_with_bm25(store, index, reason_query, table_id)?;
        if !results.is_empty() {
            return Ok(results);
        }
    }

    get_candidates_by_filter(store, query, table_id)
}

/// Search using BM25 index, preserving per-node hit scores.
fn search_with_bm25(
    store: &NodeStore,
    index: &TextIndex,
    query: &str,
    table_id: &str,
) -> Result<Vec<CandidateDocument>> {
    let results = index.search(query, MAX_CANDIDATES, Some(table_id))?;

    let mut doc_hits: HashMap<String, (f32, Vec<NodeHit>)> = HashMap::new();

    for hit in results {
        let entry = doc_hits
            .entry(hit.document_id.clone())
            .or_insert_with(|| (0.0_f32, Vec::new()));

        entry.0 = entry.0.max(hit.score);
        entry.1.push(NodeHit {
            node_id: hit.node_id,
            title: hit.title,
            score: hit.score,
            snippet: hit.snippet,
        });
    }

    let mut candidates: Vec<CandidateDocument> = doc_hits
        .into_iter()
        .filter_map(|(doc_id, (best_score, nodes))| {
            store.get_document(&doc_id).ok().flatten().map(|doc| {
                CandidateDocument {
                    document: doc,
                    bm25_score: best_score,
                    matched_nodes: nodes,
                    matched_sections: Vec::new(),
                    best_snippet: None,
                }
            })
        })
        .collect();

    candidates.sort_by(|a, b| {
        b.bm25_score
            .partial_cmp(&a.bm25_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(MAX_CANDIDATES);

    Ok(candidates)
}

/// Get candidates using filter (for tables without text index).
fn get_candidates_by_filter(
    store: &NodeStore,
    query: &Query,
    table_id: &str,
) -> Result<Vec<CandidateDocument>> {
    let mut filter = query.to_search_filter();
    filter.table_id = Some(table_id.to_string());
    filter.limit = Some(MAX_CANDIDATES.min(SAFE_TABLE_SIZE));
    let docs = store.find_documents(&filter)?;

    Ok(docs
        .into_iter()
        .map(|doc| CandidateDocument {
            document: doc,
            bm25_score: 0.0,
            matched_nodes: Vec::new(),
            matched_sections: Vec::new(),
            best_snippet: None,
        })
        .collect())
}

// ==================== Phase 2: Structural Filtering ====================

/// Phase 2: Apply recursive tree-grep to reorder and truncate candidates by structural match quality.
/// Drops candidates with zero combined score and caps to `target_docs * 3` to keep Phase 3 lean.
fn apply_tree_grep_filter(
    store: &NodeStore,
    candidates: Vec<CandidateDocument>,
    terms: &[String],
    target_docs: usize,
) -> Vec<CandidateDocument> {
    let initial_count = candidates.len();

    let mut scored: Vec<(CandidateDocument, f32)> = candidates
        .into_iter()
        .map(|mut c| {
            let grep_result = tree_grep::tree_grep(store, &c.document.id, terms)
                .unwrap_or_default();

            c.matched_sections = grep_result
                .matched_nodes
                .iter()
                .filter(|h| h.title_match)
                .map(|h| h.title.clone())
                .collect();

            c.best_snippet = c
                .matched_nodes
                .iter()
                .max_by(|a, b| {
                    a.score
                        .partial_cmp(&b.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .and_then(|hit| hit.snippet.clone());

            let bm25_weight = 0.4;
            let grep_weight = 0.6;
            let combined = c.bm25_score * bm25_weight
                + grep_result.structural_score * grep_weight;

            (c, combined)
        })
        .collect();

    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Drop candidates with zero combined score (no BM25 or structural match)
    scored.retain(|(_, score)| *score > 0.0);

    if scored.is_empty() && initial_count > 0 {
        tracing::debug!("Tree-grep filtered all candidates; skipping truncation");
    }

    // Keep at most target_docs * 3 candidates for the Phase 3 LLM prompt.
    let cap = (target_docs * 3).max(10).min(MAX_CANDIDATES);
    scored.truncate(cap);

    scored.into_iter().map(|(c, _)| c).collect()
}

// ==================== Phase 3: Rank by Summary ====================

/// Phase 3: Rank documents by their summaries using LLM, enriched with Phase 2 tree-grep signals.
async fn rank_documents_by_summary<R: ReasoningEngine>(
    store: &NodeStore,
    candidates: Vec<CandidateDocument>,
    reason_query: &str,
    target_docs: usize,
    reasoner: &Arc<R>,
) -> Vec<Document> {
    // Skip LLM ranking when the candidate count is close enough to the target —
    // spending an LLM call to drop a handful of docs isn't worth the latency.
    let skip_threshold = target_docs + (target_docs / 2).max(2);
    if candidates.len() <= skip_threshold {
        return candidates
            .into_iter()
            .take(target_docs)
            .map(|c| c.document)
            .collect();
    }

    let doc_summaries: Vec<DocumentSummary> = candidates
        .iter()
        .filter_map(|c| {
            let root = store.get_root_node(&c.document.id).ok()??;
            Some(DocumentSummary {
                id: c.document.id.clone(),
                title: c.document.title.clone(),
                summary: root.summary.clone(),
                tags: c.document.tags.clone(),
                matched_sections: c.matched_sections.clone(),
                best_snippet: c.best_snippet.clone(),
            })
        })
        .collect();

    if doc_summaries.is_empty() {
        return candidates
            .into_iter()
            .take(target_docs)
            .map(|c| c.document)
            .collect();
    }

    let rankings = reasoner
        .rank_documents(reason_query, &doc_summaries, target_docs)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("LLM ranking failed, using fallback ordering: {}", e);
            doc_summaries
                .iter()
                .take(target_docs)
                .map(|d| crate::llm::DocumentRanking {
                    document_id: d.id.clone(),
                    relevance: 0.5,
                    reasoning: "Fallback".to_string(),
                })
                .collect()
        });

    let ranked_ids: std::collections::HashSet<_> =
        rankings.iter().map(|r| r.document_id.as_str()).collect();
    candidates
        .into_iter()
        .filter(|c| ranked_ids.contains(c.document.id.as_str()))
        .map(|c| c.document)
        .collect()
}

// ==================== Phase 4: Parallel Reasoning ====================

/// Phase 4: Execute deep reasoning on documents in parallel.
async fn execute_parallel_reasoning<R: ReasoningEngine + Send + Sync + 'static>(
    engine: &SearchEngine<R>,
    documents: Vec<Document>,
    reason_query: &str,
    min_confidence: Option<f32>,
    query: &Query,
    progress_tx: &Option<mpsc::Sender<ReasonProgress>>,
) -> (Vec<DocumentMatch>, usize, usize) {
    use std::sync::atomic::{AtomicBool, Ordering};

    let total_docs = documents.len();
    let mut all_matches: Vec<DocumentMatch> = Vec::new();
    let mut total_llm_calls = 1;
    let mut docs_completed: usize = 0;
    let target_results = query.limit.as_ref().map(|l| l.count).unwrap_or(10);

    let cancel = Arc::new(AtomicBool::new(false));

    for chunk in documents.chunks(MAX_CONCURRENT) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let futures: Vec<_> = chunk
            .iter()
            .map(|doc| {
                let doc = doc.clone();
                let query = reason_query.to_string();
                let cancel = cancel.clone();
                async move {
                    let result = engine
                        .search_document_with_cancel(&query, &doc.id, cancel)
                        .await;
                    (doc, result)
                }
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        for (doc, search_result) in results {
            docs_completed += 1;

            send_progress(progress_tx, ReasonProgress {
                phase: ReasonPhase::Reasoning,
                status: ReasonPhaseStatus::Progress,
                message: format!(
                    "Analyzing document {}/{}: '{}'",
                    docs_completed, total_docs, doc.title
                ),
                detail: Some(serde_json::json!({
                    "current": docs_completed,
                    "total": total_docs,
                    "doc_title": doc.title,
                })),
            }).await;

            match search_result {
                Ok(response) => {
                    total_llm_calls += response.stats.llm_calls;

                    let mut nodes_for_doc: Vec<MatchedNode> = Vec::new();
                    let mut best_confidence: f32 = 0.0;

                    for result in response.results {
                        if let Some(min_conf) = min_confidence {
                            if result.confidence < min_conf {
                                continue;
                            }
                        }

                        best_confidence = best_confidence.max(result.confidence);
                        nodes_for_doc.push(MatchedNode {
                            node_id: result.node_id,
                            title: result.title,
                            content: result.content,
                            path: result.path,
                            confidence: result.confidence,
                            reasoning_trace: result.reasoning_trace,
                        });
                    }

                    if !nodes_for_doc.is_empty() {
                        all_matches.push(DocumentMatch {
                            document: doc.clone(),
                            score: Some(best_confidence),
                            matched_nodes: nodes_for_doc,
                            highlights: vec![],
                            confidence: Some(best_confidence),
                        });

                        if all_matches.len() >= target_results {
                            cancel.store(true, Ordering::Relaxed);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        doc_id = %doc.id,
                        doc_title = %doc.title,
                        "LLM search failed for document: {}",
                        e
                    );
                }
            }
        }

        if should_terminate_early(&all_matches, min_confidence, query) {
            break;
        }
    }

    let docs_processed = total_docs.min(all_matches.len() + MAX_CONCURRENT);
    (all_matches, total_llm_calls, docs_processed)
}

/// Check if we should terminate early (enough high-confidence results).
fn should_terminate_early(
    matches: &[DocumentMatch],
    min_confidence: Option<f32>,
    query: &Query,
) -> bool {
    let target_results = query.limit.as_ref().map(|l| l.count).unwrap_or(10);
    let high_confidence_count = matches
        .iter()
        .filter(|m| m.confidence.unwrap_or(0.0) >= min_confidence.unwrap_or(0.3))
        .count();
    high_confidence_count >= target_results * 2
}

/// Apply pagination to results.
fn apply_pagination(matches: Vec<DocumentMatch>, query: &Query) -> Vec<DocumentMatch> {
    if let Some(ref limit) = query.limit {
        let offset = limit.offset.unwrap_or(0);
        matches.into_iter().skip(offset).take(limit.count).collect()
    } else {
        matches
    }
}
