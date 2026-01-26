//! Search endpoint
//!
//! LLM-guided tree traversal search.

use axum::{extract::State, Json};
use reasondb_core::{
    engine::{SearchConfig, SearchEngine},
    llm::ReasoningEngine,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

/// Search request
#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    /// The query to search for
    pub query: String,

    /// Optional document ID to search within
    #[serde(default)]
    pub document_id: Option<String>,

    /// Maximum depth to traverse
    #[serde(default)]
    pub max_depth: Option<usize>,

    /// Beam width for parallel exploration
    #[serde(default)]
    pub beam_width: Option<usize>,

    /// Minimum confidence to continue traversal
    #[serde(default)]
    pub min_confidence: Option<f32>,
}

/// Search response
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    /// Search results
    pub results: Vec<SearchResult>,

    /// Search statistics
    pub stats: SearchStats,
}

/// Individual search result
#[derive(Debug, Serialize)]
pub struct SearchResult {
    /// Node ID where content was found
    pub node_id: String,

    /// Document ID
    pub document_id: String,

    /// Path from root to this node
    pub path: Vec<PathNode>,

    /// The relevant content
    pub content: String,

    /// LLM's extracted answer
    pub answer: Option<String>,

    /// Confidence score
    pub confidence: f32,
}

/// Node in the traversal path
#[derive(Debug, Serialize)]
pub struct PathNode {
    pub node_id: String,
    pub title: String,
    pub reasoning: String,
}

/// Search statistics
#[derive(Debug, Serialize)]
pub struct SearchStats {
    pub nodes_visited: usize,
    pub nodes_pruned: usize,
    pub llm_calls: usize,
    pub total_time_ms: u64,
}

/// POST /v1/search - Search documents
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
    let engine = SearchEngine::with_config(
        state.store.clone(),
        state.reasoner.clone(),
        config,
    );

    // Execute search
    let start = std::time::Instant::now();

    let response = if let Some(doc_id) = &request.document_id {
        // Search within specific document
        engine
            .search_document(&request.query, doc_id)
            .await
            .map_err(|e| ApiError::SearchError(e.to_string()))?
    } else {
        // Search all documents - need to iterate over all docs
        let documents = state.store.list_documents()
            .map_err(|e| ApiError::StorageError(e.to_string()))?;

        let mut all_results = Vec::new();
        for doc in documents {
            let doc_response = engine
                .search_document(&request.query, &doc.id)
                .await
                .map_err(|e| ApiError::SearchError(e.to_string()))?;
            all_results.extend(doc_response.results);
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
            let doc_id = state.store.get_node(&r.node_id)
                .ok()
                .flatten()
                .map(|n| n.document_id)
                .unwrap_or_default();

            SearchResult {
                node_id: r.node_id,
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
                answer: r.extracted_answer,
                confidence: r.confidence,
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
