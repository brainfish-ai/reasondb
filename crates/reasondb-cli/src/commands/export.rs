//! Export command - export data to files

use crate::output;
use anyhow::Result;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
struct Document {
    id: String,
    title: String,
    table_id: Option<String>,
    tags: Vec<String>,
    author: Option<String>,
    created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DocumentsResponse {
    documents: Vec<Document>,
    total: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TreeResponse {
    document: Document,
    tree: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExportDocument {
    id: String,
    title: String,
    content: String,
    tags: Vec<String>,
    author: Option<String>,
    created_at: String,
}

pub async fn run(
    url: &str,
    output_path: String,
    table: Option<String>,
    format: String,
) -> Result<()> {
    let client = reqwest::Client::new();

    // Fetch documents
    let mut request_url = format!("{}/v1/documents?limit=10000", url);
    if let Some(table_id) = &table {
        request_url.push_str(&format!("&table_id={}", table_id));
    }

    println!(
        "Fetching documents{}...",
        table
            .as_ref()
            .map(|t| format!(" from table '{}'", t.cyan()))
            .unwrap_or_default()
    );

    let response: DocumentsResponse = client.get(&request_url).send().await?.json().await?;

    if response.documents.is_empty() {
        output::info("No documents to export");
        return Ok(());
    }

    println!(
        "Found {} documents, fetching content...",
        response.total.to_string().cyan()
    );

    // Fetch full content for each document
    let mut export_docs: Vec<ExportDocument> = Vec::new();

    for doc in &response.documents {
        // Fetch the tree to get content
        match client
            .get(format!("{}/v1/documents/{}/tree", url, doc.id))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let tree: TreeResponse = resp.json().await?;
                let content = extract_content(&tree.tree);

                export_docs.push(ExportDocument {
                    id: doc.id.clone(),
                    title: doc.title.clone(),
                    content,
                    tags: doc.tags.clone(),
                    author: doc.author.clone(),
                    created_at: doc.created_at.clone(),
                });
            }
            _ => {
                // Skip documents we can't fetch
                continue;
            }
        }
    }

    // Write to file
    let path = Path::new(&output_path);

    match format.to_lowercase().as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&export_docs)?;
            let mut file = File::create(path)?;
            file.write_all(json.as_bytes())?;
        }
        "csv" => {
            let mut wtr = csv::Writer::from_path(path)?;

            // Write header
            wtr.write_record(["id", "title", "content", "tags", "author", "created_at"])?;

            // Write rows
            for doc in &export_docs {
                wtr.write_record([
                    &doc.id,
                    &doc.title,
                    &doc.content,
                    &doc.tags.join(";"),
                    doc.author.as_deref().unwrap_or(""),
                    &doc.created_at,
                ])?;
            }

            wtr.flush()?;
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown format '{}'. Use 'json' or 'csv'.",
                format
            ));
        }
    }

    output::success(&format!(
        "Exported {} documents to {}",
        export_docs.len().to_string().green(),
        output_path.cyan()
    ));

    Ok(())
}

/// Extract text content from a document tree
fn extract_content(tree: &serde_json::Value) -> String {
    let mut content = String::new();
    extract_content_recursive(tree, &mut content);
    content.trim().to_string()
}

fn extract_content_recursive(node: &serde_json::Value, content: &mut String) {
    if let Some(text) = node.get("content").and_then(|c| c.as_str()) {
        if !text.is_empty() {
            content.push_str(text);
            content.push_str("\n\n");
        }
    }

    if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        for child in children {
            extract_content_recursive(child, content);
        }
    }
}
