//! Cluster management CLI commands
//!
//! Commands for managing ReasonDB cluster operations.

use clap::Subcommand;
use colored::Colorize;
use serde::{Deserialize, Serialize};

use crate::output::Output;

/// Cluster management commands
#[derive(Debug, Subcommand)]
pub enum ClusterCommands {
    /// Get cluster status
    Status {
        /// Server URL
        #[arg(long, default_value = "http://127.0.0.1:4444")]
        server: String,
    },

    /// List all nodes in the cluster
    Nodes {
        /// Server URL
        #[arg(long, default_value = "http://127.0.0.1:4444")]
        server: String,
    },

    /// Add a node to the cluster
    #[command(name = "add-node")]
    AddNode {
        /// Node ID
        #[arg(long)]
        node_id: String,

        /// Raft address (host:port)
        #[arg(long)]
        raft_addr: String,

        /// Optional API address
        #[arg(long)]
        api_addr: Option<String>,

        /// Server URL
        #[arg(long, default_value = "http://127.0.0.1:4444")]
        server: String,
    },

    /// Remove a node from the cluster
    #[command(name = "remove-node")]
    RemoveNode {
        /// Node ID to remove
        #[arg(long)]
        node_id: String,

        /// Server URL
        #[arg(long, default_value = "http://127.0.0.1:4444")]
        server: String,
    },

    /// Get information about the current leader
    Leader {
        /// Server URL
        #[arg(long, default_value = "http://127.0.0.1:4444")]
        server: String,
    },

    /// Check cluster health
    Health {
        /// Server URL
        #[arg(long, default_value = "http://127.0.0.1:4444")]
        server: String,
    },
}

/// Cluster status response
#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterStatusResponse {
    pub enabled: bool,
    pub node_id: Option<String>,
    pub role: Option<String>,
    pub term: Option<u64>,
    pub leader_id: Option<String>,
    pub node_count: usize,
    pub last_applied: Option<u64>,
    pub commit_index: Option<u64>,
    pub has_quorum: bool,
    pub cluster_name: String,
}

/// Node info
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub api_addr: String,
    pub raft_addr: String,
    pub role: String,
    pub status: String,
    pub is_leader: bool,
    pub can_serve_reads: bool,
    pub last_heartbeat: i64,
}

/// List nodes response
#[derive(Debug, Serialize, Deserialize)]
pub struct ListNodesResponse {
    pub nodes: Vec<NodeInfo>,
    pub count: usize,
}

/// Operation response
#[derive(Debug, Serialize, Deserialize)]
pub struct OperationResponse {
    pub success: bool,
    pub message: String,
}

/// Cluster health response
#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterHealthResponse {
    pub healthy: bool,
    pub has_quorum: bool,
    pub has_leader: bool,
    pub node_count: usize,
    pub role: String,
}

/// Execute cluster commands
pub async fn execute(cmd: ClusterCommands, output: Output) -> anyhow::Result<()> {
    match cmd {
        ClusterCommands::Status { server } => get_status(&server, output).await,
        ClusterCommands::Nodes { server } => list_nodes(&server, output).await,
        ClusterCommands::AddNode {
            node_id,
            raft_addr,
            api_addr,
            server,
        } => add_node(&server, &node_id, &raft_addr, api_addr.as_deref(), output).await,
        ClusterCommands::RemoveNode { node_id, server } => {
            remove_node(&server, &node_id, output).await
        }
        ClusterCommands::Leader { server } => get_leader(&server, output).await,
        ClusterCommands::Health { server } => check_health(&server, output).await,
    }
}

async fn get_status(server: &str, output: Output) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/cluster/status", server);

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        anyhow::bail!("Failed to get cluster status: {}", error_text);
    }

    let status: ClusterStatusResponse = response.json().await?;

    if output.is_json() {
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!("{}", "Cluster Status".bold().cyan());
        println!("{}", "═".repeat(50).cyan());
        println!();

        if status.enabled {
            println!("  {} {}", "Mode:".bold(), "Clustered".green());
            println!("  {} {}", "Cluster:".bold(), status.cluster_name);
            println!(
                "  {} {}",
                "Node ID:".bold(),
                status.node_id.unwrap_or_default()
            );
            println!(
                "  {} {}",
                "Role:".bold(),
                format_role(&status.role.unwrap_or_default())
            );
            println!("  {} {}", "Term:".bold(), status.term.unwrap_or(0));
            println!();
            println!("  {} {}", "Nodes:".bold(), status.node_count);
            println!(
                "  {} {}",
                "Has Quorum:".bold(),
                format_bool(status.has_quorum)
            );

            if let Some(leader) = &status.leader_id {
                println!("  {} {}", "Leader:".bold(), leader);
            } else {
                println!("  {} {}", "Leader:".bold(), "None".red());
            }

            if let Some(applied) = status.last_applied {
                println!();
                println!("  {} {}", "Last Applied:".bold(), applied);
                println!(
                    "  {} {}",
                    "Commit Index:".bold(),
                    status.commit_index.unwrap_or(0)
                );
            }
        } else {
            println!("  {} {}", "Mode:".bold(), "Standalone".yellow());
            println!("  {} Clustering is not enabled", "ℹ".blue());
        }
        println!();
    }

    Ok(())
}

async fn list_nodes(server: &str, output: Output) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/cluster/nodes", server);

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        anyhow::bail!("Failed to list nodes: {}", error_text);
    }

    let nodes_response: ListNodesResponse = response.json().await?;

    if output.is_json() {
        println!("{}", serde_json::to_string_pretty(&nodes_response)?);
    } else {
        println!("{}", "Cluster Nodes".bold().cyan());
        println!("{}", "═".repeat(80).cyan());
        println!();

        if nodes_response.nodes.is_empty() {
            println!("  No nodes found");
        } else {
            // Table header
            println!(
                "  {:<16} {:<12} {:<10} {:<10} {:<24}",
                "ID".bold(),
                "Role".bold(),
                "Status".bold(),
                "Leader".bold(),
                "API Address".bold()
            );
            println!("  {}", "─".repeat(76));

            for node in &nodes_response.nodes {
                println!(
                    "  {:<16} {:<12} {:<10} {:<10} {:<24}",
                    truncate(&node.id, 14),
                    format_role(&node.role),
                    format_status(&node.status),
                    if node.is_leader {
                        "✓".green().to_string()
                    } else {
                        "-".dimmed().to_string()
                    },
                    node.api_addr
                );
            }

            println!();
            println!("  Total: {} node(s)", nodes_response.count);
        }
        println!();
    }

    Ok(())
}

async fn add_node(
    server: &str,
    node_id: &str,
    raft_addr: &str,
    api_addr: Option<&str>,
    output: Output,
) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/cluster/nodes/add", server);

    let body = serde_json::json!({
        "node_id": node_id,
        "raft_addr": raft_addr,
        "api_addr": api_addr,
    });

    let response = client.post(&url).json(&body).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        anyhow::bail!("Failed to add node: {}", error_text);
    }

    let result: OperationResponse = response.json().await?;

    if output.is_json() {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        if result.success {
            println!("{} {}", "✓".green().bold(), result.message);
        } else {
            println!("{} {}", "✗".red().bold(), result.message);
        }
    }

    Ok(())
}

async fn remove_node(server: &str, node_id: &str, output: Output) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/cluster/nodes/remove", server);

    let body = serde_json::json!({
        "node_id": node_id,
    });

    let response = client.post(&url).json(&body).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        anyhow::bail!("Failed to remove node: {}", error_text);
    }

    let result: OperationResponse = response.json().await?;

    if output.is_json() {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        if result.success {
            println!("{} {}", "✓".green().bold(), result.message);
        } else {
            println!("{} {}", "✗".red().bold(), result.message);
        }
    }

    Ok(())
}

async fn get_leader(server: &str, output: Output) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/cluster/leader", server);

    let response = client.get(&url).send().await?;

    if response.status().as_u16() == 503 {
        if output.is_json() {
            println!(r#"{{"error": "No leader elected"}}"#);
        } else {
            println!("{} No leader has been elected yet", "⚠".yellow().bold());
        }
        return Ok(());
    }

    if !response.status().is_success() {
        let error_text = response.text().await?;
        anyhow::bail!("Failed to get leader: {}", error_text);
    }

    let leader: NodeInfo = response.json().await?;

    if output.is_json() {
        println!("{}", serde_json::to_string_pretty(&leader)?);
    } else {
        println!("{}", "Current Leader".bold().cyan());
        println!("{}", "═".repeat(50).cyan());
        println!();
        println!("  {} {}", "ID:".bold(), leader.id);
        println!("  {} {}", "Name:".bold(), leader.name);
        println!("  {} {}", "API Address:".bold(), leader.api_addr);
        println!("  {} {}", "Raft Address:".bold(), leader.raft_addr);
        println!("  {} {}", "Status:".bold(), format_status(&leader.status));
        println!();
    }

    Ok(())
}

async fn check_health(server: &str, output: Output) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/cluster/health", server);

    let response = client.get(&url).send().await?;

    let is_healthy = response.status().is_success();

    let health: ClusterHealthResponse = if is_healthy {
        response.json().await?
    } else {
        ClusterHealthResponse {
            healthy: false,
            has_quorum: false,
            has_leader: false,
            node_count: 0,
            role: "Unknown".to_string(),
        }
    };

    if output.is_json() {
        println!("{}", serde_json::to_string_pretty(&health)?);
    } else {
        println!("{}", "Cluster Health".bold().cyan());
        println!("{}", "═".repeat(50).cyan());
        println!();

        if health.healthy {
            println!("  {} {}", "Status:".bold(), "Healthy".green().bold());
        } else {
            println!("  {} {}", "Status:".bold(), "Unhealthy".red().bold());
        }

        println!(
            "  {} {}",
            "Has Quorum:".bold(),
            format_bool(health.has_quorum)
        );
        println!(
            "  {} {}",
            "Has Leader:".bold(),
            format_bool(health.has_leader)
        );
        println!("  {} {}", "Node Count:".bold(), health.node_count);
        println!(
            "  {} {}",
            "This Node Role:".bold(),
            format_role(&health.role)
        );
        println!();

        if !health.healthy {
            std::process::exit(1);
        }
    }

    Ok(())
}

fn format_role(role: &str) -> String {
    match role.to_lowercase().as_str() {
        "leader" => "Leader".green().to_string(),
        "follower" => "Follower".blue().to_string(),
        "candidate" => "Candidate".yellow().to_string(),
        "learner" => "Learner".cyan().to_string(),
        _ => role.to_string(),
    }
}

fn format_status(status: &str) -> String {
    match status.to_lowercase().as_str() {
        "healthy" => "Healthy".green().to_string(),
        "suspect" => "Suspect".yellow().to_string(),
        "down" => "Down".red().to_string(),
        "joining" => "Joining".cyan().to_string(),
        "leaving" => "Leaving".yellow().to_string(),
        _ => status.to_string(),
    }
}

fn format_bool(value: bool) -> String {
    if value {
        "Yes".green().to_string()
    } else {
        "No".red().to_string()
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}
