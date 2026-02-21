//! ReasonDB Server CLI
//!
//! Run the ReasonDB HTTP API server.

use clap::Parser;
use reasondb_core::{
    auth::ApiKeyStore,
    llm::provider::{LLMProvider, Reasoner},
    store::NodeStore,
    text_index::TextIndex,
};
use reasondb_server::{create_server, init_metrics, jobs, AppState, AuthConfig, ClusterNodeConfig, RateLimitConfig, ServerConfig};
use redb::Database;
use std::sync::Arc;
use tracing::{info, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// ReasonDB - A Reasoning-Native Database
#[derive(Parser, Debug)]
#[command(name = "reasondb")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Host to bind to
    #[arg(short = 'H', long, default_value = "127.0.0.1", env = "REASONDB_HOST")]
    host: String,

    /// Port to bind to
    #[arg(short, long, default_value = "4444", env = "REASONDB_PORT")]
    port: u16,

    /// Database file path
    #[arg(short, long, default_value = "data/reasondb.redb", env = "REASONDB_PATH")]
    database: String,

    /// LLM provider: openai, anthropic, gemini, cohere, glm, kimi, or ollama
    #[arg(long, env = "REASONDB_LLM_PROVIDER")]
    llm_provider: String,

    /// API key for the chosen LLM provider
    #[arg(long, env = "REASONDB_LLM_API_KEY")]
    llm_api_key: Option<String>,

    /// Custom model name (overrides the provider default)
    #[arg(long, env = "REASONDB_MODEL")]
    model: Option<String>,

    /// Base URL for Ollama (only used when provider is "ollama")
    #[arg(long, env = "REASONDB_OLLAMA_BASE_URL", default_value = "http://localhost:11434/v1")]
    ollama_base_url: String,

    /// Disable summary generation during ingestion
    #[arg(long)]
    no_summaries: bool,

    /// Maximum upload size in MB
    #[arg(long, default_value = "100")]
    max_upload_mb: usize,

    /// Enable authentication
    #[arg(long, env = "REASONDB_AUTH_ENABLED")]
    auth_enabled: bool,

    /// Master key for admin access (bypasses API key checks)
    #[arg(long, env = "REASONDB_MASTER_KEY")]
    master_key: Option<String>,

    /// Enable rate limiting
    #[arg(long, env = "REASONDB_RATE_LIMIT_ENABLED", default_value = "true")]
    rate_limit_enabled: bool,

    /// Rate limit: requests per minute
    #[arg(long, env = "REASONDB_RATE_LIMIT_RPM", default_value = "60")]
    rate_limit_rpm: u32,

    /// Rate limit: requests per hour
    #[arg(long, env = "REASONDB_RATE_LIMIT_RPH", default_value = "1000")]
    rate_limit_rph: u32,

    /// Rate limit: burst size
    #[arg(long, env = "REASONDB_RATE_LIMIT_BURST", default_value = "10")]
    rate_limit_burst: u32,

    /// Enable clustering
    #[arg(long, env = "REASONDB_CLUSTER_ENABLED")]
    cluster_enabled: bool,

    /// Node ID for clustering
    #[arg(long, env = "REASONDB_NODE_ID")]
    node_id: Option<String>,

    /// Cluster name
    #[arg(long, env = "REASONDB_CLUSTER_NAME", default_value = "reasondb-cluster")]
    cluster_name: String,

    /// Raft address for cluster communication
    #[arg(long, env = "REASONDB_RAFT_ADDR", default_value = "127.0.0.1:4445")]
    raft_addr: String,

    /// Initial cluster members (comma-separated node_id@host:port)
    #[arg(long, env = "REASONDB_CLUSTER_MEMBERS")]
    cluster_members: Option<String>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Output logs as JSON
    #[arg(long)]
    json_logs: bool,

    /// Enable Prometheus metrics endpoint
    #[arg(long, env = "REASONDB_METRICS_ENABLED", default_value = "true")]
    metrics_enabled: bool,

    /// OpenTelemetry OTLP endpoint (optional, e.g., http://localhost:4317)
    #[arg(long, env = "OTEL_EXPORTER_OTLP_ENDPOINT")]
    otlp_endpoint: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging
    init_logging(args.verbose, args.json_logs);

    info!("Starting ReasonDB server v{}", env!("CARGO_PKG_VERSION"));

    // Initialize Prometheus metrics
    if args.metrics_enabled {
        let _metrics_handle = init_metrics();
        info!("Prometheus metrics enabled at /metrics");
    }

    // Log OTLP endpoint if configured
    if let Some(ref endpoint) = args.otlp_endpoint {
        info!("OpenTelemetry OTLP endpoint: {}", endpoint);
    }

    // Create data directory if it doesn't exist
    let data_dir = std::path::Path::new(&args.database)
        .parent()
        .unwrap_or(std::path::Path::new("."));
    if !data_dir.exists() && data_dir != std::path::Path::new(".") {
        std::fs::create_dir_all(data_dir)?;
    }

    // Open database (shared between all stores)
    info!("Opening database: {}", args.database);
    let db = Arc::new(Database::create(&args.database)?);
    let store = NodeStore::from_db(Arc::clone(&db))?;

    // Open or create text index for BM25 search (in same directory as database)
    let db_path = std::path::Path::new(&args.database);
    let db_name = db_path.file_stem().unwrap_or_default().to_string_lossy();
    let text_index_path = db_path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join(format!("{}_search_index", db_name));
    info!("Opening text index: {}", text_index_path.display());
    let text_index = TextIndex::open(&text_index_path)?;

    // Create API key store
    let api_key_store = ApiKeyStore::new(db)?;

    // Auth configuration
    let auth_config = AuthConfig {
        enabled: args.auth_enabled,
        master_key: args.master_key.clone(),
    };

    if auth_config.enabled {
        info!("Authentication enabled");
        if auth_config.master_key.is_some() {
            info!("Master key configured");
        }
    } else {
        info!("Authentication disabled (use --auth-enabled to enable)");
    }

    // Rate limit configuration
    let rate_limit_config = RateLimitConfig {
        enabled: args.rate_limit_enabled,
        requests_per_minute: args.rate_limit_rpm,
        requests_per_hour: args.rate_limit_rph,
        burst_size: args.rate_limit_burst,
    };

    // Cluster configuration
    let cluster_config = ClusterNodeConfig {
        enabled: args.cluster_enabled,
        node_id: args.node_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        cluster_name: args.cluster_name.clone(),
        raft_addr: args.raft_addr.clone(),
        initial_members: args.cluster_members
            .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default(),
        min_quorum: 2,
        enable_read_scaling: true,
    };

    if cluster_config.enabled {
        info!("Clustering enabled");
        info!("  Node ID: {}", cluster_config.node_id);
        info!("  Cluster: {}", cluster_config.cluster_name);
        info!("  Raft address: {}", cluster_config.raft_addr);
        if !cluster_config.initial_members.is_empty() {
            info!("  Initial members: {:?}", cluster_config.initial_members);
        }
    }

    // Create server config
    let config = ServerConfig {
        host: args.host.clone(),
        port: args.port,
        db_path: args.database.clone(),
        max_upload_size: args.max_upload_mb * 1024 * 1024,
        enable_cors: true,
        generate_summaries: !args.no_summaries,
        auth: auth_config,
        rate_limit: rate_limit_config,
        cluster: cluster_config,
    };

    let provider_name = args.llm_provider.to_lowercase();
    let api_key = args.llm_api_key.filter(|k| !k.is_empty());
    let model = args.model.filter(|m| !m.is_empty());

    let require_key = |name: &str| -> anyhow::Result<String> {
        api_key.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "{} provider requires an API key — set REASONDB_LLM_API_KEY",
                name
            )
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
        "glm" => {
            let key = require_key("GLM (Zhipu AI)")?;
            match &model {
                Some(m) => LLMProvider::Glm { api_key: key, model: m.clone() },
                None => LLMProvider::glm(&key),
            }
        }
        "kimi" => {
            let key = require_key("Kimi (Moonshot)")?;
            match &model {
                Some(m) => LLMProvider::Kimi { api_key: key, model: m.clone() },
                None => LLMProvider::kimi(&key),
            }
        }
        "ollama" => {
            let m = model.ok_or_else(|| {
                anyhow::anyhow!(
                    "Ollama provider requires a model name — set REASONDB_MODEL (e.g. llama3.3, qwen2.5, mistral)"
                )
            })?;
            LLMProvider::ollama_from_url(&args.ollama_base_url, m)
        }
        other => anyhow::bail!(
            "Unknown LLM provider '{}'. Supported: openai, anthropic, gemini, cohere, glm, kimi, ollama",
            other
        ),
    };

    info!("LLM provider: {} | model: {}", provider.provider_name(), provider.model());

    let reasoner = Reasoner::new(provider);
    let (app_state, job_rx) = AppState::new(store, text_index, reasoner, api_key_store, config);
    let state = Arc::new(app_state);

    tokio::spawn(jobs::run_worker(state.clone(), job_rx));

    let app = create_server(state);

    let addr = format!("{}:{}", args.host, args.port);
    info!("Server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn init_logging(verbose: bool, json: bool) {
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
