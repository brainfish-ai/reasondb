//! ReasonDB CLI - Command-line interface for the reasoning-native database

mod commands;
mod config;
mod output;

use anyhow::Result;
use clap::{Parser, Subcommand};
use clap_complete::Shell;
use colored::Colorize;

#[derive(Parser)]
#[command(
    name = "reasondb",
    version,
    about = "ReasonDB - The reasoning-native document database",
    long_about = "A database that thinks, not just calculates.\n\n\
                  ReasonDB is optimized for AI agent workflows with LLM-guided \
                  tree traversal and intelligent document search."
)]
struct Cli {
    /// Server URL (can also be set via REASONDB_URL env var)
    #[arg(long, env = "REASONDB_URL", default_value = "http://localhost:4444")]
    url: String,

    /// Output format
    #[arg(long, value_enum, default_value = "table")]
    format: output::OutputFormat,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the ReasonDB server
    Serve {
        /// Port to listen on
        #[arg(short, long, env = "REASONDB_PORT", default_value = "4444")]
        port: u16,

        /// Host to bind to
        #[arg(long, env = "REASONDB_HOST", default_value = "127.0.0.1")]
        host: String,

        /// Database file path
        #[arg(long, env = "REASONDB_PATH", default_value = "reasondb.redb")]
        db_path: String,
    },

    /// Execute RQL queries (interactive REPL or single query)
    Query {
        /// RQL query to execute (if not provided, starts interactive REPL)
        #[arg(short, long)]
        query: Option<String>,

        /// Read query from file
        #[arg(short, long)]
        file: Option<String>,
    },

    /// Manage tables
    #[command(subcommand)]
    Tables(TablesCommands),

    /// Manage documents
    #[command(subcommand)]
    Docs(DocsCommands),

    /// Import data from files
    Import {
        /// File to import (JSON, CSV, or text)
        file: String,

        /// Target table name
        #[arg(short, long)]
        table: Option<String>,

        /// Document title (for single text files)
        #[arg(long)]
        title: Option<String>,
    },

    /// Export data to files
    Export {
        /// Output file path
        output: String,

        /// Table to export (exports all if not specified)
        #[arg(short, long)]
        table: Option<String>,

        /// Export format (json, csv)
        #[arg(long, default_value = "json")]
        format: String,
    },

    /// Search documents
    Search {
        /// Search query
        query: String,

        /// Table to search in
        #[arg(short, long)]
        table: Option<String>,

        /// Number of results
        #[arg(short = 'k', long, default_value = "10")]
        top_k: usize,
    },

    /// Check server health
    Health,

    /// Manage configuration (LLM provider, server settings)
    #[command(subcommand)]
    Config(commands::config::ConfigCommands),

    /// Manage authentication (API keys)
    #[command(subcommand)]
    Auth(commands::auth::AuthCommands),

    /// Manage cluster operations
    #[command(subcommand)]
    Cluster(commands::cluster::ClusterCommands),

    /// Backup and restore database
    Backup(commands::backup::BackupArgs),

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Subcommand)]
enum TablesCommands {
    /// List all tables
    List,

    /// Create a new table
    Create {
        /// Table name
        name: String,

        /// Table description
        #[arg(short, long)]
        description: Option<String>,
    },

    /// Get table details
    Get {
        /// Table ID or name
        id: String,
    },

    /// Delete a table
    Delete {
        /// Table ID
        id: String,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum DocsCommands {
    /// List documents
    List {
        /// Filter by table
        #[arg(short, long)]
        table: Option<String>,

        /// Maximum number of documents
        #[arg(short, long, default_value = "50")]
        limit: usize,
    },

    /// Get document details
    Get {
        /// Document ID
        id: String,

        /// Show full tree structure
        #[arg(long)]
        tree: bool,
    },

    /// Delete a document
    Delete {
        /// Document ID
        id: String,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Ingest a text document
    Ingest {
        /// Document title
        title: String,

        /// Content (or use --file)
        #[arg(short, long)]
        content: Option<String>,

        /// Read content from file
        #[arg(short, long)]
        file: Option<String>,

        /// Target table
        #[arg(short, long)]
        table: Option<String>,

        /// Tags (comma-separated)
        #[arg(long)]
        tags: Option<String>,

        /// Author name
        #[arg(long)]
        author: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Serve {
            port,
            host,
            db_path,
        } => commands::serve::run(port, host, db_path).await,
        Commands::Query { query, file } => {
            commands::query::run(&cli.url, query, file, cli.format).await
        }
        Commands::Tables(cmd) => commands::tables::run(&cli.url, cmd, cli.format).await,
        Commands::Docs(cmd) => commands::docs::run(&cli.url, cmd, cli.format).await,
        Commands::Import { file, table, title } => {
            commands::import::run(&cli.url, file, table, title).await
        }
        Commands::Export {
            output,
            table,
            format,
        } => commands::export::run(&cli.url, output, table, format).await,
        Commands::Search {
            query,
            table,
            top_k,
        } => commands::search::run(&cli.url, query, table, top_k, cli.format).await,
        Commands::Health => commands::health::run(&cli.url).await,
        Commands::Config(cmd) => commands::config::run(cmd, &cli.url).await,
        Commands::Auth(cmd) => commands::auth::run(&cli.url, cmd).await,
        Commands::Cluster(cmd) => {
            let output = output::Output::new(cli.format);
            commands::cluster::execute(cmd, output).await
        }
        Commands::Backup(args) => {
            let output = output::Output::new(cli.format);
            args.execute(&output)
        }
        Commands::Completions { shell } => {
            commands::completions::run(shell);
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("{} {}", "Error:".red().bold(), e);
        std::process::exit(1);
    }

    Ok(())
}
