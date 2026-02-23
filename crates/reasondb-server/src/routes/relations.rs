//! Document relationship endpoints
//!
//! REST API for creating, reading, and deleting document relationships.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use reasondb_core::llm::ReasoningEngine;
use reasondb_core::{DocumentRelation, RelationType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::state::AppState;

// ==================== Request/Response Types ====================

/// Request to create a new relationship.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRelationRequest {
    /// Source document ID
    pub from_document_id: String,
    /// Target document ID
    pub to_document_id: String,
    /// Type of relationship
    pub relation_type: RelationTypeDto,
    /// Optional note about the relationship
    #[serde(default)]
    pub note: Option<String>,
    /// Optional metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Relationship type for API requests/responses.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelationTypeDto {
    References,
    ReferencedBy,
    FollowsUp,
    FollowedUpBy,
    Supersedes,
    SupersededBy,
    RelatedTo,
    ParentOf,
    ChildOf,
    Custom(String),
}

impl From<RelationType> for RelationTypeDto {
    fn from(rt: RelationType) -> Self {
        match rt {
            RelationType::References => Self::References,
            RelationType::ReferencedBy => Self::ReferencedBy,
            RelationType::FollowsUp => Self::FollowsUp,
            RelationType::FollowedUpBy => Self::FollowedUpBy,
            RelationType::Supersedes => Self::Supersedes,
            RelationType::SupersededBy => Self::SupersededBy,
            RelationType::RelatedTo => Self::RelatedTo,
            RelationType::ParentOf => Self::ParentOf,
            RelationType::ChildOf => Self::ChildOf,
            RelationType::Custom(s) => Self::Custom(s),
        }
    }
}

impl From<RelationTypeDto> for RelationType {
    fn from(dto: RelationTypeDto) -> Self {
        match dto {
            RelationTypeDto::References => Self::References,
            RelationTypeDto::ReferencedBy => Self::ReferencedBy,
            RelationTypeDto::FollowsUp => Self::FollowsUp,
            RelationTypeDto::FollowedUpBy => Self::FollowedUpBy,
            RelationTypeDto::Supersedes => Self::Supersedes,
            RelationTypeDto::SupersededBy => Self::SupersededBy,
            RelationTypeDto::RelatedTo => Self::RelatedTo,
            RelationTypeDto::ParentOf => Self::ParentOf,
            RelationTypeDto::ChildOf => Self::ChildOf,
            RelationTypeDto::Custom(s) => Self::Custom(s),
        }
    }
}

/// Response for a single relation.
#[derive(Debug, Serialize, ToSchema)]
pub struct RelationResponse {
    pub id: String,
    pub from_document_id: String,
    pub to_document_id: String,
    pub relation_type: RelationTypeDto,
    pub note: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: String,
}

impl From<DocumentRelation> for RelationResponse {
    fn from(rel: DocumentRelation) -> Self {
        Self {
            id: rel.id,
            from_document_id: rel.from_document_id,
            to_document_id: rel.to_document_id,
            relation_type: rel.relation_type.into(),
            note: rel.note,
            metadata: rel.metadata,
            created_at: rel.created_at.to_rfc3339(),
        }
    }
}

/// Response for a list of relations.
#[derive(Debug, Serialize, ToSchema)]
pub struct ListRelationsResponse {
    pub relations: Vec<RelationResponse>,
    pub total: usize,
}

/// Response for related documents.
#[derive(Debug, Serialize, ToSchema)]
pub struct RelatedDocumentsResponse {
    pub document_ids: Vec<String>,
    pub total: usize,
}

// ==================== Handlers ====================

/// Create a new document relationship.
///
/// Creates a relationship between two documents. If the relationship is
/// not symmetric, an inverse relationship is automatically created.
#[utoipa::path(
    post,
    path = "/v1/relations",
    request_body = CreateRelationRequest,
    responses(
        (status = 201, description = "Relation created", body = RelationResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Document not found"),
        (status = 409, description = "Relation already exists")
    ),
    tag = "relations"
)]
pub async fn create_relation<R: ReasoningEngine>(
    State(state): State<Arc<AppState<R>>>,
    Json(req): Json<CreateRelationRequest>,
) -> Result<(StatusCode, Json<RelationResponse>), ApiError> {
    let mut rel = DocumentRelation::new(
        req.from_document_id,
        req.to_document_id,
        req.relation_type.into(),
    );
    rel.note = req.note;
    rel.metadata = req.metadata;

    state.store.insert_relation(&rel)?;

    Ok((StatusCode::CREATED, Json(rel.into())))
}

/// Get a relationship by ID.
#[utoipa::path(
    get,
    path = "/v1/relations/{id}",
    params(
        ("id" = String, Path, description = "Relation ID")
    ),
    responses(
        (status = 200, description = "Relation found", body = RelationResponse),
        (status = 404, description = "Relation not found")
    ),
    tag = "relations"
)]
pub async fn get_relation<R: ReasoningEngine>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
) -> Result<Json<RelationResponse>, ApiError> {
    let rel = state
        .store
        .get_relation(&id)?
        .ok_or_else(|| ApiError::NotFound(format!("Relation not found: {}", id)))?;

    Ok(Json(rel.into()))
}

/// Get all relationships for a document.
///
/// Returns all relationships where the document is either the source or target.
#[utoipa::path(
    get,
    path = "/v1/documents/{id}/relations",
    params(
        ("id" = String, Path, description = "Document ID")
    ),
    responses(
        (status = 200, description = "Relations found", body = ListRelationsResponse),
        (status = 404, description = "Document not found")
    ),
    tag = "relations"
)]
pub async fn get_document_relations<R: ReasoningEngine>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
) -> Result<Json<ListRelationsResponse>, ApiError> {
    // Verify document exists
    state
        .store
        .get_document(&id)?
        .ok_or_else(|| ApiError::NotFound(format!("Document not found: {}", id)))?;

    let relations = state.store.get_all_relations(&id)?;
    let total = relations.len();

    Ok(Json(ListRelationsResponse {
        relations: relations.into_iter().map(Into::into).collect(),
        total,
    }))
}

/// Get documents related to a specific document.
///
/// Returns document IDs that are related to the given document,
/// optionally filtered by relationship type.
#[utoipa::path(
    get,
    path = "/v1/documents/{id}/related",
    params(
        ("id" = String, Path, description = "Document ID"),
        ("relation_type" = Option<String>, Query, description = "Filter by relation type")
    ),
    responses(
        (status = 200, description = "Related documents found", body = RelatedDocumentsResponse),
        (status = 404, description = "Document not found")
    ),
    tag = "relations"
)]
pub async fn get_related_documents<R: ReasoningEngine>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
) -> Result<Json<RelatedDocumentsResponse>, ApiError> {
    // Verify document exists
    state
        .store
        .get_document(&id)?
        .ok_or_else(|| ApiError::NotFound(format!("Document not found: {}", id)))?;

    let document_ids = state.store.get_related_documents(&id, None)?;
    let total = document_ids.len();

    Ok(Json(RelatedDocumentsResponse {
        document_ids,
        total,
    }))
}

/// Delete a relationship.
#[utoipa::path(
    delete,
    path = "/v1/relations/{id}",
    params(
        ("id" = String, Path, description = "Relation ID")
    ),
    responses(
        (status = 204, description = "Relation deleted"),
        (status = 404, description = "Relation not found")
    ),
    tag = "relations"
)]
pub async fn delete_relation<R: ReasoningEngine>(
    State(state): State<Arc<AppState<R>>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let deleted = state.store.delete_relation(&id)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(format!("Relation not found: {}", id)))
    }
}

/// Check if two documents are related.
#[utoipa::path(
    get,
    path = "/v1/documents/{id}/related-to/{other_id}",
    params(
        ("id" = String, Path, description = "First document ID"),
        ("other_id" = String, Path, description = "Second document ID")
    ),
    responses(
        (status = 200, description = "Relation status", body = bool)
    ),
    tag = "relations"
)]
pub async fn check_documents_related<R: ReasoningEngine>(
    State(state): State<Arc<AppState<R>>>,
    Path((id, other_id)): Path<(String, String)>,
) -> Result<Json<bool>, ApiError> {
    let related = state.store.are_documents_related(&id, &other_id)?;
    Ok(Json(related))
}
