//! ReasonDB Server CLI
//!
//! Run the ReasonDB HTTP API server.

use clap::Parser;
use reasondb_core::{
    llm::{mock::MockReasoner, provider::LLMProvider, provider::Reasoner},
    store::NodeStore,
};
use reasondb_server::{create_server, AppState, ServerConfig};
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
    #[arg(short, long, default_value = "3000", env = "REASONDB_PORT")]
    port: u16,

    /// Database file path
    #[arg(short, long, default_value = "reasondb.redb", env = "REASONDB_PATH")]
    database: String,

    /// OpenAI API key (enables LLM features)
    #[arg(long, env = "OPENAI_API_KEY")]
    openai_key: Option<String>,

    /// Anthropic API key (alternative to OpenAI)
    #[arg(long, env = "ANTHROPIC_API_KEY")]
    anthropic_key: Option<String>,

    /// Disable summary generation during ingestion
    #[arg(long)]
    no_summaries: bool,

    /// Maximum upload size in MB
    #[arg(long, default_value = "100")]
    max_upload_mb: usize,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Output logs as JSON
    #[arg(long)]
    json_logs: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging
    init_logging(args.verbose, args.json_logs);

    info!("Starting ReasonDB server v{}", env!("CARGO_PKG_VERSION"));

    // Open database
    info!("Opening database: {}", args.database);
    let store = NodeStore::open(&args.database)?;

    // Create server config
    let config = ServerConfig {
        host: args.host.clone(),
        port: args.port,
        db_path: args.database.clone(),
        max_upload_size: args.max_upload_mb * 1024 * 1024,
        enable_cors: true,
        generate_summaries: !args.no_summaries,
    };

    // Create appropriate reasoner based on available API keys
    let addr = format!("{}:{}", args.host, args.port);

    if let Some(api_key) = args.openai_key {
        info!("Using OpenAI provider (gpt-4o model)");
        let reasoner = Reasoner::new(LLMProvider::openai(&api_key));
        let state = Arc::new(AppState::new(store, reasoner, config));
        let app = create_server(state);

        info!("Server listening on http://{}", addr);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;
    } else if let Some(api_key) = args.anthropic_key {
        info!("Using Anthropic provider (Claude Sonnet)");
        let reasoner = Reasoner::new(LLMProvider::claude_sonnet(&api_key));
        let state = Arc::new(AppState::new(store, reasoner, config));
        let app = create_server(state);

        info!("Server listening on http://{}", addr);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;
    } else {
        info!("No API key provided - using mock reasoner (summaries will be placeholder text)");
        let reasoner = MockReasoner::new();
        let state = Arc::new(AppState::new(store, reasoner, config));
        let app = create_server(state);

        info!("Server listening on http://{}", addr);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;
    }

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
