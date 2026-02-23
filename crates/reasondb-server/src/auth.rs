//! Authentication middleware for ReasonDB server
//!
//! Supports:
//! - API key authentication via `Authorization: Bearer <key>` header
//! - API key authentication via `X-API-Key: <key>` header
//! - Optional authentication (for public endpoints)

use axum::{
    http::{header, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use reasondb_core::{ApiKey, KeyPrefix, Permission, Permissions};
use serde::Serialize;

/// Authenticated API key (extracted from request)
#[derive(Debug, Clone)]
pub struct AuthenticatedKey {
    pub key: ApiKey,
}

impl AuthenticatedKey {
    /// Check if the key has a specific permission
    pub fn has_permission(&self, perm: Permission) -> bool {
        self.key.permissions.has(perm)
    }

    /// Require a permission, returning error if not present
    pub fn require_permission(&self, perm: Permission) -> Result<(), AuthError> {
        if self.has_permission(perm) {
            Ok(())
        } else {
            Err(AuthError::PermissionDenied(format!(
                "Missing required permission: {}",
                perm
            )))
        }
    }

    /// Create an anonymous key with full permissions (for when auth is disabled)
    pub fn anonymous() -> Self {
        Self {
            key: ApiKey {
                id: "anonymous".to_string(),
                name: "Anonymous".to_string(),
                key_hash: "".to_string(),
                key_prefix_hint: "".to_string(),
                environment: KeyPrefix::Live,
                permissions: Permissions::all(),
                description: None,
                owner_id: None,
                rate_limit_rpm: None,
                rate_limit_rpd: None,
                created_at: 0,
                last_used_at: None,
                expires_at: None,
                is_active: true,
                usage_count: 0,
            },
        }
    }

    /// Create a master key with full permissions
    pub fn master() -> Self {
        Self {
            key: ApiKey {
                id: "master".to_string(),
                name: "Master Key".to_string(),
                key_hash: "master".to_string(),
                key_prefix_hint: "master".to_string(),
                environment: KeyPrefix::Live,
                permissions: Permissions::all(),
                description: Some("Master administration key".to_string()),
                owner_id: None,
                rate_limit_rpm: None,
                rate_limit_rpd: None,
                created_at: 0,
                last_used_at: None,
                expires_at: None,
                is_active: true,
                usage_count: 0,
            },
        }
    }
}

/// Authentication error
#[derive(Debug)]
pub enum AuthError {
    /// No API key provided
    MissingKey,
    /// Invalid API key format
    InvalidKeyFormat,
    /// API key not found or invalid
    InvalidKey,
    /// API key is expired
    ExpiredKey,
    /// API key is revoked
    RevokedKey,
    /// Permission denied
    PermissionDenied(String),
    /// Internal error
    Internal(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::MissingKey => write!(f, "API key required"),
            AuthError::InvalidKeyFormat => write!(f, "Invalid API key format"),
            AuthError::InvalidKey => write!(f, "Invalid API key"),
            AuthError::ExpiredKey => write!(f, "API key has expired"),
            AuthError::RevokedKey => write!(f, "API key has been revoked"),
            AuthError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            AuthError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AuthError::MissingKey => (
                StatusCode::UNAUTHORIZED,
                "API key required. Use 'Authorization: Bearer <key>' or 'X-API-Key: <key>' header"
                    .to_string(),
            ),
            AuthError::InvalidKeyFormat => (
                StatusCode::UNAUTHORIZED,
                "Invalid API key format".to_string(),
            ),
            AuthError::InvalidKey => (StatusCode::UNAUTHORIZED, "Invalid API key".to_string()),
            AuthError::ExpiredKey => (StatusCode::UNAUTHORIZED, "API key has expired".to_string()),
            AuthError::RevokedKey => (
                StatusCode::UNAUTHORIZED,
                "API key has been revoked".to_string(),
            ),
            AuthError::PermissionDenied(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            AuthError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };

        let body = Json(AuthErrorResponse {
            error: "authentication_error".to_string(),
            message,
        });

        (status, body).into_response()
    }
}

#[derive(Serialize)]
struct AuthErrorResponse {
    error: String,
    message: String,
}

/// Extract API key from request headers
pub fn extract_api_key(parts: &Parts) -> Option<String> {
    // Try Authorization: Bearer <key> first
    if let Some(auth_header) = parts.headers.get(header::AUTHORIZATION) {
        if let Ok(value) = auth_header.to_str() {
            if let Some(key) = value.strip_prefix("Bearer ") {
                return Some(key.trim().to_string());
            }
        }
    }

    // Fall back to X-API-Key header
    if let Some(api_key_header) = parts.headers.get("X-API-Key") {
        if let Ok(value) = api_key_header.to_str() {
            return Some(value.trim().to_string());
        }
    }

    None
}
