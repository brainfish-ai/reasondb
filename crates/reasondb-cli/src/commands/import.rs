//! Import command - import data from files

use crate::output;
use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::path::Path;

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
}

#[derive(Debug, Serialize, Deserialize)]
struct ImportDocument {
    title: String,
    content: String,
    tags: Option<Vec<String>>,
    author: Option<String>,
}

pub async fn run(
    url: &str,
    file: String,
    table: Option<String>,
    title: Option<String>,
) -> Result<()> {
    let path = Path::new(&file);

    if !path.exists() {
        return Err(anyhow::anyhow!("File not found: {}", file));
    }

    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        "json" => import_json(url, &file, table).await,
        "csv" => import_csv(url, &file, table).await,
        "txt" | "md" | "markdown" => import_text(url, &file, table, title).await,
        _ => {
            // Try to import as text
            output::warning(&format!(
                "Unknown file type '{}', attempting to import as text",
                extension
            ));
            import_text(url, &file, table, title).await
        }
    }
}

async fn import_json(url: &str, file: &str, table: Option<String>) -> Result<()> {
    let content = std::fs::read_to_string(file)?;
    let documents: Vec<ImportDocument> = serde_json::from_str(&content)?;

    println!(
        "Importing {} documents from {}...",
        documents.len().to_string().cyan(),
        file.green()
    );

    let pb = ProgressBar::new(documents.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")?
            .progress_chars("█▓░"),
    );

    let client = reqwest::Client::new();
    let mut success = 0;
    let mut failed = 0;

    for doc in documents {
        let request = IngestRequest {
            title: doc.title.clone(),
            content: doc.content,
            table_id: table.clone(),
            tags: doc.tags,
            author: doc.author,
        };

        match client
            .post(format!("{}/v1/ingest/text", url))
            .json(&request)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                success += 1;
            }
            Ok(_) | Err(_) => {
                failed += 1;
            }
        }

        pb.inc(1);
    }

    pb.finish_with_message("Done");
    println!();

    output::success(&format!(
        "Imported {} documents ({} failed)",
        success.to_string().green(),
        failed.to_string().red()
    ));

    Ok(())
}

async fn import_csv(url: &str, file: &str, table: Option<String>) -> Result<()> {
    let mut reader = csv::Reader::from_path(file)?;

    // Check headers
    let headers = reader.headers()?.clone();
    let title_idx = headers.iter().position(|h| h == "title");
    let content_idx = headers.iter().position(|h| h == "content");

    if title_idx.is_none() || content_idx.is_none() {
        return Err(anyhow::anyhow!(
            "CSV must have 'title' and 'content' columns"
        ));
    }

    let title_idx = title_idx.unwrap();
    let content_idx = content_idx.unwrap();
    let tags_idx = headers.iter().position(|h| h == "tags");
    let author_idx = headers.iter().position(|h| h == "author");

    // Count rows for progress bar
    let row_count = csv::Reader::from_path(file)?.records().count();

    println!(
        "Importing {} rows from {}...",
        row_count.to_string().cyan(),
        file.green()
    );

    let pb = ProgressBar::new(row_count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")?
            .progress_chars("█▓░"),
    );

    let client = reqwest::Client::new();
    let mut reader = csv::Reader::from_path(file)?;
    let mut success = 0;
    let mut failed = 0;

    for result in reader.records() {
        let record = result?;

        let title = record.get(title_idx).unwrap_or("").to_string();
        let content = record.get(content_idx).unwrap_or("").to_string();

        if title.is_empty() || content.is_empty() {
            failed += 1;
            pb.inc(1);
            continue;
        }

        let tags: Option<Vec<String>> = tags_idx
            .and_then(|i| record.get(i))
            .filter(|s| !s.is_empty())
            .map(|s| s.split(';').map(|t| t.trim().to_string()).collect());

        let author = author_idx
            .and_then(|i| record.get(i))
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let request = IngestRequest {
            title,
            content,
            table_id: table.clone(),
            tags,
            author,
        };

        match client
            .post(format!("{}/v1/ingest/text", url))
            .json(&request)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                success += 1;
            }
            Ok(_) | Err(_) => {
                failed += 1;
            }
        }

        pb.inc(1);
    }

    pb.finish_with_message("Done");
    println!();

    output::success(&format!(
        "Imported {} documents ({} failed)",
        success.to_string().green(),
        failed.to_string().red()
    ));

    Ok(())
}

async fn import_text(
    url: &str,
    file: &str,
    table: Option<String>,
    title: Option<String>,
) -> Result<()> {
    let content = std::fs::read_to_string(file)?;
    let path = Path::new(file);

    let title = title.unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string()
    });

    println!("Importing {} as '{}'...", file.green(), title.cyan());

    let client = reqwest::Client::new();
    let request = IngestRequest {
        title: title.clone(),
        content,
        table_id: table,
        tags: None,
        author: None,
    };

    let response: IngestResponse = client
        .post(format!("{}/v1/ingest/text", url))
        .json(&request)
        .send()
        .await?
        .json()
        .await?;

    output::success(&format!("Imported document '{}'", title.green()));
    println!("  {} {}", "ID:".dimmed(), response.document_id);
    println!("  {} {}", "Nodes:".dimmed(), response.total_nodes);

    Ok(())
}
