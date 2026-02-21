//! RQL Query endpoint
//!
//! Execute SQL-like queries against documents.

use axum::{extract::State, response::sse::{Event, Sse}, Json};
use futures::stream::{Stream, StreamExt};
use reasondb_core::llm::ReasoningEngine;
use reasondb_core::rql::{AggregateValue, DocumentMatch, Query, QueryResult, QueryStats, ReasonProgress};
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
    /// Matched documents
    pub documents: Vec<QueryDocumentMatch>,

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

    /// LLM-extracted answer (for REASON queries)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,

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
            answer: m.answer,
            confidence: m.confidence,
        }
    }
}

impl From<QueryResult> for QueryResponse {
    fn from(r: QueryResult) -> Self {
        Self {
            documents: r.documents.into_iter().map(|m| m.into()).collect(),
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
        }
    }
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
) -> Result<Json<QueryResponse>, ApiError> {
    use reasondb_core::cache::{CachedMatch, CachedQueryResult};
    use std::time::Instant;

    // Parse the query
    let query = Query::parse(&request.query)
        .map_err(|e| ApiError::BadRequest(format!("Invalid query: {}", e)))?;

    // Check if this is a REASON query (needs async LLM execution)
    let result = if let Some(ref reason_clause) = query.reason {
        // Check cache first for REASON queries
        if let Some(cached) = state.query_cache.get(&reason_clause.query, &query.from.table) {
            tracing::info!(
                "Cache HIT for query '{}' - saved {} LLM calls",
                reason_clause.query,
                cached.llm_calls_saved
            );
            
            // Convert cached result to QueryResult
            let matches: Vec<DocumentMatch> = cached.matches.iter().map(|m| {
                DocumentMatch {
                    document: reasondb_core::Document::new(m.document_title.clone(), &cached.table_id),
                    score: Some(m.score),
                    matched_nodes: vec![],
                    highlights: m.highlights.clone(),
                    answer: m.answer.clone(),
                    confidence: Some(m.confidence),
                }
            }).collect();
            
            QueryResult {
                documents: matches,
                total_count: cached.matches.len(),
                execution_time_ms: 0, // Cached
                stats: QueryStats {
                    index_used: Some("cache".to_string()),
                    rows_scanned: 0,
                    rows_returned: cached.matches.len(),
                    search_executed: false,
                    reason_executed: false, // Already done
                    llm_calls: 0, // Cached
                },
                aggregates: None,
                explain: None,
            }
        } else {
            // Cache miss - execute query
            let result = state
                .store
                .execute_rql_async(&query, Some(state.text_index.as_ref()), state.reasoner.clone())
                .await
                .map_err(|e| ApiError::Internal(format!("Query execution failed: {}", e)))?;
            
            // Cache the result
            let cached_matches: Vec<CachedMatch> = result.documents.iter().map(|m| {
                CachedMatch {
                    document_id: m.document.id.clone(),
                    document_title: m.document.title.clone(),
                    score: m.score.unwrap_or(0.0),
                    answer: m.answer.clone(),
                    confidence: m.confidence.unwrap_or(0.0),
                    highlights: m.highlights.clone(),
                }
            }).collect();
            
            let cache_entry = CachedQueryResult {
                query: reason_clause.query.clone(),
                table_id: query.from.table.clone(),
                matches: cached_matches,
                cached_at: Instant::now(),
                llm_calls_saved: result.stats.llm_calls,
            };
            
            state.query_cache.insert(&reason_clause.query, &query.from.table, cache_entry);
            tracing::info!(
                "Cache MISS for query '{}' - cached {} results",
                reason_clause.query,
                result.documents.len()
            );
            
            result
        }
    } else {
        // Use sync executor for SEARCH/WHERE queries (no caching needed - fast enough)
        state
            .store
            .execute_rql_with_search(&query, Some(state.text_index.as_ref()))
            .map_err(|e| ApiError::Internal(format!("Query execution failed: {}", e)))?
    };

    Ok(Json(result.into()))
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

    let query = Query::parse(&request.query)
        .map_err(|e| ApiError::BadRequest(format!("Invalid query: {}", e)))?;

    let (sse_tx, sse_rx) = mpsc::channel::<Event>(32);

    // Non-REASON queries: send a single complete event
    if query.reason.is_none() {
        let result = state
            .store
            .execute_rql_with_search(&query, Some(state.text_index.as_ref()))
            .map_err(|e| ApiError::Internal(format!("Query execution failed: {}", e)))?;
        let response: QueryResponse = result.into();
        let event = Event::default()
            .event("complete")
            .json_data(&response)
            .unwrap_or_else(|_| Event::default().event("complete").data("{}"));
        let _ = sse_tx.send(event).await;
        drop(sse_tx);
    } else {
        let reason_clause = query.reason.as_ref().unwrap().clone();

        // Check cache first
        if let Some(cached) = state.query_cache.get(&reason_clause.query, &query.from.table) {
            tracing::info!(
                "Cache HIT for streaming query '{}' - saved {} LLM calls",
                reason_clause.query,
                cached.llm_calls_saved
            );
            let matches: Vec<DocumentMatch> = cached.matches.iter().map(|m| {
                DocumentMatch {
                    document: reasondb_core::Document::new(m.document_title.clone(), &cached.table_id),
                    score: Some(m.score),
                    matched_nodes: vec![],
                    highlights: m.highlights.clone(),
                    answer: m.answer.clone(),
                    confidence: Some(m.confidence),
                }
            }).collect();
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
            };
            let response: QueryResponse = result.into();
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

                let result = store
                    .execute_rql_async_with_progress(
                        &query_clone,
                        Some(text_index.as_ref()),
                        reasoner,
                        Some(progress_tx),
                    )
                    .await;

                // Wait for forwarder to finish draining
                let _ = forwarder.await;

                match result {
                    Ok(result) => {
                        // Cache the result
                        let cached_matches: Vec<CachedMatch> = result.documents.iter().map(|m| {
                            CachedMatch {
                                document_id: m.document.id.clone(),
                                document_title: m.document.title.clone(),
                                score: m.score.unwrap_or(0.0),
                                answer: m.answer.clone(),
                                confidence: m.confidence.unwrap_or(0.0),
                                highlights: m.highlights.clone(),
                            }
                        }).collect();
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

                        let response: QueryResponse = result.into();
                        let event = Event::default()
                            .event("complete")
                            .json_data(&response)
                            .unwrap_or_else(|_| Event::default().event("complete").data("{}"));
                        let _ = sse_tx_bg.send(event).await;
                    }
                    Err(e) => {
                        let event = Event::default()
                            .event("error")
                            .data(format!("Query execution failed: {}", e));
                        let _ = sse_tx_bg.send(event).await;
                    }
                }
            });
            drop(sse_tx);
        }
    }

    let stream = ReceiverStream::new(sse_rx).map(Ok::<_, Infallible>);
    Ok(Sse::new(stream))
}
