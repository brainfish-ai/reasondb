//! Document management endpoints
//!
//! List, get, update, and delete documents.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::Utc;
use reasondb_core::llm::ReasoningEngine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
use tracing::{debug, info, warn};
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult, ErrorResponse},
    jobs::JobRequest,
    routes::ingest::IngestTextRequest,
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

/// Partial update for node metadata fields
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateNodeMetadataRequest {
    /// Page number in the source document (optional)
    #[schema(example = 3)]
    pub page_number: Option<u32>,

    /// Section type (e.g. "chapter", "section", "paragraph") (optional)
    #[schema(example = "section")]
    pub section_type: Option<String>,

    /// Confidence score assigned during ingestion (optional)
    #[schema(example = 0.95)]
    pub confidence_score: Option<f32>,

    /// Custom key-value attributes to merge into the node (optional)
    #[schema(example = json!({"source": "manual", "reviewed": "true"}))]
    pub attributes: Option<HashMap<String, String>>,

    /// IDs of sibling nodes this node explicitly cross-references (optional)
    #[schema(example = json!(["node_abc", "node_def"]))]
    pub cross_ref_node_ids: Option<Vec<String>>,
}

/// Request to update a document node
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateNodeRequest {
    /// Updated title (optional)
    #[schema(example = "Chapter 3: Neural Networks — Revised")]
    pub title: Option<String>,

    /// Updated LLM summary for tree traversal (optional)
    #[schema(
        example = "This chapter covers neural network fundamentals including backpropagation."
    )]
    pub summary: Option<String>,

    /// Updated leaf content (optional, only meaningful on leaf nodes)
    #[schema(
        example = "Neural networks are computing systems loosely inspired by biological neural networks..."
    )]
    pub content: Option<String>,

    /// Updated path to associated image (optional)
    #[schema(example = "/uploads/figure-3-1.png")]
    pub image_path: Option<String>,

    /// Metadata fields to merge/update (optional)
    pub metadata: Option<UpdateNodeMetadataRequest>,
}

/// Response for a single updated node
#[derive(Debug, Serialize, ToSchema)]
pub struct NodeDetail {
    /// Unique node ID
    #[schema(example = "node_xyz789")]
    pub id: String,
    /// Parent document ID
    #[schema(example = "doc_abc123")]
    pub document_id: String,
    /// Node title
    #[schema(example = "Chapter 3: Neural Networks")]
    pub title: String,
    /// LLM-generated summary
    #[schema(example = "This chapter covers neural network fundamentals...")]
    pub summary: String,
    /// Depth in the tree (0 = root)
    #[schema(example = 1)]
    pub depth: u8,
    /// Whether this is a leaf node
    #[schema(example = false)]
    pub is_leaf: bool,
    /// Actual content (only present for leaf nodes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Path to associated image
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_path: Option<String>,
    /// Parent node ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Child node IDs
    pub children_ids: Vec<String>,
    /// Page number in source document
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_number: Option<u32>,
    /// Section type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section_type: Option<String>,
    /// Confidence score
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_score: Option<f32>,
    /// Custom attributes
    pub attributes: std::collections::HashMap<String, String>,
    /// Cross-referenced sibling node IDs
    pub cross_ref_node_ids: Vec<String>,
    /// Creation timestamp (ISO 8601)
    #[schema(example = "2024-01-15T10:30:00Z")]
    pub created_at: String,
    /// Last update timestamp (ISO 8601)
    #[schema(example = "2024-01-15T10:35:00Z")]
    pub updated_at: String,
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
    /// IDs of sibling nodes this node explicitly cross-references
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub cross_ref_node_ids: Vec<String>,
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

    let documents = state.store.list_documents().map_err(ApiError::from)?;

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
        .map_err(ApiError::from)?
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
        .map_err(ApiError::from)?
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

    state.store.update_document(&doc).map_err(ApiError::from)?;

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
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Document not found: {}", id)))?;

    // Delete with cascade (deletes document and all nodes)
    state.store.delete_document(&id).map_err(ApiError::from)?;

    // Remove from BM25 text index
    state
        .text_index
        .delete_document(&id)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    state
        .text_index
        .commit()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

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
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Document not found: {}", id)))?;

    let nodes = state
        .store
        .get_nodes_for_document(&id)
        .map_err(ApiError::from)?;

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

/// Update a document node
///
/// Partially updates a single node identified by its document and node IDs.
/// Only provided fields are changed. The `metadata.attributes` map is merged
/// key-by-key (not replaced wholesale). `cross_ref_node_ids` is replaced when provided.
#[utoipa::path(
    patch,
    path = "/v1/documents/{id}/nodes/{node_id}",
    tag = "documents",
    params(
        ("id" = String, Path, description = "Document ID"),
        ("node_id" = String, Path, description = "Node ID"),
    ),
    request_body = UpdateNodeRequest,
    responses(
        (status = 200, description = "Node updated", body = NodeDetail),
        (status = 404, description = "Document or node not found", body = ErrorResponse),
        (status = 500, description = "Storage error", body = ErrorResponse),
    )
)]
pub async fn update_node<R: ReasoningEngine + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path((doc_id, node_id)): Path<(String, String)>,
    Json(request): Json<UpdateNodeRequest>,
) -> ApiResult<Json<NodeDetail>> {
    info!("Updating node {} in document {}", node_id, doc_id);

    // Verify document exists
    let _doc = state
        .store
        .get_document(&doc_id)
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Document not found: {}", doc_id)))?;

    let mut node = state
        .store
        .get_node(&node_id)
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Node not found: {}", node_id)))?;

    if node.document_id != doc_id {
        return Err(ApiError::NotFound(format!(
            "Node {} does not belong to document {}",
            node_id, doc_id
        )));
    }

    if let Some(title) = request.title {
        node.title = title;
    }
    if let Some(summary) = request.summary {
        node.summary = summary;
    }
    if let Some(content) = request.content {
        node.content = Some(content);
    }
    if let Some(image_path) = request.image_path {
        node.image_path = Some(image_path);
    }

    if let Some(meta) = request.metadata {
        if let Some(page_number) = meta.page_number {
            node.metadata.page_number = Some(page_number);
        }
        if let Some(section_type) = meta.section_type {
            node.metadata.section_type = Some(section_type);
        }
        if let Some(confidence_score) = meta.confidence_score {
            node.metadata.confidence_score = Some(confidence_score);
        }
        if let Some(attributes) = meta.attributes {
            for (k, v) in attributes {
                node.metadata.attributes.insert(k, v);
            }
        }
        if let Some(cross_ref_node_ids) = meta.cross_ref_node_ids {
            node.metadata.cross_ref_node_ids = cross_ref_node_ids;
        }
    }

    node.updated_at = Utc::now();

    state.store.update_node(&node).map_err(ApiError::from)?;

    let is_leaf = node.is_leaf();
    Ok(Json(NodeDetail {
        id: node.id,
        document_id: node.document_id,
        title: node.title,
        summary: node.summary,
        depth: node.depth,
        is_leaf,
        content: node.content,
        image_path: node.image_path,
        parent_id: node.parent_id,
        children_ids: node.children_ids,
        page_number: node.metadata.page_number,
        section_type: node.metadata.section_type,
        confidence_score: node.metadata.confidence_score,
        attributes: node.metadata.attributes,
        cross_ref_node_ids: node.metadata.cross_ref_node_ids,
        created_at: node.created_at.to_rfc3339(),
        updated_at: node.updated_at.to_rfc3339(),
    }))
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
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Document not found: {}", id)))?;

    // Get root node
    let root = state
        .store
        .get_node(&document.root_node_id)
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::Internal("Root node not found".to_string()))?;

    // Build tree recursively
    fn build_tree(
        store: &reasondb_core::store::NodeStore,
        node_id: &str,
    ) -> Result<TreeNode, ApiError> {
        let node = store
            .get_node(node_id)
            .map_err(ApiError::from)?
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
            cross_ref_node_ids: node.metadata.cross_ref_node_ids,
            children,
        })
    }

    let tree = build_tree(&state.store, &root.id)?;

    Ok(Json(tree))
}

// ==================== Migrate ====================

/// Per-document result from a bulk migration
#[derive(Debug, Serialize, ToSchema)]
pub struct MigrateResult {
    /// Document ID
    pub document_id: String,
    /// Number of nodes rewritten in the new storage format
    pub nodes_migrated: usize,
    /// Error message if this document could not be migrated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Migrate a single document's nodes to the current storage format
///
/// Reads all existing nodes (handling old bincode format via shadow-struct fallback)
/// and rewrites them in the current rmp-serde format. All summaries, content, and
/// tree structure are preserved exactly — no pipeline re-run, no LLM calls.
///
/// Use this after upgrading ReasonDB to repair nodes that were serialized with an
/// older schema version.
#[utoipa::path(
    post,
    path = "/v1/documents/{id}/migrate",
    tag = "documents",
    params(("id" = String, Path, description = "Document ID")),
    responses(
        (status = 200, description = "Nodes migrated", body = MigrateResult),
        (status = 404, description = "Document not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn migrate_document<R: ReasoningEngine + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
) -> ApiResult<Json<MigrateResult>> {
    info!("Migrating document nodes: {}", id);

    let _doc = state
        .store
        .get_document(&id)
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Document not found: {}", id)))?;

    let nodes = state
        .store
        .get_nodes_for_document(&id)
        .map_err(ApiError::from)?;

    let count = nodes.len();
    for node in &nodes {
        state.store.update_node(node).map_err(ApiError::from)?;
    }

    // Rewrite the document record itself (bincode → rmp-serde)
    let doc = state
        .store
        .get_document(&id)
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Document not found: {}", id)))?;
    state.store.update_document(&doc).map_err(ApiError::from)?;

    info!("Migrated {} nodes for document {}", count, id);

    Ok(Json(MigrateResult {
        document_id: id,
        nodes_migrated: count,
        error: None,
    }))
}

/// Migrate all documents to the current storage format
///
/// Reads every node across all documents and rewrites them in the current
/// rmp-serde format. All summaries, content, and tree structure are preserved.
/// Safe to run multiple times (idempotent).
#[utoipa::path(
    post,
    path = "/v1/documents/migrate",
    tag = "documents",
    responses(
        (status = 200, description = "All nodes migrated"),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn migrate_all_documents<R: ReasoningEngine + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
) -> ApiResult<Json<serde_json::Value>> {
    info!("Migrating all documents to current storage format");

    let docs = state.store.list_documents().map_err(ApiError::from)?;
    let total = docs.len();
    let mut results: Vec<MigrateResult> = Vec::with_capacity(total);

    for doc in &docs {
        let doc_id = doc.id.clone();

        let nodes = match state
            .store
            .get_nodes_for_document(&doc_id)
            .map_err(ApiError::from)
        {
            Ok(n) => n,
            Err(e) => {
                warn!("Failed to read nodes for {}: {}", doc_id, e);
                results.push(MigrateResult {
                    document_id: doc_id,
                    nodes_migrated: 0,
                    error: Some(e.to_string()),
                });
                continue;
            }
        };

        let count = nodes.len();
        let mut node_err: Option<String> = None;
        for node in &nodes {
            if let Err(e) = state.store.update_node(node).map_err(ApiError::from) {
                warn!(
                    "Failed to rewrite node {} for doc {}: {}",
                    node.id, doc_id, e
                );
                node_err = Some(e.to_string());
                break;
            }
        }

        // Rewrite the document record itself
        if node_err.is_none() {
            if let Err(e) = state.store.update_document(doc).map_err(ApiError::from) {
                node_err = Some(e.to_string());
            }
        }

        results.push(MigrateResult {
            document_id: doc_id,
            nodes_migrated: if node_err.is_none() { count } else { 0 },
            error: node_err,
        });
    }

    let migrated_docs = results.iter().filter(|r| r.error.is_none()).count();
    let total_nodes: usize = results.iter().map(|r| r.nodes_migrated).sum();
    let failed = results.iter().filter(|r| r.error.is_some()).count();

    info!(
        "Migration complete: {}/{} documents, {} nodes total",
        migrated_docs, total, total_nodes
    );

    Ok(Json(serde_json::json!({
        "total_documents": total,
        "migrated_documents": migrated_docs,
        "total_nodes_migrated": total_nodes,
        "failed": failed,
        "results": results,
    })))
}

// ==================== Resync ====================

/// Per-document result from a bulk resync
#[derive(Debug, Serialize, ToSchema)]
pub struct ResyncResult {
    /// Original document ID (before deletion)
    pub document_id: String,
    /// Job ID for the re-ingestion (poll /v1/jobs/:id for progress)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    /// Error message if this document could not be resynced
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Query parameters for the bulk resync endpoint
#[derive(Debug, Deserialize)]
pub struct ResyncAllParams {
    /// Filter to a specific table (ignored when document_ids is provided)
    pub table_id: Option<String>,
}

/// Optional request body for the bulk resync endpoint
#[derive(Debug, Deserialize, Default, ToSchema)]
pub struct ResyncBulkBody {
    /// Explicit list of document IDs to resync (resync all/table-filtered if absent)
    #[serde(default)]
    pub document_ids: Option<Vec<String>>,
}

/// Re-ingest a single document
///
/// Deletes the document and all its nodes, then re-ingests from the stored
/// source content including full LLM summarization. Returns a job ID to poll
/// for progress.
#[utoipa::path(
    post,
    path = "/v1/documents/{id}/resync",
    tag = "documents",
    params(("id" = String, Path, description = "Document ID")),
    responses(
        (status = 202, description = "Resync job queued", body = ResyncResult),
        (status = 404, description = "Document not found", body = ErrorResponse),
        (status = 422, description = "No source content available for re-ingestion", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn resync_document<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
) -> ApiResult<Json<ResyncResult>> {
    info!("Resyncing document: {}", id);

    let doc = state
        .store
        .get_document(&id)
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("Document not found: {}", id)))?;

    let content = resolve_resync_content(&state, &doc).await?;

    let title = doc.title.clone();
    let table_id = doc.table_id.clone();
    let tags = doc.tags.clone();

    state.store.delete_document(&id).map_err(ApiError::from)?;
    state
        .text_index
        .delete_document(&id)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    state
        .text_index
        .commit()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let job_id = state
        .job_queue
        .enqueue(JobRequest::Text(IngestTextRequest {
            title,
            content,
            table_id,
            tags: Some(tags),
            metadata: None,
            generate_summaries: Some(state.config.generate_summaries),
            chunk_strategy: None,
        }))
        .map_err(ApiError::Internal)?;

    Ok(Json(ResyncResult {
        document_id: id,
        job_id: Some(job_id),
        error: None,
    }))
}

/// Re-ingest documents (all, or filtered by table)
///
/// Deletes each document and its nodes, then re-ingests from stored source
/// content including full LLM summarization. Optionally filter to a specific
/// table via `?table_id=`. Returns a list of job IDs to poll for progress.
#[utoipa::path(
    post,
    path = "/v1/documents/resync",
    tag = "documents",
    params(
        ("table_id" = Option<String>, Query, description = "Limit resync to a specific table")
    ),
    responses(
        (status = 202, description = "Resync jobs queued"),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn resync_all_documents<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Query(params): Query<ResyncAllParams>,
    body: Option<Json<ResyncBulkBody>>,
) -> ApiResult<Json<serde_json::Value>> {
    let body = body.map(|b| b.0).unwrap_or_default();
    info!(
        "Resyncing documents (ids={:?}, table_id={:?})",
        body.document_ids, params.table_id
    );

    let all_docs = state.store.list_documents().map_err(ApiError::from)?;

    let docs: Vec<_> = if let Some(ids) = &body.document_ids {
        // Explicit ID list takes priority
        let id_set: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
        all_docs
            .into_iter()
            .filter(|d| id_set.contains(d.id.as_str()))
            .collect()
    } else {
        match &params.table_id {
            Some(tid) => all_docs
                .into_iter()
                .filter(|d| &d.table_id == tid)
                .collect(),
            None => all_docs,
        }
    };

    let total = docs.len();
    let mut results: Vec<ResyncResult> = Vec::with_capacity(total);

    for doc in docs {
        let doc_id = doc.id.clone();
        let content = match resolve_resync_content(&state, &doc).await {
            Ok(c) => c,
            Err(e) => {
                warn!("Skipping resync for {}: {}", doc_id, e);
                results.push(ResyncResult {
                    document_id: doc_id,
                    job_id: None,
                    error: Some(e.to_string()),
                });
                continue;
            }
        };

        let title = doc.title.clone();
        let table_id = doc.table_id.clone();
        let tags = doc.tags.clone();

        if let Err(e) = state.store.delete_document(&doc_id).map_err(ApiError::from) {
            warn!("Failed to delete {} before resync: {}", doc_id, e);
            results.push(ResyncResult {
                document_id: doc_id,
                job_id: None,
                error: Some(e.to_string()),
            });
            continue;
        }
        // Best-effort: remove stale BM25 entries for this document
        let _ = state.text_index.delete_document(&doc_id);

        match state
            .job_queue
            .enqueue(JobRequest::Text(IngestTextRequest {
                title,
                content,
                table_id,
                tags: Some(tags),
                metadata: None,
                generate_summaries: Some(state.config.generate_summaries),
                chunk_strategy: None,
            }))
            .map_err(ApiError::Internal)
        {
            Ok(job_id) => results.push(ResyncResult {
                document_id: doc_id,
                job_id: Some(job_id),
                error: None,
            }),
            Err(e) => results.push(ResyncResult {
                document_id: doc_id,
                job_id: None,
                error: Some(e.to_string()),
            }),
        }
    }

    // Commit all BM25 deletions in one pass
    let _ = state.text_index.commit();

    let succeeded = results.iter().filter(|r| r.job_id.is_some()).count();
    let failed = results.iter().filter(|r| r.error.is_some()).count();

    Ok(Json(serde_json::json!({
        "total": total,
        "queued": succeeded,
        "failed": failed,
        "results": results,
    })))
}

/// Resolve the content to re-ingest for a document.
///
/// Priority:
/// 1. `source_content` stored on the document (set for all ingestions after this release)
/// 2. Reconstruct by concatenating leaf node content from the tree
async fn resolve_resync_content<R: ReasoningEngine + Send + Sync + 'static>(
    state: &AppState<R>,
    doc: &reasondb_core::model::Document,
) -> ApiResult<String> {
    // Use stored content if present (all docs ingested after this release)
    if let Some(ref content) = doc.source_content {
        if !content.is_empty() {
            return Ok(content.clone());
        }
    }

    // Reconstruct from leaf nodes (docs ingested before this release)
    let nodes = state
        .store
        .get_nodes_for_document(&doc.id)
        .map_err(ApiError::from)?;

    let mut leaf_nodes: Vec<_> = nodes.into_iter().filter(|n| n.is_leaf()).collect();
    leaf_nodes.sort_by_key(|n| n.start_index);

    let content: String = leaf_nodes
        .iter()
        .filter_map(|n| n.content.as_deref())
        .collect::<Vec<_>>()
        .join("\n\n");

    if content.is_empty() {
        return Err(ApiError::BadRequest(format!(
            "Document '{}' has no stored content and no readable leaf nodes to reconstruct from",
            doc.id
        )));
    }

    Ok(content)
}
