//! RQL Query endpoint
//!
//! Execute SQL-like queries against documents.

use axum::{
    extract::{Path, State},
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::{Stream, StreamExt};
use futures::FutureExt;
use reasondb_core::llm::ReasoningEngine;
use reasondb_core::rql::{
    AggregateValue, DocumentMatch, FieldPath, FieldSelector, MatchedNode, MutationResult,
    PathSegment, Query, QueryResult, QueryStats, ReasonProgress, SelectClause, Statement,
};
use reasondb_core::trace::{QueryTrace, QueryTraceSummary};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::state::AppState;

/// RQL query request
#[derive(Debug, Deserialize, ToSchema)]
pub struct QueryRequest {
    /// RQL query string (e.g., "SELECT * FROM legal WHERE status = 'active'")
    pub query: String,

    /// Optional timeout in milliseconds
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Query response
#[derive(Debug, Serialize, ToSchema)]
pub struct QueryResponse {
    /// Matched documents (projected to only the columns named in the SELECT clause)
    pub documents: Vec<serde_json::Value>,

    /// Total count before pagination
    pub total_count: usize,

    /// Execution time in milliseconds
    pub execution_time_ms: u64,

    /// Aggregate results (for COUNT/SUM/AVG queries)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregates: Option<Vec<AggregateResultResponse>>,

    /// Query plan (for EXPLAIN queries)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explain: Option<QueryPlanResponse>,

    /// Trace ID for this query (set for REASON queries; use GET /tables/{id}/traces/{trace_id})
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

/// Aggregate result in query response
#[derive(Debug, Serialize, ToSchema)]
pub struct AggregateResultResponse {
    /// Alias or function name
    pub name: String,
    /// Computed value
    pub value: serde_json::Value,
    /// Group key (for GROUP BY queries)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_key: Option<Vec<(String, serde_json::Value)>>,
}

/// Query execution plan
#[derive(Debug, Serialize, ToSchema)]
pub struct QueryPlanResponse {
    /// Steps in the execution plan
    pub steps: Vec<PlanStepResponse>,
    /// Estimated row count
    pub estimated_rows: usize,
    /// Indexes that would be used
    pub indexes_used: Vec<String>,
}

/// A single step in the query plan
#[derive(Debug, Serialize, ToSchema)]
pub struct PlanStepResponse {
    /// Step type (e.g., "TableScan", "IndexScan", "Filter", "Aggregate")
    pub step_type: String,
    /// Description of what this step does
    pub description: String,
    /// Estimated cost (0-100)
    pub estimated_cost: u32,
}

/// A step in the reasoning trace
#[derive(Debug, Serialize, ToSchema)]
pub struct ReasoningStepResponse {
    /// Node title at this step
    pub node_title: String,
    /// Decision made (which child was chosen)
    pub decision: String,
    /// Confidence at this step
    pub confidence: f32,
}

/// A matched node returned from REASON queries
#[derive(Debug, Serialize, ToSchema)]
pub struct MatchedNodeResponse {
    /// Node ID
    pub node_id: String,
    /// Node title
    pub title: String,
    /// The actual content of the node
    pub content: String,
    /// Path from root to this node (titles)
    pub path: Vec<String>,
    /// Confidence score for this match
    pub confidence: f32,
    /// The reasoning trace showing decisions that led here
    pub reasoning_trace: Vec<ReasoningStepResponse>,
}

impl From<MatchedNode> for MatchedNodeResponse {
    fn from(n: MatchedNode) -> Self {
        Self {
            node_id: n.node_id,
            title: n.title,
            content: n.content,
            path: n.path,
            confidence: n.confidence,
            reasoning_trace: n
                .reasoning_trace
                .into_iter()
                .map(|s| ReasoningStepResponse {
                    node_title: s.node_title,
                    decision: s.decision,
                    confidence: s.confidence,
                })
                .collect(),
        }
    }
}

/// A matched document in query results
#[derive(Debug, Serialize, ToSchema)]
pub struct QueryDocumentMatch {
    /// Document ID
    pub id: String,

    /// Document title
    pub title: String,

    /// Table ID
    pub table_id: String,

    /// Tags
    pub tags: Vec<String>,

    /// Document metadata
    pub metadata: std::collections::HashMap<String, serde_json::Value>,

    /// Total nodes in document
    pub total_nodes: usize,

    /// Created timestamp
    pub created_at: String,

    /// Relevance score (BM25 for SEARCH, confidence for REASON)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,

    /// Highlighted snippets
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<String>,

    /// Matched nodes with full details (for REASON queries)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matched_nodes: Vec<MatchedNodeResponse>,

    /// Confidence score from LLM (for REASON queries)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

impl From<DocumentMatch> for QueryDocumentMatch {
    fn from(m: DocumentMatch) -> Self {
        Self {
            id: m.document.id,
            title: m.document.title,
            table_id: m.document.table_id,
            tags: m.document.tags,
            metadata: m.document.metadata,
            total_nodes: m.document.total_nodes,
            created_at: m.document.created_at.to_rfc3339(),
            score: m.score,
            highlights: m.highlights,
            matched_nodes: m.matched_nodes.into_iter().map(|n| n.into()).collect(),
            confidence: m.confidence,
        }
    }
}

impl From<QueryResult> for QueryResponse {
    fn from(r: QueryResult) -> Self {
        Self {
            documents: r
                .documents
                .into_iter()
                .map(|m| {
                    let qdm = QueryDocumentMatch::from(m);
                    serde_json::to_value(qdm).unwrap_or_default()
                })
                .collect(),
            total_count: r.total_count,
            execution_time_ms: r.execution_time_ms,
            aggregates: r.aggregates.map(|aggs| {
                aggs.into_iter()
                    .map(|a| AggregateResultResponse {
                        name: a.name,
                        value: match a.value {
                            AggregateValue::Count(c) => serde_json::json!(c),
                            AggregateValue::Float(f) => serde_json::json!(f),
                            AggregateValue::Null => serde_json::Value::Null,
                        },
                        group_key: a.group_key,
                    })
                    .collect()
            }),
            explain: r.explain.map(|p| QueryPlanResponse {
                steps: p
                    .steps
                    .into_iter()
                    .map(|s| PlanStepResponse {
                        step_type: s.step_type,
                        description: s.description,
                        estimated_cost: s.estimated_cost,
                    })
                    .collect(),
                estimated_rows: p.estimated_rows,
                indexes_used: p.indexes_used,
            }),
            trace_id: r.trace_id,
        }
    }
}

/// Response for UPDATE/DELETE mutations
#[derive(Debug, Serialize, ToSchema)]
pub struct MutationResponse {
    /// Number of documents affected
    pub rows_affected: usize,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
}

impl From<MutationResult> for MutationResponse {
    fn from(r: MutationResult) -> Self {
        Self {
            rows_affected: r.rows_affected,
            execution_time_ms: r.execution_time_ms,
        }
    }
}

/// Unified response for all RQL statement types
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum StatementResponse {
    /// Response for SELECT queries
    Query(QueryResponse),
    /// Response for UPDATE/DELETE mutations
    Mutation(MutationResponse),
}

/// Validate request: array of query strings to check
#[derive(Debug, Deserialize, ToSchema)]
pub struct ValidateRequest {
    /// Array of individual query strings to validate
    pub queries: Vec<String>,
}

/// Validation result for a single query
#[derive(Debug, Serialize, ToSchema)]
pub struct ValidationResult {
    /// Index of the query in the input array
    pub index: usize,
    /// Whether the query is syntactically valid
    pub valid: bool,
    /// Error message if invalid
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Line number of the error (1-indexed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// Column number of the error (1-indexed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
}

/// Validate response
#[derive(Debug, Serialize, ToSchema)]
pub struct ValidateResponse {
    pub results: Vec<ValidationResult>,
}

/// Validate one or more RQL queries without executing them.
///
/// Parses each query and returns structured error information including
/// line/column positions for editor integration.
#[utoipa::path(
    post,
    path = "/v1/query/validate",
    request_body = ValidateRequest,
    responses(
        (status = 200, description = "Validation results", body = ValidateResponse),
    ),
    tag = "query"
)]
pub async fn validate_query<R: ReasoningEngine + Send + Sync + 'static>(
    State(_state): State<Arc<AppState<R>>>,
    Json(request): Json<ValidateRequest>,
) -> Json<ValidateResponse> {
    use reasondb_core::rql::RqlError;

    let results = request
        .queries
        .iter()
        .enumerate()
        .map(|(index, q)| {
            let trimmed = q.trim();
            if trimmed.is_empty() {
                return ValidationResult {
                    index,
                    valid: true,
                    error: None,
                    line: None,
                    column: None,
                };
            }
            match Statement::parse(trimmed) {
                Ok(_) => ValidationResult {
                    index,
                    valid: true,
                    error: None,
                    line: None,
                    column: None,
                },
                Err(e) => {
                    let (line, column) = match &e {
                        RqlError::Lexer(le) => (Some(le.line), Some(le.column)),
                        RqlError::Parser(_) => (Some(1), Some(1)),
                        _ => (None, None),
                    };
                    ValidationResult {
                        index,
                        valid: false,
                        error: Some(e.to_string()),
                        line,
                        column,
                    }
                }
            }
        })
        .collect();

    Json(ValidateResponse { results })
}

/// Execute an RQL query
///
/// Supports:
/// - WHERE clauses for filtering
/// - SEARCH clause for BM25 full-text search (fast keyword matching)
/// - REASON clause for LLM semantic search (intelligent answer extraction)
///
/// # Example
///
/// ```bash
/// # Filter query
/// curl -X POST http://localhost:4444/v1/query \
///   -H "Content-Type: application/json" \
///   -d '{"query": "SELECT * FROM legal_contracts WHERE author = '\''Alice'\'' LIMIT 10"}'
///
/// # Full-text search with BM25
/// curl -X POST http://localhost:4444/v1/query \
///   -H "Content-Type: application/json" \
///   -d '{"query": "SELECT * FROM legal_contracts SEARCH '\''payment terms'\''"}'
///
/// # Semantic search with LLM
/// curl -X POST http://localhost:4444/v1/query \
///   -H "Content-Type: application/json" \
///   -d '{"query": "SELECT * FROM legal_contracts REASON '\''What are the late payment penalties?'\''"}'
/// ```
#[utoipa::path(
    post,
    path = "/v1/query",
    request_body = QueryRequest,
    responses(
        (status = 200, description = "Query executed successfully", body = QueryResponse),
        (status = 400, description = "Invalid query syntax"),
        (status = 500, description = "Internal server error")
    ),
    tag = "query"
)]
pub async fn execute_query<R: ReasoningEngine + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Json(request): Json<QueryRequest>,
) -> Result<Json<StatementResponse>, ApiError> {
    // Parse as a general statement (SELECT, UPDATE, or DELETE)
    let stmt = Statement::parse(&request.query)
        .map_err(|e| ApiError::BadRequest(format!("Invalid query: {}", e)))?;

    match stmt {
        Statement::Update(ref uq) => {
            let result = state
                .store
                .execute_update(uq)
                .map_err(|e| ApiError::Internal(format!("Update failed: {}", e)))?;
            Ok(Json(StatementResponse::Mutation(result.into())))
        }
        Statement::Delete(ref dq) => {
            let result = state
                .store
                .execute_delete(dq)
                .map_err(|e| ApiError::Internal(format!("Delete failed: {}", e)))?;
            Ok(Json(StatementResponse::Mutation(result.into())))
        }
        Statement::Select(ref query) => {
            let result = execute_select_query(query, &state).await?;
            let response = apply_projection(result.into(), &query.select);
            Ok(Json(StatementResponse::Query(response)))
        }
    }
}

/// Execute a SELECT query, handling REASON caching and search.
async fn execute_select_query<R: ReasoningEngine + Send + Sync + 'static>(
    query: &Query,
    state: &Arc<AppState<R>>,
) -> Result<QueryResult, ApiError> {
    use reasondb_core::cache::{CachedMatch, CachedQueryResult};
    use std::time::Instant;

    if let Some(ref reason_clause) = query.reason {
        // Check cache first for REASON queries
        if let Some(cached) = state
            .query_cache
            .get(&reason_clause.query, &query.from.table)
        {
            tracing::info!(
                "Cache HIT for query '{}' - saved {} LLM calls",
                reason_clause.query,
                cached.llm_calls_saved
            );

            let matches: Vec<DocumentMatch> = cached
                .matches
                .iter()
                .map(|m| {
                    let matched_nodes = m
                        .matched_nodes
                        .iter()
                        .map(|n| MatchedNode {
                            node_id: n.node_id.clone(),
                            title: n.title.clone(),
                            content: n.content.clone(),
                            path: n.path.clone(),
                            confidence: n.confidence,
                            reasoning_trace: n.reasoning_trace.clone(),
                        })
                        .collect();
                    let mut doc =
                        reasondb_core::Document::new(m.document_title.clone(), &m.table_id);
                    doc.id = m.document_id.clone();
                    doc.total_nodes = m.total_nodes;
                    doc.tags = m.tags.clone();
                    doc.metadata = m.metadata.clone();
                    doc.created_at = m.created_at;
                    DocumentMatch {
                        document: doc,
                        score: Some(m.score),
                        matched_nodes,
                        highlights: m.highlights.clone(),
                        confidence: Some(m.confidence),
                    }
                })
                .collect();

            Ok(QueryResult {
                documents: matches,
                total_count: cached.matches.len(),
                execution_time_ms: 0,
                stats: QueryStats {
                    index_used: Some("cache".to_string()),
                    rows_scanned: 0,
                    rows_returned: cached.matches.len(),
                    search_executed: false,
                    reason_executed: false,
                    llm_calls: 0,
                },
                aggregates: None,
                explain: None,
                trace_id: None,
            })
        } else {
            let result = state
                .store
                .execute_rql_async(
                    query,
                    Some(state.text_index.as_ref()),
                    state.reasoner.clone(),
                )
                .await
                .map_err(|e| ApiError::Internal(format!("Query execution failed: {}", e)))?;

            let cached_matches: Vec<CachedMatch> = result
                .documents
                .iter()
                .map(|m| CachedMatch {
                    document_id: m.document.id.clone(),
                    document_title: m.document.title.clone(),
                    table_id: m.document.table_id.clone(),
                    total_nodes: m.document.total_nodes,
                    tags: m.document.tags.clone(),
                    metadata: m.document.metadata.clone(),
                    created_at: m.document.created_at,
                    score: m.score.unwrap_or(0.0),
                    confidence: m.confidence.unwrap_or(0.0),
                    highlights: m.highlights.clone(),
                    matched_nodes: m
                        .matched_nodes
                        .iter()
                        .map(|n| reasondb_core::cache::CachedMatchedNode {
                            node_id: n.node_id.clone(),
                            title: n.title.clone(),
                            content: n.content.clone(),
                            path: n.path.clone(),
                            confidence: n.confidence,
                            reasoning_trace: n.reasoning_trace.clone(),
                        })
                        .collect(),
                })
                .collect();

            let cache_entry = CachedQueryResult {
                query: reason_clause.query.clone(),
                table_id: query.from.table.clone(),
                matches: cached_matches,
                cached_at: Instant::now(),
                llm_calls_saved: result.stats.llm_calls,
            };

            state
                .query_cache
                .insert(&reason_clause.query, &query.from.table, cache_entry);
            tracing::info!(
                "Cache MISS for query '{}' - cached {} results",
                reason_clause.query,
                result.documents.len()
            );

            Ok(result)
        }
    } else {
        state
            .store
            .execute_rql_with_search(query, Some(state.text_index.as_ref()))
            .map_err(|e| ApiError::Internal(format!("Query execution failed: {}", e)))
    }
}

/// Execute an RQL query with SSE progress streaming.
///
/// Emits `progress` events during REASON execution and a final `complete`
/// event with the full query response. Non-REASON queries emit a single
/// `complete` event immediately.
pub async fn execute_query_stream<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Json(request): Json<QueryRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    use reasondb_core::cache::{CachedMatch, CachedQueryResult};
    use std::time::Instant;

    let stmt = Statement::parse(&request.query)
        .map_err(|e| ApiError::BadRequest(format!("Invalid query: {}", e)))?;

    let (sse_tx, sse_rx) = mpsc::channel::<Event>(32);

    // UPDATE/DELETE: execute and send a single complete event
    match &stmt {
        Statement::Update(uq) => {
            let result = state
                .store
                .execute_update(uq)
                .map_err(|e| ApiError::Internal(format!("Update failed: {}", e)))?;
            let response: MutationResponse = result.into();
            let event = Event::default()
                .event("complete")
                .json_data(&response)
                .unwrap_or_else(|_| Event::default().event("complete").data("{}"));
            let _ = sse_tx.send(event).await;
            drop(sse_tx);
            let stream = ReceiverStream::new(sse_rx).map(Ok::<_, Infallible>);
            return Ok(Sse::new(stream));
        }
        Statement::Delete(dq) => {
            let result = state
                .store
                .execute_delete(dq)
                .map_err(|e| ApiError::Internal(format!("Delete failed: {}", e)))?;
            let response: MutationResponse = result.into();
            let event = Event::default()
                .event("complete")
                .json_data(&response)
                .unwrap_or_else(|_| Event::default().event("complete").data("{}"));
            let _ = sse_tx.send(event).await;
            drop(sse_tx);
            let stream = ReceiverStream::new(sse_rx).map(Ok::<_, Infallible>);
            return Ok(Sse::new(stream));
        }
        Statement::Select(_) => {}
    }

    let query = match stmt {
        Statement::Select(q) => q,
        _ => unreachable!(),
    };

    if let Some(reason_clause) = query.reason.clone() {
        // Check cache first
        if let Some(cached) = state
            .query_cache
            .get(&reason_clause.query, &query.from.table)
        {
            tracing::info!(
                "Cache HIT for streaming query '{}' - saved {} LLM calls",
                reason_clause.query,
                cached.llm_calls_saved
            );
            let matches: Vec<DocumentMatch> = cached
                .matches
                .iter()
                .map(|m| {
                    let matched_nodes = m
                        .matched_nodes
                        .iter()
                        .map(|n| MatchedNode {
                            node_id: n.node_id.clone(),
                            title: n.title.clone(),
                            content: n.content.clone(),
                            path: n.path.clone(),
                            confidence: n.confidence,
                            reasoning_trace: n.reasoning_trace.clone(),
                        })
                        .collect();
                    let mut doc =
                        reasondb_core::Document::new(m.document_title.clone(), &m.table_id);
                    doc.id = m.document_id.clone();
                    doc.total_nodes = m.total_nodes;
                    doc.tags = m.tags.clone();
                    doc.metadata = m.metadata.clone();
                    doc.created_at = m.created_at;
                    DocumentMatch {
                        document: doc,
                        score: Some(m.score),
                        matched_nodes,
                        highlights: m.highlights.clone(),
                        confidence: Some(m.confidence),
                    }
                })
                .collect();
            let result = QueryResult {
                documents: matches,
                total_count: cached.matches.len(),
                execution_time_ms: 0,
                stats: QueryStats {
                    index_used: Some("cache".to_string()),
                    rows_scanned: 0,
                    rows_returned: cached.matches.len(),
                    search_executed: false,
                    reason_executed: false,
                    llm_calls: 0,
                },
                aggregates: None,
                explain: None,
                trace_id: None,
            };
            let response = apply_projection(result.into(), &query.select);
            let event = Event::default()
                .event("complete")
                .json_data(&response)
                .unwrap_or_else(|_| Event::default().event("complete").data("{}"));
            let _ = sse_tx.send(event).await;
            drop(sse_tx);
        } else {
            // Spawn executor in a background task with progress channel
            let (progress_tx, mut progress_rx) = mpsc::channel::<ReasonProgress>(32);
            let store = state.store.clone();
            let text_index = state.text_index.clone();
            let reasoner = state.reasoner.clone();
            let query_cache = state.query_cache.clone();
            let query_clone = query.clone();
            let reason_query_str = reason_clause.query.clone();
            let table_name = query.from.table.clone();

            // Background task: run the executor, forward progress to SSE
            let sse_tx_bg = sse_tx.clone();
            tokio::spawn(async move {
                // Forward progress events to SSE
                let sse_tx_fwd = sse_tx_bg.clone();
                let forwarder = tokio::spawn(async move {
                    while let Some(progress) = progress_rx.recv().await {
                        let event = Event::default()
                            .event("progress")
                            .json_data(&progress)
                            .unwrap_or_else(|_| Event::default().event("progress").data("{}"));
                        if sse_tx_fwd.send(event).await.is_err() {
                            break;
                        }
                    }
                });

                let result = std::panic::AssertUnwindSafe(store.execute_rql_async_with_progress(
                    &query_clone,
                    Some(text_index.as_ref()),
                    reasoner,
                    Some(progress_tx),
                ))
                .catch_unwind()
                .await;

                // Wait for forwarder to finish draining
                let _ = forwarder.await;

                match result {
                    Ok(Ok(result)) => {
                        // Cache the result
                        let cached_matches: Vec<CachedMatch> = result
                            .documents
                            .iter()
                            .map(|m| CachedMatch {
                                document_id: m.document.id.clone(),
                                document_title: m.document.title.clone(),
                                table_id: m.document.table_id.clone(),
                                total_nodes: m.document.total_nodes,
                                tags: m.document.tags.clone(),
                                metadata: m.document.metadata.clone(),
                                created_at: m.document.created_at,
                                score: m.score.unwrap_or(0.0),
                                confidence: m.confidence.unwrap_or(0.0),
                                highlights: m.highlights.clone(),
                                matched_nodes: m
                                    .matched_nodes
                                    .iter()
                                    .map(|n| reasondb_core::cache::CachedMatchedNode {
                                        node_id: n.node_id.clone(),
                                        title: n.title.clone(),
                                        content: n.content.clone(),
                                        path: n.path.clone(),
                                        confidence: n.confidence,
                                        reasoning_trace: n.reasoning_trace.clone(),
                                    })
                                    .collect(),
                            })
                            .collect();
                        let cache_entry = CachedQueryResult {
                            query: reason_query_str,
                            table_id: table_name,
                            matches: cached_matches,
                            cached_at: Instant::now(),
                            llm_calls_saved: result.stats.llm_calls,
                        };
                        let q = cache_entry.query.clone();
                        let t = cache_entry.table_id.clone();
                        query_cache.insert(&q, &t, cache_entry);

                        let response = apply_projection(result.into(), &query_clone.select);
                        let event = Event::default()
                            .event("complete")
                            .json_data(&response)
                            .unwrap_or_else(|_| Event::default().event("complete").data("{}"));
                        let _ = sse_tx_bg.send(event).await;
                    }
                    Ok(Err(e)) => {
                        tracing::error!("REASON query failed: {}", e);
                        let event = Event::default()
                            .event("error")
                            .data(format!("Query execution failed: {}", e));
                        let _ = sse_tx_bg.send(event).await;
                    }
                    Err(panic_err) => {
                        let msg = if let Some(s) = panic_err.downcast_ref::<String>() {
                            s.clone()
                        } else if let Some(s) = panic_err.downcast_ref::<&str>() {
                            s.to_string()
                        } else {
                            "Unknown panic during query execution".to_string()
                        };
                        tracing::error!("REASON query panicked: {}", msg);
                        let event = Event::default()
                            .event("error")
                            .data("Query execution failed. Please try again.");
                        let _ = sse_tx_bg.send(event).await;
                    }
                }
            });
            drop(sse_tx);
        }
    } else {
        // Non-REASON queries: send a single complete event
        let result = state
            .store
            .execute_rql_with_search(&query, Some(state.text_index.as_ref()))
            .map_err(|e| ApiError::Internal(format!("Query execution failed: {}", e)))?;
        let response = apply_projection(result.into(), &query.select);
        let event = Event::default()
            .event("complete")
            .json_data(&response)
            .unwrap_or_else(|_| Event::default().event("complete").data("{}"));
        let _ = sse_tx.send(event).await;
        drop(sse_tx);
    }

    let stream = ReceiverStream::new(sse_rx).map(Ok::<_, Infallible>);
    Ok(Sse::new(stream))
}

// ---------------------------------------------------------------------------
// Projection helpers
// ---------------------------------------------------------------------------

/// Apply SELECT field projection to a `QueryResponse`.
///
/// `SELECT *` and aggregate queries are returned unchanged.  
/// `SELECT col1, col2, metadata.key` projects each document to only the
/// requested columns, resolving dot-paths into the document JSON.
fn apply_projection(mut response: QueryResponse, select: &SelectClause) -> QueryResponse {
    if let SelectClause::Fields(fields) = select {
        response.documents = response
            .documents
            .into_iter()
            .map(|doc| project_document(doc, fields))
            .collect();
    }
    response
}

/// Project a single document JSON value to only the fields named in `fields`.
fn project_document(doc: serde_json::Value, fields: &[FieldSelector]) -> serde_json::Value {
    let mut result = serde_json::Map::new();
    for selector in fields {
        // Use alias if provided, otherwise the dotted path string (e.g. "metadata.topic")
        let col_name = selector
            .alias
            .clone()
            .unwrap_or_else(|| selector.path.to_string());
        let val = resolve_path(&doc, &selector.path);
        result.insert(col_name, val);
    }
    serde_json::Value::Object(result)
}

/// Walk a `FieldPath` into a JSON value, returning `null` if any segment is missing.
fn resolve_path(doc: &serde_json::Value, path: &FieldPath) -> serde_json::Value {
    let mut current = doc;
    // Temporary storage so references stay alive across loop iterations
    let mut owned: serde_json::Value;
    for segment in &path.segments {
        match segment {
            PathSegment::Field(key) => match current.get(key.as_str()) {
                Some(v) => {
                    owned = v.clone();
                    current = &owned;
                }
                None => return serde_json::Value::Null,
            },
            PathSegment::Index(idx) => match current.get(idx) {
                Some(v) => {
                    owned = v.clone();
                    current = &owned;
                }
                None => return serde_json::Value::Null,
            },
        }
    }
    current.clone()
}

// ==================== Trace Endpoints ====================

/// List recent query traces for a table (newest first).
///
/// Returns compact summaries. Use the `GET /tables/{id}/traces/{trace_id}` endpoint
/// for the full structured trace.
#[utoipa::path(
    get,
    path = "/v1/tables/{id}/traces",
    params(
        ("id" = String, Path, description = "Table ID or slug"),
        ("limit" = Option<usize>, Query, description = "Max traces to return (default 50)"),
    ),
    responses(
        (status = 200, description = "List of trace summaries"),
        (status = 404, description = "Table not found"),
    ),
    tag = "query"
)]
pub async fn list_traces<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<QueryTraceSummary>>, ApiError> {
    let table_id = state.store.resolve_table_id(&id).map_err(ApiError::from)?;

    let limit: usize = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);

    let summaries = state
        .store
        .list_traces(&table_id, limit)
        .map_err(|e| ApiError::StorageError(e.to_string()))?;

    Ok(Json(summaries))
}

/// Get the full structured trace for a single REASON query execution.
///
/// The trace captures every phase of the retrieval pipeline:
/// - Phase 0: Query decomposition (sub-queries + domain context used)
/// - Phase 1: BM25 candidate scores per document
/// - Phase 2: Tree-grep structural filter scores
/// - Phase 3: LLM summary ranking decisions
/// - Phase 4: Beam search traversal with per-node LLM decisions
#[utoipa::path(
    get,
    path = "/v1/tables/{id}/traces/{trace_id}",
    params(
        ("id" = String, Path, description = "Table ID or slug"),
        ("trace_id" = String, Path, description = "Trace ID from query response"),
    ),
    responses(
        (status = 200, description = "Full query trace"),
        (status = 404, description = "Trace not found"),
    ),
    tag = "query"
)]
pub async fn get_trace<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path((id, trace_id)): Path<(String, String)>,
) -> Result<Json<QueryTrace>, ApiError> {
    // Resolve table to verify it exists and the trace belongs to it
    let table_id = state.store.resolve_table_id(&id).map_err(ApiError::from)?;

    let trace = state
        .store
        .get_trace(&trace_id)
        .map_err(|e| ApiError::StorageError(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Trace '{}' not found", trace_id)))?;

    if trace.table_id != table_id {
        return Err(ApiError::NotFound(format!(
            "Trace '{}' not found for table '{}'",
            trace_id, id
        )));
    }

    Ok(Json(trace))
}
