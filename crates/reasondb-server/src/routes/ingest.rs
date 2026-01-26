//! Ingestion endpoints
//!
//! Handle document ingestion from files, text, and URLs.

use axum::{
    extract::{Multipart, State},
    Json,
};
use reasondb_core::llm::ReasoningEngine;
use reasondb_ingest::{IngestPipeline, PipelineConfig};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tempfile::NamedTempFile;
use tracing::{debug, info};

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

/// Response for ingestion operations
#[derive(Debug, Serialize)]
pub struct IngestResponse {
    pub document_id: String,
    pub title: String,
    pub total_nodes: usize,
    pub max_depth: usize,
    pub stats: IngestStats,
}

#[derive(Debug, Serialize)]
pub struct IngestStats {
    pub chars_extracted: usize,
    pub chunks_created: usize,
    pub nodes_created: usize,
    pub summaries_generated: usize,
    pub total_time_ms: u64,
}

impl From<reasondb_ingest::IngestStats> for IngestStats {
    fn from(s: reasondb_ingest::IngestStats) -> Self {
        Self {
            chars_extracted: s.chars_extracted,
            chunks_created: s.chunks_created,
            nodes_created: s.nodes_created,
            summaries_generated: s.summaries_generated,
            total_time_ms: s.total_time_ms,
        }
    }
}

/// Request for text ingestion
#[derive(Debug, Deserialize)]
pub struct IngestTextRequest {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub generate_summaries: Option<bool>,
}

/// Request for URL ingestion
#[derive(Debug, Deserialize)]
pub struct IngestUrlRequest {
    pub url: String,
    #[serde(default)]
    pub generate_summaries: Option<bool>,
}

/// POST /v1/ingest/file - Ingest a file (multipart upload)
pub async fn ingest_file<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    mut multipart: Multipart,
) -> ApiResult<Json<IngestResponse>> {
    // Get the file from multipart
    let mut file_data: Option<(String, Vec<u8>)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("Failed to read multipart: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();

        if name == "file" {
            let filename = field
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".to_string());

            let data = field
                .bytes()
                .await
                .map_err(|e| ApiError::BadRequest(format!("Failed to read file: {}", e)))?;

            if data.len() > state.config.max_upload_size {
                return Err(ApiError::BadRequest(format!(
                    "File too large. Max size: {} bytes",
                    state.config.max_upload_size
                )));
            }

            file_data = Some((filename, data.to_vec()));
        }
    }

    let (filename, data) = file_data.ok_or_else(|| ApiError::BadRequest("No file provided".to_string()))?;

    info!("Ingesting file: {} ({} bytes)", filename, data.len());

    // Write to temp file
    let temp_file = NamedTempFile::new()
        .map_err(|e| ApiError::Internal(format!("Failed to create temp file: {}", e)))?;

    std::fs::write(temp_file.path(), &data)
        .map_err(|e| ApiError::Internal(format!("Failed to write temp file: {}", e)))?;

    // Rename with original extension
    let extension = std::path::Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");

    let temp_path = temp_file.path().with_extension(extension);
    std::fs::rename(temp_file.path(), &temp_path)
        .map_err(|e| ApiError::Internal(format!("Failed to rename temp file: {}", e)))?;

    // Create pipeline and ingest
    let config = PipelineConfig {
        generate_summaries: state.config.generate_summaries,
        store_in_db: true,
        ..Default::default()
    };

    let pipeline = IngestPipeline::new((*state.reasoner).clone())
        .with_config(config);

    let result = pipeline
        .ingest_and_store(&temp_path, &state.store)
        .await
        .map_err(ApiError::from)?;

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_path);

    debug!("Ingestion complete: {} nodes created", result.stats.nodes_created);

    Ok(Json(IngestResponse {
        document_id: result.document.id.clone(),
        title: result.document.title.clone(),
        total_nodes: result.document.total_nodes,
        max_depth: result.document.max_depth as usize,
        stats: result.stats.into(),
    }))
}

/// POST /v1/ingest/text - Ingest raw text or markdown
pub async fn ingest_text<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Json(request): Json<IngestTextRequest>,
) -> ApiResult<Json<IngestResponse>> {
    if request.title.is_empty() {
        return Err(ApiError::ValidationError("Title is required".to_string()));
    }

    if request.content.is_empty() {
        return Err(ApiError::ValidationError("Content is required".to_string()));
    }

    info!("Ingesting text: {} ({} chars)", request.title, request.content.len());

    let generate_summaries = request.generate_summaries.unwrap_or(state.config.generate_summaries);

    let config = PipelineConfig {
        generate_summaries,
        store_in_db: true,
        ..Default::default()
    };

    let pipeline = IngestPipeline::new((*state.reasoner).clone())
        .with_config(config);

    let result = pipeline
        .ingest_text_and_store(&request.title, &request.content, &state.store)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(IngestResponse {
        document_id: result.document.id.clone(),
        title: result.document.title.clone(),
        total_nodes: result.document.total_nodes,
        max_depth: result.document.max_depth as usize,
        stats: result.stats.into(),
    }))
}

/// POST /v1/ingest/url - Ingest from URL
pub async fn ingest_url<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Json(request): Json<IngestUrlRequest>,
) -> ApiResult<Json<IngestResponse>> {
    if request.url.is_empty() {
        return Err(ApiError::ValidationError("URL is required".to_string()));
    }

    // Validate URL
    url::Url::parse(&request.url)
        .map_err(|e| ApiError::ValidationError(format!("Invalid URL: {}", e)))?;

    info!("Ingesting URL: {}", request.url);

    let generate_summaries = request.generate_summaries.unwrap_or(state.config.generate_summaries);

    let config = PipelineConfig {
        generate_summaries,
        store_in_db: true,
        ..Default::default()
    };

    let pipeline = IngestPipeline::new((*state.reasoner).clone())
        .with_config(config);

    let result = pipeline
        .ingest_url_and_store(&request.url, &state.store)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(IngestResponse {
        document_id: result.document.id.clone(),
        title: result.document.title.clone(),
        total_nodes: result.document.total_nodes,
        max_depth: result.document.max_depth as usize,
        stats: result.stats.into(),
    }))
}
