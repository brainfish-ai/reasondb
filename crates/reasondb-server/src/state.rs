//! Application state management
//!
//! Shared state accessible to all request handlers.

use crate::jobs::JobQueue;
use reasondb_core::{
    auth::ApiKeyStore,
    cache::QueryCache,
    cluster::{ClusterConfig, ClusterStateMachine, NodeId, RaftNode},
    llm::ReasoningEngine,
    ratelimit::RateLimitStore,
    shard::ShardRouter,
    store::NodeStore,
    text_index::TextIndex,
};
use reasondb_plugin::PluginManager;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Application state shared across handlers
pub struct AppState<R: ReasoningEngine = reasondb_core::llm::dynamic::DynamicReasoner> {
    /// Database store
    pub store: Arc<NodeStore>,
    /// Full-text search index (BM25)
    pub text_index: Arc<TextIndex>,
    /// LLM reasoning engine
    pub reasoner: Arc<R>,
    /// Query result cache (saves LLM calls)
    pub query_cache: Arc<QueryCache>,
    /// API key store for authentication
    pub api_key_store: Arc<ApiKeyStore>,
    /// Rate limit store
    pub rate_limit_store: Arc<RateLimitStore>,
    /// Cluster node (if clustering enabled)
    pub cluster_node: Option<Arc<RaftNode>>,
    /// Server configuration
    pub config: ServerConfig,
    /// Background ingestion job queue
    pub job_queue: Arc<JobQueue>,
    /// Shard router for table-level partitioning
    pub shard_router: Arc<ShardRouter>,
    /// Plugin manager for the plugin system
    pub plugin_manager: Arc<PluginManager>,
}

impl<R: ReasoningEngine> AppState<R> {
    /// Create new app state, returning the state and a receiver for job notifications.
    pub fn new(
        store: NodeStore,
        text_index: TextIndex,
        reasoner: R,
        api_key_store: ApiKeyStore,
        config: ServerConfig,
    ) -> (Self, mpsc::Receiver<String>) {
        let rate_limit_store = RateLimitStore::new(config.rate_limit.clone());
        let store = Arc::new(store);
        let (job_queue, job_rx) = JobQueue::new(store.clone());

        let cluster_node = if config.cluster.enabled {
            let node_id = NodeId::new(config.cluster.node_id.clone());
            let cluster_config = ClusterConfig {
                cluster_name: config.cluster.cluster_name.clone(),
                min_quorum: config.cluster.min_quorum,
                auto_failover: true,
                enable_read_scaling: config.cluster.enable_read_scaling,
                ..Default::default()
            };
            let apply_cb = crate::replication::create_apply_callback(store.clone());
            let state_machine = Arc::new(ClusterStateMachine::with_callback(apply_cb));
            Some(Arc::new(RaftNode::new(
                node_id,
                cluster_config,
                state_machine,
            )))
        } else {
            None
        };

        let shard_router = Arc::new(ShardRouter::single_node(&config.cluster.node_id));

        let plugins_dir =
            std::env::var("REASONDB_PLUGINS_DIR").unwrap_or_else(|_| "./plugins".to_string());
        let plugins_enabled = std::env::var("REASONDB_PLUGINS_ENABLED")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        let plugin_manager = if plugins_enabled {
            let mut pm = PluginManager::new(std::path::Path::new(&plugins_dir));
            if let Err(e) = pm.discover() {
                tracing::warn!("Plugin discovery failed: {}", e);
            } else {
                tracing::info!(
                    "Discovered {} plugins from {}",
                    pm.plugin_count(),
                    plugins_dir
                );
            }
            Arc::new(pm)
        } else {
            Arc::new(PluginManager::disabled())
        };

        (
            Self {
                store,
                text_index: Arc::new(text_index),
                reasoner: Arc::new(reasoner),
                query_cache: Arc::new(QueryCache::new()),
                api_key_store: Arc::new(api_key_store),
                rate_limit_store: Arc::new(rate_limit_store),
                cluster_node,
                config,
                job_queue,
                shard_router,
                plugin_manager,
            },
            job_rx,
        )
    }

    /// Check if this node is the leader
    pub async fn is_leader(&self) -> bool {
        match &self.cluster_node {
            Some(node) => node.is_leader().await,
            None => true, // Single node is always leader
        }
    }

    /// Check if this node can accept writes
    pub async fn can_accept_writes(&self) -> bool {
        self.is_leader().await
    }

    /// Check if clustering is enabled
    pub fn is_clustered(&self) -> bool {
        self.cluster_node.is_some()
    }
}

/// Authentication configuration
#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    /// Enable authentication (if false, all requests are allowed - disabled by default for backward compatibility)
    pub enabled: bool,
    /// Master key that bypasses all checks (for admin/setup)
    pub master_key: Option<String>,
}

impl AuthConfig {
    /// Create config from environment
    pub fn from_env() -> Self {
        let enabled = std::env::var("REASONDB_AUTH_ENABLED")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        let master_key = std::env::var("REASONDB_MASTER_KEY").ok();

        Self {
            enabled,
            master_key,
        }
    }
}

/// Rate limit configuration (re-exported)
pub use reasondb_core::ratelimit::RateLimitConfig;

/// Cluster configuration for this server node
#[derive(Debug, Clone)]
pub struct ClusterNodeConfig {
    /// Enable clustering (if false, runs in single-node mode)
    pub enabled: bool,
    /// This node's unique identifier
    pub node_id: String,
    /// Cluster name for identification
    pub cluster_name: String,
    /// Address for Raft communication (e.g., "127.0.0.1:4445")
    pub raft_addr: String,
    /// Initial cluster members (comma-separated "node_id@host:port")
    pub initial_members: Vec<String>,
    /// Minimum quorum size
    pub min_quorum: usize,
    /// Enable read scaling (serve reads from replicas)
    pub enable_read_scaling: bool,
}

impl Default for ClusterNodeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            node_id: uuid::Uuid::new_v4().to_string(),
            cluster_name: "reasondb-cluster".to_string(),
            raft_addr: "127.0.0.1:4445".to_string(),
            initial_members: vec![],
            min_quorum: 2,
            enable_read_scaling: true,
        }
    }
}

impl ClusterNodeConfig {
    /// Create from environment variables
    pub fn from_env() -> Self {
        let enabled = std::env::var("REASONDB_CLUSTER_ENABLED")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        let node_id =
            std::env::var("REASONDB_NODE_ID").unwrap_or_else(|_| uuid::Uuid::new_v4().to_string());

        let cluster_name = std::env::var("REASONDB_CLUSTER_NAME")
            .unwrap_or_else(|_| "reasondb-cluster".to_string());

        let raft_addr =
            std::env::var("REASONDB_RAFT_ADDR").unwrap_or_else(|_| "127.0.0.1:4445".to_string());

        let initial_members = std::env::var("REASONDB_CLUSTER_MEMBERS")
            .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();

        let min_quorum = std::env::var("REASONDB_MIN_QUORUM")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2);

        let enable_read_scaling = std::env::var("REASONDB_READ_SCALING")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(true);

        Self {
            enabled,
            node_id,
            cluster_name,
            raft_addr,
            initial_members,
            min_quorum,
            enable_read_scaling,
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
    /// Authentication configuration
    pub auth: AuthConfig,
    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,
    /// Cluster configuration
    pub cluster: ClusterNodeConfig,
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
            auth: AuthConfig::default(),
            rate_limit: RateLimitConfig::default(),
            cluster: ClusterNodeConfig::default(),
        }
    }
}

/// Type alias for state with the default reasoner
pub type RealAppState = AppState<reasondb_core::llm::dynamic::DynamicReasoner>;
