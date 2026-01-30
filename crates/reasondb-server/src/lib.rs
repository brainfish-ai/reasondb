//! ReasonDB HTTP Server
//!
//! REST API for the reasoning-native database.
//!
//! # Endpoints
//!
//! ## Ingestion
//! - `POST /v1/ingest/file` - Upload and ingest a file
//! - `POST /v1/ingest/text` - Ingest raw text/markdown
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
//! ## Documentation
//! - `GET /swagger-ui` - Interactive API documentation
//! - `GET /api-docs/openapi.json` - OpenAPI specification
//!
//! # Example
//!
//! ```no_run
//! use reasondb_server::{AppState, ServerConfig, create_server};
//! use reasondb_core::{store::NodeStore, llm::mock::MockReasoner, TextIndex};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = ServerConfig::default();
//!     let store = NodeStore::open(&config.db_path).unwrap();
//!     let text_index = TextIndex::open("./search_index").unwrap();
//!     let reasoner = MockReasoner::new();
//!     let state = AppState::new(store, text_index, reasoner, config.clone());
//!     
//!     // Server would be started here
//! }
//! ```

pub mod error;
pub mod openapi;
pub mod routes;
pub mod state;

pub use error::{ApiError, ApiResult, ErrorResponse};
pub use openapi::ApiDoc;
pub use routes::create_routes;
pub use state::{AppState, MockAppState, RealAppState, ServerConfig};

use axum::Router;
use reasondb_core::{
    llm::{mock::MockReasoner, provider::LLMProvider, provider::Reasoner, ReasoningEngine},
    store::NodeStore,
    text_index::TextIndex,
};
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
    let mut app = create_routes(state.clone());

    // Add OpenAPI documentation
    app = app.merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()));

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

    // Create server config
    let config = ServerConfig {
        host: host.clone(),
        port,
        db_path: database.clone(),
        max_upload_size: 100 * 1024 * 1024,
        enable_cors: true,
        generate_summaries: true,
    };

    let addr = format!("{}:{}", host, port);

    // Check for API keys
    let openai_key = std::env::var("OPENAI_API_KEY").ok();
    let anthropic_key = std::env::var("ANTHROPIC_API_KEY").ok();

    if let Some(api_key) = openai_key {
        info!("Using OpenAI provider (gpt-4o model)");
        let reasoner = Reasoner::new(LLMProvider::openai(&api_key));
        let state = Arc::new(AppState::new(store, text_index, reasoner, config));
        let app = create_server(state);

        info!("Server listening on http://{}", addr);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;
    } else if let Some(api_key) = anthropic_key {
        info!("Using Anthropic provider (Claude Sonnet)");
        let reasoner = Reasoner::new(LLMProvider::claude_sonnet(&api_key));
        let state = Arc::new(AppState::new(store, text_index, reasoner, config));
        let app = create_server(state);

        info!("Server listening on http://{}", addr);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;
    } else {
        info!("No API key provided - using mock reasoner");
        let reasoner = MockReasoner::new();
        let state = Arc::new(AppState::new(store, text_index, reasoner, config));
        let app = create_server(state);

        info!("Server listening on http://{}", addr);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;
    }

    Ok(())
}
