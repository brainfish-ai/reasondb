//! RQL query command with REPL support

use crate::output::{self, OutputFormat};
use anyhow::Result;
use colored::Colorize;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct QueryRequest {
    query: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct QueryResponse {
    documents: Vec<serde_json::Value>,
    #[serde(alias = "total", alias = "total_count")]
    total_count: u64,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    aggregates: Option<serde_json::Value>,
    #[serde(default)]
    explain: Option<serde_json::Value>,
    #[serde(default)]
    execution_time_ms: Option<u64>,
}

pub async fn run(
    url: &str,
    query: Option<String>,
    file: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    // If a query is provided directly or via file, execute it
    if let Some(q) = query {
        return execute_query(url, &q, format).await;
    }

    if let Some(file_path) = file {
        let q = std::fs::read_to_string(&file_path)?;
        return execute_query(url, &q, format).await;
    }

    // Otherwise, start the REPL
    start_repl(url, format).await
}

async fn execute_query(url: &str, query: &str, format: OutputFormat) -> Result<()> {
    let client = reqwest::Client::new();

    let request = QueryRequest {
        query: query.to_string(),
    };

    let response = client
        .post(format!("{}/v1/query", url))
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let error: serde_json::Value = response.json().await?;
        return Err(anyhow::anyhow!(
            "Query failed: {}",
            error["error"].as_str().unwrap_or("Unknown error")
        ));
    }

    let result: QueryResponse = response.json().await?;

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        OutputFormat::Csv => {
            // Simple CSV output for documents
            if !result.documents.is_empty() {
                // Get headers from first document
                if let Some(first) = result.documents.first() {
                    if let Some(obj) = first.as_object() {
                        let headers: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
                        println!("{}", headers.join(","));

                        for doc in &result.documents {
                            if let Some(obj) = doc.as_object() {
                                let values: Vec<String> = headers
                                    .iter()
                                    .map(|h| {
                                        obj.get(*h)
                                            .map(|v| v.to_string().trim_matches('"').to_string())
                                            .unwrap_or_default()
                                    })
                                    .collect();
                                println!("{}", values.join(","));
                            }
                        }
                    }
                }
            }
        }
        OutputFormat::Table => {
            // Check for EXPLAIN output
            if let Some(explain) = &result.explain {
                println!("\n{}", "Query Plan:".cyan().bold());
                println!("{}", serde_json::to_string_pretty(explain)?);
            }

            // Check for aggregates
            if let Some(aggs) = &result.aggregates {
                println!("\n{}", "Aggregates:".cyan().bold());
                println!("{}", serde_json::to_string_pretty(aggs)?);
            }

            // Print reasoning if present
            if let Some(reasoning) = &result.reasoning {
                println!("\n{}", "Reasoning:".cyan().bold());
                println!("{}\n", reasoning);
            }

            // Print documents
            if !result.documents.is_empty() {
                println!("{}", "Results:".cyan().bold());
                for (i, doc) in result.documents.iter().enumerate() {
                    println!("\n{}. {}", (i + 1).to_string().yellow(), "-".repeat(50));
                    print_document(doc);
                }
            }

            println!(
                "\n{} {} result(s)\n",
                "Total:".dimmed(),
                result.total_count.to_string().green()
            );
        }
    }

    Ok(())
}

fn print_document(doc: &serde_json::Value) {
    if let Some(obj) = doc.as_object() {
        for (key, value) in obj {
            let display_value = match value {
                serde_json::Value::String(s) => {
                    if s.len() > 100 {
                        format!("{}...", &s[..100])
                    } else {
                        s.clone()
                    }
                }
                serde_json::Value::Array(arr) => {
                    let items: Vec<String> = arr
                        .iter()
                        .take(5)
                        .map(|v| v.to_string().trim_matches('"').to_string())
                        .collect();
                    if arr.len() > 5 {
                        format!("[{}, ...]", items.join(", "))
                    } else {
                        format!("[{}]", items.join(", "))
                    }
                }
                _ => value.to_string(),
            };
            println!("  {}: {}", key.dimmed(), display_value);
        }
    }
}

async fn start_repl(url: &str, format: OutputFormat) -> Result<()> {
    println!();
    println!("{}", "ReasonDB Query REPL".cyan().bold());
    println!(
        "{}",
        "Type RQL queries, or 'help' for commands, 'exit' to quit.".dimmed()
    );
    println!();

    let mut rl = DefaultEditor::new()?;

    // Try to load history
    let history_path = dirs::data_dir().map(|d| d.join("reasondb").join("history.txt"));

    if let Some(ref path) = history_path {
        let _ = rl.load_history(path);
    }

    loop {
        let prompt = format!("{} ", "rql>".green().bold());

        match rl.readline(&prompt) {
            Ok(line) => {
                let line = line.trim();

                if line.is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(line);

                match line.to_lowercase().as_str() {
                    "exit" | "quit" | "q" => {
                        println!("{}", "Goodbye!".dimmed());
                        break;
                    }
                    "help" | "h" | "?" => {
                        print_help();
                        continue;
                    }
                    "tables" => {
                        // Shortcut to list tables
                        let _ = execute_query(url, "SELECT * FROM __tables__", format).await;
                        continue;
                    }
                    _ => {}
                }

                // Execute the query
                if let Err(e) = execute_query(url, line, format).await {
                    output::error(&e.to_string());
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("{}", "Ctrl-C pressed, use 'exit' to quit".dimmed());
            }
            Err(ReadlineError::Eof) => {
                println!("{}", "Goodbye!".dimmed());
                break;
            }
            Err(e) => {
                output::error(&format!("Readline error: {}", e));
                break;
            }
        }
    }

    // Save history
    if let Some(ref path) = history_path {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = rl.save_history(path);
    }

    Ok(())
}

fn print_help() {
    println!();
    println!("{}", "RQL Query Language".cyan().bold());
    println!();
    println!("{}", "Basic Queries:".yellow());
    println!("  SELECT * FROM <table>              List all documents");
    println!("  SELECT * FROM <table> LIMIT 10     Limit results");
    println!("  SELECT title, author FROM <table>  Select specific fields");
    println!();
    println!("{}", "Filtering:".yellow());
    println!("  WHERE author = 'Alice'             Exact match");
    println!("  WHERE tags CONTAINS 'legal'        Array contains");
    println!("  WHERE created_at > '2024-01-01'    Date comparison");
    println!();
    println!("{}", "Search (BM25):".yellow());
    println!("  SEARCH 'payment terms'             Full-text search");
    println!();
    println!("{}", "Reasoning (LLM):".yellow());
    println!("  REASON 'What are the key terms?'   AI-powered analysis");
    println!();
    println!("{}", "Combined:".yellow());
    println!("  SELECT * FROM contracts");
    println!("  WHERE author = 'Legal'");
    println!("  SEARCH 'confidentiality'");
    println!("  REASON 'Summarize obligations'");
    println!();
    println!("{}", "Aggregations:".yellow());
    println!("  SELECT COUNT(*) FROM <table>       Count documents");
    println!("  SELECT AVG(score) FROM <table>     Average value");
    println!("  GROUP BY author                    Group results");
    println!();
    println!("{}", "Other:".yellow());
    println!("  EXPLAIN SELECT ...                 Show query plan");
    println!("  RELATED TO '<doc_id>'              Find related docs");
    println!();
    println!("{}", "Commands:".yellow());
    println!("  tables                             List all tables");
    println!("  help, h, ?                         Show this help");
    println!("  exit, quit, q                      Exit REPL");
    println!();
}
