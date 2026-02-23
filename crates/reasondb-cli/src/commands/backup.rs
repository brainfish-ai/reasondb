//! Backup and restore CLI commands

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

use reasondb_core::{
    BackupManager, BackupOptions, BackupType, ExportFormat, ExportOptions, ExportScope, Exporter,
    ImportOptions, Importer, NodeStore, RestoreOptions,
};

use crate::output::Output;

/// Backup and restore commands
#[derive(Debug, Args)]
pub struct BackupArgs {
    #[command(subcommand)]
    pub command: BackupCommand,
}

#[derive(Debug, Subcommand)]
pub enum BackupCommand {
    /// Create a backup of the database
    Create {
        /// Database path
        #[arg(short, long, default_value = "data/reasondb.redb")]
        database: PathBuf,

        /// Backup directory
        #[arg(short, long, default_value = "backups")]
        backup_dir: PathBuf,

        /// Backup type (full or incremental)
        #[arg(short = 't', long, default_value = "full")]
        backup_type: String,

        /// Optional description
        #[arg(short = 'D', long)]
        description: Option<String>,

        /// Skip verification after backup
        #[arg(long)]
        no_verify: bool,
    },

    /// List all backups
    List {
        /// Backup directory
        #[arg(short, long, default_value = "backups")]
        backup_dir: PathBuf,

        /// Database path (for manager initialization)
        #[arg(short, long, default_value = "data/reasondb.redb")]
        database: PathBuf,
    },

    /// Show backup details
    Show {
        /// Backup ID
        backup_id: String,

        /// Backup directory
        #[arg(short, long, default_value = "backups")]
        backup_dir: PathBuf,

        /// Database path
        #[arg(short, long, default_value = "data/reasondb.redb")]
        database: PathBuf,
    },

    /// Restore from a backup
    Restore {
        /// Backup ID to restore
        backup_id: String,

        /// Target database path
        #[arg(short, long)]
        target: PathBuf,

        /// Backup directory
        #[arg(short, long, default_value = "backups")]
        backup_dir: PathBuf,

        /// Source database path (for manager initialization)
        #[arg(short, long, default_value = "data/reasondb.redb")]
        database: PathBuf,

        /// Overwrite existing database
        #[arg(short, long)]
        force: bool,
    },

    /// Delete a backup
    Delete {
        /// Backup ID to delete
        backup_id: String,

        /// Backup directory
        #[arg(short, long, default_value = "backups")]
        backup_dir: PathBuf,

        /// Database path
        #[arg(short, long, default_value = "data/reasondb.redb")]
        database: PathBuf,

        /// Skip confirmation
        #[arg(short, long)]
        yes: bool,
    },

    /// Verify backup integrity
    Verify {
        /// Backup ID to verify
        backup_id: String,

        /// Backup directory
        #[arg(short, long, default_value = "backups")]
        backup_dir: PathBuf,

        /// Database path
        #[arg(short, long, default_value = "data/reasondb.redb")]
        database: PathBuf,
    },

    /// Prune old backups
    Prune {
        /// Number of full backups to keep
        #[arg(short, long, default_value = "5")]
        keep_full: usize,

        /// Number of incremental backups to keep
        #[arg(short = 'i', long, default_value = "10")]
        keep_incremental: usize,

        /// Backup directory
        #[arg(short, long, default_value = "backups")]
        backup_dir: PathBuf,

        /// Database path
        #[arg(short, long, default_value = "data/reasondb.redb")]
        database: PathBuf,
    },

    /// Export data to file
    Export {
        /// Output file path
        output: PathBuf,

        /// Database path
        #[arg(short, long, default_value = "data/reasondb.redb")]
        database: PathBuf,

        /// Export format (json, jsonl, csv)
        #[arg(short, long, default_value = "json")]
        format: String,

        /// Export specific table only
        #[arg(short, long)]
        table: Option<String>,

        /// Include nodes in export
        #[arg(long)]
        include_nodes: bool,

        /// Compact output (no pretty printing)
        #[arg(long)]
        compact: bool,
    },

    /// Import data from file
    Import {
        /// Input file path
        input: PathBuf,

        /// Database path
        #[arg(short, long, default_value = "data/reasondb.redb")]
        database: PathBuf,

        /// Import format (json, jsonl, csv)
        #[arg(short, long, default_value = "json")]
        format: String,

        /// Target table for CSV/JSONL imports
        #[arg(short, long)]
        table: Option<String>,

        /// Update existing records instead of skipping
        #[arg(long)]
        update: bool,
    },
}

impl BackupArgs {
    pub fn execute(&self, output: &Output) -> Result<()> {
        match &self.command {
            BackupCommand::Create {
                database,
                backup_dir,
                backup_type,
                description,
                no_verify,
            } => {
                let store = NodeStore::open(database).context("Failed to open database")?;
                let manager = BackupManager::new(&store, backup_dir)
                    .context("Failed to initialize backup manager")?;

                let backup_type = match backup_type.to_lowercase().as_str() {
                    "full" => BackupType::Full,
                    "incremental" | "incr" => BackupType::Incremental,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Invalid backup type: {}. Use 'full' or 'incremental'",
                            backup_type
                        ));
                    }
                };

                let options = BackupOptions {
                    backup_type,
                    description: description.clone(),
                    compression_level: 6,
                    verify: !no_verify,
                };

                if output.is_json() {
                    let backup = manager.create_backup(&store, options)?;
                    println!("{}", serde_json::to_string_pretty(&backup)?);
                } else {
                    println!("{}", "Creating backup...".cyan());
                    let backup = manager.create_backup(&store, options)?;
                    println!("\n{}", "✓ Backup created successfully!".green().bold());
                    println!();
                    println!("  {} {}", "ID:".bold(), backup.id);
                    println!("  {} {:?}", "Type:".bold(), backup.backup_type);
                    println!("  {} {}", "Tables:".bold(), backup.table_count);
                    println!("  {} {}", "Documents:".bold(), backup.document_count);
                    println!("  {} {}", "Nodes:".bold(), backup.node_count);
                    println!("  {} {}", "Size:".bold(), format_size(backup.size_bytes));
                    println!("  {} {}", "Path:".bold(), backup.path.display());
                }

                Ok(())
            }

            BackupCommand::List {
                backup_dir,
                database,
            } => {
                let store = NodeStore::open(database).context("Failed to open database")?;
                let manager = BackupManager::new(&store, backup_dir)
                    .context("Failed to initialize backup manager")?;
                let backups = manager.list_backups()?;

                if output.is_json() {
                    println!("{}", serde_json::to_string_pretty(&backups)?);
                } else if backups.is_empty() {
                    println!("{}", "No backups found.".yellow());
                } else {
                    println!("{}", "Backups:".bold().cyan());
                    println!();
                    for backup in backups {
                        let type_str = match backup.backup_type {
                            BackupType::Full => "FULL".green(),
                            BackupType::Incremental => "INCR".yellow(),
                        };
                        println!(
                            "  {} [{}] {} - {} tables, {} docs ({})",
                            backup.id.bold(),
                            type_str,
                            backup.created_at.format("%Y-%m-%d %H:%M:%S"),
                            backup.table_count,
                            backup.document_count,
                            format_size(backup.size_bytes)
                        );
                        if let Some(desc) = &backup.description {
                            println!("    {}", desc.dimmed());
                        }
                    }
                }

                Ok(())
            }

            BackupCommand::Show {
                backup_id,
                backup_dir,
                database,
            } => {
                let store = NodeStore::open(database).context("Failed to open database")?;
                let manager = BackupManager::new(&store, backup_dir)
                    .context("Failed to initialize backup manager")?;

                let backup = manager
                    .get_backup(backup_id)?
                    .ok_or_else(|| anyhow::anyhow!("Backup not found: {}", backup_id))?;

                if output.is_json() {
                    println!("{}", serde_json::to_string_pretty(&backup)?);
                } else {
                    println!("{}", "Backup Details:".bold().cyan());
                    println!();
                    println!("  {} {}", "ID:".bold(), backup.id);
                    println!("  {} {:?}", "Type:".bold(), backup.backup_type);
                    println!("  {} {}", "Created:".bold(), backup.created_at);
                    println!("  {} {}", "Tables:".bold(), backup.table_count);
                    println!("  {} {}", "Documents:".bold(), backup.document_count);
                    println!("  {} {}", "Nodes:".bold(), backup.node_count);
                    println!("  {} {}", "Size:".bold(), format_size(backup.size_bytes));
                    println!("  {} {}", "Path:".bold(), backup.path.display());
                    if let Some(desc) = &backup.description {
                        println!("  {} {}", "Description:".bold(), desc);
                    }
                    if let Some(parent) = &backup.parent_id {
                        println!("  {} {}", "Parent:".bold(), parent);
                    }
                }

                Ok(())
            }

            BackupCommand::Restore {
                backup_id,
                target,
                backup_dir,
                database,
                force,
            } => {
                let store = NodeStore::open(database).context("Failed to open database")?;
                let manager = BackupManager::new(&store, backup_dir)
                    .context("Failed to initialize backup manager")?;

                let options = RestoreOptions {
                    overwrite: *force,
                    verify_first: true,
                };

                if output.is_json() {
                    manager.restore(backup_id, target, options)?;
                    println!(
                        r#"{{"status": "restored", "target": "{}"}}"#,
                        target.display()
                    );
                } else {
                    println!("{}", "Restoring backup...".cyan());
                    manager.restore(backup_id, target, options)?;
                    println!("\n{}", "✓ Backup restored successfully!".green().bold());
                    println!("  {} {}", "Target:".bold(), target.display());
                }

                Ok(())
            }

            BackupCommand::Delete {
                backup_id,
                backup_dir,
                database,
                yes,
            } => {
                let store = NodeStore::open(database).context("Failed to open database")?;
                let manager = BackupManager::new(&store, backup_dir)
                    .context("Failed to initialize backup manager")?;

                if !yes {
                    println!(
                        "{} Are you sure you want to delete backup {}?",
                        "Warning:".yellow().bold(),
                        backup_id.bold()
                    );
                    println!("This action cannot be undone. Use --yes to confirm.");
                    return Ok(());
                }

                manager.delete_backup(backup_id)?;

                if output.is_json() {
                    println!(r#"{{"status": "deleted", "backup_id": "{}"}}"#, backup_id);
                } else {
                    println!("{} Backup {} deleted.", "✓".green(), backup_id.bold());
                }

                Ok(())
            }

            BackupCommand::Verify {
                backup_id,
                backup_dir,
                database,
            } => {
                let store = NodeStore::open(database).context("Failed to open database")?;
                let manager = BackupManager::new(&store, backup_dir)
                    .context("Failed to initialize backup manager")?;

                let valid = manager.verify(backup_id)?;

                if output.is_json() {
                    println!(r#"{{"backup_id": "{}", "valid": {}}}"#, backup_id, valid);
                } else if valid {
                    println!("{} Backup {} is valid.", "✓".green(), backup_id.bold());
                } else {
                    println!("{} Backup {} is corrupted!", "✗".red(), backup_id.bold());
                }

                Ok(())
            }

            BackupCommand::Prune {
                keep_full,
                keep_incremental,
                backup_dir,
                database,
            } => {
                let store = NodeStore::open(database).context("Failed to open database")?;
                let manager = BackupManager::new(&store, backup_dir)
                    .context("Failed to initialize backup manager")?;

                let deleted = manager.prune(*keep_full, *keep_incremental)?;

                if output.is_json() {
                    println!("{}", serde_json::to_string_pretty(&deleted)?);
                } else if deleted.is_empty() {
                    println!("{}", "No backups to prune.".yellow());
                } else {
                    println!("{} Pruned {} backup(s):", "✓".green(), deleted.len());
                    for backup in deleted {
                        println!("  - {}", backup.id);
                    }
                }

                Ok(())
            }

            BackupCommand::Export {
                output: output_path,
                database,
                format,
                table,
                include_nodes,
                compact,
            } => {
                let store = NodeStore::open(database).context("Failed to open database")?;

                let export_format = match format.to_lowercase().as_str() {
                    "json" => ExportFormat::Json,
                    "jsonl" | "jsonlines" => ExportFormat::JsonLines,
                    "csv" => ExportFormat::Csv,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Invalid format: {}. Use 'json', 'jsonl', or 'csv'",
                            format
                        ));
                    }
                };

                let mut options = ExportOptions {
                    format: export_format,
                    scope: if table.is_some() {
                        ExportScope::Table
                    } else {
                        ExportScope::All
                    },
                    table_id: table.clone(),
                    include_nodes: *include_nodes,
                    pretty: !compact,
                };

                if let Some(tbl) = table {
                    options = options.for_table(tbl);
                }

                if output.is_json() {
                    let metadata = Exporter::export(&store, output_path, options)?;
                    println!("{}", serde_json::to_string_pretty(&metadata)?);
                } else {
                    println!("{}", "Exporting data...".cyan());
                    let metadata = Exporter::export(&store, output_path, options)?;
                    println!("\n{}", "✓ Export completed!".green().bold());
                    println!();
                    println!("  {} {}", "Output:".bold(), output_path.display());
                    println!("  {} {}", "Format:".bold(), metadata.format);
                    println!("  {} {}", "Tables:".bold(), metadata.table_count);
                    println!("  {} {}", "Documents:".bold(), metadata.document_count);
                    println!("  {} {}", "Nodes:".bold(), metadata.node_count);
                }

                Ok(())
            }

            BackupCommand::Import {
                input,
                database,
                format,
                table,
                update,
            } => {
                let store = NodeStore::open(database).context("Failed to open database")?;

                let import_format = match format.to_lowercase().as_str() {
                    "json" => ExportFormat::Json,
                    "jsonl" | "jsonlines" => ExportFormat::JsonLines,
                    "csv" => ExportFormat::Csv,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Invalid format: {}. Use 'json', 'jsonl', or 'csv'",
                            format
                        ));
                    }
                };

                // CSV and JSONL require a target table
                if (import_format == ExportFormat::Csv || import_format == ExportFormat::JsonLines)
                    && table.is_none()
                {
                    return Err(anyhow::anyhow!(
                        "CSV and JSONL imports require a target table (--table)"
                    ));
                }

                let mut options = ImportOptions {
                    format: import_format,
                    skip_existing: !update,
                    update_existing: *update,
                    target_table_id: table.clone(),
                };

                if let Some(tbl) = table {
                    options = options.into_table(tbl);
                }

                if output.is_json() {
                    let result = Importer::import(&store, input, options)?;
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    println!("{}", "Importing data...".cyan());
                    let result = Importer::import(&store, input, options)?;
                    println!("\n{}", "✓ Import completed!".green().bold());
                    println!();
                    println!("  {} {}", "Tables imported:".bold(), result.tables_imported);
                    println!("  {} {}", "Tables skipped:".bold(), result.tables_skipped);
                    println!(
                        "  {} {}",
                        "Documents imported:".bold(),
                        result.documents_imported
                    );
                    println!(
                        "  {} {}",
                        "Documents skipped:".bold(),
                        result.documents_skipped
                    );
                    println!("  {} {}", "Nodes imported:".bold(), result.nodes_imported);
                    println!("  {} {}", "Nodes skipped:".bold(), result.nodes_skipped);
                }

                Ok(())
            }
        }
    }
}

fn format_size(bytes: u64) -> String {
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
