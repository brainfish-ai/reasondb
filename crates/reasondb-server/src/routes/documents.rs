//! Document management endpoints
//!
//! List, get, and delete documents.

use axum::{
    extract::{Path, State},
    Json,
};
use reasondb_core::llm::ReasoningEngine;
use serde::Serialize;
use std::sync::Arc;
use tracing::{debug, info};

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

/// Document summary for listing
#[derive(Debug, Serialize)]
pub struct DocumentSummary {
    pub id: String,
    pub title: String,
    pub total_nodes: usize,
    pub max_depth: u8,
    pub source_path: String,
    pub mime_type: Option<String>,
    pub file_size: Option<u64>,
    pub created_at: String,
}

/// Full document with root node info
#[derive(Debug, Serialize)]
pub struct DocumentDetail {
    pub id: String,
    pub title: String,
    pub root_node_id: String,
    pub total_nodes: usize,
    pub max_depth: u8,
    pub source_path: String,
    pub mime_type: Option<String>,
    pub file_size: Option<u64>,
    pub created_at: String,
    pub updated_at: String,
}

/// Node summary for listing
#[derive(Debug, Serialize)]
pub struct NodeSummary {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub depth: u8,
    pub is_leaf: bool,
    pub children_count: usize,
}

/// Tree node with children
#[derive(Debug, Serialize)]
pub struct TreeNode {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub depth: u8,
    pub is_leaf: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeNode>,
}

/// GET /v1/documents - List all documents
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
            created_at: doc.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(summaries))
}

/// GET /v1/documents/:id - Get document details
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

/// DELETE /v1/documents/:id - Delete a document
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

/// GET /v1/documents/:id/nodes - Get all nodes for a document
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

/// GET /v1/documents/:id/tree - Get document as tree structure
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
            depth: node.depth,
            is_leaf,
            children,
        })
    }

    let tree = build_tree(&state.store, &root.id)?;

    Ok(Json(tree))
}
