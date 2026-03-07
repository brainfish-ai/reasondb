//! API route definitions
//!
//! All routes are versioned under `/v1/`.

pub mod auth;
pub mod backup;
pub mod cluster;
pub mod config;
pub mod documents;
pub mod ingest;
pub mod jobs;
pub mod plugins;
pub mod query;
pub mod relations;
pub mod search;
pub mod tables;

use axum::{
    routing::{delete, get, patch, post},
    Json, Router,
};
use reasondb_core::llm::ReasoningEngine;
use serde::Serialize;
use std::sync::Arc;

use crate::state::AppState;

/// Create all API routes
pub fn create_routes<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    state: Arc<AppState<R>>,
) -> Router {
    Router::new()
        // Health check
        .route("/health", get(health_check))
        // API v1
        .nest("/v1", v1_routes(state))
}

/// V1 API routes
fn v1_routes<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    state: Arc<AppState<R>>,
) -> Router {
    Router::new()
        // Tables
        .route("/tables", post(tables::create_table::<R>))
        .route("/tables", get(tables::list_tables::<R>))
        .route("/tables/:id", get(tables::get_table::<R>))
        .route("/tables/:id", patch(tables::update_table::<R>))
        .route("/tables/:id", delete(tables::delete_table::<R>))
        .route(
            "/tables/:id/documents",
            get(tables::get_table_documents::<R>),
        )
        .route(
            "/tables/:id/schema/metadata",
            get(tables::get_table_metadata_schema::<R>),
        )
        .route(
            "/tables/:id/values/:column",
            get(tables::get_column_values::<R>),
        )
        // Ingestion (table name in URL path)
        .route(
            "/tables/:table_name/ingest/file",
            post(ingest::ingest_file_for_table::<R>),
        )
        .route(
            "/tables/:table_name/ingest/text",
            post(ingest::ingest_text_for_table::<R>),
        )
        .route(
            "/tables/:table_name/ingest/batch",
            post(ingest::ingest_batch_for_table::<R>),
        )
        .route(
            "/tables/:table_name/ingest/url",
            post(ingest::ingest_url_for_table::<R>),
        )
        // Jobs
        .route("/jobs", get(jobs::list_jobs::<R>))
        .route("/jobs/:id", get(jobs::get_job::<R>))
        // Search
        .route("/search", post(search::search::<R>))
        // RQL Query
        .route("/query", post(query::execute_query::<R>))
        .route("/query/stream", post(query::execute_query_stream::<R>))
        .route("/query/validate", post(query::validate_query::<R>))
        // Query Traces
        .route("/tables/:id/traces", get(query::list_traces::<R>))
        .route("/tables/:id/traces/:trace_id", get(query::get_trace::<R>))
        // Documents
        .route("/documents", get(documents::list_documents::<R>))
        .route("/documents/:id", get(documents::get_document::<R>))
        .route("/documents/:id", patch(documents::update_document::<R>))
        .route("/documents/:id", delete(documents::delete_document::<R>))
        .route(
            "/documents/:id/nodes",
            get(documents::get_document_nodes::<R>),
        )
        .route(
            "/documents/:id/tree",
            get(documents::get_document_tree::<R>),
        )
        // Document Relations
        .route(
            "/documents/:id/relations",
            get(relations::get_document_relations::<R>),
        )
        .route(
            "/documents/:id/related",
            get(relations::get_related_documents::<R>),
        )
        .route(
            "/documents/:id/related-to/:other_id",
            get(relations::check_documents_related::<R>),
        )
        // Relations
        .route("/relations", post(relations::create_relation::<R>))
        .route("/relations/:id", get(relations::get_relation::<R>))
        .route("/relations/:id", delete(relations::delete_relation::<R>))
        // Authentication & API Keys
        .route("/auth/keys", post(auth::create_key::<R>))
        .route("/auth/keys", get(auth::list_keys::<R>))
        .route("/auth/keys/:id", get(auth::get_key::<R>))
        .route("/auth/keys/:id", delete(auth::revoke_key::<R>))
        .route("/auth/keys/:id/rotate", post(auth::rotate_key::<R>))
        // Cluster
        .nest("/cluster", cluster::cluster_routes::<R>())
        // Backup & Recovery
        .nest("", backup::routes::<R>())
        // Plugins
        .route("/plugins", get(plugins::list_plugins::<R>))
        .route("/plugins/:name", get(plugins::get_plugin::<R>))
        .route("/plugins/:name/test", post(plugins::test_plugin::<R>))
        // State
        .with_state(state)
}

/// Health check response
#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

/// Health check endpoint
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}
