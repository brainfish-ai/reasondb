//! Search command

use crate::output::{self, OutputFormat};
use anyhow::Result;
use colored::Colorize;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct SearchRequest {
    query: String,
    table_id: Option<String>,
    top_k: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchResult {
    content: String,
    #[serde(default)]
    title: Option<String>,
    confidence: f64,
    document_id: String,
    #[serde(default)]
    node_id: Option<String>,
    #[serde(default)]
    path: Option<Vec<PathNode>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PathNode {
    node_id: String,
    title: String,
    #[serde(default)]
    reasoning: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
    stats: SearchStats,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchStats {
    nodes_visited: u64,
    llm_calls: u64,
    total_time_ms: u64,
}

pub async fn run(
    url: &str,
    query: String,
    table: Option<String>,
    top_k: usize,
    format: OutputFormat,
) -> Result<()> {
    let client = reqwest::Client::new();

    println!("Searching for: {}", query.cyan());
    println!();

    let request = SearchRequest {
        query,
        table_id: table,
        top_k: Some(top_k),
    };

    let response: SearchResponse = client
        .post(format!("{}/v1/search", url))
        .json(&request)
        .send()
        .await?
        .json()
        .await?;

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
        OutputFormat::Csv => {
            println!("document_id,confidence,title,content");
            for result in &response.results {
                println!(
                    "{},{:.2},{},\"{}\"",
                    result.document_id,
                    result.confidence,
                    result.title.as_deref().unwrap_or(""),
                    result.content.replace('"', "\"\"")
                );
            }
        }
        OutputFormat::Table => {
            if response.results.is_empty() {
                output::info("No results found");
            } else {
                for (i, result) in response.results.iter().enumerate() {
                    println!(
                        "{} {} (confidence: {:.0}%)",
                        format!("{}.", i + 1).yellow().bold(),
                        output::format_id(&result.document_id).dimmed(),
                        result.confidence * 100.0
                    );

                    if let Some(title) = &result.title {
                        println!("   {}", title.green());
                    }

                    // Truncate content for display
                    let content = if result.content.len() > 200 {
                        format!("{}...", &result.content[..200])
                    } else {
                        result.content.clone()
                    };
                    println!("   {}", content.dimmed());
                    println!();
                }
            }

            println!("{}", "Stats:".cyan().bold());
            println!(
                "  {} {}",
                "Nodes visited:".dimmed(),
                response.stats.nodes_visited
            );
            println!("  {} {}", "LLM calls:".dimmed(), response.stats.llm_calls);
            println!(
                "  {} {}",
                "Time:".dimmed(),
                output::format_duration_ms(response.stats.total_time_ms)
            );
        }
    }

    Ok(())
}
