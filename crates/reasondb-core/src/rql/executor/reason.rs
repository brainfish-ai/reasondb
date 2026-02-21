//! LLM-powered semantic search execution
//!
//! This module handles REASON clause execution using an agentic search pattern:
//! 1. BM25 pre-filter (if SEARCH clause) or table filter → get candidates
//! 2. LLM scans document summaries → ranks top N most relevant
//! 3. LLM deep reasoning → only on top N documents (parallel execution)

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::engine::{SearchConfig, SearchEngine};
use crate::error::Result;
use crate::llm::{DocumentSummary, ReasoningEngine};
use crate::model::Document;
use crate::store::NodeStore;
use crate::text_index::TextIndex;
use crate::rql::ast::Query;

use super::types::{
    DocumentMatch, QueryResult, QueryStats,
    ReasonPhase, ReasonPhaseStatus, ReasonProgress,
};

/// Configuration constants
const MAX_CANDIDATES: usize = 100;
const SAFE_TABLE_SIZE: usize = 1000;
const MAX_CONCURRENT: usize = 5;

/// Send a progress event, ignoring channel errors (receiver may have dropped).
async fn send_progress(tx: &Option<mpsc::Sender<ReasonProgress>>, event: ReasonProgress) {
    if let Some(tx) = tx {
        let _ = tx.send(event).await;
    }
}

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

    // Target number of documents to reason on
    let target_docs = query.limit.as_ref().map(|l| l.count).unwrap_or(10);

    // Build search config
    let config = SearchConfig {
        min_confidence: min_confidence.unwrap_or(0.3),
        max_results: target_docs,
        ..Default::default()
    };

    // Create search engine for deep reasoning
    let engine = SearchEngine::with_config(store.clone(), reasoner.clone(), config);

    // PHASE 1: Get candidate documents
    send_progress(&progress_tx, ReasonProgress {
        phase: ReasonPhase::Candidates,
        status: ReasonPhaseStatus::Started,
        message: "Searching for candidates...".to_string(),
        detail: None,
    }).await;

    let candidates = get_candidates(store, query, reason_query, text_index, &table_id)?;
    tracing::info!(
        candidate_count = candidates.len(),
        reason_query = %reason_query,
        "REASON Phase 1: candidates retrieved"
    );

    send_progress(&progress_tx, ReasonProgress {
        phase: ReasonPhase::Candidates,
        status: ReasonPhaseStatus::Completed,
        message: format!("Found {} candidates", candidates.len()),
        detail: Some(serde_json::json!({ "count": candidates.len() })),
    }).await;

    // PHASE 2: Agentic summary scan (rank by relevance)
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
        "REASON Phase 2: documents ranked"
    );

    send_progress(&progress_tx, ReasonProgress {
        phase: ReasonPhase::Ranking,
        status: ReasonPhaseStatus::Completed,
        message: format!("Selected top {} documents", documents.len()),
        detail: Some(serde_json::json!({ "count": documents.len() })),
    }).await;

    // PHASE 3: Deep LLM reasoning (parallel)
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
            Some("hybrid_bm25_llm".to_string())
        } else {
            Some("llm_semantic".to_string())
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

/// Get candidate documents using BM25 or table filter.
fn get_candidates(
    store: &NodeStore,
    query: &Query,
    reason_query: &str,
    text_index: Option<&TextIndex>,
    table_id: &str,
) -> Result<Vec<Document>> {
    // Try BM25 search first (handles millions of documents efficiently)
    if let (Some(ref search_clause), Some(index)) = (&query.search, text_index) {
        return search_with_bm25(store, index, &search_clause.query, table_id);
    }

    // No explicit SEARCH clause - try using reason_query for BM25
    if let Some(index) = text_index {
        let results = search_with_bm25(store, index, reason_query, table_id)?;
        if !results.is_empty() {
            return Ok(results);
        }
    }

    // Fallback to filter-based search with strict limit
    get_candidates_by_filter(store, query, table_id)
}

/// Search using BM25 index.
fn search_with_bm25(
    store: &NodeStore,
    index: &TextIndex,
    query: &str,
    table_id: &str,
) -> Result<Vec<Document>> {
    let results = index.search(query, MAX_CANDIDATES, Some(table_id))?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut docs = Vec::new();

    for hit in results {
        if seen.contains(&hit.document_id) {
            continue;
        }
        if let Ok(Some(doc)) = store.get_document(&hit.document_id) {
            docs.push(doc);
            seen.insert(hit.document_id);
        }
    }

    Ok(docs)
}

/// Get candidates using filter (for tables without text index).
fn get_candidates_by_filter(
    store: &NodeStore,
    query: &Query,
    table_id: &str,
) -> Result<Vec<Document>> {
    let mut filter = query.to_search_filter();
    filter.table_id = Some(table_id.to_string());
    filter.limit = Some(MAX_CANDIDATES.min(SAFE_TABLE_SIZE));
    store.find_documents(&filter)
}

// ==================== Phase 2: Rank by Summary ====================

/// Rank documents by their summaries using LLM.
async fn rank_documents_by_summary<R: ReasoningEngine>(
    store: &NodeStore,
    candidates: Vec<Document>,
    reason_query: &str,
    target_docs: usize,
    reasoner: &Arc<R>,
) -> Vec<Document> {
    // Skip ranking if we have few enough candidates
    if candidates.len() <= target_docs {
        return candidates;
    }

    // Build document summaries
    let doc_summaries: Vec<DocumentSummary> = candidates
        .iter()
        .filter_map(|doc| {
            let root = store.get_root_node(&doc.id).ok()??;
            Some(DocumentSummary {
                id: doc.id.clone(),
                title: doc.title.clone(),
                summary: root.summary.clone(),
                tags: doc.tags.clone(),
            })
        })
        .collect();

    if doc_summaries.is_empty() {
        // Fallback: take first N if no summaries
        return candidates.into_iter().take(target_docs).collect();
    }

    // LLM ranks documents by relevance
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

    // Filter to ranked documents only
    let ranked_ids: HashSet<_> = rankings.iter().map(|r| r.document_id.as_str()).collect();
    candidates
        .into_iter()
        .filter(|d| ranked_ids.contains(d.id.as_str()))
        .collect()
}

// ==================== Phase 3: Parallel Reasoning ====================

/// Execute deep reasoning on documents in parallel.
async fn execute_parallel_reasoning<R: ReasoningEngine + Send + Sync + 'static>(
    engine: &SearchEngine<R>,
    documents: Vec<Document>,
    reason_query: &str,
    min_confidence: Option<f32>,
    query: &Query,
    progress_tx: &Option<mpsc::Sender<ReasonProgress>>,
) -> (Vec<DocumentMatch>, usize, usize) {
    let total_docs = documents.len();
    let mut all_matches: Vec<DocumentMatch> = Vec::new();
    let mut total_llm_calls = 1; // Count the ranking call
    let mut docs_completed: usize = 0;

    // Process in batches for controlled parallelism
    for chunk in documents.chunks(MAX_CONCURRENT) {
        // Create futures for parallel execution
        let futures: Vec<_> = chunk
            .iter()
            .map(|doc| {
                let doc = doc.clone();
                let query = reason_query.to_string();
                async move {
                    let result = engine.search_document(&query, &doc.id).await;
                    (doc, result)
                }
            })
            .collect();

        // Execute all futures in parallel
        let results = futures::future::join_all(futures).await;

        // Collect results
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

                    for result in response.results {
                        if let Some(min_conf) = min_confidence {
                            if result.confidence < min_conf {
                                continue;
                            }
                        }

                        all_matches.push(DocumentMatch {
                            document: doc.clone(),
                            score: Some(result.confidence),
                            matched_nodes: vec![result.node_id.clone()],
                            highlights: vec![result.content.clone()],
                            answer: result.extracted_answer,
                            confidence: Some(result.confidence),
                        });
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

        // Early termination check
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
