//! ReasonDB HTTP Server
//!
//! REST API for the reasoning-native database.
//!
//! # Endpoints
//!
//! ## Ingestion
//! - `POST /v1/ingest/file` - Upload and ingest a file
//! - `POST /v1/ingest/text` - Ingest raw text/markdown
//! - `POST /v1/ingest/batch` - Ingest multiple documents in one request
//! - `POST /v1/ingest/url` - Ingest from URL
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
pub use metrics::{init_metrics, init_tracing, metrics_handler, metrics_middleware, shutdown_tracing};
pub use openapi::ApiDoc;
pub use ratelimit::{rate_limit_middleware, RateLimitError};
pub use routes::create_routes;
pub use state::{AppState, AuthConfig, ClusterNodeConfig, RealAppState, ServerConfig};
pub use reasondb_core::ratelimit::RateLimitConfig;

use axum::Router;
use reasondb_core::{
    auth::ApiKeyStore,
    llm::{provider::{LLMProvider, Reasoner}, ReasoningEngine},
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
use tracing::{info, Level};
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
        info!("Rate limiting enabled: {} req/min, {} req/hour, burst: {}",
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
    app = app.layer(RequestBodyLimitLayer::new(state.config.max_upload_size));

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
    let database = std::env::var("REASONDB_PATH").unwrap_or_else(|_| "data/reasondb.redb".to_string());

    init_logging(false, false);

    info!("Starting ReasonDB server v{}", env!("CARGO_PKG_VERSION"));

    // Create data directory if it doesn't exist
    let data_dir = std::path::Path::new(&database).parent().unwrap_or(std::path::Path::new("."));
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
        auth: auth_config,
        rate_limit: rate_limit_config,
        cluster: cluster_config,
    };

    let addr = format!("{}:{}", host, port);

    let provider_name = std::env::var("REASONDB_LLM_PROVIDER")
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    let api_key = std::env::var("REASONDB_LLM_API_KEY").ok().filter(|k| !k.is_empty());
    let model = std::env::var("REASONDB_MODEL").ok().filter(|m| !m.is_empty());

    let require_key = |name: &str| -> anyhow::Result<String> {
        api_key.clone().ok_or_else(|| {
            anyhow::anyhow!("{} provider requires an API key — set REASONDB_LLM_API_KEY", name)
        })
    };

    let provider = match provider_name.as_str() {
        "openai" => {
            let key = require_key("OpenAI")?;
            match &model {
                Some(m) => LLMProvider::OpenAI { api_key: key, model: m.clone() },
                None => LLMProvider::openai(&key),
            }
        }
        "anthropic" => {
            let key = require_key("Anthropic")?;
            match &model {
                Some(m) => LLMProvider::anthropic_custom(&key, m),
                None => LLMProvider::claude_sonnet(&key),
            }
        }
        "gemini" => {
            let key = require_key("Gemini")?;
            match &model {
                Some(m) => LLMProvider::Gemini { api_key: key, model: m.clone() },
                None => LLMProvider::gemini(&key),
            }
        }
        "cohere" => {
            let key = require_key("Cohere")?;
            match &model {
                Some(m) => LLMProvider::Cohere { api_key: key, model: m.clone() },
                None => LLMProvider::cohere(&key),
            }
        }
        other => anyhow::bail!(
            "Unknown or missing LLM provider '{}'. Set REASONDB_LLM_PROVIDER to one of: openai, anthropic, gemini, cohere",
            other
        ),
    };

    info!("LLM provider: {} | model: {}", provider.provider_name(), provider.model());

    let reasoner = Reasoner::new(provider);
    let (app_state, job_rx) = AppState::new(store, text_index, reasoner, api_key_store, config);
    let state = Arc::new(app_state);

    // Restore rate limit state from previous run
    match state.store.load_all_rate_limits() {
        Ok(snapshots) if !snapshots.is_empty() => {
            info!("Restoring {} rate limit entries from database", snapshots.len());
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
                let refs: Vec<(&str, _)> = snapshots.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
                if let Err(e) = snapshot_store.save_rate_limits(&refs) {
                    tracing::warn!("Failed to persist rate limit snapshots: {}", e);
                }
            }
            snapshot_rl.cleanup();
        }
    });

    tokio::spawn(jobs::run_worker(state.clone(), job_rx));

    let app = create_server(state);

    info!("Server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
