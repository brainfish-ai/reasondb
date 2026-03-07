//! Document management endpoints
//!
//! List, get, update, and delete documents.

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use reasondb_core::llm::ReasoningEngine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
use tracing::{debug, info};
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult, ErrorResponse},
    state::AppState,
};

/// Document summary for listing
#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentSummary {
    /// Unique document ID
    #[schema(example = "doc_abc123")]
    pub id: String,
    /// Document title
    #[schema(example = "Machine Learning Handbook")]
    pub title: String,
    /// Total nodes in the tree
    #[schema(example = 42)]
    pub total_nodes: usize,
    /// Maximum tree depth
    #[schema(example = 4)]
    pub max_depth: u8,
    /// Original source path or URL
    #[schema(example = "/uploads/ml-handbook.pdf")]
    pub source_path: String,
    /// MIME type of original file
    #[schema(example = "application/pdf")]
    pub mime_type: Option<String>,
    /// Original file size in bytes
    #[schema(example = 2048576)]
    pub file_size: Option<u64>,
    /// Table ID the document belongs to
    #[schema(example = "tbl_legal")]
    pub table_id: Option<String>,
    /// Document tags
    pub tags: Vec<String>,
    /// Creation timestamp (ISO 8601)
    #[schema(example = "2024-01-15T10:30:00Z")]
    pub created_at: String,
}

/// Request to update document metadata
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateDocumentRequest {
    /// Updated title (optional)
    #[schema(example = "NDA Agreement - Updated")]
    pub title: Option<String>,

    /// Table ID to assign to (optional)
    #[schema(example = "tbl_archived")]
    pub table_id: Option<String>,

    /// Document tags (replaces existing tags)
    #[schema(example = json!(["nda", "archived"]))]
    pub tags: Option<Vec<String>>,

    /// Document author
    #[schema(example = "Legal Team")]
    pub author: Option<String>,

    /// Metadata to merge (optional)
    #[serde(default)]
    pub metadata: Option<HashMap<String, Value>>,
}

/// Request to move a document to a different table
#[derive(Debug, Deserialize, ToSchema)]
pub struct MoveDocumentRequest {
    /// Target table ID
    #[schema(example = "tbl_archived")]
    pub table_id: String,
}

/// Full document details
#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentDetail {
    /// Unique document ID
    #[schema(example = "doc_abc123")]
    pub id: String,
    /// Document title
    #[schema(example = "Machine Learning Handbook")]
    pub title: String,
    /// Root node ID of the tree
    #[schema(example = "node_root_abc")]
    pub root_node_id: String,
    /// Total nodes in the tree
    #[schema(example = 42)]
    pub total_nodes: usize,
    /// Maximum tree depth
    #[schema(example = 4)]
    pub max_depth: u8,
    /// Original source path or URL
    #[schema(example = "/uploads/ml-handbook.pdf")]
    pub source_path: String,
    /// MIME type of original file
    #[schema(example = "application/pdf")]
    pub mime_type: Option<String>,
    /// Original file size in bytes
    #[schema(example = 2048576)]
    pub file_size: Option<u64>,
    /// Creation timestamp (ISO 8601)
    #[schema(example = "2024-01-15T10:30:00Z")]
    pub created_at: String,
    /// Last update timestamp (ISO 8601)
    #[schema(example = "2024-01-15T10:35:00Z")]
    pub updated_at: String,
}

/// Node summary for listing
#[derive(Debug, Serialize, ToSchema)]
pub struct NodeSummary {
    /// Unique node ID
    #[schema(example = "node_xyz789")]
    pub id: String,
    /// Node title
    #[schema(example = "Chapter 3: Neural Networks")]
    pub title: String,
    /// LLM-generated summary of this node
    #[schema(example = "This chapter covers the fundamentals of neural networks...")]
    pub summary: String,
    /// Depth in the tree (0 = root)
    #[schema(example = 1)]
    pub depth: u8,
    /// Whether this is a leaf node (no children)
    #[schema(example = false)]
    pub is_leaf: bool,
    /// Number of direct children
    #[schema(example = 5)]
    pub children_count: usize,
}

/// Tree node with nested children
#[derive(Debug, Serialize, ToSchema)]
#[schema(no_recursion)]
pub struct TreeNode {
    /// Unique node ID
    #[schema(example = "node_xyz789")]
    pub id: String,
    /// Node title
    #[schema(example = "Chapter 3: Neural Networks")]
    pub title: String,
    /// LLM-generated summary
    #[schema(example = "This chapter covers neural network fundamentals...")]
    pub summary: String,
    /// Original content (only present for leaf nodes)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(
        example = "Neural networks are computing systems inspired by biological neural networks..."
    )]
    pub content: Option<String>,
    /// Depth in the tree (0 = root)
    #[schema(example = 1)]
    pub depth: u8,
    /// Whether this is a leaf node
    #[schema(example = false)]
    pub is_leaf: bool,
    /// Child nodes (recursive structure)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeNode>,
}

/// List all documents
///
/// Returns a summary of all ingested documents in the database.
#[utoipa::path(
    get,
    path = "/v1/documents",
    tag = "documents",
    responses(
        (status = 200, description = "List of documents", body = Vec<DocumentSummary>),
        (status = 500, description = "Storage error", body = ErrorResponse),
    )
)]
pub async fn list_documents<R: ReasoningEngine + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
) -> ApiResult<Json<Vec<DocumentSummary>>> {
    debug!("Listing all documents");

    let documents = state
        .store
        .list_documents()
        .map_err(|e| ApiError::StorageError(e.to_string()))?;

    let summaries: Vec<DocumentSummary> = documents
        .into_iter()
        .map(|doc| DocumentSummary {
            id: doc.id,
            title: doc.title,
            total_nodes: doc.total_nodes,
            max_depth: doc.max_depth,
            source_path: doc.source_path,
            mime_type: doc.mime_type,
            file_size: doc.file_size,
            table_id: Some(doc.table_id),
            tags: doc.tags,
            created_at: doc.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(summaries))
}

/// Get document details
///
/// Returns full details for a specific document including metadata and tree info.
#[utoipa::path(
    get,
    path = "/v1/documents/{id}",
    tag = "documents",
    params(
        ("id" = String, Path, description = "Document ID")
    ),
    responses(
        (status = 200, description = "Document details", body = DocumentDetail),
        (status = 404, description = "Document not found", body = ErrorResponse),
        (status = 500, description = "Storage error", body = ErrorResponse),
    )
)]
pub async fn get_document<R: ReasoningEngine + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
) -> ApiResult<Json<DocumentDetail>> {
    debug!("Getting document: {}", id);

    let document = state
        .store
        .get_document(&id)
        .map_err(|e| ApiError::StorageError(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Document not found: {}", id)))?;

    Ok(Json(DocumentDetail {
        id: document.id,
        title: document.title,
        root_node_id: document.root_node_id,
        total_nodes: document.total_nodes,
        max_depth: document.max_depth,
        source_path: document.source_path,
        mime_type: document.mime_type,
        file_size: document.file_size,
        created_at: document.created_at.to_rfc3339(),
        updated_at: document.updated_at.to_rfc3339(),
    }))
}

/// Update document metadata, title, or tags (merge patch)
///
/// Merges the provided fields into the document. Only supplied fields are
/// changed; `metadata` is merged key-by-key, not replaced wholesale.
#[utoipa::path(
    patch,
    path = "/v1/documents/{id}",
    tag = "documents",
    params(
        ("id" = String, Path, description = "Document ID")
    ),
    request_body = UpdateDocumentRequest,
    responses(
        (status = 200, description = "Document updated"),
        (status = 404, description = "Document not found", body = ErrorResponse),
        (status = 500, description = "Storage error", body = ErrorResponse),
    )
)]
pub async fn update_document<R: ReasoningEngine + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
    Json(request): Json<UpdateDocumentRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    info!("Updating document: {}", id);

    let mut doc = state
        .store
        .get_document(&id)
        .map_err(|e| ApiError::StorageError(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Document not found: {}", id)))?;

    if let Some(title) = request.title {
        doc.title = title;
    }
    if let Some(table_id) = request.table_id {
        doc.table_id = table_id;
    }
    if let Some(tags) = request.tags {
        doc.tags = tags;
    }
    if let Some(metadata) = request.metadata {
        for (key, value) in metadata {
            doc.metadata.insert(key, value);
        }
    }
    doc.updated_at = Utc::now();

    state
        .store
        .update_document(&doc)
        .map_err(|e| ApiError::StorageError(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "updated": true,
        "document_id": id
    })))
}

/// Delete a document
///
/// Deletes a document and all its associated nodes (cascade delete).
#[utoipa::path(
    delete,
    path = "/v1/documents/{id}",
    tag = "documents",
    params(
        ("id" = String, Path, description = "Document ID to delete")
    ),
    responses(
        (status = 200, description = "Document deleted successfully"),
        (status = 404, description = "Document not found", body = ErrorResponse),
        (status = 500, description = "Storage error", body = ErrorResponse),
    )
)]
pub async fn delete_document<R: ReasoningEngine + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    info!("Deleting document: {}", id);

    // Check if exists
    let _document = state
        .store
        .get_document(&id)
        .map_err(|e| ApiError::StorageError(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Document not found: {}", id)))?;

    // Delete with cascade (deletes document and all nodes)
    state
        .store
        .delete_document(&id)
        .map_err(|e| ApiError::StorageError(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "deleted": true,
        "document_id": id
    })))
}

/// Get all nodes for a document
///
/// Returns a flat list of all nodes in the document tree.
#[utoipa::path(
    get,
    path = "/v1/documents/{id}/nodes",
    tag = "documents",
    params(
        ("id" = String, Path, description = "Document ID")
    ),
    responses(
        (status = 200, description = "List of nodes", body = Vec<NodeSummary>),
        (status = 404, description = "Document not found", body = ErrorResponse),
        (status = 500, description = "Storage error", body = ErrorResponse),
    )
)]
pub async fn get_document_nodes<R: ReasoningEngine + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<NodeSummary>>> {
    debug!("Getting nodes for document: {}", id);

    // Check document exists
    let _document = state
        .store
        .get_document(&id)
        .map_err(|e| ApiError::StorageError(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Document not found: {}", id)))?;

    let nodes = state
        .store
        .get_nodes_for_document(&id)
        .map_err(|e| ApiError::StorageError(e.to_string()))?;

    let summaries: Vec<NodeSummary> = nodes
        .into_iter()
        .map(|node| {
            let is_leaf = node.is_leaf();
            let children_count = node.children_ids.len();
            NodeSummary {
                id: node.id,
                title: node.title,
                summary: node.summary,
                depth: node.depth,
                is_leaf,
                children_count,
            }
        })
        .collect();

    Ok(Json(summaries))
}

/// Get document as tree structure
///
/// Returns the complete hierarchical tree structure with nested children.
#[utoipa::path(
    get,
    path = "/v1/documents/{id}/tree",
    tag = "documents",
    params(
        ("id" = String, Path, description = "Document ID")
    ),
    responses(
        (status = 200, description = "Document tree", body = TreeNode),
        (status = 404, description = "Document not found", body = ErrorResponse),
        (status = 500, description = "Storage error", body = ErrorResponse),
    )
)]
pub async fn get_document_tree<R: ReasoningEngine + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
) -> ApiResult<Json<TreeNode>> {
    debug!("Getting tree for document: {}", id);

    let document = state
        .store
        .get_document(&id)
        .map_err(|e| ApiError::StorageError(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Document not found: {}", id)))?;

    // Get root node
    let root = state
        .store
        .get_node(&document.root_node_id)
        .map_err(|e| ApiError::StorageError(e.to_string()))?
        .ok_or_else(|| ApiError::Internal("Root node not found".to_string()))?;

    // Build tree recursively
    fn build_tree(
        store: &reasondb_core::store::NodeStore,
        node_id: &str,
    ) -> Result<TreeNode, ApiError> {
        let node = store
            .get_node(node_id)
            .map_err(|e| ApiError::StorageError(e.to_string()))?
            .ok_or_else(|| ApiError::Internal(format!("Node not found: {}", node_id)))?;

        let is_leaf = node.is_leaf();
        let children: Vec<TreeNode> = node
            .children_ids
            .iter()
            .map(|child_id| build_tree(store, child_id))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TreeNode {
            id: node.id,
            title: node.title,
            summary: node.summary,
            content: node.content,
            depth: node.depth,
            is_leaf,
            children,
        })
    }

    let tree = build_tree(&state.store, &root.id)?;

    Ok(Json(tree))
}
