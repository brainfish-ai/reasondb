//! RQL Query endpoint
//!
//! Execute SQL-like queries against documents.

use axum::{extract::State, Json};
use reasondb_core::llm::ReasoningEngine;
use reasondb_core::rql::{Query, QueryResult, DocumentMatch};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
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

    /// Author
    pub author: Option<String>,

    /// Relevance score (BM25 for SEARCH, confidence for REASON)
    pub score: Option<f32>,

    /// Highlighted snippets
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
            author: m.document.author,
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
    // Parse the query
    let query = Query::parse(&request.query)
        .map_err(|e| ApiError::BadRequest(format!("Invalid query: {}", e)))?;

    // Check if this is a REASON query (needs async LLM execution)
    let result = if query.reason.is_some() {
        // Use async executor for REASON queries (supports hybrid SEARCH + REASON)
        state
            .store
            .execute_rql_async(&query, Some(state.text_index.as_ref()), state.reasoner.clone())
            .await
            .map_err(|e| ApiError::Internal(format!("Query execution failed: {}", e)))?
    } else {
        // Use sync executor for SEARCH/WHERE queries
        state
            .store
            .execute_rql_with_search(&query, Some(state.text_index.as_ref()))
            .map_err(|e| ApiError::Internal(format!("Query execution failed: {}", e)))?
    };

    Ok(Json(result.into()))
}
