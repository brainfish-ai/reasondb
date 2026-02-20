//! Job status endpoints
//!
//! Query background ingestion job progress and results.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use reasondb_core::llm::ReasoningEngine;
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    error::{ApiError, ApiResult},
    jobs::JobStatusResponse,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct ListJobsQuery {
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

/// Get status of a specific job
pub async fn get_job<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(job_id): Path<String>,
) -> ApiResult<Json<JobStatusResponse>> {
    state
        .job_queue
        .get_status(&job_id)
        .map(Json)
        .ok_or_else(|| ApiError::NotFound(format!("Job '{}' not found", job_id)))
}

/// List recent jobs
pub async fn list_jobs<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Query(params): Query<ListJobsQuery>,
) -> Json<Vec<JobStatusResponse>> {
    let limit = params.limit.min(200);
    Json(state.job_queue.list_jobs(limit))
}
