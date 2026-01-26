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
//! use reasondb_core::{store::NodeStore, llm::mock::MockReasoner};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = ServerConfig::default();
//!     let store = NodeStore::open(&config.db_path).unwrap();
//!     let reasoner = MockReasoner::new();
//!     let state = AppState::new(store, reasoner, config.clone());
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
use reasondb_core::llm::ReasoningEngine;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};
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
