//! Output formatting utilities

use clap::ValueEnum;
use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Cell, ContentArrangement, Table};
use serde::Serialize;

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
    Csv,
}

/// Print data in the specified format
#[allow(dead_code)]
pub fn print_data<T: Serialize>(data: &T, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(data).unwrap());
        }
        OutputFormat::Csv => {
            // For CSV, we'd need to handle this differently per type
            println!("{}", serde_json::to_string(data).unwrap());
        }
        OutputFormat::Table => {
            // Table format is handled by specific functions
            println!("{}", serde_json::to_string_pretty(data).unwrap());
        }
    }
}

/// Create a styled table
pub fn create_table(headers: Vec<&str>) -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);

    let header_cells: Vec<Cell> = headers
        .iter()
        .map(|h| Cell::new(h.to_uppercase()).fg(comfy_table::Color::Cyan))
        .collect();
    table.set_header(header_cells);

    table
}

/// Print success message
pub fn success(msg: &str) {
    println!("{} {}", "✓".green().bold(), msg);
}

/// Print info message
pub fn info(msg: &str) {
    println!("{} {}", "ℹ".blue().bold(), msg);
}

/// Print warning message
pub fn warning(msg: &str) {
    println!("{} {}", "⚠".yellow().bold(), msg);
}

/// Print error message
pub fn error(msg: &str) {
    eprintln!("{} {}", "✗".red().bold(), msg);
}

/// Format a document ID for display (shortened)
pub fn format_id(id: &str) -> String {
    if id.len() > 8 {
        format!("{}...", &id[..8])
    } else {
        id.to_string()
    }
}

/// Format bytes as human-readable size
#[allow(dead_code)]
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format duration in milliseconds as human-readable
pub fn format_duration_ms(ms: u64) -> String {
    if ms >= 60000 {
        format!("{:.1}m", ms as f64 / 60000.0)
    } else if ms >= 1000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}
