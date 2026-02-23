//! Documents management commands

use crate::output::{self, OutputFormat};
use crate::DocsCommands;
use anyhow::Result;
use colored::Colorize;
use comfy_table::Cell;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Document {
    id: String,
    title: String,
    #[serde(default)]
    table_id: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    total_nodes: Option<u64>,
    #[serde(default)]
    max_depth: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct IngestRequest {
    title: String,
    content: String,
    table_id: Option<String>,
    tags: Option<Vec<String>>,
    author: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct IngestResponse {
    document_id: String,
    title: String,
    total_nodes: u64,
    max_depth: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TreeResponse {
    document: Document,
    tree: serde_json::Value,
}

pub async fn run(url: &str, cmd: DocsCommands, format: OutputFormat) -> Result<()> {
    let client = reqwest::Client::new();

    match cmd {
        DocsCommands::List { table, limit } => {
            let mut request_url = format!("{}/v1/documents?limit={}", url, limit);
            if let Some(table_id) = &table {
                request_url.push_str(&format!("&table_id={}", table_id));
            }

            let documents: Vec<Document> = client.get(&request_url).send().await?.json().await?;

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&documents)?);
                }
                OutputFormat::Csv => {
                    println!("id,title,table_id,tags,author,created_at");
                    for doc in &documents {
                        println!(
                            "{},{},{},{},{},{}",
                            doc.id,
                            doc.title,
                            doc.table_id.as_deref().unwrap_or(""),
                            doc.tags.join(";"),
                            doc.author.as_deref().unwrap_or(""),
                            doc.created_at.as_deref().unwrap_or("")
                        );
                    }
                }
                OutputFormat::Table => {
                    if documents.is_empty() {
                        output::info(
                            "No documents found. Ingest one with: reasondb docs ingest <title>",
                        );
                    } else {
                        let mut t =
                            output::create_table(vec!["ID", "Title", "Tags", "Author", "Nodes"]);

                        for doc in &documents {
                            t.add_row(vec![
                                Cell::new(output::format_id(&doc.id)),
                                Cell::new(truncate(&doc.title, 40)),
                                Cell::new(truncate(&doc.tags.join(", "), 20)),
                                Cell::new(doc.author.as_deref().unwrap_or("-")),
                                Cell::new(
                                    doc.total_nodes
                                        .map(|n| n.to_string())
                                        .unwrap_or("-".to_string()),
                                ),
                            ]);
                        }

                        println!("\n{}\n", t);
                        println!(
                            "{} {} document(s)",
                            "Total:".dimmed(),
                            documents.len().to_string().green()
                        );
                    }
                }
            }
        }

        DocsCommands::Get { id, tree } => {
            if tree {
                let response: TreeResponse = client
                    .get(format!("{}/v1/documents/{}/tree", url, id))
                    .send()
                    .await?
                    .json()
                    .await?;

                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                let response: Document = client
                    .get(format!("{}/v1/documents/{}", url, id))
                    .send()
                    .await?
                    .json()
                    .await?;

                match format {
                    OutputFormat::Json => {
                        println!("{}", serde_json::to_string_pretty(&response)?);
                    }
                    _ => {
                        println!();
                        println!("  {} {}", "ID:".dimmed(), response.id);
                        println!("  {} {}", "Title:".dimmed(), response.title.green());
                        println!(
                            "  {} {}",
                            "Table:".dimmed(),
                            response.table_id.as_deref().unwrap_or("-")
                        );
                        println!("  {} {}", "Tags:".dimmed(), response.tags.join(", "));
                        println!(
                            "  {} {}",
                            "Author:".dimmed(),
                            response.author.as_deref().unwrap_or("-")
                        );
                        if let Some(nodes) = response.total_nodes {
                            println!("  {} {}", "Nodes:".dimmed(), nodes);
                        }
                        if let Some(created) = &response.created_at {
                            println!("  {} {}", "Created:".dimmed(), created);
                        }
                        println!();
                    }
                }
            }
        }

        DocsCommands::Delete { id, force } => {
            if !force {
                println!(
                    "{} Are you sure you want to delete document '{}'? [y/N] ",
                    "Warning:".yellow().bold(),
                    id.cyan()
                );

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;

                if !input.trim().eq_ignore_ascii_case("y") {
                    output::info("Cancelled");
                    return Ok(());
                }
            }

            client
                .delete(format!("{}/v1/documents/{}", url, id))
                .send()
                .await?;

            output::success(&format!("Deleted document '{}'", id));
        }

        DocsCommands::Ingest {
            title,
            content,
            file,
            table,
            tags,
            author,
        } => {
            let content = if let Some(file_path) = file {
                std::fs::read_to_string(&file_path)?
            } else if let Some(c) = content {
                c
            } else {
                return Err(anyhow::anyhow!(
                    "Either --content or --file must be provided"
                ));
            };

            let tags: Option<Vec<String>> =
                tags.map(|t| t.split(',').map(|s| s.trim().to_string()).collect());

            let request = IngestRequest {
                title: title.clone(),
                content,
                table_id: table,
                tags,
                author,
            };

            let response: IngestResponse = client
                .post(format!("{}/v1/ingest/text", url))
                .json(&request)
                .send()
                .await?
                .json()
                .await?;

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                }
                _ => {
                    output::success(&format!("Ingested document '{}'", title.green()));
                    println!("  {} {}", "ID:".dimmed(), response.document_id);
                    println!("  {} {}", "Nodes:".dimmed(), response.total_nodes);
                    println!("  {} {}", "Depth:".dimmed(), response.max_depth);
                }
            }
        }
    }

    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len - 3])
    } else {
        s.to_string()
    }
}
