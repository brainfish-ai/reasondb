//! Configuration management commands
//!
//! Configuration management for ReasonDB settings including LLM API keys.

use anyhow::Result;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// ReasonDB configuration file structure
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ReasonDBConfig {
    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,

    /// LLM configuration
    #[serde(default)]
    pub llm: LLMConfig,

    /// Default table to use
    #[serde(default)]
    pub default_table: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server URL
    #[serde(default = "default_url")]
    pub url: String,

    /// Default port
    #[serde(default = "default_port")]
    pub port: u16,

    /// Default host
    #[serde(default = "default_host")]
    pub host: String,

    /// Database path
    #[serde(default = "default_db_path")]
    pub db_path: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            url: default_url(),
            port: default_port(),
            host: default_host(),
            db_path: default_db_path(),
        }
    }
}

fn default_url() -> String {
    "http://localhost:4444".to_string()
}

fn default_port() -> u16 {
    4444
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_db_path() -> String {
    "data/reasondb.redb".to_string()
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct LLMConfig {
    /// LLM provider (openai, anthropic, gemini, cohere, glm, kimi, ollama)
    #[serde(default)]
    pub provider: Option<String>,

    /// API key (stored securely)
    #[serde(default)]
    pub api_key: Option<String>,

    /// Model name override
    #[serde(default)]
    pub model: Option<String>,
}

impl ReasonDBConfig {
    /// Get the config file path
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;

        Ok(config_dir.join("reasondb").join("config.toml"))
    }

    /// Load configuration from file
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Set restrictive permissions on config file (contains API keys)
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path)?.permissions();
            perms.set_mode(0o600); // Owner read/write only
            std::fs::set_permissions(&path, perms)?;
        }

        Ok(())
    }

    /// Get a config value by key path (e.g., "llm.api_key")
    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "server.url" => Some(self.server.url.clone()),
            "server.port" => Some(self.server.port.to_string()),
            "server.host" => Some(self.server.host.clone()),
            "server.db_path" => Some(self.server.db_path.clone()),
            "llm.provider" => self.llm.provider.clone(),
            "llm.api_key" => self.llm.api_key.clone().map(|k| mask_key(&k)),
            "llm.model" => self.llm.model.clone(),
            "default_table" => self.default_table.clone(),
            _ => None,
        }
    }

    /// Set a config value by key path
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "server.url" => self.server.url = value.to_string(),
            "server.port" => self.server.port = value.parse()?,
            "server.host" => self.server.host = value.to_string(),
            "server.db_path" => self.server.db_path = value.to_string(),
            "llm.provider" => self.llm.provider = Some(value.to_string()),
            "llm.api_key" => self.llm.api_key = Some(value.to_string()),
            "llm.model" => self.llm.model = Some(value.to_string()),
            "default_table" => self.default_table = Some(value.to_string()),
            _ => return Err(anyhow::anyhow!("Unknown config key: {}", key)),
        }
        Ok(())
    }

    /// Unset a config value
    pub fn unset(&mut self, key: &str) -> Result<()> {
        match key {
            "llm.provider" => self.llm.provider = None,
            "llm.api_key" => self.llm.api_key = None,
            "llm.model" => self.llm.model = None,
            "default_table" => self.default_table = None,
            _ => return Err(anyhow::anyhow!("Cannot unset key: {}", key)),
        }
        Ok(())
    }
}

/// Mask an API key for display (show first 8 and last 4 chars)
fn mask_key(key: &str) -> String {
    if key.len() > 16 {
        format!("{}...{}", &key[..8], &key[key.len() - 4..])
    } else {
        "********".to_string()
    }
}

/// Config subcommands
#[derive(clap::Subcommand)]
pub enum ConfigCommands {
    /// Set a configuration value
    Set {
        /// Config key (e.g., llm.api_key, server.port)
        key: String,
        /// Value to set
        value: String,
    },

    /// Get a configuration value
    Get {
        /// Config key to get
        key: String,
    },

    /// Unset a configuration value
    Unset {
        /// Config key to unset
        key: String,
    },

    /// List all configuration values
    List,

    /// Show config file path
    Path,

    /// Initialize configuration interactively
    Init,
}

pub async fn run(cmd: ConfigCommands) -> Result<()> {
    match cmd {
        ConfigCommands::Set { key, value } => {
            let mut config = ReasonDBConfig::load()?;
            config.set(&key, &value)?;
            config.save()?;

            if key == "llm.api_key" {
                println!("  {} {} = {}", "✓".green(), key.cyan(), mask_key(&value));
            } else {
                println!("  {} {} = {}", "✓".green(), key.cyan(), value.green());
            }
        }

        ConfigCommands::Get { key } => {
            let config = ReasonDBConfig::load()?;
            match config.get(&key) {
                Some(value) => println!("{}", value),
                None => {
                    eprintln!("{} Key '{}' not set", "⚠".yellow(), key);
                    std::process::exit(1);
                }
            }
        }

        ConfigCommands::Unset { key } => {
            let mut config = ReasonDBConfig::load()?;
            config.unset(&key)?;
            config.save()?;
            println!("  {} Unset {}", "✓".green(), key.cyan());
        }

        ConfigCommands::List => {
            let config = ReasonDBConfig::load()?;
            let path = ReasonDBConfig::config_path()?;

            println!();
            println!("{}", "ReasonDB Configuration".cyan().bold());
            println!("{}", format!("Config file: {}", path.display()).dimmed());
            println!();

            println!("{}", "[server]".yellow());
            println!("  url = {}", config.server.url.green());
            println!("  host = {}", config.server.host.green());
            println!("  port = {}", config.server.port.to_string().green());
            println!("  db_path = {}", config.server.db_path.green());
            println!();

            println!("{}", "[llm]".yellow());
            println!(
                "  provider = {}",
                config
                    .llm
                    .provider
                    .as_deref()
                    .unwrap_or("(not set)")
                    .green()
            );
            println!(
                "  api_key = {}",
                config
                    .llm
                    .api_key
                    .as_ref()
                    .map(|k| mask_key(k))
                    .unwrap_or_else(|| "(not set)".to_string())
                    .green()
            );
            println!(
                "  model = {}",
                config.llm.model.as_deref().unwrap_or("(default)").green()
            );
            println!();

            if let Some(table) = &config.default_table {
                println!("default_table = {}", table.green());
                println!();
            }
        }

        ConfigCommands::Path => {
            let path = ReasonDBConfig::config_path()?;
            println!("{}", path.display());
        }

        ConfigCommands::Init => {
            init_interactive().await?;
        }
    }

    Ok(())
}

async fn init_interactive() -> Result<()> {
    use std::io::{self, Write};

    println!();
    println!("{}", "ReasonDB Configuration Setup".cyan().bold());
    println!("{}", "This will help you configure ReasonDB.\n".dimmed());

    let mut config = ReasonDBConfig::load()?;

    // LLM Provider
    println!("{}", "LLM Provider".yellow());
    println!("  1. Anthropic (Claude)");
    println!("  2. OpenAI (GPT-4)");
    println!("  3. Google (Gemini)");
    println!("  4. Cohere");
    println!("  5. GLM (Zhipu AI)");
    println!("  6. Kimi (Moonshot)");
    println!("  7. Ollama (local)");
    println!("  8. Skip (use mock provider)");
    print!("\nSelect provider [1-8]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let provider = match input.trim() {
        "1" => Some("anthropic"),
        "2" => Some("openai"),
        "3" => Some("gemini"),
        "4" => Some("cohere"),
        "5" => Some("glm"),
        "6" => Some("kimi"),
        "7" => Some("ollama"),
        _ => None,
    };

    if let Some(p) = provider {
        config.llm.provider = Some(p.to_string());

        if p == "ollama" {
            print!("Enter model name (e.g. llama3.3, qwen2.5, mistral): ");
            io::stdout().flush()?;

            let mut model_name = String::new();
            io::stdin().read_line(&mut model_name)?;
            let model_name = model_name.trim();
            if !model_name.is_empty() {
                config.llm.model = Some(model_name.to_string());
            }

            print!("Ollama base URL [http://localhost:11434/v1]: ");
            io::stdout().flush()?;

            let mut base_url = String::new();
            io::stdin().read_line(&mut base_url)?;
            let base_url = base_url.trim();
            if !base_url.is_empty() {
                config.llm.api_key = Some(base_url.to_string());
            }
        } else {
            print!("Enter {} API key: ", p.to_uppercase());
            io::stdout().flush()?;

            let mut key = String::new();
            io::stdin().read_line(&mut key)?;
            let key = key.trim();

            if !key.is_empty() {
                config.llm.api_key = Some(key.to_string());
            }
        }
    }

    // Server URL
    print!(
        "\nServer URL [{}]: ",
        config.server.url.dimmed()
    );
    io::stdout().flush()?;

    let mut url = String::new();
    io::stdin().read_line(&mut url)?;
    let url = url.trim();
    if !url.is_empty() {
        config.server.url = url.to_string();
    }

    // Save
    config.save()?;

    println!();
    println!("  {} Configuration saved!", "✓".green());
    println!();
    println!("{}", "You can now run:".dimmed());
    println!("  {} - Start the server", "reasondb serve".cyan());
    println!("  {} - Check server health", "reasondb health".cyan());
    println!("  {} - View configuration", "reasondb config list".cyan());
    println!();

    Ok(())
}
