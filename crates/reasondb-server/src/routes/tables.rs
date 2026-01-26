//! Table management endpoints
//!
//! CRUD operations for tables (collections) that group documents.

use axum::{
    extract::{Path, State},
    Json,
};
use reasondb_core::{llm::ReasoningEngine, Table};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
use tracing::{debug, info};
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult, ErrorResponse},
    state::AppState,
};

/// Request to create a new table
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTableRequest {
    /// Human-readable name for the table
    #[schema(example = "Legal Contracts")]
    pub name: String,

    /// Optional description
    #[schema(example = "All legal documents and contracts")]
    pub description: Option<String>,

    /// Custom metadata (key-value pairs)
    #[schema(example = json!({"department": "legal", "confidential": true}))]
    #[serde(default)]
    pub metadata: Option<HashMap<String, Value>>,
}

/// Request to update a table
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateTableRequest {
    /// Updated name (optional)
    #[schema(example = "Legal Documents")]
    pub name: Option<String>,

    /// Updated description (optional)
    #[schema(example = "All legal documents")]
    pub description: Option<String>,

    /// Metadata to merge (optional)
    #[serde(default)]
    pub metadata: Option<HashMap<String, Value>>,
}

/// Response containing table details
#[derive(Debug, Serialize, ToSchema)]
pub struct TableResponse {
    /// Unique table ID
    #[schema(example = "tbl_abc123")]
    pub id: String,

    /// Table name
    #[schema(example = "Legal Contracts")]
    pub name: String,

    /// Table description
    #[schema(example = "All legal documents and contracts")]
    pub description: Option<String>,

    /// Custom metadata
    pub metadata: HashMap<String, Value>,

    /// Number of documents in this table
    #[schema(example = 15)]
    pub document_count: usize,

    /// Total nodes across all documents
    #[schema(example = 234)]
    pub total_nodes: usize,

    /// Creation timestamp
    #[schema(example = "2026-01-27T10:00:00Z")]
    pub created_at: String,

    /// Last update timestamp
    #[schema(example = "2026-01-27T12:30:00Z")]
    pub updated_at: String,
}

impl From<&Table> for TableResponse {
    fn from(table: &Table) -> Self {
        Self {
            id: table.id.clone(),
            name: table.name.clone(),
            description: table.description.clone(),
            metadata: table.metadata.clone(),
            document_count: table.document_count,
            total_nodes: table.total_nodes,
            created_at: table.created_at.to_rfc3339(),
            updated_at: table.updated_at.to_rfc3339(),
        }
    }
}

/// Summary view of a table (for list endpoint)
#[derive(Debug, Serialize, ToSchema)]
pub struct TableSummary {
    /// Unique table ID
    #[schema(example = "tbl_abc123")]
    pub id: String,

    /// Table name
    #[schema(example = "Legal Contracts")]
    pub name: String,

    /// Number of documents
    #[schema(example = 15)]
    pub document_count: usize,

    /// Total nodes
    #[schema(example = 234)]
    pub total_nodes: usize,
}

impl From<&Table> for TableSummary {
    fn from(table: &Table) -> Self {
        Self {
            id: table.id.clone(),
            name: table.name.clone(),
            document_count: table.document_count,
            total_nodes: table.total_nodes,
        }
    }
}

/// Response for list tables endpoint
#[derive(Debug, Serialize, ToSchema)]
pub struct ListTablesResponse {
    /// List of tables
    pub tables: Vec<TableSummary>,

    /// Total number of tables
    #[schema(example = 5)]
    pub total: usize,
}

/// Request to delete a table
#[derive(Debug, Deserialize, ToSchema)]
pub struct DeleteTableRequest {
    /// If true, delete all documents in the table. If false, move them to default table.
    #[serde(default)]
    #[schema(example = false)]
    pub cascade: bool,
}

// ==================== Endpoints ====================

/// Create a new table
///
/// Creates a new table (collection) to organize documents. Documents can be
/// assigned to tables during ingestion or moved later.
#[utoipa::path(
    post,
    path = "/v1/tables",
    tag = "tables",
    request_body = CreateTableRequest,
    responses(
        (status = 201, description = "Table created successfully", body = TableResponse),
        (status = 422, description = "Validation failed", body = ErrorResponse),
        (status = 500, description = "Failed to create table", body = ErrorResponse),
    )
)]
pub async fn create_table<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Json(request): Json<CreateTableRequest>,
) -> ApiResult<Json<TableResponse>> {
    if request.name.is_empty() {
        return Err(ApiError::ValidationError("Name is required".to_string()));
    }

    info!("Creating table: {}", request.name);

    let mut table = Table::new(request.name);

    if let Some(desc) = request.description {
        table.description = Some(desc);
    }

    if let Some(metadata) = request.metadata {
        table.metadata = metadata;
    }

    state
        .store
        .insert_table(&table)
        .map_err(|e| ApiError::StorageError(e.to_string()))?;

    debug!("Table created: {}", table.id);

    Ok(Json(TableResponse::from(&table)))
}

/// List all tables
///
/// Returns a list of all tables in the database, including document counts.
#[utoipa::path(
    get,
    path = "/v1/tables",
    tag = "tables",
    responses(
        (status = 200, description = "List of tables", body = ListTablesResponse),
        (status = 500, description = "Failed to list tables", body = ErrorResponse),
    )
)]
pub async fn list_tables<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
) -> ApiResult<Json<ListTablesResponse>> {
    let tables = state
        .store
        .list_tables()
        .map_err(|e| ApiError::StorageError(e.to_string()))?;

    let summaries: Vec<TableSummary> = tables.iter().map(TableSummary::from).collect();
    let total = summaries.len();

    Ok(Json(ListTablesResponse {
        tables: summaries,
        total,
    }))
}

/// Get table details
///
/// Returns detailed information about a specific table.
#[utoipa::path(
    get,
    path = "/v1/tables/{id}",
    tag = "tables",
    params(
        ("id" = String, Path, description = "Table ID")
    ),
    responses(
        (status = 200, description = "Table details", body = TableResponse),
        (status = 404, description = "Table not found", body = ErrorResponse),
        (status = 500, description = "Failed to get table", body = ErrorResponse),
    )
)]
pub async fn get_table<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
) -> ApiResult<Json<TableResponse>> {
    let table = state
        .store
        .get_table(&id)
        .map_err(|e| ApiError::StorageError(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Table not found: {}", id)))?;

    Ok(Json(TableResponse::from(&table)))
}

/// Update a table
///
/// Update table metadata, name, or description. Only provided fields are updated.
#[utoipa::path(
    patch,
    path = "/v1/tables/{id}",
    tag = "tables",
    params(
        ("id" = String, Path, description = "Table ID")
    ),
    request_body = UpdateTableRequest,
    responses(
        (status = 200, description = "Table updated", body = TableResponse),
        (status = 404, description = "Table not found", body = ErrorResponse),
        (status = 500, description = "Failed to update table", body = ErrorResponse),
    )
)]
pub async fn update_table<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
    Json(request): Json<UpdateTableRequest>,
) -> ApiResult<Json<TableResponse>> {
    let mut table = state
        .store
        .get_table(&id)
        .map_err(|e| ApiError::StorageError(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Table not found: {}", id)))?;

    info!("Updating table: {}", id);

    if let Some(name) = request.name {
        table.name = name;
    }

    if let Some(desc) = request.description {
        table.description = Some(desc);
    }

    if let Some(metadata) = request.metadata {
        // Merge metadata
        for (key, value) in metadata {
            table.metadata.insert(key, value);
        }
    }

    state
        .store
        .update_table(&table)
        .map_err(|e| ApiError::StorageError(e.to_string()))?;

    Ok(Json(TableResponse::from(&table)))
}

/// Delete a table
///
/// Delete a table. By default, documents in the table are moved to the default table.
/// Set `cascade=true` to delete all documents in the table.
#[utoipa::path(
    delete,
    path = "/v1/tables/{id}",
    tag = "tables",
    params(
        ("id" = String, Path, description = "Table ID"),
        ("cascade" = Option<bool>, Query, description = "Delete documents too")
    ),
    responses(
        (status = 200, description = "Table deleted"),
        (status = 400, description = "Cannot delete default table", body = ErrorResponse),
        (status = 404, description = "Table not found", body = ErrorResponse),
        (status = 500, description = "Failed to delete table", body = ErrorResponse),
    )
)]
pub async fn delete_table<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
    Json(request): Json<Option<DeleteTableRequest>>,
) -> ApiResult<Json<serde_json::Value>> {
    if id == "default" {
        return Err(ApiError::BadRequest(
            "Cannot delete the default table".to_string(),
        ));
    }

    let cascade = request.map(|r| r.cascade).unwrap_or(false);

    info!("Deleting table: {} (cascade: {})", id, cascade);

    let deleted = state
        .store
        .delete_table(&id, cascade)
        .map_err(|e| ApiError::StorageError(e.to_string()))?;

    if !deleted {
        return Err(ApiError::NotFound(format!("Table not found: {}", id)));
    }

    Ok(Json(serde_json::json!({
        "deleted": true,
        "id": id,
        "cascade": cascade
    })))
}

/// Get documents in a table
///
/// Returns all documents assigned to a specific table.
#[utoipa::path(
    get,
    path = "/v1/tables/{id}/documents",
    tag = "tables",
    params(
        ("id" = String, Path, description = "Table ID")
    ),
    responses(
        (status = 200, description = "Documents in table", body = TableDocumentsResponse),
        (status = 404, description = "Table not found", body = ErrorResponse),
        (status = 500, description = "Failed to get documents", body = ErrorResponse),
    )
)]
pub async fn get_table_documents<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
) -> ApiResult<Json<TableDocumentsResponse>> {
    // Verify table exists
    let _ = state
        .store
        .get_table(&id)
        .map_err(|e| ApiError::StorageError(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Table not found: {}", id)))?;

    let documents = state
        .store
        .get_documents_in_table(&id)
        .map_err(|e| ApiError::StorageError(e.to_string()))?;

    let doc_summaries: Vec<TableDocumentSummary> = documents
        .iter()
        .map(|doc| TableDocumentSummary {
            id: doc.id.clone(),
            title: doc.title.clone(),
            total_nodes: doc.total_nodes,
            tags: doc.tags.clone(),
            author: doc.author.clone(),
            created_at: doc.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(TableDocumentsResponse {
        table_id: id,
        documents: doc_summaries,
        total: documents.len(),
    }))
}

/// Response for table documents endpoint
#[derive(Debug, Serialize, ToSchema)]
pub struct TableDocumentsResponse {
    /// Table ID
    #[schema(example = "tbl_abc123")]
    pub table_id: String,

    /// Documents in the table
    pub documents: Vec<TableDocumentSummary>,

    /// Total number of documents
    #[schema(example = 15)]
    pub total: usize,
}

/// Summary of a document in a table
#[derive(Debug, Serialize, ToSchema)]
pub struct TableDocumentSummary {
    /// Document ID
    #[schema(example = "doc_xyz789")]
    pub id: String,

    /// Document title
    #[schema(example = "NDA Agreement")]
    pub title: String,

    /// Number of nodes
    #[schema(example = 12)]
    pub total_nodes: usize,

    /// Document tags
    pub tags: Vec<String>,

    /// Document author
    pub author: Option<String>,

    /// Creation timestamp
    #[schema(example = "2026-01-27T10:00:00Z")]
    pub created_at: String,
}
