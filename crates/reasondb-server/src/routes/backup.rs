//! Backup API endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use reasondb_core::{
    BackupInfo, BackupManager, BackupOptions, BackupType, ExportFormat, ExportOptions, ExportScope,
    Exporter, ImportOptions, Importer, RestoreOptions,
};

use reasondb_core::llm::ReasoningEngine;

use crate::error::ApiError;
use crate::state::AppState;

/// Create backup routes
pub fn routes<R: ReasoningEngine + Clone + Send + Sync + 'static>() -> Router<Arc<AppState<R>>> {
    Router::new()
        .route("/backup", post(create_backup::<R>))
        .route("/backup", get(list_backups::<R>))
        .route("/backup/:id", get(get_backup::<R>))
        .route("/backup/:id", delete(delete_backup::<R>))
        .route("/backup/:id/verify", post(verify_backup::<R>))
        .route("/backup/:id/restore", post(restore_backup::<R>))
        .route("/backup/prune", post(prune_backups::<R>))
        .route("/export", post(export_data::<R>))
        .route("/import", post(import_data::<R>))
}

// Request/Response types

#[derive(Debug, Deserialize)]
pub struct CreateBackupRequest {
    /// Backup type (full or incremental)
    #[serde(default = "default_backup_type")]
    pub backup_type: String,
    /// Optional description
    pub description: Option<String>,
    /// Whether to verify after creation
    #[serde(default = "default_verify")]
    pub verify: bool,
}

fn default_backup_type() -> String {
    "full".to_string()
}

fn default_verify() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct BackupResponse {
    pub id: String,
    pub backup_type: String,
    pub created_at: String,
    pub table_count: usize,
    pub document_count: usize,
    pub node_count: usize,
    pub size_bytes: u64,
    pub description: Option<String>,
}

impl From<BackupInfo> for BackupResponse {
    fn from(info: BackupInfo) -> Self {
        Self {
            id: info.id,
            backup_type: format!("{:?}", info.backup_type),
            created_at: info.created_at.to_rfc3339(),
            table_count: info.table_count,
            document_count: info.document_count,
            node_count: info.node_count,
            size_bytes: info.size_bytes,
            description: info.description,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RestoreRequest {
    /// Target database path
    pub target_path: String,
    /// Overwrite existing
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Deserialize)]
pub struct PruneRequest {
    /// Number of full backups to keep
    #[serde(default = "default_keep_full")]
    pub keep_full: usize,
    /// Number of incremental backups to keep
    #[serde(default = "default_keep_incremental")]
    pub keep_incremental: usize,
}

fn default_keep_full() -> usize {
    5
}

fn default_keep_incremental() -> usize {
    10
}

#[derive(Debug, Serialize)]
pub struct PruneResponse {
    pub deleted_count: usize,
    pub deleted: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    /// Output file path
    pub output_path: String,
    /// Export format (json, jsonl, csv)
    #[serde(default = "default_export_format")]
    pub format: String,
    /// Specific table to export
    pub table_id: Option<String>,
    /// Include nodes in export
    #[serde(default)]
    pub include_nodes: bool,
}

fn default_export_format() -> String {
    "json".to_string()
}

#[derive(Debug, Serialize)]
pub struct ExportResponse {
    pub output_path: String,
    pub format: String,
    pub table_count: usize,
    pub document_count: usize,
    pub node_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    /// Input file path
    pub input_path: String,
    /// Import format (json, jsonl, csv)
    #[serde(default = "default_export_format")]
    pub format: String,
    /// Target table for CSV/JSONL imports
    pub target_table_id: Option<String>,
    /// Update existing records
    #[serde(default)]
    pub update_existing: bool,
}

#[derive(Debug, Serialize)]
pub struct ImportResponse {
    pub tables_imported: usize,
    pub tables_skipped: usize,
    pub documents_imported: usize,
    pub documents_skipped: usize,
    pub nodes_imported: usize,
    pub nodes_skipped: usize,
}

#[derive(Debug, Deserialize)]
pub struct BackupQueryParams {
    pub backup_dir: Option<String>,
}

// Handlers

async fn create_backup<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Query(params): Query<BackupQueryParams>,
    Json(request): Json<CreateBackupRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let backup_dir = params.backup_dir.unwrap_or_else(|| "backups".to_string());

    let manager = BackupManager::new(&state.store, &backup_dir)
        .map_err(|e| ApiError::Internal(format!("Failed to initialize backup manager: {}", e)))?;

    let backup_type = match request.backup_type.to_lowercase().as_str() {
        "full" => BackupType::Full,
        "incremental" | "incr" => BackupType::Incremental,
        _ => return Err(ApiError::BadRequest("Invalid backup type".to_string())),
    };

    let options = BackupOptions {
        backup_type,
        description: request.description,
        compression_level: 6,
        verify: request.verify,
    };

    let backup = manager
        .create_backup(&state.store, options)
        .map_err(|e| ApiError::Internal(format!("Failed to create backup: {}", e)))?;

    Ok((StatusCode::CREATED, Json(BackupResponse::from(backup))))
}

async fn list_backups<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Query(params): Query<BackupQueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let backup_dir = params.backup_dir.unwrap_or_else(|| "backups".to_string());

    let manager = BackupManager::new(&state.store, &backup_dir)
        .map_err(|e| ApiError::Internal(format!("Failed to initialize backup manager: {}", e)))?;

    let backups = manager
        .list_backups()
        .map_err(|e| ApiError::Internal(format!("Failed to list backups: {}", e)))?;

    let responses: Vec<BackupResponse> = backups.into_iter().map(BackupResponse::from).collect();

    Ok(Json(responses))
}

async fn get_backup<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
    Query(params): Query<BackupQueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let backup_dir = params.backup_dir.unwrap_or_else(|| "backups".to_string());

    let manager = BackupManager::new(&state.store, &backup_dir)
        .map_err(|e| ApiError::Internal(format!("Failed to initialize backup manager: {}", e)))?;

    let backup = manager
        .get_backup(&id)
        .map_err(|e| ApiError::Internal(format!("Failed to get backup: {}", e)))?
        .ok_or_else(|| ApiError::NotFound(format!("Backup not found: {}", id)))?;

    Ok(Json(BackupResponse::from(backup)))
}

async fn delete_backup<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
    Query(params): Query<BackupQueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let backup_dir = params.backup_dir.unwrap_or_else(|| "backups".to_string());

    let manager = BackupManager::new(&state.store, &backup_dir)
        .map_err(|e| ApiError::Internal(format!("Failed to initialize backup manager: {}", e)))?;

    manager
        .delete_backup(&id)
        .map_err(|e| ApiError::Internal(format!("Failed to delete backup: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

async fn verify_backup<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
    Query(params): Query<BackupQueryParams>,
) -> Result<impl IntoResponse, ApiError> {
    let backup_dir = params.backup_dir.unwrap_or_else(|| "backups".to_string());

    let manager = BackupManager::new(&state.store, &backup_dir)
        .map_err(|e| ApiError::Internal(format!("Failed to initialize backup manager: {}", e)))?;

    let valid = manager
        .verify(&id)
        .map_err(|e| ApiError::Internal(format!("Failed to verify backup: {}", e)))?;

    Ok(Json(serde_json::json!({ "backup_id": id, "valid": valid })))
}

async fn restore_backup<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
    Query(params): Query<BackupQueryParams>,
    Json(request): Json<RestoreRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let backup_dir = params.backup_dir.unwrap_or_else(|| "backups".to_string());

    let manager = BackupManager::new(&state.store, &backup_dir)
        .map_err(|e| ApiError::Internal(format!("Failed to initialize backup manager: {}", e)))?;

    let options = RestoreOptions {
        overwrite: request.overwrite,
        verify_first: true,
    };

    manager
        .restore(&id, &request.target_path, options)
        .map_err(|e| ApiError::Internal(format!("Failed to restore backup: {}", e)))?;

    Ok(Json(serde_json::json!({
        "status": "restored",
        "backup_id": id,
        "target_path": request.target_path
    })))
}

async fn prune_backups<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Query(params): Query<BackupQueryParams>,
    Json(request): Json<PruneRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let backup_dir = params.backup_dir.unwrap_or_else(|| "backups".to_string());

    let manager = BackupManager::new(&state.store, &backup_dir)
        .map_err(|e| ApiError::Internal(format!("Failed to initialize backup manager: {}", e)))?;

    let deleted = manager
        .prune(request.keep_full, request.keep_incremental)
        .map_err(|e| ApiError::Internal(format!("Failed to prune backups: {}", e)))?;

    let deleted_ids: Vec<String> = deleted.iter().map(|b| b.id.clone()).collect();

    Ok(Json(PruneResponse {
        deleted_count: deleted_ids.len(),
        deleted: deleted_ids,
    }))
}

async fn export_data<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Json(request): Json<ExportRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let format = match request.format.to_lowercase().as_str() {
        "json" => ExportFormat::Json,
        "jsonl" | "jsonlines" => ExportFormat::JsonLines,
        "csv" => ExportFormat::Csv,
        _ => return Err(ApiError::BadRequest("Invalid format".to_string())),
    };

    let options = ExportOptions {
        format,
        scope: if request.table_id.is_some() {
            ExportScope::Table
        } else {
            ExportScope::All
        },
        table_id: request.table_id,
        include_nodes: request.include_nodes,
        pretty: true,
    };

    let metadata = Exporter::export(&state.store, &request.output_path, options)
        .map_err(|e| ApiError::Internal(format!("Failed to export: {}", e)))?;

    Ok(Json(ExportResponse {
        output_path: request.output_path,
        format: metadata.format,
        table_count: metadata.table_count,
        document_count: metadata.document_count,
        node_count: metadata.node_count,
    }))
}

async fn import_data<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Json(request): Json<ImportRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let format = match request.format.to_lowercase().as_str() {
        "json" => ExportFormat::Json,
        "jsonl" | "jsonlines" => ExportFormat::JsonLines,
        "csv" => ExportFormat::Csv,
        _ => return Err(ApiError::BadRequest("Invalid format".to_string())),
    };

    // CSV and JSONL require a target table
    if (format == ExportFormat::Csv || format == ExportFormat::JsonLines)
        && request.target_table_id.is_none()
    {
        return Err(ApiError::BadRequest(
            "CSV and JSONL imports require a target_table_id".to_string(),
        ));
    }

    let options = ImportOptions {
        format,
        skip_existing: !request.update_existing,
        update_existing: request.update_existing,
        target_table_id: request.target_table_id,
    };

    let result = Importer::import(&state.store, &request.input_path, options)
        .map_err(|e| ApiError::Internal(format!("Failed to import: {}", e)))?;

    Ok(Json(ImportResponse {
        tables_imported: result.tables_imported,
        tables_skipped: result.tables_skipped,
        documents_imported: result.documents_imported,
        documents_skipped: result.documents_skipped,
        nodes_imported: result.nodes_imported,
        nodes_skipped: result.nodes_skipped,
    }))
}
