//! Error handling for HTTP API
//!
//! Provides consistent error responses across all endpoints.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;
use utoipa::ToSchema;

/// API error type
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Invalid request: {0}")]
    BadRequest(String),

    #[error("Validation failed: {0}")]
    ValidationError(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Rate limit exceeded: {0}")]
    RateLimited(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Ingestion failed: {0}")]
    IngestionError(String),

    #[error("Search failed: {0}")]
    SearchError(String),

    #[error("LLM error: {0}")]
    LLMError(String),

    #[error("Internal server error: {0}")]
    Internal(String),
}

/// Error response body
#[derive(Debug, Serialize, ToSchema)]
#[schema(example = json!({"error": {"code": "NOT_FOUND", "message": "Resource not found: doc123"}}))]
pub struct ErrorResponse {
    /// Error details
    pub error: ErrorDetail,
}

/// Error details
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    /// Error code (e.g., "NOT_FOUND", "VALIDATION_ERROR")
    #[schema(example = "NOT_FOUND")]
    pub code: String,
    /// Human-readable error message
    #[schema(example = "Document not found: doc123")]
    pub message: String,
    /// Additional error details (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "BAD_REQUEST"),
            ApiError::ValidationError(_) => (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR"),
            ApiError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            ApiError::Forbidden(_) => (StatusCode::FORBIDDEN, "FORBIDDEN"),
            ApiError::RateLimited(_) => (StatusCode::TOO_MANY_REQUESTS, "RATE_LIMITED"),
            ApiError::ServiceUnavailable(_) => {
                (StatusCode::SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE")
            }
            ApiError::StorageError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "STORAGE_ERROR"),
            ApiError::IngestionError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INGESTION_ERROR"),
            ApiError::SearchError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "SEARCH_ERROR"),
            ApiError::LLMError(_) => (StatusCode::SERVICE_UNAVAILABLE, "LLM_ERROR"),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
        };

        let body = ErrorResponse {
            error: ErrorDetail {
                code: code.to_string(),
                message: self.to_string(),
                details: None,
            },
        };

        (status, Json(body)).into_response()
    }
}

// Conversion from core errors
impl From<reasondb_core::error::ReasonError> for ApiError {
    fn from(err: reasondb_core::error::ReasonError) -> Self {
        use reasondb_core::error::ReasonError;
        match err {
            ReasonError::NodeNotFound(id) => ApiError::NotFound(format!("Node not found: {}", id)),
            ReasonError::DocumentNotFound(id) => {
                ApiError::NotFound(format!("Document not found: {}", id))
            }
            ReasonError::TableNotFound(id) => {
                ApiError::NotFound(format!("Table not found: {}", id))
            }
            ReasonError::NotFound(msg) => ApiError::NotFound(msg),
            ReasonError::InvalidOperation(msg) => ApiError::BadRequest(msg),
            ReasonError::Storage(e) => ApiError::StorageError(e.to_string()),
            ReasonError::Serialization(msg) => {
                ApiError::Internal(format!("Serialization: {}", msg))
            }
            ReasonError::Reasoning(msg) => ApiError::LLMError(msg),
            ReasonError::Auth(msg) => ApiError::Unauthorized(msg),
            ReasonError::PermissionDenied(msg) => ApiError::Forbidden(msg),
            ReasonError::Internal(msg) => ApiError::Internal(msg),
            ReasonError::Backup(msg) => ApiError::Internal(format!("Backup error: {}", msg)),
            ReasonError::Config(msg) => ApiError::BadRequest(format!("Config error: {}", msg)),
        }
    }
}

// Conversion from ingest errors
impl From<reasondb_ingest::IngestError> for ApiError {
    fn from(err: reasondb_ingest::IngestError) -> Self {
        ApiError::IngestionError(err.to_string())
    }
}

/// Result type for API handlers
pub type ApiResult<T> = Result<T, ApiError>;
