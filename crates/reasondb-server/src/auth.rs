//! Authentication middleware for ReasonDB server
//!
//! Supports:
//! - API key authentication via `Authorization: Bearer <key>` header
//! - API key authentication via `X-API-Key: <key>` header
//! - Optional authentication (for public endpoints)

use async_trait::async_trait;
use axum::{
    extract::{FromRequestParts, Request, State},
    http::{header, request::Parts, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use reasondb_core::{ApiKey, KeyPrefix, Permission, Permissions};
use serde::Serialize;
use std::sync::Arc;

use crate::state::AppState;

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

/// Axum middleware that enforces API key authentication on all routes when
/// `REASONDB_AUTH_ENABLED=true`.
///
/// Public routes (`/health`, `/metrics`, `/swagger-ui`, `/api-docs`) bypass
/// auth so monitoring and documentation remain accessible without a key.
pub async fn auth_middleware<
    R: reasondb_core::llm::ReasoningEngine + Clone + Send + Sync + 'static,
>(
    State(state): State<Arc<AppState<R>>>,
    request: Request,
    next: Next,
) -> Response {
    // Auth disabled — pass through
    if !state.config.auth.enabled {
        return next.run(request).await;
    }

    // Public paths that never require auth
    let path = request.uri().path().to_owned();
    if path == "/health"
        || path == "/metrics"
        || path.starts_with("/swagger-ui")
        || path.starts_with("/api-docs")
    {
        return next.run(request).await;
    }

    // Extract API key from headers before consuming the request
    let raw_key = {
        let headers = request.headers();
        // Try Authorization: Bearer <key>
        let from_auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer ").map(|s| s.trim().to_string()));
        // Fall back to X-API-Key
        let from_x_key = headers
            .get("X-API-Key")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string());
        from_auth.or(from_x_key)
    };

    let raw_key = match raw_key {
        Some(k) => k,
        None => return AuthError::MissingKey.into_response(),
    };

    // Validate against master key
    if let Some(ref master_key) = state.config.auth.master_key {
        if raw_key == *master_key {
            return next.run(request).await;
        }
    }

    // Validate against stored API keys
    match state.api_key_store.authenticate(&raw_key) {
        Ok(Some(key)) if key.is_active => next.run(request).await,
        Ok(Some(_)) => AuthError::RevokedKey.into_response(),
        Ok(None) => AuthError::InvalidKey.into_response(),
        Err(e) => AuthError::Internal(e.to_string()).into_response(),
    }
}

/// `FromRequestParts` impl so handlers can optionally extract an
/// `AuthenticatedKey` without the middleware (used by auth management routes).
#[async_trait]
impl<R> FromRequestParts<Arc<AppState<R>>> for AuthenticatedKey
where
    R: reasondb_core::llm::ReasoningEngine + Clone + Send + Sync + 'static,
{
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState<R>>,
    ) -> Result<Self, Self::Rejection> {
        if !state.config.auth.enabled {
            return Ok(AuthenticatedKey::anonymous());
        }

        let raw_key = extract_api_key(parts).ok_or(AuthError::MissingKey)?;

        if let Some(ref master_key) = state.config.auth.master_key {
            if raw_key == *master_key {
                return Ok(AuthenticatedKey::master());
            }
        }

        let key = state
            .api_key_store
            .authenticate(&raw_key)
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .ok_or(AuthError::InvalidKey)?;

        if !key.is_active {
            return Err(AuthError::RevokedKey);
        }

        Ok(AuthenticatedKey { key })
    }
}
