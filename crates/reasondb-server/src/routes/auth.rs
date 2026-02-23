//! Authentication routes for API key management
//!
//! Endpoints:
//! - `POST /v1/auth/keys` - Create a new API key
//! - `GET /v1/auth/keys` - List all API keys
//! - `GET /v1/auth/keys/:id` - Get API key details
//! - `DELETE /v1/auth/keys/:id` - Revoke an API key
//! - `POST /v1/auth/keys/:id/rotate` - Rotate an API key

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use reasondb_core::{ApiKey, KeyPrefix, Permission, Permissions, ReasoningEngine};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::auth::AuthenticatedKey;
use crate::error::ApiError;
use crate::state::AppState;

/// Request to create a new API key
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateKeyRequest {
    /// Human-readable name for the key
    pub name: String,

    /// Environment (live or test)
    #[serde(default = "default_environment")]
    pub environment: String,

    /// Permissions to grant (defaults to all except admin)
    #[serde(default)]
    pub permissions: Option<Vec<String>>,

    /// Optional description
    pub description: Option<String>,

    /// Rate limit: requests per minute (None = default 60)
    pub rate_limit_rpm: Option<u32>,

    /// Rate limit: requests per day (None = default 10000)
    pub rate_limit_rpd: Option<u32>,

    /// Expiration time in days (None = never expires)
    pub expires_in_days: Option<u32>,
}

fn default_environment() -> String {
    "test".to_string()
}

/// Response after creating an API key
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateKeyResponse {
    /// Key ID
    pub id: String,

    /// The raw API key (ONLY shown once!)
    pub key: String,

    /// Key prefix hint (for identification)
    pub key_prefix_hint: String,

    /// Name
    pub name: String,

    /// Environment
    pub environment: String,

    /// Permissions
    pub permissions: Vec<String>,

    /// When the key expires (if set)
    pub expires_at: Option<i64>,

    /// Warning message
    pub warning: String,
}

/// API key info (for responses)
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiKeyInfo {
    /// Key ID
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// First 12 chars of key for identification
    pub key_prefix_hint: String,
    /// Environment (live or test)
    pub environment: String,
    /// Granted permissions
    pub permissions: Vec<String>,
    /// Optional description
    pub description: Option<String>,
    /// Rate limit: requests per minute
    pub rate_limit_rpm: Option<u32>,
    /// Rate limit: requests per day
    pub rate_limit_rpd: Option<u32>,
    /// When the key was created (Unix millis)
    pub created_at: i64,
    /// When the key was last used (Unix millis)
    pub last_used_at: Option<i64>,
    /// When the key expires (Unix millis)
    pub expires_at: Option<i64>,
    /// Whether the key is active
    pub is_active: bool,
    /// Total number of uses
    pub usage_count: u64,
}

impl From<&ApiKey> for ApiKeyInfo {
    fn from(key: &ApiKey) -> Self {
        Self {
            id: key.id.clone(),
            name: key.name.clone(),
            key_prefix_hint: key.key_prefix_hint.clone(),
            environment: format!("{:?}", key.environment).to_lowercase(),
            permissions: key
                .permissions
                .to_vec()
                .iter()
                .map(|p| p.to_string())
                .collect(),
            description: key.description.clone(),
            rate_limit_rpm: key.rate_limit_rpm,
            rate_limit_rpd: key.rate_limit_rpd,
            created_at: key.created_at,
            last_used_at: key.last_used_at,
            expires_at: key.expires_at,
            is_active: key.is_active,
            usage_count: key.usage_count,
        }
    }
}

/// Response for listing API keys
#[derive(Debug, Serialize, ToSchema)]
pub struct ListKeysResponse {
    pub keys: Vec<ApiKeyInfo>,
    pub total: usize,
}

/// Response for a single API key
#[derive(Debug, Serialize, ToSchema)]
pub struct KeyResponse {
    pub key: ApiKeyInfo,
}

/// Helper to authenticate and check admin permission
fn authenticate_admin<R: ReasoningEngine>(
    state: &AppState<R>,
    raw_key: Option<String>,
) -> Result<AuthenticatedKey, ApiError> {
    // If auth is disabled, allow everything
    if !state.config.auth.enabled {
        return Ok(AuthenticatedKey::anonymous());
    }

    let raw_key = raw_key.ok_or_else(|| {
        ApiError::Unauthorized(
            "API key required. Use 'Authorization: Bearer <key>' or 'X-API-Key: <key>' header"
                .to_string(),
        )
    })?;

    // Check master key
    if let Some(ref master_key) = state.config.auth.master_key {
        if raw_key == *master_key {
            return Ok(AuthenticatedKey::master());
        }
    }

    // Authenticate against store
    let key = state
        .api_key_store
        .authenticate(&raw_key)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::Unauthorized("Invalid API key".to_string()))?;

    let auth = AuthenticatedKey { key };

    // Check admin permission
    if !auth.has_permission(Permission::Admin) {
        return Err(ApiError::Forbidden("Admin permission required".to_string()));
    }

    Ok(auth)
}

/// Create a new API key
///
/// Requires admin permission or master key.
#[utoipa::path(
    post,
    path = "/v1/auth/keys",
    request_body = CreateKeyRequest,
    responses(
        (status = 201, description = "API key created", body = CreateKeyResponse),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Admin permission required"),
    ),
    tag = "Authentication"
)]
pub async fn create_key<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreateKeyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Build Parts for key extraction
    let raw_key = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").map(|s| s.trim().to_string()))
        .or_else(|| {
            headers
                .get("X-API-Key")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
        });

    authenticate_admin(&state, raw_key)?;

    // Parse environment
    let environment = match req.environment.to_lowercase().as_str() {
        "live" | "production" | "prod" => KeyPrefix::Live,
        _ => KeyPrefix::Test,
    };

    // Parse permissions
    let permissions = if let Some(perm_strs) = req.permissions {
        let perms: Vec<Permission> = perm_strs.iter().filter_map(|s| s.parse().ok()).collect();
        if perms.is_empty() {
            Permissions::default_user()
        } else {
            Permissions::new(perms)
        }
    } else {
        Permissions::default_user()
    };

    // Generate the key
    let (mut key, raw_key) = ApiKey::generate(req.name.clone(), environment, permissions);

    // Apply optional settings
    key.description = req.description;
    if let Some(rpm) = req.rate_limit_rpm {
        key.rate_limit_rpm = Some(rpm);
    }
    if let Some(rpd) = req.rate_limit_rpd {
        key.rate_limit_rpd = Some(rpd);
    }
    if let Some(days) = req.expires_in_days {
        let expires_at =
            chrono::Utc::now().timestamp_millis() + (days as i64 * 24 * 60 * 60 * 1000);
        key.expires_at = Some(expires_at);
    }

    // Store the key
    state
        .api_key_store
        .insert(&key)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let response = CreateKeyResponse {
        id: key.id.clone(),
        key: raw_key,
        key_prefix_hint: key.key_prefix_hint.clone(),
        name: key.name.clone(),
        environment: format!("{:?}", key.environment).to_lowercase(),
        permissions: key
            .permissions
            .to_vec()
            .iter()
            .map(|p| p.to_string())
            .collect(),
        expires_at: key.expires_at,
        warning: "⚠️ Save this key now! It will not be shown again.".to_string(),
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// List all API keys
///
/// Requires admin permission.
#[utoipa::path(
    get,
    path = "/v1/auth/keys",
    responses(
        (status = 200, description = "List of API keys", body = ListKeysResponse),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Admin permission required"),
    ),
    tag = "Authentication"
)]
pub async fn list_keys<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<ListKeysResponse>, ApiError> {
    let raw_key = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").map(|s| s.trim().to_string()))
        .or_else(|| {
            headers
                .get("X-API-Key")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
        });

    authenticate_admin(&state, raw_key)?;

    let keys_metadata = state
        .api_key_store
        .list()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let keys: Vec<ApiKeyInfo> = keys_metadata
        .iter()
        .map(|m| ApiKeyInfo {
            id: m.id.clone(),
            name: m.name.clone(),
            key_prefix_hint: m.key_prefix_hint.clone(),
            environment: format!("{:?}", m.environment).to_lowercase(),
            permissions: m
                .permissions
                .to_vec()
                .iter()
                .map(|p| p.to_string())
                .collect(),
            description: m.description.clone(),
            rate_limit_rpm: m.rate_limit_rpm,
            rate_limit_rpd: m.rate_limit_rpd,
            created_at: m.created_at,
            last_used_at: m.last_used_at,
            expires_at: m.expires_at,
            is_active: m.is_active,
            usage_count: m.usage_count,
        })
        .collect();

    let total = keys.len();

    Ok(Json(ListKeysResponse { keys, total }))
}

/// Get API key details
#[utoipa::path(
    get,
    path = "/v1/auth/keys/{id}",
    params(
        ("id" = String, Path, description = "API key ID")
    ),
    responses(
        (status = 200, description = "API key details", body = KeyResponse),
        (status = 404, description = "Key not found"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Admin permission required"),
    ),
    tag = "Authentication"
)]
pub async fn get_key<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<KeyResponse>, ApiError> {
    let raw_key = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").map(|s| s.trim().to_string()))
        .or_else(|| {
            headers
                .get("X-API-Key")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
        });

    authenticate_admin(&state, raw_key)?;

    let key = state
        .api_key_store
        .get(&id)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("API key not found: {}", id)))?;

    Ok(Json(KeyResponse {
        key: ApiKeyInfo::from(&key),
    }))
}

/// Revoke an API key
#[utoipa::path(
    delete,
    path = "/v1/auth/keys/{id}",
    params(
        ("id" = String, Path, description = "API key ID")
    ),
    responses(
        (status = 204, description = "Key revoked"),
        (status = 404, description = "Key not found"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Admin permission required"),
    ),
    tag = "Authentication"
)]
pub async fn revoke_key<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let raw_key = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").map(|s| s.trim().to_string()))
        .or_else(|| {
            headers
                .get("X-API-Key")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
        });

    authenticate_admin(&state, raw_key)?;

    state.api_key_store.revoke(&id).map_err(|e| match e {
        reasondb_core::ReasonError::NotFound(_) => {
            ApiError::NotFound(format!("API key not found: {}", id))
        }
        _ => ApiError::Internal(e.to_string()),
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Rotate an API key (revoke old, create new with same settings)
#[utoipa::path(
    post,
    path = "/v1/auth/keys/{id}/rotate",
    params(
        ("id" = String, Path, description = "API key ID")
    ),
    responses(
        (status = 200, description = "Key rotated", body = CreateKeyResponse),
        (status = 404, description = "Key not found"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Admin permission required"),
    ),
    tag = "Authentication"
)]
pub async fn rotate_key<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<CreateKeyResponse>, ApiError> {
    let raw_key = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").map(|s| s.trim().to_string()))
        .or_else(|| {
            headers
                .get("X-API-Key")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
        });

    authenticate_admin(&state, raw_key)?;

    // Get the old key
    let old_key = state
        .api_key_store
        .get(&id)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("API key not found: {}", id)))?;

    // Create new key with same settings
    let (mut new_key, raw_key) = ApiKey::generate(
        old_key.name.clone(),
        old_key.environment,
        old_key.permissions.clone(),
    );
    new_key.description = old_key.description.clone();
    new_key.owner_id = old_key.owner_id.clone();
    new_key.rate_limit_rpm = old_key.rate_limit_rpm;
    new_key.rate_limit_rpd = old_key.rate_limit_rpd;
    new_key.expires_at = old_key.expires_at;

    // Store new key
    state
        .api_key_store
        .insert(&new_key)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Revoke old key
    state
        .api_key_store
        .revoke(&id)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let response = CreateKeyResponse {
        id: new_key.id.clone(),
        key: raw_key,
        key_prefix_hint: new_key.key_prefix_hint.clone(),
        name: new_key.name.clone(),
        environment: format!("{:?}", new_key.environment).to_lowercase(),
        permissions: new_key
            .permissions
            .to_vec()
            .iter()
            .map(|p| p.to_string())
            .collect(),
        expires_at: new_key.expires_at,
        warning: "⚠️ Save this key now! The old key has been revoked.".to_string(),
    };

    Ok(Json(response))
}
