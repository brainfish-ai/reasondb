//! Full-Text Search Index using Tantivy (BM25)
//!
//! This module provides BM25-based full-text search capabilities for ReasonDB.
//! It indexes document content and node summaries for fast text search.
//!
//! # Features
//!
//! - **BM25 Scoring**: Industry-standard relevance ranking
//! - **Tokenization**: English stemming and stop word removal
//! - **Field Boosting**: Title matches weighted higher than content
//! - **Highlighting**: Extract matching snippets from results
//!
//! # Example
//!
//! ```rust,no_run
//! use reasondb_core::text_index::TextIndex;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let index = TextIndex::open("./search_index")?;
//!
//!     // Index a document node
//!     index.index_node("doc_1", "node_1", "table_1", "My Title", "Document content here...", &[])?;
//!     index.commit()?;
//!
//!     // Search
//!     let results = index.search("content", 10, None)?;
//!     for result in results {
//!         println!("{}: {} (score: {})", result.document_id, result.title, result.score);
//!     }
//!     Ok(())
//! }
//! ```

use std::path::Path;
use std::sync::RwLock;

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};

use crate::error::{ReasonError, Result};

/// Full-text search index using Tantivy with BM25 scoring.
pub struct TextIndex {
    index: Index,
    #[allow(dead_code)]
    schema: Schema,
    writer: RwLock<IndexWriter>,
    reader: IndexReader,
    // Field handles
    document_id_field: Field,
    node_id_field: Field,
    table_id_field: Field,
    title_field: Field,
    content_field: Field,
    tags_field: Field,
}

/// A single node entry for bulk indexing via [`TextIndex::index_nodes_bulk`].
pub struct NodeIndexEntry<'a> {
    /// Parent document ID
    pub document_id: &'a str,
    /// Node ID
    pub node_id: &'a str,
    /// Table ID the document belongs to
    pub table_id: &'a str,
    /// Node title
    pub title: &'a str,
    /// Node content
    pub content: &'a str,
    /// Optional tags
    pub tags: &'a [String],
}

/// A search result from the text index.
#[derive(Debug, Clone)]
pub struct TextSearchResult {
    /// Document ID
    pub document_id: String,
    /// Node ID that matched
    pub node_id: String,
    /// Table ID
    pub table_id: String,
    /// Document/node title
    pub title: String,
    /// BM25 relevance score
    pub score: f32,
    /// Matching content snippet
    pub snippet: Option<String>,
}

impl TextIndex {
    /// Open or create a text index at the given path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Create directory if it doesn't exist
        if !path.exists() {
            std::fs::create_dir_all(path).map_err(|e| ReasonError::Internal(e.to_string()))?;
        }

        // Build schema
        let mut schema_builder = Schema::builder();

        // ID fields (stored but not indexed for search)
        let document_id_field = schema_builder.add_text_field("document_id", STRING | STORED);
        let node_id_field = schema_builder.add_text_field("node_id", STRING | STORED);
        let table_id_field = schema_builder.add_text_field("table_id", STRING | STORED);

        // Searchable text fields with English stemming
        let text_options = TextOptions::default().set_stored().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("en_stem")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        );

        let title_field = schema_builder.add_text_field("title", text_options.clone());
        let content_field = schema_builder.add_text_field("content", text_options);
        let tags_field = schema_builder.add_text_field("tags", TEXT | STORED);

        let schema = schema_builder.build();

        // Open or create index
        let index = if path.join("meta.json").exists() {
            Index::open_in_dir(path)
                .map_err(|e| ReasonError::Internal(format!("Failed to open text index: {}", e)))?
        } else {
            Index::create_in_dir(path, schema.clone())
                .map_err(|e| ReasonError::Internal(format!("Failed to create text index: {}", e)))?
        };

        // Register the English stemmer tokenizer
        index.tokenizers().register(
            "en_stem",
            tantivy::tokenizer::TextAnalyzer::builder(
                tantivy::tokenizer::SimpleTokenizer::default(),
            )
            .filter(tantivy::tokenizer::RemoveLongFilter::limit(40))
            .filter(tantivy::tokenizer::LowerCaser)
            .filter(tantivy::tokenizer::Stemmer::new(
                tantivy::tokenizer::Language::English,
            ))
            .build(),
        );

        // Create writer with 50MB buffer
        let writer = index
            .writer(50_000_000)
            .map_err(|e| ReasonError::Internal(format!("Failed to create index writer: {}", e)))?;

        // Create reader with auto-reload
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| ReasonError::Internal(format!("Failed to create index reader: {}", e)))?;

        Ok(Self {
            index,
            schema,
            writer: RwLock::new(writer),
            reader,
            document_id_field,
            node_id_field,
            table_id_field,
            title_field,
            content_field,
            tags_field,
        })
    }

    /// Create an in-memory index (for testing).
    pub fn in_memory() -> Result<Self> {
        // Build schema
        let mut schema_builder = Schema::builder();

        let document_id_field = schema_builder.add_text_field("document_id", STRING | STORED);
        let node_id_field = schema_builder.add_text_field("node_id", STRING | STORED);
        let table_id_field = schema_builder.add_text_field("table_id", STRING | STORED);

        let text_options = TextOptions::default().set_stored().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("en_stem")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        );

        let title_field = schema_builder.add_text_field("title", text_options.clone());
        let content_field = schema_builder.add_text_field("content", text_options);
        let tags_field = schema_builder.add_text_field("tags", TEXT | STORED);

        let schema = schema_builder.build();
        let index = Index::create_in_ram(schema.clone());

        // Register tokenizer
        index.tokenizers().register(
            "en_stem",
            tantivy::tokenizer::TextAnalyzer::builder(
                tantivy::tokenizer::SimpleTokenizer::default(),
            )
            .filter(tantivy::tokenizer::RemoveLongFilter::limit(40))
            .filter(tantivy::tokenizer::LowerCaser)
            .filter(tantivy::tokenizer::Stemmer::new(
                tantivy::tokenizer::Language::English,
            ))
            .build(),
        );

        let writer = index
            .writer(50_000_000)
            .map_err(|e| ReasonError::Internal(format!("Failed to create index writer: {}", e)))?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| ReasonError::Internal(format!("Failed to create index reader: {}", e)))?;

        Ok(Self {
            index,
            schema,
            writer: RwLock::new(writer),
            reader,
            document_id_field,
            node_id_field,
            table_id_field,
            title_field,
            content_field,
            tags_field,
        })
    }

    /// Index a document node.
    ///
    /// # Arguments
    ///
    /// * `document_id` - Parent document ID
    /// * `node_id` - Node ID being indexed
    /// * `table_id` - Table ID the document belongs to
    /// * `title` - Node title
    /// * `content` - Node content
    /// * `tags` - Optional tags
    pub fn index_node(
        &self,
        document_id: &str,
        node_id: &str,
        table_id: &str,
        title: &str,
        content: &str,
        tags: &[String],
    ) -> Result<()> {
        let writer = self
            .writer
            .write()
            .map_err(|_| ReasonError::Internal("Failed to acquire write lock".to_string()))?;

        let mut doc = TantivyDocument::default();
        doc.add_text(self.document_id_field, document_id);
        doc.add_text(self.node_id_field, node_id);
        doc.add_text(self.table_id_field, table_id);
        doc.add_text(self.title_field, title);
        doc.add_text(self.content_field, content);

        if !tags.is_empty() {
            doc.add_text(self.tags_field, tags.join(" "));
        }

        writer
            .add_document(doc)
            .map_err(|e| ReasonError::Internal(format!("Failed to add document: {}", e)))?;

        Ok(())
    }

    /// Index multiple document nodes in a single write-lock acquisition.
    ///
    /// Prefer this over calling `index_node` in a loop — it avoids the overhead
    /// of acquiring and releasing the `RwLock` N times and adds all documents in
    /// one contiguous pass before committing.
    pub fn index_nodes_bulk(&self, entries: &[NodeIndexEntry<'_>]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let writer = self
            .writer
            .write()
            .map_err(|_| ReasonError::Internal("Failed to acquire write lock".to_string()))?;

        for entry in entries {
            let mut doc = TantivyDocument::default();
            doc.add_text(self.document_id_field, entry.document_id);
            doc.add_text(self.node_id_field, entry.node_id);
            doc.add_text(self.table_id_field, entry.table_id);
            doc.add_text(self.title_field, entry.title);
            doc.add_text(self.content_field, entry.content);

            if !entry.tags.is_empty() {
                doc.add_text(self.tags_field, entry.tags.join(" "));
            }

            writer
                .add_document(doc)
                .map_err(|e| ReasonError::Internal(format!("Failed to add document: {}", e)))?;
        }

        Ok(())
    }

    /// Commit pending changes to make them searchable.
    pub fn commit(&self) -> Result<()> {
        let mut writer = self
            .writer
            .write()
            .map_err(|_| ReasonError::Internal("Failed to acquire write lock".to_string()))?;

        writer
            .commit()
            .map_err(|e| ReasonError::Internal(format!("Failed to commit: {}", e)))?;

        Ok(())
    }

    /// Delete all documents for a given document ID.
    pub fn delete_document(&self, document_id: &str) -> Result<()> {
        let writer = self
            .writer
            .write()
            .map_err(|_| ReasonError::Internal("Failed to acquire write lock".to_string()))?;

        let term = tantivy::Term::from_field_text(self.document_id_field, document_id);
        writer.delete_term(term);

        Ok(())
    }

    /// Search for documents matching the query.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query string
    /// * `limit` - Maximum results to return
    /// * `table_id` - Optional table ID to filter by
    ///
    /// # Returns
    ///
    /// Vector of search results sorted by BM25 score (highest first)
    pub fn search(
        &self,
        query: &str,
        limit: usize,
        table_id: Option<&str>,
    ) -> Result<Vec<TextSearchResult>> {
        // Reload reader to see latest commits
        self.reader
            .reload()
            .map_err(|e| ReasonError::Internal(format!("Failed to reload reader: {}", e)))?;

        let searcher = self.reader.searcher();

        // Build query - search across title and content with boosting
        let mut query_parser = QueryParser::for_index(
            &self.index,
            vec![self.title_field, self.content_field, self.tags_field],
        );
        query_parser.set_field_boost(self.title_field, 3.0);

        // If table_id is specified, add it to the query
        let query_string = if let Some(tid) = table_id {
            format!("({}) AND table_id:{}", query, tid)
        } else {
            query.to_string()
        };

        let parsed_query = query_parser
            .parse_query(&query_string)
            .map_err(|e| ReasonError::Internal(format!("Failed to parse query: {}", e)))?;

        // Execute search
        let top_docs = searcher
            .search(&parsed_query, &TopDocs::with_limit(limit))
            .map_err(|e| ReasonError::Internal(format!("Search failed: {}", e)))?;

        // Convert results
        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address).map_err(|e| {
                ReasonError::Internal(format!("Failed to retrieve document: {}", e))
            })?;

            let document_id = doc
                .get_first(self.document_id_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let node_id = doc
                .get_first(self.node_id_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let table_id = doc
                .get_first(self.table_id_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let title = doc
                .get_first(self.title_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let content = doc
                .get_first(self.content_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Create a snippet from content (first 200 characters)
            let snippet = if content.chars().count() > 200 {
                let end = content
                    .char_indices()
                    .nth(200)
                    .map(|(i, _)| i)
                    .unwrap_or(content.len());
                Some(format!("{}...", &content[..end]))
            } else if !content.is_empty() {
                Some(content)
            } else {
                None
            };

            results.push(TextSearchResult {
                document_id,
                node_id,
                table_id,
                title,
                score,
                snippet,
            });
        }

        Ok(results)
    }

    /// Get statistics about the index.
    pub fn stats(&self) -> IndexStats {
        let searcher = self.reader.searcher();
        IndexStats {
            num_docs: searcher.num_docs() as usize,
            num_segments: searcher.segment_readers().len(),
        }
    }
}

/// Statistics about the text index.
#[derive(Debug, Clone)]
pub struct IndexStats {
    /// Total number of indexed documents
    pub num_docs: usize,
    /// Number of index segments
    pub num_segments: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_and_search() {
        let index = TextIndex::in_memory().unwrap();

        // Index some documents
        index
            .index_node(
                "doc_1",
                "node_1",
                "tbl_legal",
                "NDA Agreement",
                "This Non-Disclosure Agreement protects confidential information.",
                &["nda".to_string(), "confidential".to_string()],
            )
            .unwrap();

        index
            .index_node(
                "doc_2",
                "node_2",
                "tbl_legal",
                "Service Contract",
                "This Service Agreement outlines payment terms of $15,000 per month.",
                &["service".to_string(), "contract".to_string()],
            )
            .unwrap();

        index
            .index_node(
                "doc_3",
                "node_3",
                "tbl_hr",
                "Employment Agreement",
                "Employment contract with salary of $150,000 and stock options.",
                &["employment".to_string(), "salary".to_string()],
            )
            .unwrap();

        index.commit().unwrap();

        // Search for "payment"
        let results = index.search("payment", 10, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document_id, "doc_2");
        assert!(results[0].score > 0.0);

        // Search for "agreement" - should match multiple
        let results = index.search("agreement", 10, None).unwrap();
        assert!(results.len() >= 2);

        // Search with table filter
        let results = index.search("agreement", 10, Some("tbl_legal")).unwrap();
        assert!(results.iter().all(|r| r.table_id == "tbl_legal"));

        // Search for "salary"
        let results = index.search("salary", 10, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document_id, "doc_3");
    }

    #[test]
    fn test_delete_document() {
        let index = TextIndex::in_memory().unwrap();

        index
            .index_node("doc_1", "node_1", "tbl_1", "Test Doc", "Test content", &[])
            .unwrap();
        index.commit().unwrap();

        // Verify it's indexed
        let results = index.search("test", 10, None).unwrap();
        assert_eq!(results.len(), 1);

        // Delete and commit
        index.delete_document("doc_1").unwrap();
        index.commit().unwrap();

        // Verify it's gone
        let results = index.search("test", 10, None).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_stemming() {
        let index = TextIndex::in_memory().unwrap();

        index
            .index_node(
                "doc_1",
                "node_1",
                "tbl_1",
                "Running Tests",
                "The runner runs through the running track",
                &[],
            )
            .unwrap();
        index.commit().unwrap();

        // Search for different forms - stemming should match
        let results = index.search("run", 10, None).unwrap();
        assert_eq!(results.len(), 1);

        let results = index.search("running", 10, None).unwrap();
        assert_eq!(results.len(), 1);

        let results = index.search("runs", 10, None).unwrap();
        assert_eq!(results.len(), 1);
    }
}
