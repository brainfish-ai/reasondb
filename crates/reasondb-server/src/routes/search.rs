//! Search endpoint
//!
//! LLM-guided tree traversal search.

use axum::{extract::State, Json};
use futures::future::join_all;
use reasondb_core::{
    engine::{SearchConfig, SearchEngine},
    llm::ReasoningEngine,
    SearchFilter,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult, ErrorResponse},
    state::AppState,
};

/// Search request
#[derive(Debug, Deserialize, ToSchema)]
pub struct SearchRequest {
    /// The natural language query to search for
    #[schema(example = "What are the key benefits of machine learning?")]
    pub query: String,

    /// Optional document ID to search within (searches all if not provided)
    #[serde(default)]
    #[schema(example = "doc_abc123")]
    pub document_id: Option<String>,

    /// Optional table ID to restrict search to
    #[serde(default)]
    #[schema(example = "tbl_legal")]
    pub table_id: Option<String>,

    /// Filter by document tags (any match)
    #[serde(default)]
    #[schema(example = json!(["nda", "confidential"]))]
    pub tags: Option<Vec<String>>,

    /// Filter by document metadata
    #[serde(default)]
    #[schema(example = json!({"contract_type": "nda"}))]
    pub metadata: Option<std::collections::HashMap<String, serde_json::Value>>,

    /// Maximum tree depth to traverse (default: 10)
    #[serde(default)]
    #[schema(example = 10)]
    pub max_depth: Option<usize>,

    /// Beam width for parallel exploration (default: 3)
    #[serde(default)]
    #[schema(example = 3)]
    pub beam_width: Option<usize>,

    /// Minimum confidence to continue traversal (default: 0.3)
    #[serde(default)]
    #[schema(example = 0.3)]
    pub min_confidence: Option<f32>,

    /// Maximum results to return (default: 10)
    #[serde(default)]
    #[schema(example = 10)]
    pub limit: Option<usize>,
}

/// Search response
#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResponse {
    /// Search results ordered by relevance
    pub results: Vec<SearchResult>,

    /// Search statistics
    pub stats: SearchStats,
}

/// Individual search result
#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResult {
    /// Node ID where content was found
    #[schema(example = "node_xyz789")]
    pub node_id: String,

    /// Node title
    pub title: String,

    /// Document ID containing this result
    #[schema(example = "doc_abc123")]
    pub document_id: String,

    /// Path from root to this node (breadcrumbs)
    pub path: Vec<PathNode>,

    /// The relevant content at this node
    #[schema(example = "Machine learning enables computers to learn from data...")]
    pub content: String,

    /// Confidence score (0.0 to 1.0)
    #[schema(example = 0.85)]
    pub confidence: f32,

    /// Sibling sections that this result node explicitly references inline
    /// (resolved from cross-section references detected during ingestion).
    pub cross_ref_sections: Vec<CrossRefSectionResponse>,
}

/// A sibling section referenced inline by a search result node.
#[derive(Debug, Serialize, ToSchema)]
pub struct CrossRefSectionResponse {
    /// Node ID of the referenced section
    pub node_id: String,
    /// Title of the referenced section
    pub title: String,
    /// Content of the referenced section
    pub content: String,
}

/// Node in the traversal path
#[derive(Debug, Serialize, ToSchema)]
pub struct PathNode {
    /// Node ID
    #[schema(example = "node_abc")]
    pub node_id: String,
    /// Node title
    #[schema(example = "Chapter 3: Machine Learning")]
    pub title: String,
    /// LLM's reasoning for selecting this path
    #[schema(example = "This chapter covers ML fundamentals relevant to the query")]
    pub reasoning: String,
}

/// Search statistics
#[derive(Debug, Serialize, ToSchema)]
pub struct SearchStats {
    /// Total nodes visited during traversal
    #[schema(example = 15)]
    pub nodes_visited: usize,
    /// Nodes pruned (not explored due to low confidence)
    #[schema(example = 8)]
    pub nodes_pruned: usize,
    /// Number of LLM API calls made
    #[schema(example = 7)]
    pub llm_calls: usize,
    /// Total search time in milliseconds
    #[schema(example = 1250)]
    pub total_time_ms: u64,
}

/// Search documents using LLM-guided tree traversal
///
/// Performs an intelligent search across documents using an LLM to navigate
/// the hierarchical tree structure. The LLM evaluates each node's summary
/// to decide which branches to explore, mimicking human reasoning.
#[utoipa::path(
    post,
    path = "/v1/search",
    tag = "search",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Search completed successfully", body = SearchResponse),
        (status = 422, description = "Validation failed", body = ErrorResponse),
        (status = 500, description = "Search failed", body = ErrorResponse),
    )
)]
pub async fn search<R: ReasoningEngine + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Json(request): Json<SearchRequest>,
) -> ApiResult<Json<SearchResponse>> {
    if request.query.is_empty() {
        return Err(ApiError::ValidationError("Query is required".to_string()));
    }

    info!("Searching: {}", request.query);

    // Build search config
    let config = SearchConfig {
        max_depth: request.max_depth.map(|d| d as u8).unwrap_or(10),
        beam_width: request.beam_width.unwrap_or(3),
        min_confidence: request.min_confidence.unwrap_or(0.3),
        ..Default::default()
    };

    // Create search engine
    let engine = SearchEngine::with_config(state.store.clone(), state.reasoner.clone(), config);

    // Execute search
    let start = std::time::Instant::now();

    let response = if let Some(doc_id) = &request.document_id {
        // Search within specific document
        engine
            .search_document(&request.query, doc_id)
            .await
            .map_err(|e| ApiError::SearchError(e.to_string()))?
    } else {
        // Build filter from request
        let mut filter = SearchFilter::new();

        if let Some(table_id) = &request.table_id {
            filter = filter.with_table_id(table_id);
        }

        if let Some(tags) = &request.tags {
            let tags_ref: Vec<&str> = tags.iter().map(|s| s.as_str()).collect();
            filter = filter.with_tags(tags_ref);
        }

        if let Some(metadata) = &request.metadata {
            for (key, value) in metadata {
                filter = filter.with_metadata(key, value.clone());
            }
        }

        // Get matching documents
        let documents = state
            .store
            .find_documents(&filter)
            .map_err(|e| ApiError::StorageError(e.to_string()))?;

        // Search all matching documents in parallel — avoids sequential
        // per-document tree traversal which multiplies latency by doc count.
        let futures: Vec<_> = documents
            .iter()
            .map(|doc| {
                let engine = engine.clone();
                let query = request.query.clone();
                let doc_id = doc.id.clone();
                async move {
                    engine
                        .search_document(&query, &doc_id)
                        .await
                        .map_err(|e| ApiError::SearchError(e.to_string()))
                }
            })
            .collect();

        let responses = join_all(futures).await;

        let mut all_results = Vec::new();
        for resp in responses {
            all_results.extend(resp?.results);
        }

        // Sort by confidence descending so the best results surface first
        all_results.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Apply limit
        if let Some(limit) = request.limit {
            all_results.truncate(limit);
        }

        reasondb_core::engine::SearchResponse {
            results: all_results,
            stats: reasondb_core::engine::TraversalStats::default(),
        }
    };

    let elapsed = start.elapsed();

    debug!(
        "Search complete: {} results in {}ms",
        response.results.len(),
        elapsed.as_millis()
    );

    // Convert to response format
    let results: Vec<SearchResult> = response
        .results
        .into_iter()
        .map(|r| {
            // Get document_id from the node if needed
            let doc_id = state
                .store
                .get_node(&r.node_id)
                .ok()
                .flatten()
                .map(|n| n.document_id)
                .unwrap_or_default();

            SearchResult {
                node_id: r.node_id,
                title: r.title,
                document_id: doc_id,
                path: r
                    .path
                    .into_iter()
                    .enumerate()
                    .map(|(i, title)| PathNode {
                        node_id: format!("path_{}", i),
                        title,
                        reasoning: String::new(),
                    })
                    .collect(),
                content: r.content,
                confidence: r.confidence,
                cross_ref_sections: r
                    .cross_ref_sections
                    .into_iter()
                    .map(|s| CrossRefSectionResponse {
                        node_id: s.node_id,
                        title: s.title,
                        content: s.content,
                    })
                    .collect(),
            }
        })
        .collect();

    // Get stats from response
    let stats = SearchStats {
        nodes_visited: response.stats.nodes_visited,
        nodes_pruned: response.stats.nodes_pruned,
        llm_calls: response.stats.llm_calls,
        total_time_ms: elapsed.as_millis() as u64,
    };

    Ok(Json(SearchResponse { results, stats }))
}
