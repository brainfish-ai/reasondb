//! Export and import functionality for JSON/CSV formats

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

use crate::error::{ReasonError, Result};
use crate::model::{Document, PageNode, Table};
use crate::store::NodeStore;

/// Export format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// JSON format (full fidelity)
    Json,
    /// JSON Lines format (one record per line)
    JsonLines,
    /// CSV format (documents only, flattened)
    Csv,
}

/// What to export
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportScope {
    /// Export everything
    All,
    /// Export only tables
    Tables,
    /// Export only documents
    Documents,
    /// Export a specific table's documents
    Table,
}

/// Options for exporting data
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// Export format
    pub format: ExportFormat,
    /// What to export
    pub scope: ExportScope,
    /// Table ID (for Table scope)
    pub table_id: Option<String>,
    /// Include nodes in export
    pub include_nodes: bool,
    /// Pretty print JSON
    pub pretty: bool,
}

impl ExportOptions {
    /// Create options for JSON export
    pub fn json() -> Self {
        Self {
            format: ExportFormat::Json,
            scope: ExportScope::All,
            table_id: None,
            include_nodes: true,
            pretty: true,
        }
    }

    /// Create options for JSON Lines export
    pub fn jsonl() -> Self {
        Self {
            format: ExportFormat::JsonLines,
            scope: ExportScope::Documents,
            table_id: None,
            include_nodes: false,
            pretty: false,
        }
    }

    /// Create options for CSV export
    pub fn csv() -> Self {
        Self {
            format: ExportFormat::Csv,
            scope: ExportScope::Documents,
            table_id: None,
            include_nodes: false,
            pretty: false,
        }
    }

    /// Set scope to a specific table
    pub fn for_table(mut self, table_id: impl Into<String>) -> Self {
        self.scope = ExportScope::Table;
        self.table_id = Some(table_id.into());
        self
    }

    /// Include nodes in export
    pub fn with_nodes(mut self) -> Self {
        self.include_nodes = true;
        self
    }

    /// Disable pretty printing
    pub fn compact(mut self) -> Self {
        self.pretty = false;
        self
    }
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self::json()
    }
}

/// Options for importing data
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// Import format
    pub format: ExportFormat,
    /// Whether to skip existing records
    pub skip_existing: bool,
    /// Whether to update existing records
    pub update_existing: bool,
    /// Target table ID (for CSV/JSONL imports)
    pub target_table_id: Option<String>,
}

impl ImportOptions {
    /// Create options for JSON import
    pub fn json() -> Self {
        Self {
            format: ExportFormat::Json,
            skip_existing: true,
            update_existing: false,
            target_table_id: None,
        }
    }

    /// Create options for JSON Lines import
    pub fn jsonl() -> Self {
        Self {
            format: ExportFormat::JsonLines,
            skip_existing: true,
            update_existing: false,
            target_table_id: None,
        }
    }

    /// Create options for CSV import
    pub fn csv() -> Self {
        Self {
            format: ExportFormat::Csv,
            skip_existing: true,
            update_existing: false,
            target_table_id: None,
        }
    }

    /// Set target table for import
    pub fn into_table(mut self, table_id: impl Into<String>) -> Self {
        self.target_table_id = Some(table_id.into());
        self
    }

    /// Update existing records instead of skipping
    pub fn update_mode(mut self) -> Self {
        self.skip_existing = false;
        self.update_existing = true;
        self
    }
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self::json()
    }
}

/// Exported data structure for JSON format
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportData {
    /// Export metadata
    pub metadata: ExportMetadata,
    /// Tables
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tables: Vec<Table>,
    /// Documents
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub documents: Vec<Document>,
    /// Nodes (optional)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub nodes: Vec<PageNode>,
}

/// Export metadata
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportMetadata {
    /// ReasonDB version
    pub version: String,
    /// Export timestamp
    pub exported_at: String,
    /// Export format
    pub format: String,
    /// Number of tables
    pub table_count: usize,
    /// Number of documents
    pub document_count: usize,
    /// Number of nodes
    pub node_count: usize,
}

/// Handles data export
pub struct Exporter;

impl Exporter {
    /// Export data to a file
    pub fn export<P: AsRef<Path>>(
        store: &NodeStore,
        path: P,
        options: ExportOptions,
    ) -> Result<ExportMetadata> {
        let path = path.as_ref();

        // Collect data based on scope
        let tables = match options.scope {
            ExportScope::All | ExportScope::Tables => store.list_tables()?,
            _ => Vec::new(),
        };

        let documents = match options.scope {
            ExportScope::All | ExportScope::Documents => store.list_all_documents()?,
            ExportScope::Table => {
                let table_id = options.table_id.as_ref().ok_or_else(|| {
                    ReasonError::Backup("Table ID required for table export".to_string())
                })?;
                store.get_table_documents(table_id)?
            }
            ExportScope::Tables => Vec::new(),
        };

        let nodes = if options.include_nodes {
            let mut all_nodes = Vec::new();
            for doc in &documents {
                all_nodes.extend(store.get_nodes_for_document(&doc.id)?);
            }
            all_nodes
        } else {
            Vec::new()
        };

        let metadata = ExportMetadata {
            version: env!("CARGO_PKG_VERSION").to_string(),
            exported_at: chrono::Utc::now().to_rfc3339(),
            format: format!("{:?}", options.format),
            table_count: tables.len(),
            document_count: documents.len(),
            node_count: nodes.len(),
        };

        // Write to file based on format
        match options.format {
            ExportFormat::Json => {
                Self::export_json(path, &tables, &documents, &nodes, &metadata, options.pretty)?;
            }
            ExportFormat::JsonLines => {
                Self::export_jsonl(path, &documents)?;
            }
            ExportFormat::Csv => {
                Self::export_csv(path, &documents)?;
            }
        }

        Ok(metadata)
    }

    fn export_json(
        path: &Path,
        tables: &[Table],
        documents: &[Document],
        nodes: &[PageNode],
        metadata: &ExportMetadata,
        pretty: bool,
    ) -> Result<()> {
        let data = ExportData {
            metadata: ExportMetadata {
                version: metadata.version.clone(),
                exported_at: metadata.exported_at.clone(),
                format: metadata.format.clone(),
                table_count: metadata.table_count,
                document_count: metadata.document_count,
                node_count: metadata.node_count,
            },
            tables: tables.to_vec(),
            documents: documents.to_vec(),
            nodes: nodes.to_vec(),
        };

        let file = File::create(path)
            .map_err(|e| ReasonError::Backup(format!("Failed to create export file: {}", e)))?;
        let writer = BufWriter::new(file);

        if pretty {
            serde_json::to_writer_pretty(writer, &data)
        } else {
            serde_json::to_writer(writer, &data)
        }
        .map_err(|e| ReasonError::Backup(format!("Failed to write JSON: {}", e)))?;

        Ok(())
    }

    fn export_jsonl(path: &Path, documents: &[Document]) -> Result<()> {
        let file = File::create(path)
            .map_err(|e| ReasonError::Backup(format!("Failed to create export file: {}", e)))?;
        let mut writer = BufWriter::new(file);

        for doc in documents {
            let line = serde_json::to_string(doc)
                .map_err(|e| ReasonError::Backup(format!("Failed to serialize document: {}", e)))?;
            writeln!(writer, "{}", line)
                .map_err(|e| ReasonError::Backup(format!("Failed to write line: {}", e)))?;
        }

        Ok(())
    }

    fn export_csv(path: &Path, documents: &[Document]) -> Result<()> {
        let file = File::create(path)
            .map_err(|e| ReasonError::Backup(format!("Failed to create export file: {}", e)))?;
        let mut writer = csv::Writer::from_writer(BufWriter::new(file));

        // Write header
        writer
            .write_record([
                "id",
                "table_id",
                "title",
                "tags",
                "source_path",
                "created_at",
                "updated_at",
            ])
            .map_err(|e| ReasonError::Backup(format!("Failed to write CSV header: {}", e)))?;

        // Write documents
        for doc in documents {
            writer
                .write_record([
                    &doc.id,
                    &doc.table_id,
                    &doc.title,
                    &doc.tags.join(";"),
                    &doc.source_path,
                    &doc.created_at.to_rfc3339(),
                    &doc.updated_at.to_rfc3339(),
                ])
                .map_err(|e| ReasonError::Backup(format!("Failed to write CSV row: {}", e)))?;
        }

        writer
            .flush()
            .map_err(|e| ReasonError::Backup(format!("Failed to flush CSV: {}", e)))?;

        Ok(())
    }
}

/// Handles data import
pub struct Importer;

impl Importer {
    /// Import data from a file
    pub fn import<P: AsRef<Path>>(
        store: &NodeStore,
        path: P,
        options: ImportOptions,
    ) -> Result<ImportResult> {
        let path = path.as_ref();

        match options.format {
            ExportFormat::Json => Self::import_json(store, path, &options),
            ExportFormat::JsonLines => Self::import_jsonl(store, path, &options),
            ExportFormat::Csv => Self::import_csv(store, path, &options),
        }
    }

    fn import_json(
        store: &NodeStore,
        path: &Path,
        options: &ImportOptions,
    ) -> Result<ImportResult> {
        let file = File::open(path)
            .map_err(|e| ReasonError::Backup(format!("Failed to open import file: {}", e)))?;

        let data: ExportData = serde_json::from_reader(BufReader::new(file))
            .map_err(|e| ReasonError::Backup(format!("Failed to parse JSON: {}", e)))?;

        let mut result = ImportResult::default();

        // Import tables
        for table in data.tables {
            match store.insert_table(&table) {
                Ok(_) => result.tables_imported += 1,
                Err(_) if options.skip_existing => result.tables_skipped += 1,
                Err(e) => return Err(e),
            }
        }

        // Import documents
        for doc in data.documents {
            match store.insert_document(&doc) {
                Ok(_) => result.documents_imported += 1,
                Err(_) if options.skip_existing => result.documents_skipped += 1,
                Err(e) => return Err(e),
            }
        }

        // Import nodes
        for node in data.nodes {
            match store.insert_node(&node) {
                Ok(_) => result.nodes_imported += 1,
                Err(_) if options.skip_existing => result.nodes_skipped += 1,
                Err(e) => return Err(e),
            }
        }

        Ok(result)
    }

    fn import_jsonl(
        store: &NodeStore,
        path: &Path,
        options: &ImportOptions,
    ) -> Result<ImportResult> {
        use std::io::BufRead;

        let file = File::open(path)
            .map_err(|e| ReasonError::Backup(format!("Failed to open import file: {}", e)))?;

        let reader = BufReader::new(file);
        let mut result = ImportResult::default();

        for line in reader.lines() {
            let line =
                line.map_err(|e| ReasonError::Backup(format!("Failed to read line: {}", e)))?;

            if line.trim().is_empty() {
                continue;
            }

            let mut doc: Document = serde_json::from_str(&line)
                .map_err(|e| ReasonError::Backup(format!("Failed to parse document: {}", e)))?;

            // Override table_id if specified
            if let Some(ref table_id) = options.target_table_id {
                doc.table_id = table_id.clone();
            }

            match store.insert_document(&doc) {
                Ok(_) => result.documents_imported += 1,
                Err(_) if options.skip_existing => result.documents_skipped += 1,
                Err(e) => return Err(e),
            }
        }

        Ok(result)
    }

    fn import_csv(store: &NodeStore, path: &Path, options: &ImportOptions) -> Result<ImportResult> {
        let file = File::open(path)
            .map_err(|e| ReasonError::Backup(format!("Failed to open import file: {}", e)))?;

        let mut reader = csv::Reader::from_reader(BufReader::new(file));
        let mut result = ImportResult::default();

        let table_id = options.target_table_id.clone().ok_or_else(|| {
            ReasonError::Backup("Target table ID required for CSV import".to_string())
        })?;

        for record in reader.records() {
            let record = record
                .map_err(|e| ReasonError::Backup(format!("Failed to read CSV record: {}", e)))?;

            // Parse CSV fields
            let title = record.get(2).unwrap_or("Untitled").to_string();
            let tags: Vec<String> = record
                .get(3)
                .map(|s| {
                    s.split(';')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let source_path = record.get(4).unwrap_or("").to_string();

            let mut doc = Document::new(title, &table_id);
            doc.tags = tags;
            doc.source_path = source_path;

            match store.insert_document(&doc) {
                Ok(_) => result.documents_imported += 1,
                Err(_) if options.skip_existing => result.documents_skipped += 1,
                Err(e) => return Err(e),
            }
        }

        Ok(result)
    }
}

/// Result of an import operation
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    /// Number of tables imported
    pub tables_imported: usize,
    /// Number of tables skipped (already exist)
    pub tables_skipped: usize,
    /// Number of documents imported
    pub documents_imported: usize,
    /// Number of documents skipped
    pub documents_skipped: usize,
    /// Number of nodes imported
    pub nodes_imported: usize,
    /// Number of nodes skipped
    pub nodes_skipped: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_export_options() {
        let opts = ExportOptions::json().for_table("tbl_123").compact();

        assert_eq!(opts.format, ExportFormat::Json);
        assert_eq!(opts.scope, ExportScope::Table);
        assert_eq!(opts.table_id, Some("tbl_123".to_string()));
        assert!(!opts.pretty);
    }

    #[test]
    fn test_json_export_import() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.redb");
        let export_path = dir.path().join("export.json");

        // Create store with data
        let store = NodeStore::open(&db_path).unwrap();
        let table = Table::new("Test".to_string());
        store.insert_table(&table).unwrap();

        let mut doc = Document::new("Doc 1".to_string(), &table.id);
        doc.tags = vec!["test".to_string()];
        store.insert_document(&doc).unwrap();

        // Export
        let metadata = Exporter::export(&store, &export_path, ExportOptions::json()).unwrap();
        assert_eq!(metadata.table_count, 1);
        assert_eq!(metadata.document_count, 1);

        // Import to new store
        let db_path2 = dir.path().join("test2.redb");
        let store2 = NodeStore::open(&db_path2).unwrap();
        let result = Importer::import(&store2, &export_path, ImportOptions::json()).unwrap();

        assert_eq!(result.tables_imported, 1);
        assert_eq!(result.documents_imported, 1);
    }

    #[test]
    fn test_csv_export() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.redb");
        let export_path = dir.path().join("export.csv");

        let store = NodeStore::open(&db_path).unwrap();
        let table = Table::new("Test".to_string());
        store.insert_table(&table).unwrap();

        let mut doc = Document::new("Test Doc".to_string(), &table.id);
        doc.set_metadata("author", serde_json::json!("Alice"));
        doc.tags = vec!["tag1".to_string(), "tag2".to_string()];
        store.insert_document(&doc).unwrap();

        // Export
        let metadata = Exporter::export(&store, &export_path, ExportOptions::csv()).unwrap();
        assert_eq!(metadata.document_count, 1);

        // Check file exists
        assert!(export_path.exists());
    }
}
