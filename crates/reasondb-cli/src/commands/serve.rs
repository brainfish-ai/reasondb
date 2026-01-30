//! Server command - starts the ReasonDB server

use super::config::ReasonDBConfig;
use anyhow::Result;
use colored::Colorize;

pub async fn run(port: u16, host: String, db_path: String) -> Result<()> {
    // Load config file for LLM settings
    let config = ReasonDBConfig::load().unwrap_or_default();

    println!(
        "\n{}\n",
        r#"
  ____                            ____  ____  
 |  _ \ ___  __ _ ___  ___  _ __ |  _ \| __ ) 
 | |_) / _ \/ _` / __|/ _ \| '_ \| | | |  _ \ 
 |  _ <  __/ (_| \__ \ (_) | | | | |_| | |_) |
 |_| \_\___|\__,_|___/\___/|_| |_|____/|____/ 
"#
        .cyan()
    );

    println!("{}", "The reasoning-native document database".dimmed());
    println!();

    // Use config values with CLI overrides
    let effective_port = if port != 4444 {
        port
    } else {
        config.server.port
    };
    let effective_host = if host != "127.0.0.1" {
        host
    } else {
        config.server.host.clone()
    };
    let effective_db_path = if db_path != "reasondb.redb" {
        db_path
    } else {
        config.server.db_path.clone()
    };

    println!("  {} {}", "Database:".dimmed(), effective_db_path.green());
    println!(
        "  {} {}:{}",
        "Listening:".dimmed(),
        effective_host.green(),
        effective_port.to_string().green()
    );
    println!(
        "  {} http://{}:{}/swagger-ui/",
        "Swagger UI:".dimmed(),
        effective_host,
        effective_port
    );

    // Show LLM provider info
    if let Some(provider) = &config.llm.provider {
        println!(
            "  {} {} {}",
            "LLM:".dimmed(),
            provider.green(),
            config
                .llm
                .model
                .as_ref()
                .map(|m| format!("({})", m))
                .unwrap_or_default()
                .dimmed()
        );
    } else {
        println!(
            "  {} {}",
            "LLM:".dimmed(),
            "mock (no API key configured)".yellow()
        );
        println!(
            "      {}",
            "Run 'reasondb config init' to configure an LLM provider".dimmed()
        );
    }
    println!();

    // Set environment variables for the server
    std::env::set_var("REASONDB_PORT", effective_port.to_string());
    std::env::set_var("REASONDB_HOST", &effective_host);
    std::env::set_var("REASONDB_PATH", &effective_db_path);

    // Set LLM API key from config if not already in environment
    if let (Some(provider), Some(api_key)) = (&config.llm.provider, &config.llm.api_key) {
        match provider.to_lowercase().as_str() {
            "anthropic" | "claude" => {
                if std::env::var("ANTHROPIC_API_KEY").is_err() {
                    std::env::set_var("ANTHROPIC_API_KEY", api_key);
                }
            }
            "openai" | "gpt" => {
                if std::env::var("OPENAI_API_KEY").is_err() {
                    std::env::set_var("OPENAI_API_KEY", api_key);
                }
            }
            "gemini" | "google" => {
                if std::env::var("GOOGLE_API_KEY").is_err() {
                    std::env::set_var("GOOGLE_API_KEY", api_key);
                }
            }
            "cohere" => {
                if std::env::var("COHERE_API_KEY").is_err() {
                    std::env::set_var("COHERE_API_KEY", api_key);
                }
            }
            _ => {}
        }

        // Set model override if specified
        if let Some(model) = &config.llm.model {
            std::env::set_var("REASONDB_MODEL", model);
        }
    }

    // Run the server
    reasondb_server::run_server().await?;

    Ok(())
}
