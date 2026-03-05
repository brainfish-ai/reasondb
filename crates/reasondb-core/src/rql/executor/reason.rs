//! LLM-powered semantic search execution
//!
//! This module handles REASON clause execution using an agentic search pattern:
//! 1. BM25 pre-filter (if SEARCH clause) or table filter → get candidates
//! 2. Recursive tree-grep pre-filter → structural re-ranking (zero LLM calls)
//! 3. LLM scans document summaries → ranks top N most relevant
//! 4. LLM deep reasoning → only on top N documents (parallel execution)

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::mpsc;

use crate::engine::{SearchConfig, SearchEngine};
use crate::error::Result;
use crate::llm::{DocumentSummary, ReasoningEngine};
use crate::model::Document;
use crate::query_decomposer::{DomainContext, SubQuery};
use crate::query_filter::extract_query_terms;
use crate::rql::ast::Query;
use crate::store::NodeStore;
use crate::text_index::TextIndex;
use crate::trace::{
    BeamDocumentTrace, BeamReasoningStep, BeamReasoningTrace, Bm25SelectionTrace,
    DecompositionTrace, DomainContextTrace, FinalResultTrace, LeafVerificationTrace,
    LlmRankingTrace, QueryTrace, StructuralFilterTrace, SubQueryTrace, TreeGrepScoreTrace,
};
use crate::tree_grep;

use super::types::{
    DocumentMatch, MatchedNode, QueryResult, QueryStats, ReasonPhase, ReasonPhaseStatus,
    ReasonProgress,
};

/// Configuration constants
const MAX_CANDIDATES: usize = 100;
const SAFE_TABLE_SIZE: usize = 1000;
const MAX_CONCURRENT: usize = 5;

// ==================== Candidate Document ====================

/// A BM25 node-level hit preserved through the pipeline.
#[derive(Debug, Clone)]
#[allow(dead_code)]
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
///
/// A `QueryTrace` is assembled across all phases and persisted to the store at the end.
/// The `trace_id` is included in the returned `QueryResult`.
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

    // Generate a unique trace ID for this query
    let trace_id = format!("trc_{}", uuid_short());

    // LIMIT controls how many results to return; reason over more to find the best ones
    let result_limit = query.limit.as_ref().map(|l| l.count).unwrap_or(10);
    let target_docs = (result_limit * 2).clamp(6, 20);

    let config = SearchConfig {
        min_confidence: min_confidence.unwrap_or(0.3),
        max_results: result_limit,
        ..Default::default()
    };

    // Create search engine for deep reasoning
    let engine = SearchEngine::with_config(store.clone(), reasoner.clone(), config);

    // ---------------------------------------------------------------
    // PHASE 0: Query Decomposition
    // ---------------------------------------------------------------
    let table = store.get_table(&table_id).ok().flatten();
    let domain_context: Option<DomainContext> = table.as_ref().map(|t| {
        let vocab_hints = t
            .metadata
            .get("domain_vocab")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        DomainContext {
            table_name: t.name.clone(),
            description: t.description.clone(),
            vocab_hints,
        }
    });

    send_progress(
        &progress_tx,
        ReasonProgress {
            phase: ReasonPhase::Candidates,
            status: ReasonPhaseStatus::Started,
            message: "Fetching candidates...".to_string(),
            detail: Some(serde_json::json!({ "trace_id": trace_id })),
        },
    )
    .await;

    // ---------------------------------------------------------------
    // PHASE 1: BM25 Candidate Selection (original query, immediate)
    //
    // Phase 0 (query decomposition) is deferred to run concurrently
    // with Phase 4 so it is completely off the critical path.
    // ---------------------------------------------------------------
    let orig_candidates =
        get_candidates(store, query, reason_query, text_index, &table_id).unwrap_or_default();
    let orig_candidate_count = orig_candidates.len();

    let mut phase1_hit_traces: Vec<crate::trace::Bm25HitTrace> = Vec::new();
    let mut combined_candidates: HashMap<String, CandidateDocument> = HashMap::new();

    for c in orig_candidates {
        let doc_id = c.document.id.clone();
        phase1_hit_traces.push(crate::trace::Bm25HitTrace {
            document_id: doc_id.clone(),
            document_title: c.document.title.clone(),
            score: c.bm25_score,
            matched_node_count: c.matched_nodes.len(),
            sub_query_index: 0,
        });
        combined_candidates.entry(doc_id).or_insert(c);
    }

    let mut candidates: Vec<CandidateDocument> = combined_candidates.into_values().collect();
    candidates.sort_by(|a, b| {
        b.bm25_score
            .partial_cmp(&a.bm25_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(MAX_CANDIDATES);

    let phase1_trace = Bm25SelectionTrace {
        total_candidates: candidates.len(),
        hits: phase1_hit_traces,
    };

    tracing::info!(
        candidate_count = candidates.len(),
        reason_query = %reason_query,
        "REASON Phase 1 (BM25): candidates retrieved"
    );

    send_progress(
        &progress_tx,
        ReasonProgress {
            phase: ReasonPhase::Candidates,
            status: ReasonPhaseStatus::Completed,
            message: format!("Found {} candidates", candidates.len()),
            detail: Some(serde_json::json!({ "count": candidates.len(), "trace_id": trace_id })),
        },
    )
    .await;

    // ---------------------------------------------------------------
    // PHASE 2: Structural Filtering via recursive tree-grep
    // ---------------------------------------------------------------
    send_progress(
        &progress_tx,
        ReasonProgress {
            phase: ReasonPhase::Filtering,
            status: ReasonPhaseStatus::Started,
            message: "Analyzing document structure...".to_string(),
            detail: None,
        },
    )
    .await;

    // Extract terms from the original query for tree-grep.
    // Sub-query terms are not yet available (decompose runs concurrently
    // with Phase 4); original query terms are sufficient for structural filtering.
    let terms = extract_query_terms(reason_query);

    let phase2_trace;
    if !terms.is_empty() {
        let (filtered, tree_grep_scores) =
            apply_tree_grep_filter_with_trace(store, candidates, &terms, target_docs);
        phase2_trace = StructuralFilterTrace {
            terms: terms.clone(),
            filtered_count: filtered.len(),
            scores: tree_grep_scores,
        };
        candidates = filtered;
    } else {
        phase2_trace = StructuralFilterTrace {
            terms: vec![],
            filtered_count: candidates.len(),
            scores: vec![],
        };
    }

    tracing::info!(
        filtered_count = candidates.len(),
        terms = ?terms,
        "REASON Phase 2 (tree-grep): structural filter applied"
    );

    send_progress(
        &progress_tx,
        ReasonProgress {
            phase: ReasonPhase::Filtering,
            status: ReasonPhaseStatus::Completed,
            message: format!("Structural analysis complete ({} terms)", terms.len()),
            detail: Some(serde_json::json!({ "terms": terms, "count": candidates.len() })),
        },
    )
    .await;

    // ---------------------------------------------------------------
    // PHASE 3: LLM Summary Ranking
    // ---------------------------------------------------------------
    send_progress(
        &progress_tx,
        ReasonProgress {
            phase: ReasonPhase::Ranking,
            status: ReasonPhaseStatus::Started,
            message: "Ranking documents by relevance...".to_string(),
            detail: None,
        },
    )
    .await;

    let phase3_input_count = candidates.len();
    let skip_threshold = target_docs + (target_docs / 2).max(2);
    let skipped_llm = phase3_input_count <= skip_threshold;

    let (documents, phase3_rankings) = rank_documents_by_summary_with_trace(
        store,
        candidates,
        reason_query,
        target_docs,
        &reasoner,
    )
    .await;

    let phase3_trace = LlmRankingTrace {
        input_count: phase3_input_count,
        selected_count: documents.len(),
        skipped_llm,
        rankings: phase3_rankings,
    };

    tracing::info!(
        ranked_count = documents.len(),
        "REASON Phase 3 (LLM ranking): documents ranked"
    );

    send_progress(
        &progress_tx,
        ReasonProgress {
            phase: ReasonPhase::Ranking,
            status: ReasonPhaseStatus::Completed,
            message: format!("Selected top {} documents", documents.len()),
            detail: Some(serde_json::json!({ "count": documents.len() })),
        },
    )
    .await;

    // ---------------------------------------------------------------
    // PHASE 4: Deep LLM reasoning (parallel)
    // ---------------------------------------------------------------
    send_progress(
        &progress_tx,
        ReasonProgress {
            phase: ReasonPhase::Reasoning,
            status: ReasonPhaseStatus::Started,
            message: format!("Deep reasoning on {} documents...", documents.len()),
            detail: Some(serde_json::json!({ "total": documents.len() })),
        },
    )
    .await;

    // ---------------------------------------------------------------
    // PHASE 0 + PHASE 4: Run concurrently
    //
    // Phase 0 (decompose, ~5s LLM call) is overlapped with Phase 4
    // (deep beam search, ~5–10s).  Both run as independent futures
    // polled by a single tokio::join!.  Decompose results are used for
    // trace enrichment; Phase 4 operates on original BM25 candidates.
    // ---------------------------------------------------------------
    let ((all_matches, total_llm_calls, docs_processed, beam_doc_traces), sub_queries_result) = tokio::join!(
        execute_parallel_reasoning_with_trace(
            &engine,
            documents,
            reason_query,
            min_confidence,
            query,
            &progress_tx,
        ),
        reasoner.decompose_query(reason_query, domain_context.as_ref())
    );

    let sub_queries: Vec<SubQuery> = sub_queries_result.unwrap_or_else(|e| {
        tracing::warn!("Query decomposition failed, using passthrough: {}", e);
        vec![SubQuery {
            text: reason_query.to_string(),
            rationale: "Fallback to original query".to_string(),
        }]
    });

    tracing::info!(
        sub_query_count = sub_queries.len(),
        reason_query = %reason_query,
        "REASON Phase 0 (decomposition): sub-queries generated (concurrent with Phase 4)"
    );

    let phase4_trace = BeamReasoningTrace {
        documents_processed: docs_processed,
        total_llm_calls,
        documents: beam_doc_traces,
    };

    send_progress(
        &progress_tx,
        ReasonProgress {
            phase: ReasonPhase::Reasoning,
            status: ReasonPhaseStatus::Completed,
            message: "Reasoning complete".to_string(),
            detail: Some(serde_json::json!({ "matches": all_matches.len() })),
        },
    )
    .await;

    // Sort by confidence
    let mut sorted_matches = all_matches;
    sorted_matches.sort_by(|a, b| {
        b.confidence
            .unwrap_or(0.0)
            .partial_cmp(&a.confidence.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Build final result traces
    let final_results: Vec<FinalResultTrace> = sorted_matches
        .iter()
        .flat_map(|m| {
            m.matched_nodes.iter().map(|n| FinalResultTrace {
                document_id: m.document.id.clone(),
                document_title: m.document.title.clone(),
                node_id: n.node_id.clone(),
                node_title: n.title.clone(),
                confidence: n.confidence,
                path: n.path.clone(),
            })
        })
        .collect();

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

    let duration_ms = start.elapsed().as_millis() as u64;

    // Assemble and persist the full trace
    // Build sub_query_traces from decompose results captured concurrently.
    // BM25 was not run for sub-queries (they overlapped Phase 4), so
    // bm25_hits is the original-query count for index 0 and 0 for the rest.
    let sub_query_traces: Vec<SubQueryTrace> = std::iter::once(SubQueryTrace {
        text: reason_query.to_string(),
        rationale: "Original query (BM25)".to_string(),
        bm25_hits: orig_candidate_count,
    })
    .chain(sub_queries.iter().map(|sq| SubQueryTrace {
        text: sq.text.clone(),
        rationale: sq.rationale.clone(),
        bm25_hits: 0,
    }))
    .collect();

    let decomposition_trace = if sub_queries.len() > 1
        || sub_queries
            .first()
            .map(|sq| sq.text != reason_query)
            .unwrap_or(false)
    {
        Some(DecompositionTrace {
            domain_context: domain_context.as_ref().map(|ctx| DomainContextTrace {
                table_name: ctx.table_name.clone(),
                description: ctx.description.clone(),
                vocab_hints: ctx.vocab_hints.clone(),
            }),
            sub_queries: sub_query_traces,
        })
    } else {
        None
    };

    let trace = QueryTrace {
        trace_id: trace_id.clone(),
        query: reason_query.to_string(),
        table_id: table_id.clone(),
        created_at: Utc::now(),
        duration_ms,
        decomposition: decomposition_trace,
        bm25_selection: phase1_trace,
        structural_filter: phase2_trace,
        llm_ranking: phase3_trace,
        beam_reasoning: phase4_trace,
        final_results,
    };

    if let Err(e) = store.save_trace(&trace) {
        tracing::warn!(trace_id = %trace_id, "Failed to persist query trace: {}", e);
    }

    Ok(QueryResult {
        documents: paginated,
        total_count,
        execution_time_ms: duration_ms,
        stats,
        aggregates: None,
        explain: None,
        trace_id: Some(trace_id),
    })
}

/// Generate a short random ID suffix.
fn uuid_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{:08x}{:04x}", nanos, (nanos ^ 0xABCD) & 0xFFFF)
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
        let candidates = search_with_bm25(store, index, &search_clause.query, table_id)?;
        return Ok(apply_where_filter(candidates, query));
    }

    if let Some(index) = text_index {
        let results = search_with_bm25(store, index, reason_query, table_id)?;
        if !results.is_empty() {
            let filtered = apply_where_filter(results, query);
            if !filtered.is_empty() {
                return Ok(filtered);
            }
            // All BM25 hits were excluded by WHERE clause; fall through to filter-based scan
        }
    }

    get_candidates_by_filter(store, query, table_id)
}

/// Post-filter BM25 candidates to honour the WHERE clause.
///
/// `search_with_bm25` scopes by `table_id` only, so a query like
/// `WHERE metadata.company = 'Apple Inc.'` would otherwise be ignored and
/// candidates from all companies in the table would flow into deep reasoning.
fn apply_where_filter(candidates: Vec<CandidateDocument>, query: &Query) -> Vec<CandidateDocument> {
    let filter = query.to_search_filter();

    let has_metadata = filter
        .document_metadata
        .as_ref()
        .map(|m| !m.is_empty())
        .unwrap_or(false);
    let has_tags = filter.tags.as_ref().map(|t| !t.is_empty()).unwrap_or(false);
    let has_tags_all = filter
        .tags_all
        .as_ref()
        .map(|t| !t.is_empty())
        .unwrap_or(false);

    if !has_metadata && !has_tags && !has_tags_all {
        return candidates;
    }

    candidates
        .into_iter()
        .filter(|c| {
            if let Some(ref meta_filter) = filter.document_metadata {
                for (key, expected) in meta_filter {
                    match c.document.metadata.get(key) {
                        Some(val) if val == expected => {}
                        _ => return false,
                    }
                }
            }
            if let Some(ref tags) = filter.tags {
                let doc_tags: std::collections::HashSet<_> =
                    c.document.tags.iter().map(|t| t.to_lowercase()).collect();
                if !tags.iter().any(|t| doc_tags.contains(&t.to_lowercase())) {
                    return false;
                }
            }
            if let Some(ref tags_all) = filter.tags_all {
                let doc_tags: std::collections::HashSet<_> =
                    c.document.tags.iter().map(|t| t.to_lowercase()).collect();
                if !tags_all
                    .iter()
                    .all(|t| doc_tags.contains(&t.to_lowercase()))
                {
                    return false;
                }
            }
            true
        })
        .collect()
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
            store
                .get_document(&doc_id)
                .ok()
                .flatten()
                .map(|doc| CandidateDocument {
                    document: doc,
                    bm25_score: best_score,
                    matched_nodes: nodes,
                    matched_sections: Vec::new(),
                    best_snippet: None,
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

/// Phase 2: Apply recursive tree-grep with trace data collection.
fn apply_tree_grep_filter_with_trace(
    store: &NodeStore,
    candidates: Vec<CandidateDocument>,
    terms: &[String],
    target_docs: usize,
) -> (Vec<CandidateDocument>, Vec<TreeGrepScoreTrace>) {
    let initial_count = candidates.len();
    let mut score_traces: Vec<TreeGrepScoreTrace> = Vec::new();

    let mut scored: Vec<(CandidateDocument, f32)> = candidates
        .into_iter()
        .map(|mut c| {
            let grep_result =
                tree_grep::tree_grep(store, &c.document.id, terms).unwrap_or_default();

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
            let combined = c.bm25_score * bm25_weight + grep_result.structural_score * grep_weight;

            score_traces.push(TreeGrepScoreTrace {
                document_id: c.document.id.clone(),
                document_title: c.document.title.clone(),
                combined_score: combined,
                matched_sections: c.matched_sections.clone(),
            });

            (c, combined)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    score_traces.sort_by(|a, b| {
        b.combined_score
            .partial_cmp(&a.combined_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Drop candidates with zero combined score (no BM25 or structural match)
    scored.retain(|(_, score)| *score > 0.0);

    if scored.is_empty() && initial_count > 0 {
        tracing::debug!("Tree-grep filtered all candidates; skipping truncation");
    }

    // Keep at most target_docs * 3 candidates for the Phase 3 LLM prompt.
    let cap = (target_docs * 3).clamp(10, MAX_CANDIDATES);
    scored.truncate(cap);

    let filtered = scored.into_iter().map(|(c, _)| c).collect();
    (filtered, score_traces)
}

// ==================== Phase 3: Rank by Summary ====================

/// Phase 3: Rank documents by their summaries using LLM, returning trace data alongside results.
async fn rank_documents_by_summary_with_trace<R: ReasoningEngine>(
    store: &NodeStore,
    candidates: Vec<CandidateDocument>,
    reason_query: &str,
    target_docs: usize,
    reasoner: &Arc<R>,
) -> (Vec<Document>, Vec<crate::trace::DocumentRankingTrace>) {
    let skip_threshold = target_docs + (target_docs / 2).max(2);
    if candidates.len() <= skip_threshold {
        let traces = candidates
            .iter()
            .take(target_docs)
            .map(|c| crate::trace::DocumentRankingTrace {
                document_id: c.document.id.clone(),
                document_title: c.document.title.clone(),
                relevance: 0.5,
                reasoning: "Skipped LLM ranking (below threshold)".to_string(),
            })
            .collect();
        let docs = candidates
            .into_iter()
            .take(target_docs)
            .map(|c| c.document)
            .collect();
        return (docs, traces);
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
        let docs = candidates
            .into_iter()
            .take(target_docs)
            .map(|c| c.document)
            .collect();
        return (docs, vec![]);
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

    let ranking_traces = rankings
        .iter()
        .filter_map(|r| {
            candidates
                .iter()
                .find(|c| c.document.id == r.document_id)
                .map(|c| crate::trace::DocumentRankingTrace {
                    document_id: r.document_id.clone(),
                    document_title: c.document.title.clone(),
                    relevance: r.relevance,
                    reasoning: r.reasoning.clone(),
                })
        })
        .collect();

    let ranked_ids: std::collections::HashSet<_> =
        rankings.iter().map(|r| r.document_id.as_str()).collect();
    let docs = candidates
        .into_iter()
        .filter(|c| ranked_ids.contains(c.document.id.as_str()))
        .map(|c| c.document)
        .collect();

    (docs, ranking_traces)
}

// ==================== Phase 4: Parallel Reasoning ====================

/// Phase 4: Execute deep reasoning on documents in parallel, collecting beam trace data.
async fn execute_parallel_reasoning_with_trace<R: ReasoningEngine + Send + Sync + 'static>(
    engine: &SearchEngine<R>,
    documents: Vec<Document>,
    reason_query: &str,
    min_confidence: Option<f32>,
    query: &Query,
    progress_tx: &Option<mpsc::Sender<ReasonProgress>>,
) -> (Vec<DocumentMatch>, usize, usize, Vec<BeamDocumentTrace>) {
    use std::sync::atomic::{AtomicBool, Ordering};

    let total_docs = documents.len();
    let mut all_matches: Vec<DocumentMatch> = Vec::new();
    let mut total_llm_calls = 1;
    let mut docs_completed: usize = 0;
    let mut beam_doc_traces: Vec<BeamDocumentTrace> = Vec::new();
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
                // Scope the query to this document so the beam search LLM evaluates
                // nodes as "contributing to" the answer rather than "fully answering"
                // it. This is critical for comparative queries (e.g. "Compare Apple,
                // Tesla, and Microsoft revenue") where each document only holds data
                // for one entity — without context, the LLM prunes all branches.
                let doc_scoped_query = if doc.title.is_empty() {
                    reason_query.to_string()
                } else {
                    format!(
                        "{}\n[Document context: searching within '{}'. Content that addresses any part of the above query for this document's subject is relevant.]",
                        reason_query, doc.title
                    )
                };
                let cancel = cancel.clone();
                async move {
                    let result = engine
                        .search_document_with_cancel(&doc_scoped_query, &doc.id, cancel)
                        .await;
                    (doc, result)
                }
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        for (doc, search_result) in results {
            docs_completed += 1;

            send_progress(
                progress_tx,
                ReasonProgress {
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
                },
            )
            .await;

            match search_result {
                Ok(response) => {
                    total_llm_calls += response.stats.llm_calls;

                    let mut nodes_for_doc: Vec<MatchedNode> = Vec::new();
                    let mut best_confidence: f32 = 0.0;

                    // Build per-document beam trace from SearchResults
                    let relevant_leaves: Vec<LeafVerificationTrace> = response
                        .results
                        .iter()
                        .map(|r| LeafVerificationTrace {
                            node_id: r.node_id.clone(),
                            node_title: r.title.clone(),
                            is_relevant: r.confidence >= min_confidence.unwrap_or(0.3),
                            confidence: r.confidence,
                            path: r.path.clone(),
                            reasoning_steps: r
                                .reasoning_trace
                                .iter()
                                .map(|step| BeamReasoningStep {
                                    node_title: step.node_title.clone(),
                                    decision: step.decision.clone(),
                                    confidence: step.confidence,
                                })
                                .collect(),
                        })
                        .collect();

                    beam_doc_traces.push(BeamDocumentTrace {
                        document_id: doc.id.clone(),
                        document_title: doc.title.clone(),
                        nodes_visited: response.stats.nodes_visited,
                        nodes_pruned: response.stats.nodes_pruned,
                        llm_calls: response.stats.llm_calls,
                        relevant_leaves,
                    });

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
    (
        all_matches,
        total_llm_calls,
        docs_processed,
        beam_doc_traces,
    )
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
