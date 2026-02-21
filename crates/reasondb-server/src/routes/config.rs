//! LLM configuration endpoints
//!
//! Runtime management of ingestion and retrieval LLM settings.
//!
//! These routes are specific to `DynamicReasoner` because they need
//! hot-swap access. They are registered separately in `run_server()`.

use axum::{
    extract::State,
    routing::{get, patch, post, put},
    Json, Router,
};
use reasondb_core::llm::{
    config::LlmSettings,
    dynamic::{build_reasoner, DynamicReasoner},
    ReasoningEngine, SummarizationContext,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

/// Partial update for LLM settings (PATCH request body)
#[derive(Debug, Deserialize)]
pub struct PatchLlmSettings {
    pub ingestion: Option<reasondb_core::llm::config::LlmModelConfig>,
    pub retrieval: Option<reasondb_core::llm::config::LlmModelConfig>,
}

/// Build the config sub-router (DynamicReasoner-specific).
///
/// Includes its own CORS layer because these routes are merged after
/// `create_server` has already applied middleware to the main router.
pub fn config_routes(state: Arc<AppState<DynamicReasoner>>) -> Router {
    Router::new()
        .route("/v1/config/llm", get(get_llm_config))
        .route("/v1/config/llm", put(put_llm_config))
        .route("/v1/config/llm", patch(patch_llm_config))
        .route("/v1/config/llm/test", post(test_llm_config))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state)
}

/// GET /v1/config/llm — return current settings (keys masked)
async fn get_llm_config(
    State(state): State<Arc<AppState<DynamicReasoner>>>,
) -> ApiResult<Json<LlmSettings>> {
    let settings = state
        .store
        .get_llm_settings()
        .map_err(|e| ApiError::Internal(format!("Failed to read LLM settings: {}", e)))?;

    match settings {
        Some(s) => Ok(Json(s.masked())),
        None => Err(ApiError::NotFound(
            "LLM settings not configured yet. PUT to /v1/config/llm to initialize.".into(),
        )),
    }
}

/// PUT /v1/config/llm — replace both ingestion and retrieval config
async fn put_llm_config(
    State(state): State<Arc<AppState<DynamicReasoner>>>,
    Json(settings): Json<LlmSettings>,
) -> ApiResult<Json<LlmSettings>> {
    validate_settings(&settings)?;

    let reasoner = state.reasoner.as_ref();
    reasoner.swap_all(&settings).map_err(|e| {
        ApiError::BadRequest(format!("Failed to build reasoner from new config: {}", e))
    })?;

    state.store.set_llm_settings(&settings).map_err(|e| {
        ApiError::Internal(format!("Settings applied but failed to persist: {}", e))
    })?;

    info!(
        ingestion_provider = settings.ingestion.provider,
        retrieval_provider = settings.retrieval.provider,
        "LLM settings updated (both)"
    );

    Ok(Json(settings.masked()))
}

/// PATCH /v1/config/llm — update ingestion and/or retrieval config
async fn patch_llm_config(
    State(state): State<Arc<AppState<DynamicReasoner>>>,
    Json(patch): Json<PatchLlmSettings>,
) -> ApiResult<Json<LlmSettings>> {
    let mut current = state
        .store
        .get_llm_settings()
        .map_err(|e| ApiError::Internal(format!("Failed to read LLM settings: {}", e)))?
        .ok_or_else(|| {
            ApiError::NotFound(
                "No LLM settings configured yet. Use PUT /v1/config/llm to initialize.".into(),
            )
        })?;

    let reasoner = state.reasoner.as_ref();

    if let Some(ingestion) = patch.ingestion {
        let new_r = build_reasoner(&ingestion).map_err(|e| {
            ApiError::BadRequest(format!("Invalid ingestion config: {}", e))
        })?;
        reasoner.swap_ingestion(new_r);
        current.ingestion = ingestion;
        info!(provider = current.ingestion.provider, "Ingestion LLM updated");
    }

    if let Some(retrieval) = patch.retrieval {
        let new_r = build_reasoner(&retrieval).map_err(|e| {
            ApiError::BadRequest(format!("Invalid retrieval config: {}", e))
        })?;
        reasoner.swap_retrieval(new_r);
        current.retrieval = retrieval;
        info!(provider = current.retrieval.provider, "Retrieval LLM updated");
    }

    state.store.set_llm_settings(&current).map_err(|e| {
        ApiError::Internal(format!("Settings applied but failed to persist: {}", e))
    })?;

    Ok(Json(current.masked()))
}

fn validate_settings(settings: &LlmSettings) -> ApiResult<()> {
    settings.ingestion.to_provider().map_err(|e| {
        ApiError::BadRequest(format!("Invalid ingestion provider config: {}", e))
    })?;
    settings.retrieval.to_provider().map_err(|e| {
        ApiError::BadRequest(format!("Invalid retrieval provider config: {}", e))
    })?;
    Ok(())
}

#[derive(Serialize)]
struct LlmTestResult {
    ingestion: LlmTestStatus,
    retrieval: LlmTestStatus,
}

#[derive(Serialize)]
struct LlmTestStatus {
    ok: bool,
    error: Option<String>,
    latency_ms: Option<u64>,
}

/// POST /v1/config/llm/test — test both ingestion and retrieval LLM connectivity
async fn test_llm_config(
    State(state): State<Arc<AppState<DynamicReasoner>>>,
) -> ApiResult<Json<LlmTestResult>> {
    let settings = state
        .store
        .get_llm_settings()
        .map_err(|e| ApiError::Internal(format!("Failed to read LLM settings: {}", e)))?
        .ok_or_else(|| {
            ApiError::NotFound(
                "No LLM settings configured yet. Use PUT /v1/config/llm to initialize.".into(),
            )
        })?;

    let ingestion_reasoner = build_reasoner(&settings.ingestion)
        .map_err(|e| ApiError::BadRequest(format!("Cannot build ingestion reasoner: {}", e)))?;
    let retrieval_reasoner = build_reasoner(&settings.retrieval)
        .map_err(|e| ApiError::BadRequest(format!("Cannot build retrieval reasoner: {}", e)))?;

    let test_ctx = SummarizationContext {
        title: Some("test".into()),
        parent_summary: None,
        depth: 0,
        is_leaf: true,
    };

    let timeout = std::time::Duration::from_secs(15);

    let (ing_result, ret_result) = tokio::join!(
        async {
            let start = std::time::Instant::now();
            let res = tokio::time::timeout(
                timeout,
                ingestion_reasoner.summarize("Say hello in one word.", &test_ctx),
            )
            .await;
            let elapsed = start.elapsed().as_millis() as u64;
            match res {
                Ok(Ok(_)) => LlmTestStatus {
                    ok: true,
                    error: None,
                    latency_ms: Some(elapsed),
                },
                Ok(Err(e)) => LlmTestStatus {
                    ok: false,
                    error: Some(e.to_string()),
                    latency_ms: Some(elapsed),
                },
                Err(_) => LlmTestStatus {
                    ok: false,
                    error: Some("Timed out after 15s".into()),
                    latency_ms: Some(elapsed),
                },
            }
        },
        async {
            let start = std::time::Instant::now();
            let res = tokio::time::timeout(
                timeout,
                retrieval_reasoner.summarize("Say hello in one word.", &test_ctx),
            )
            .await;
            let elapsed = start.elapsed().as_millis() as u64;
            match res {
                Ok(Ok(_)) => LlmTestStatus {
                    ok: true,
                    error: None,
                    latency_ms: Some(elapsed),
                },
                Ok(Err(e)) => LlmTestStatus {
                    ok: false,
                    error: Some(e.to_string()),
                    latency_ms: Some(elapsed),
                },
                Err(_) => LlmTestStatus {
                    ok: false,
                    error: Some("Timed out after 15s".into()),
                    latency_ms: Some(elapsed),
                },
            }
        }
    );

    info!(
        ingestion_ok = ing_result.ok,
        retrieval_ok = ret_result.ok,
        "LLM config test completed"
    );

    Ok(Json(LlmTestResult {
        ingestion: ing_result,
        retrieval: ret_result,
    }))
}
