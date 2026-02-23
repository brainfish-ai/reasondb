//! Tables management commands

use crate::output::{self, OutputFormat};
use crate::TablesCommands;
use anyhow::Result;
use colored::Colorize;
use comfy_table::Cell;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Table {
    id: String,
    name: String,
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    description: Option<String>,
    document_count: u64,
    #[serde(default)]
    total_nodes: Option<u64>,
    #[serde(default)]
    created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TablesResponse {
    tables: Vec<Table>,
    total: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateTableRequest {
    name: String,
    description: Option<String>,
}

pub async fn run(url: &str, cmd: TablesCommands, format: OutputFormat) -> Result<()> {
    let client = reqwest::Client::new();

    match cmd {
        TablesCommands::List => {
            let response: TablesResponse = client
                .get(format!("{}/v1/tables", url))
                .send()
                .await?
                .json()
                .await?;

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                }
                OutputFormat::Csv => {
                    println!("id,name,document_count,total_nodes");
                    for table in &response.tables {
                        println!(
                            "{},{},{},{}",
                            table.id,
                            table.name,
                            table.document_count,
                            table.total_nodes.unwrap_or(0)
                        );
                    }
                }
                OutputFormat::Table => {
                    if response.tables.is_empty() {
                        output::info(
                            "No tables found. Create one with: reasondb tables create <name>",
                        );
                    } else {
                        let mut t = output::create_table(vec!["ID", "Name", "Docs", "Nodes"]);

                        for table in &response.tables {
                            t.add_row(vec![
                                Cell::new(output::format_id(&table.id)),
                                Cell::new(&table.name),
                                Cell::new(table.document_count.to_string()),
                                Cell::new(
                                    table
                                        .total_nodes
                                        .map(|n| n.to_string())
                                        .unwrap_or("-".to_string()),
                                ),
                            ]);
                        }

                        println!("\n{}\n", t);
                        println!(
                            "{} {} table(s)",
                            "Total:".dimmed(),
                            response.total.to_string().green()
                        );
                    }
                }
            }
        }

        TablesCommands::Create { name, description } => {
            let request = CreateTableRequest {
                name: name.clone(),
                description,
            };

            let response: Table = client
                .post(format!("{}/v1/tables", url))
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
                    output::success(&format!("Created table '{}'", name.green()));
                    println!("  {} {}", "ID:".dimmed(), response.id);
                    if let Some(slug) = &response.slug {
                        println!("  {} {}", "Slug:".dimmed(), slug);
                    }
                }
            }
        }

        TablesCommands::Get { id } => {
            let response: Table = client
                .get(format!("{}/v1/tables/{}", url, id))
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
                    println!("  {} {}", "Name:".dimmed(), response.name.green());
                    if let Some(slug) = &response.slug {
                        println!("  {} {}", "Slug:".dimmed(), slug);
                    }
                    println!(
                        "  {} {}",
                        "Description:".dimmed(),
                        response.description.as_deref().unwrap_or("-")
                    );
                    println!("  {} {}", "Documents:".dimmed(), response.document_count);
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

        TablesCommands::Delete { id, force } => {
            if !force {
                println!(
                    "{} Are you sure you want to delete table '{}'? [y/N] ",
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
                .delete(format!("{}/v1/tables/{}", url, id))
                .send()
                .await?;

            output::success(&format!("Deleted table '{}'", id));
        }
    }

    Ok(())
}
