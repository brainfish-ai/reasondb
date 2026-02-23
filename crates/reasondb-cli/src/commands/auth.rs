//! Authentication commands for API key management
//!
//! Commands:
//! - `reasondb auth keys create` - Create a new API key
//! - `reasondb auth keys list` - List all API keys
//! - `reasondb auth keys revoke` - Revoke an API key
//! - `reasondb auth keys rotate` - Rotate an API key

use anyhow::Result;
use colored::Colorize;
use serde::{Deserialize, Serialize};

/// Auth subcommands
#[derive(clap::Subcommand)]
pub enum AuthCommands {
    /// Manage API keys
    #[command(subcommand)]
    Keys(KeysCommands),
}

/// API key management subcommands
#[derive(clap::Subcommand)]
pub enum KeysCommands {
    /// Create a new API key
    Create {
        /// Name for the API key
        name: String,

        /// Environment (test or live)
        #[arg(short, long, default_value = "test")]
        environment: String,

        /// Permissions (comma-separated: read,write,admin,ingest,relations,query)
        #[arg(short, long)]
        permissions: Option<String>,

        /// Description
        #[arg(short, long)]
        description: Option<String>,

        /// Expiration in days
        #[arg(long)]
        expires_in_days: Option<u32>,
    },

    /// List all API keys
    List,

    /// Get details of an API key
    Get {
        /// Key ID
        id: String,
    },

    /// Revoke an API key
    Revoke {
        /// Key ID
        id: String,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Rotate an API key (create new, revoke old)
    Rotate {
        /// Key ID
        id: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateKeyRequest {
    name: String,
    environment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    permissions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in_days: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CreateKeyResponse {
    id: String,
    key: String,
    key_prefix_hint: String,
    name: String,
    environment: String,
    permissions: Vec<String>,
    expires_at: Option<i64>,
    warning: String,
}

#[derive(Debug, Deserialize)]
struct ListKeysResponse {
    keys: Vec<ApiKeyInfo>,
    total: usize,
}

#[derive(Debug, Deserialize)]
struct KeyResponse {
    key: ApiKeyInfo,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ApiKeyInfo {
    id: String,
    name: String,
    key_prefix_hint: String,
    environment: String,
    permissions: Vec<String>,
    description: Option<String>,
    rate_limit_rpm: Option<u32>,
    rate_limit_rpd: Option<u32>,
    created_at: i64,
    last_used_at: Option<i64>,
    expires_at: Option<i64>,
    is_active: bool,
    usage_count: u64,
}

pub async fn run(url: &str, cmd: AuthCommands) -> Result<()> {
    match cmd {
        AuthCommands::Keys(keys_cmd) => run_keys(url, keys_cmd).await,
    }
}

async fn run_keys(url: &str, cmd: KeysCommands) -> Result<()> {
    let client = reqwest::Client::new();

    // Get API key from config or environment for auth
    let api_key = get_api_key()?;

    match cmd {
        KeysCommands::Create {
            name,
            environment,
            permissions,
            description,
            expires_in_days,
        } => {
            let req = CreateKeyRequest {
                name: name.clone(),
                environment,
                permissions: permissions
                    .map(|p| p.split(',').map(|s| s.trim().to_string()).collect()),
                description,
                expires_in_days,
            };

            let res = client
                .post(format!("{}/v1/auth/keys", url))
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&req)
                .send()
                .await?;

            if res.status().is_success() {
                let data: CreateKeyResponse = res.json().await?;

                println!();
                println!("{}", "✓ API Key Created".green().bold());
                println!();
                println!("  {} {}", "ID:".dimmed(), data.id);
                println!("  {} {}", "Name:".dimmed(), data.name);
                println!("  {} {}", "Environment:".dimmed(), data.environment);
                println!(
                    "  {} {}",
                    "Permissions:".dimmed(),
                    data.permissions.join(", ")
                );
                if let Some(exp) = data.expires_at {
                    let dt = chrono::DateTime::from_timestamp_millis(exp)
                        .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                        .unwrap_or_default();
                    println!("  {} {}", "Expires:".dimmed(), dt);
                }
                println!();
                println!(
                    "  {}",
                    "⚠️  Save this key now - it will not be shown again!".yellow()
                );
                println!();
                println!("  {} {}", "API Key:".cyan().bold(), data.key.green());
                println!();
            } else {
                let status = res.status();
                let text = res.text().await?;
                eprintln!("{} {} - {}", "Error:".red(), status, text);
            }
        }

        KeysCommands::List => {
            let res = client
                .get(format!("{}/v1/auth/keys", url))
                .header("Authorization", format!("Bearer {}", api_key))
                .send()
                .await?;

            if res.status().is_success() {
                let data: ListKeysResponse = res.json().await?;

                println!();
                println!("{} ({})", "API Keys".cyan().bold(), data.total);
                println!();

                if data.keys.is_empty() {
                    println!("  {}", "No API keys found".dimmed());
                } else {
                    for key in &data.keys {
                        let status = if key.is_active {
                            "active".green()
                        } else {
                            "revoked".red()
                        };

                        println!(
                            "  {} {} [{}]",
                            key.key_prefix_hint.cyan(),
                            key.name.white(),
                            status
                        );
                        println!(
                            "      {} {} | {} | {} uses",
                            "ID:".dimmed(),
                            key.id,
                            key.environment,
                            key.usage_count
                        );
                    }
                }
                println!();
            } else {
                let status = res.status();
                let text = res.text().await?;
                eprintln!("{} {} - {}", "Error:".red(), status, text);
            }
        }

        KeysCommands::Get { id } => {
            let res = client
                .get(format!("{}/v1/auth/keys/{}", url, id))
                .header("Authorization", format!("Bearer {}", api_key))
                .send()
                .await?;

            if res.status().is_success() {
                let data: KeyResponse = res.json().await?;
                let key = data.key;

                println!();
                println!("{}", "API Key Details".cyan().bold());
                println!();
                println!("  {} {}", "ID:".dimmed(), key.id);
                println!("  {} {}", "Name:".dimmed(), key.name);
                println!("  {} {}", "Prefix:".dimmed(), key.key_prefix_hint);
                println!("  {} {}", "Environment:".dimmed(), key.environment);
                println!(
                    "  {} {}",
                    "Permissions:".dimmed(),
                    key.permissions.join(", ")
                );
                println!(
                    "  {} {}",
                    "Status:".dimmed(),
                    if key.is_active {
                        "active".green()
                    } else {
                        "revoked".red()
                    }
                );
                if let Some(desc) = &key.description {
                    println!("  {} {}", "Description:".dimmed(), desc);
                }
                println!("  {} {}", "Uses:".dimmed(), key.usage_count);
                if let Some(rpm) = key.rate_limit_rpm {
                    println!("  {} {}/min", "Rate limit:".dimmed(), rpm);
                }
                println!();
            } else {
                let status = res.status();
                let text = res.text().await?;
                eprintln!("{} {} - {}", "Error:".red(), status, text);
            }
        }

        KeysCommands::Revoke { id, force } => {
            if !force {
                println!("Are you sure you want to revoke API key {}? [y/N]", id);
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled");
                    return Ok(());
                }
            }

            let res = client
                .delete(format!("{}/v1/auth/keys/{}", url, id))
                .header("Authorization", format!("Bearer {}", api_key))
                .send()
                .await?;

            if res.status().is_success() {
                println!("  {} API key {} revoked", "✓".green(), id);
            } else {
                let status = res.status();
                let text = res.text().await?;
                eprintln!("{} {} - {}", "Error:".red(), status, text);
            }
        }

        KeysCommands::Rotate { id } => {
            let res = client
                .post(format!("{}/v1/auth/keys/{}/rotate", url, id))
                .header("Authorization", format!("Bearer {}", api_key))
                .send()
                .await?;

            if res.status().is_success() {
                let data: CreateKeyResponse = res.json().await?;

                println!();
                println!("{}", "✓ API Key Rotated".green().bold());
                println!();
                println!("  {} {}", "New ID:".dimmed(), data.id);
                println!("  {} {}", "Name:".dimmed(), data.name);
                println!();
                println!("  {}", "⚠️  The old key has been revoked!".yellow());
                println!(
                    "  {}",
                    "⚠️  Save this new key now - it will not be shown again!".yellow()
                );
                println!();
                println!("  {} {}", "New API Key:".cyan().bold(), data.key.green());
                println!();
            } else {
                let status = res.status();
                let text = res.text().await?;
                eprintln!("{} {} - {}", "Error:".red(), status, text);
            }
        }
    }

    Ok(())
}

/// Get API key from config or environment (for authenticating CLI requests)
fn get_api_key() -> Result<String> {
    // First try environment variable
    if let Ok(key) = std::env::var("REASONDB_API_KEY") {
        return Ok(key);
    }

    // Then try master key from environment
    if let Ok(key) = std::env::var("REASONDB_MASTER_KEY") {
        return Ok(key);
    }

    // Note: Config file doesn't store auth API keys for security
    // Users should use REASONDB_MASTER_KEY env var

    Err(anyhow::anyhow!(
        "No API key found. Set REASONDB_MASTER_KEY or REASONDB_API_KEY environment variable.\n\
         Hint: When running with --auth-enabled, use --master-key to set an admin key."
    ))
}
