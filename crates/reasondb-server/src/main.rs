//! ReasonDB Server CLI
//!
//! Run the ReasonDB HTTP API server.

use clap::Parser;
use reasondb_core::{
    auth::ApiKeyStore,
    llm::{
        config::{LlmModelConfig, LlmOptions, LlmSettings},
        dynamic::DynamicReasoner,
    },
    store::NodeStore,
    text_index::TextIndex,
};
use reasondb_server::{
    create_server, init_metrics, jobs, routes, AppState, AuthConfig, ClusterNodeConfig,
    RateLimitConfig, ServerConfig,
};
use redb::Database;
use std::sync::Arc;
use tracing::{info, warn, Level};
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
    #[arg(
        short,
        long,
        default_value = "data/reasondb.redb",
        env = "REASONDB_PATH"
    )]
    database: String,

    /// LLM provider: openai, anthropic, gemini, cohere, glm, kimi, or ollama
    #[arg(long, env = "REASONDB_LLM_PROVIDER")]
    llm_provider: Option<String>,

    /// API key for the chosen LLM provider
    #[arg(long, env = "REASONDB_LLM_API_KEY")]
    llm_api_key: Option<String>,

    /// Custom model name (overrides the provider default)
    #[arg(long, env = "REASONDB_MODEL")]
    model: Option<String>,

    /// Base URL for Ollama (only used when provider is "ollama")
    #[arg(
        long,
        env = "REASONDB_OLLAMA_BASE_URL",
        default_value = "http://localhost:11434/v1"
    )]
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
    #[arg(
        long,
        env = "REASONDB_CLUSTER_NAME",
        default_value = "reasondb-cluster"
    )]
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
        node_id: args
            .node_id
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        cluster_name: args.cluster_name.clone(),
        raft_addr: args.raft_addr.clone(),
        initial_members: args
            .cluster_members
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

    // ------ LLM Configuration ------
    // Priority: 1) DB-persisted settings  2) CLI/env arg seed  3) placeholder
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
        let provider_name = args
            .llm_provider
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase());
        let api_key = args.llm_api_key.filter(|k| !k.is_empty());
        let model = args.model.filter(|m| !m.is_empty());
        let base_url = if provider_name.as_deref() == Some("ollama") {
            Some(args.ollama_base_url.clone())
        } else {
            std::env::var("REASONDB_LLM_BASE_URL")
                .ok()
                .filter(|u| !u.is_empty())
        };

        match provider_name {
            Some(pn) => {
                let supported = [
                    "openai",
                    "anthropic",
                    "gemini",
                    "cohere",
                    "glm",
                    "kimi",
                    "ollama",
                ];
                if !supported.contains(&pn.as_str()) {
                    anyhow::bail!(
                        "Unknown LLM provider '{}'. Supported: {}",
                        pn,
                        supported.join(", ")
                    );
                }
                let cli_config = LlmModelConfig {
                    provider: pn,
                    api_key,
                    model,
                    base_url,
                    options: LlmOptions::default(),
                };
                let env_settings = LlmSettings {
                    ingestion: cli_config.clone(),
                    retrieval: cli_config,
                };
                info!("Seeding LLM settings from CLI / environment variables");
                if let Err(e) = store.set_llm_settings(&env_settings) {
                    warn!("Failed to persist initial LLM settings: {}", e);
                }
                Some(env_settings)
            }
            None => {
                warn!(
                    "No LLM settings found in DB and --llm-provider not provided. \
                     Server will start but LLM features require configuration via PUT /v1/config/llm"
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
            let placeholder = LlmModelConfig {
                provider: "openai".into(),
                api_key: Some("unconfigured".into()),
                model: Some("gpt-4o-mini".into()),
                base_url: None,
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
    app = app.merge(routes::config::config_routes(state));

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
