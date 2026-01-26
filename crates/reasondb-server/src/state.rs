//! Application state management
//!
//! Shared state accessible to all request handlers.

use reasondb_core::{
    llm::{mock::MockReasoner, provider::Reasoner, ReasoningEngine},
    store::NodeStore,
};
use std::sync::Arc;

/// Application state shared across handlers
pub struct AppState<R: ReasoningEngine = Reasoner> {
    /// Database store
    pub store: Arc<NodeStore>,
    /// LLM reasoning engine
    pub reasoner: Arc<R>,
    /// Server configuration
    pub config: ServerConfig,
}

impl<R: ReasoningEngine> AppState<R> {
    /// Create new app state
    pub fn new(store: NodeStore, reasoner: R, config: ServerConfig) -> Self {
        Self {
            store: Arc::new(store),
            reasoner: Arc::new(reasoner),
            config,
        }
    }
}

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Host to bind to
    pub host: String,
    /// Port to bind to
    pub port: u16,
    /// Database path
    pub db_path: String,
    /// Maximum upload size in bytes
    pub max_upload_size: usize,
    /// Enable CORS
    pub enable_cors: bool,
    /// Generate summaries during ingestion
    pub generate_summaries: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            db_path: "reasondb.redb".to_string(),
            max_upload_size: 100 * 1024 * 1024, // 100MB
            enable_cors: true,
            generate_summaries: true,
        }
    }
}

/// Type alias for state with mock reasoner (testing)
pub type MockAppState = AppState<MockReasoner>;

/// Type alias for state with real reasoner
pub type RealAppState = AppState<Reasoner>;
