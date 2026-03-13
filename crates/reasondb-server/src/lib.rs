//! ReasonDB HTTP Server
//!
//! REST API for the reasoning-native database.
//!
//! # Endpoints
//!
//! ## Ingestion
//! - `POST /v1/tables/:name/ingest/file` - Upload and ingest a file into a table
//! - `POST /v1/tables/:name/ingest/text` - Ingest raw text/markdown into a table
//! - `POST /v1/tables/:name/ingest/batch` - Ingest multiple documents into a table
//! - `POST /v1/tables/:name/ingest/url` - Ingest from URL into a table
//!
//! ## Search
//! - `POST /v1/search` - LLM-guided tree traversal search
//!
//! ## Documents
//! - `GET /v1/documents` - List all documents
//! - `GET /v1/documents/:id` - Get document details
//! - `DELETE /v1/documents/:id` - Delete a document
//! - `GET /v1/documents/:id/nodes` - Get all nodes
//! - `GET /v1/documents/:id/tree` - Get as tree structure
//!
//! ## Authentication
//! - `POST /v1/auth/keys` - Create API key
//! - `GET /v1/auth/keys` - List API keys
//! - `DELETE /v1/auth/keys/:id` - Revoke API key
//!
//! ## Documentation
//! - `GET /swagger-ui` - Interactive API documentation
//! - `GET /api-docs/openapi.json` - OpenAPI specification
//!
//! # Example
//!
//! ```no_run
//! use reasondb_server::{AppState, ServerConfig, create_server};
//! use reasondb_core::{store::NodeStore, llm::provider::{LLMProvider, Reasoner}, TextIndex, ApiKeyStore};
//! use std::sync::Arc;
//! use redb::Database;
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = ServerConfig::default();
//!     let db = Arc::new(Database::create(&config.db_path).unwrap());
//!     let store = NodeStore::open(&config.db_path).unwrap();
//!     let text_index = TextIndex::open("./search_index").unwrap();
//!     let api_key_store = ApiKeyStore::new(db).unwrap();
//!     let reasoner = Reasoner::new(LLMProvider::openai("sk-..."));
//!     let (state, _job_rx) = AppState::new(store, text_index, reasoner, api_key_store, config.clone());
//!     
//!     // Server would be started here
//! }
//! ```

pub mod auth;
pub mod error;
pub mod jobs;
pub mod metrics;
pub mod openapi;
pub mod ratelimit;
pub mod replication;
pub mod routes;
pub mod state;

pub use error::{ApiError, ApiResult, ErrorResponse};
pub use metrics::{init_metrics, metrics_handler, metrics_middleware};
#[cfg(feature = "telemetry")]
pub use metrics::{init_tracing, shutdown_tracing};
pub use openapi::ApiDoc;
pub use ratelimit::{rate_limit_middleware, RateLimitError};
pub use reasondb_core::ratelimit::RateLimitConfig;
pub use routes::create_routes;
pub use state::{AppState, AuthConfig, ClusterNodeConfig, RealAppState, ServerConfig};

use axum::{extract::DefaultBodyLimit, Router};
use reasondb_core::{
    auth::ApiKeyStore,
    llm::{
        config::{LlmModelConfig, LlmOptions, LlmSettings},
        dynamic::DynamicReasoner,
        ReasoningEngine,
    },
    store::NodeStore,
    text_index::TextIndex,
};
use redb::Database;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};
use tracing::{info, warn, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// Create the server with all middleware
pub fn create_server<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    state: Arc<AppState<R>>,
) -> Router {
    use axum::routing::get;

    let mut app = create_routes(state.clone());

    // Add OpenAPI documentation
    app = app.merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()));

    // Add Prometheus metrics endpoint
    app = app.route("/metrics", get(metrics::metrics_handler));

    // Add rate limiting middleware
    if state.config.rate_limit.enabled {
        info!(
            "Rate limiting enabled: {} req/min, {} req/hour, burst: {}",
            state.config.rate_limit.requests_per_minute,
            state.config.rate_limit.requests_per_hour,
            state.config.rate_limit.burst_size
        );
        app = app.layer(axum::middleware::from_fn_with_state(
            state.rate_limit_store.clone(),
            ratelimit::rate_limit_middleware,
        ));
    }

    // Add metrics middleware (before other middleware to capture all requests)
    app = app.layer(axum::middleware::from_fn(metrics::metrics_middleware));

    // Add middleware
    app = app.layer(TraceLayer::new_for_http());
    // RequestBodyLimitLayer caps the total request body at the tower/HTTP level.
    // DefaultBodyLimit overrides axum's per-extractor 2 MB default so that
    // Multipart file uploads up to max_upload_size are accepted.
    app = app.layer(RequestBodyLimitLayer::new(state.config.max_upload_size));
    app = app.layer(DefaultBodyLimit::max(state.config.max_upload_size));

    if state.config.enable_cors {
        app = app.layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );
    }

    app
}

/// Initialize logging with optional verbosity and JSON format
pub fn init_logging(verbose: bool, json: bool) {
    let filter = if verbose {
        EnvFilter::from_default_env()
            .add_directive(Level::DEBUG.into())
            .add_directive("hyper=info".parse().unwrap())
            .add_directive("tower_http=debug".parse().unwrap())
    } else {
        EnvFilter::from_default_env()
            .add_directive(Level::INFO.into())
            .add_directive("hyper=warn".parse().unwrap())
    };

    if json {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().pretty())
            .init();
    }
}

/// Run the ReasonDB server with default configuration from environment
pub async fn run_server() -> anyhow::Result<()> {
    let host = std::env::var("REASONDB_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("REASONDB_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(4444);
    let database =
        std::env::var("REASONDB_PATH").unwrap_or_else(|_| "data/reasondb.redb".to_string());

    init_logging(false, false);

    info!("Starting ReasonDB server v{}", env!("CARGO_PKG_VERSION"));

    // Create data directory if it doesn't exist
    let data_dir = std::path::Path::new(&database)
        .parent()
        .unwrap_or(std::path::Path::new("."));
    if !data_dir.exists() && data_dir != std::path::Path::new(".") {
        std::fs::create_dir_all(data_dir)?;
    }

    // Open database
    info!("Opening database: {}", database);
    let db = Arc::new(Database::create(&database)?);
    let store = NodeStore::open(&database)?;

    // Open or create text index for BM25 search
    let db_path = std::path::Path::new(&database);
    let db_name = db_path.file_stem().unwrap_or_default().to_string_lossy();
    let text_index_path = db_path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join(format!("{}_search_index", db_name));
    info!("Opening text index: {}", text_index_path.display());
    let text_index = TextIndex::open(&text_index_path)?;

    // Create API key store (shares database)
    let api_key_store = ApiKeyStore::new(db)?;

    // Load auth configuration from environment
    let auth_config = AuthConfig::from_env();
    if auth_config.enabled {
        info!("Authentication enabled");
        if auth_config.master_key.is_some() {
            info!("Master key configured");
        }
    } else {
        info!("Authentication disabled (set REASONDB_AUTH_ENABLED=true to enable)");
    }

    // Load rate limit configuration from environment
    let rate_limit_config = reasondb_core::ratelimit::RateLimitConfig::from_env();

    // Load cluster configuration from environment
    let cluster_config = ClusterNodeConfig::from_env();
    if cluster_config.enabled {
        info!("Clustering enabled - Node: {}", cluster_config.node_id);
    }

    // Create server config
    let config = ServerConfig {
        host: host.clone(),
        port,
        db_path: database.clone(),
        max_upload_size: 100 * 1024 * 1024,
        enable_cors: true,
        generate_summaries: true,
        chunk_strategy: std::env::var("REASONDB_CHUNK_STRATEGY")
            .unwrap_or_else(|_| "agentic".to_string()),
        auth: auth_config,
        rate_limit: rate_limit_config,
        cluster: cluster_config,
    };

    let addr = format!("{}:{}", host, port);

    // ------ LLM Configuration ------
    // Priority: 1) DB-persisted settings  2) env var seed  3) error
    let db_settings = store.get_llm_settings().unwrap_or_else(|e| {
        warn!("Failed to load LLM settings from DB: {}", e);
        None
    });

    let settings = if let Some(s) = db_settings {
        info!(
            "Loaded LLM settings from database (ingestion={}/{}, retrieval={}/{})",
            s.ingestion.provider,
            s.ingestion.model.as_deref().unwrap_or("default"),
            s.retrieval.provider,
            s.retrieval.model.as_deref().unwrap_or("default"),
        );
        Some(s)
    } else {
        match llm_settings_from_env() {
            Ok(env_settings) => {
                info!("Seeding LLM settings from environment variables");
                if let Err(e) = store.set_llm_settings(&env_settings) {
                    warn!("Failed to persist initial LLM settings: {}", e);
                }
                Some(env_settings)
            }
            Err(e) => {
                warn!(
                    "No LLM settings found in DB or env vars ({}). \
                     Server will start but LLM features require configuration via PUT /v1/config/llm",
                    e
                );
                None
            }
        }
    };

    let dynamic_reasoner = match &settings {
        Some(s) => {
            info!(
                "LLM ingestion: {} / {}",
                s.ingestion.provider,
                s.ingestion.model.as_deref().unwrap_or("default")
            );
            info!(
                "LLM retrieval: {} / {}",
                s.retrieval.provider,
                s.retrieval.model.as_deref().unwrap_or("default")
            );
            DynamicReasoner::from_settings(s)?
        }
        None => {
            // Placeholder: use a dummy OpenAI config that will fail on actual requests.
            // Users must configure via the API before using LLM features.
            let placeholder = LlmModelConfig {
                provider: "openai".into(),
                api_key: Some("unconfigured".into()),
                model: Some("gpt-4o-mini".into()),
                base_url: None,
                region: None,
                options: LlmOptions::default(),
            };
            let placeholder_settings = LlmSettings {
                ingestion: placeholder.clone(),
                retrieval: placeholder,
            };
            DynamicReasoner::from_settings(&placeholder_settings)?
        }
    };

    let (app_state, job_rx) =
        AppState::new(store, text_index, dynamic_reasoner, api_key_store, config);
    let state = Arc::new(app_state);

    // Restore rate limit state from previous run
    match state.store.load_all_rate_limits() {
        Ok(snapshots) if !snapshots.is_empty() => {
            info!(
                "Restoring {} rate limit entries from database",
                snapshots.len()
            );
            state.rate_limit_store.import_snapshots(&snapshots);
        }
        _ => {}
    }

    // Periodically snapshot rate limit state to redb
    let snapshot_store = state.store.clone();
    let snapshot_rl = state.rate_limit_store.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let snapshots = snapshot_rl.export_snapshots();
            if !snapshots.is_empty() {
                let refs: Vec<(&str, _)> = snapshots
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.clone()))
                    .collect();
                if let Err(e) = snapshot_store.save_rate_limits(&refs) {
                    tracing::warn!("Failed to persist rate limit snapshots: {}", e);
                }
            }
            snapshot_rl.cleanup();
        }
    });

    tokio::spawn(jobs::run_worker(state.clone(), job_rx));

    let mut app = create_server(state.clone());

    // Merge DynamicReasoner-specific config routes
    app = app.merge(routes::config::config_routes(state));

    info!("Server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Build `LlmSettings` from environment variables (initial seed).
///
/// Returns `Err` if `REASONDB_LLM_PROVIDER` is missing or unrecognized,
/// which is normal when the user intends to configure via the API later.
fn llm_settings_from_env() -> anyhow::Result<LlmSettings> {
    let provider_name = std::env::var("REASONDB_LLM_PROVIDER")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    if provider_name.is_empty() {
        anyhow::bail!("REASONDB_LLM_PROVIDER not set");
    }

    let supported = [
        "openai",
        "anthropic",
        "gemini",
        "glm",
        "kimi",
        "ollama",
        "vertex",
        "bedrock",
    ];
    if !supported.contains(&provider_name.as_str()) {
        anyhow::bail!(
            "Unknown LLM provider '{}'. Supported: {}",
            provider_name,
            supported.join(", ")
        );
    }

    let api_key = std::env::var("REASONDB_LLM_API_KEY")
        .ok()
        .filter(|k| !k.is_empty());
    let model = std::env::var("REASONDB_MODEL")
        .ok()
        .filter(|m| !m.is_empty());

    let base_config = LlmModelConfig {
        provider: provider_name,
        api_key,
        model,
        base_url: std::env::var("REASONDB_LLM_BASE_URL")
            .ok()
            .filter(|u| !u.is_empty()),
        region: std::env::var("AWS_REGION")
            .ok()
            .or_else(|| std::env::var("REASONDB_LLM_REGION").ok()),
        options: LlmOptions::default(),
    };

    Ok(LlmSettings {
        ingestion: base_config.clone(),
        retrieval: base_config,
    })
}
