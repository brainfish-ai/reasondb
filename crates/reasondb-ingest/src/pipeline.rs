//! Document ingestion pipeline
//!
//! Orchestrates the complete document ingestion process:
//! 1. Extract text/markdown from documents (via MarkItDown or native extractors)
//! 2. Chunk into semantic segments
//! 3. Build hierarchical tree
//! 4. Generate summaries
//! 5. Store in database

use std::path::Path;
use tracing::{debug, info};

use reasondb_core::llm::ReasoningEngine;
use reasondb_core::model::{Document, PageNode};
use reasondb_core::NodeStore;

use crate::chunker::{ChunkerConfig, SemanticChunker};
use crate::error::{IngestError, Result};
use crate::extractor::{DocumentType, SmartExtractor};
use crate::summarizer::{MockSummarizer, NodeSummarizer, SummarizerConfig};
use crate::tree_builder::TreeBuilder;

/// Configuration for the ingestion pipeline
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Chunker configuration
    pub chunker: ChunkerConfig,
    /// Summarizer configuration
    pub summarizer: SummarizerConfig,
    /// Whether to use ToC for structure detection
    pub use_toc_detection: bool,
    /// Whether to generate summaries (requires LLM)
    pub generate_summaries: bool,
    /// Whether to store in database
    pub store_in_db: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            chunker: ChunkerConfig::default(),
            summarizer: SummarizerConfig::default(),
            use_toc_detection: true,
            generate_summaries: true,
            store_in_db: true,
        }
    }
}

/// Result of document ingestion
#[derive(Debug)]
pub struct IngestResult {
    /// The created document
    pub document: Document,
    /// All created nodes
    pub nodes: Vec<PageNode>,
    /// Statistics about the ingestion
    pub stats: IngestStats,
}

/// Statistics from ingestion
#[derive(Debug, Default)]
pub struct IngestStats {
    /// Number of pages extracted
    pub pages_extracted: usize,
    /// Total characters extracted
    pub chars_extracted: usize,
    /// Number of chunks created
    pub chunks_created: usize,
    /// Number of nodes created
    pub nodes_created: usize,
    /// Number of summaries generated
    pub summaries_generated: usize,
    /// Time taken for extraction (ms)
    pub extraction_time_ms: u64,
    /// Time taken for chunking (ms)
    pub chunking_time_ms: u64,
    /// Time taken for summarization (ms)
    pub summarization_time_ms: u64,
    /// Total time (ms)
    pub total_time_ms: u64,
}

/// The main ingestion pipeline
pub struct IngestPipeline<R: ReasoningEngine> {
    config: PipelineConfig,
    extractor: SmartExtractor,
    chunker: SemanticChunker,
    tree_builder: TreeBuilder,
    reasoner: Option<R>,
}

impl<R: ReasoningEngine> IngestPipeline<R> {
    /// Create a new pipeline with an LLM for summarization
    pub fn new(reasoner: R) -> Self {
        Self {
            config: PipelineConfig::default(),
            extractor: SmartExtractor::new(),
            chunker: SemanticChunker::default(),
            tree_builder: TreeBuilder::new(),
            reasoner: Some(reasoner),
        }
    }

    /// Create a pipeline without LLM (no summarization)
    pub fn without_llm() -> IngestPipeline<NoOpReasoner> {
        IngestPipeline {
            config: PipelineConfig {
                generate_summaries: false,
                ..Default::default()
            },
            extractor: SmartExtractor::new(),
            chunker: SemanticChunker::default(),
            tree_builder: TreeBuilder::new(),
            reasoner: None,
        }
    }

    /// Ingest any supported file using MarkItDown (if available) or native extractors
    ///
    /// Supports: PDF, Word, PowerPoint, Excel, Images (OCR), Audio (transcription),
    /// HTML, CSV, JSON, XML, EPUB, ZIP files
    ///
    /// The `table_id` must reference an existing table in the database.
    pub async fn ingest_file<P: AsRef<Path>>(&self, path: P, table_id: &str) -> Result<IngestResult> {
        let path = path.as_ref();
        let start = std::time::Instant::now();
        let mut stats = IngestStats::default();

        let doc_type = DocumentType::from_path(path);
        info!("Starting ingestion of {} file: {}", doc_type.name(), path.display());

        // Extract document content
        let extraction_start = std::time::Instant::now();
        let extraction = self.extractor.extract(path)?;
        stats.extraction_time_ms = extraction_start.elapsed().as_millis() as u64;
        stats.chars_extracted = extraction.char_count;
        stats.pages_extracted = 1; // MarkItDown doesn't give page counts

        debug!(
            "Extracted {} chars in {}ms",
            stats.chars_extracted, stats.extraction_time_ms
        );

        // Process the markdown content
        let result = self
            .process_markdown(&extraction.title, table_id, &extraction.markdown, &mut stats)
            .await?;

        stats.total_time_ms = start.elapsed().as_millis() as u64;
        info!(
            "Ingestion complete: {} nodes in {}ms",
            stats.nodes_created, stats.total_time_ms
        );

        Ok(IngestResult {
            document: result.0,
            nodes: result.1,
            stats,
        })
    }

    /// Ingest from a URL (YouTube videos, web pages, etc.)
    ///
    /// The `table_id` must reference an existing table in the database.
    pub async fn ingest_url(&self, url: &str, table_id: &str) -> Result<IngestResult> {
        let start = std::time::Instant::now();
        let mut stats = IngestStats::default();

        info!("Starting ingestion of URL: {}", url);

        // Extract from URL
        let extraction_start = std::time::Instant::now();
        let extraction = self.extractor.extract_url(url)?;
        stats.extraction_time_ms = extraction_start.elapsed().as_millis() as u64;
        stats.chars_extracted = extraction.char_count;

        debug!(
            "Extracted {} chars in {}ms",
            stats.chars_extracted, stats.extraction_time_ms
        );

        // Process the markdown content
        let result = self
            .process_markdown(&extraction.title, table_id, &extraction.markdown, &mut stats)
            .await?;

        stats.total_time_ms = start.elapsed().as_millis() as u64;
        info!(
            "Ingestion complete: {} nodes in {}ms",
            stats.nodes_created, stats.total_time_ms
        );

        Ok(IngestResult {
            document: result.0,
            nodes: result.1,
            stats,
        })
    }

    /// Process extracted markdown content into a document tree
    async fn process_markdown(
        &self,
        title: &str,
        table_id: &str,
        markdown: &str,
        stats: &mut IngestStats,
    ) -> Result<(Document, Vec<PageNode>)> {
        // Chunk the markdown
        let chunking_start = std::time::Instant::now();
        let chunks = self.chunker.chunk_text(markdown)?;
        stats.chunking_time_ms = chunking_start.elapsed().as_millis() as u64;
        stats.chunks_created = chunks.len();

        debug!(
            "Created {} chunks in {}ms",
            stats.chunks_created, stats.chunking_time_ms
        );

        // Build tree
        let (document, mut nodes) = self.tree_builder.build(title, table_id, chunks)?;
        stats.nodes_created = nodes.len();

        // Generate summaries
        if self.config.generate_summaries {
            let summarization_start = std::time::Instant::now();

            if let Some(ref reasoner) = self.reasoner {
                let summarizer = NodeSummarizer::new(reasoner);
                summarizer.summarize_tree(&mut nodes).await?;
                stats.summaries_generated = nodes.len();
            } else {
                MockSummarizer::summarize_tree(&mut nodes);
                stats.summaries_generated = nodes.len();
            }

            stats.summarization_time_ms = summarization_start.elapsed().as_millis() as u64;
            debug!(
                "Generated {} summaries in {}ms",
                stats.summaries_generated, stats.summarization_time_ms
            );
        }

        Ok((document, nodes))
    }

    /// Set custom configuration
    pub fn with_config(mut self, config: PipelineConfig) -> Self {
        self.config = config.clone();
        self.chunker = SemanticChunker::new(config.chunker);
        self
    }

    /// Ingest from raw text (or markdown)
    /// Ingest plain text or markdown content.
    ///
    /// The `table_id` must reference an existing table in the database.
    pub async fn ingest_text(&self, title: &str, table_id: &str, text: &str) -> Result<IngestResult> {
        let start = std::time::Instant::now();
        let mut stats = IngestStats::default();

        info!("Starting text ingestion: {}", title);
        stats.chars_extracted = text.chars().count();

        let result = self.process_markdown(title, table_id, text, &mut stats).await?;

        stats.total_time_ms = start.elapsed().as_millis() as u64;

        Ok(IngestResult {
            document: result.0,
            nodes: result.1,
            stats,
        })
    }

    /// Ingest file and store in database
    ///
    /// The `table_id` must reference an existing table in the database.
    pub async fn ingest_and_store<P: AsRef<Path>>(
        &self,
        path: P,
        table_id: &str,
        store: &NodeStore,
    ) -> Result<IngestResult> {
        let result = self.ingest_file(path, table_id).await?;

        if self.config.store_in_db {
            Self::store_result(&result, store)?;
        }

        Ok(result)
    }

    /// Ingest text and store in database
    ///
    /// The `table_id` must reference an existing table in the database.
    pub async fn ingest_text_and_store(
        &self,
        title: &str,
        table_id: &str,
        text: &str,
        store: &NodeStore,
    ) -> Result<IngestResult> {
        let result = self.ingest_text(title, table_id, text).await?;

        if self.config.store_in_db {
            Self::store_result(&result, store)?;
        }

        Ok(result)
    }

    /// Ingest URL and store in database
    ///
    /// The `table_id` must reference an existing table in the database.
    pub async fn ingest_url_and_store(
        &self,
        url: &str,
        table_id: &str,
        store: &NodeStore,
    ) -> Result<IngestResult> {
        let result = self.ingest_url(url, table_id).await?;

        if self.config.store_in_db {
            Self::store_result(&result, store)?;
        }

        Ok(result)
    }

    /// Store an ingestion result (document + nodes) in a single batch transaction
    fn store_result(result: &IngestResult, store: &NodeStore) -> Result<()> {
        store
            .insert_document(&result.document)
            .map_err(IngestError::Storage)?;

        store
            .insert_nodes(&result.nodes)
            .map_err(IngestError::Storage)?;

        info!(
            "Stored document {} with {} nodes (batch)",
            result.document.id,
            result.nodes.len()
        );
        Ok(())
    }
}

/// A no-op reasoner for when LLM is not needed
pub struct NoOpReasoner;

#[async_trait::async_trait]
impl ReasoningEngine for NoOpReasoner {
    async fn decide_next_step(
        &self,
        _query: &str,
        _current_context: &str,
        _candidates: &[reasondb_core::llm::NodeSummary],
    ) -> reasondb_core::Result<Vec<reasondb_core::llm::TraversalDecision>> {
        Ok(vec![])
    }

    async fn verify_answer(
        &self,
        _query: &str,
        _content: &str,
    ) -> reasondb_core::Result<reasondb_core::llm::VerificationResult> {
        Ok(reasondb_core::llm::VerificationResult {
            is_relevant: false,
            confidence: 0.0,
            extracted_answer: None,
        })
    }

    async fn summarize(
        &self,
        content: &str,
        context: &reasondb_core::llm::SummarizationContext,
    ) -> reasondb_core::Result<String> {
        // Return a simple summary without LLM
        let preview: String = content.chars().take(100).collect();
        Ok(format!(
            "{}: {}...",
            context.title.as_deref().unwrap_or("Section"),
            preview
        ))
    }

    fn name(&self) -> &str {
        "no-op"
    }
}

/// Builder for configuring the pipeline
pub struct PipelineBuilder<R: ReasoningEngine> {
    reasoner: Option<R>,
    config: PipelineConfig,
}

impl<R: ReasoningEngine> PipelineBuilder<R> {
    /// Start building a pipeline
    pub fn new() -> PipelineBuilder<NoOpReasoner> {
        PipelineBuilder {
            reasoner: None,
            config: PipelineConfig::default(),
        }
    }

    /// Set the reasoning engine
    pub fn with_reasoner<R2: ReasoningEngine>(self, reasoner: R2) -> PipelineBuilder<R2> {
        PipelineBuilder {
            reasoner: Some(reasoner),
            config: self.config,
        }
    }

    /// Configure chunk size
    pub fn chunk_size(mut self, target: usize, min: usize, max: usize) -> Self {
        self.config.chunker.target_chunk_size = target;
        self.config.chunker.min_chunk_size = min;
        self.config.chunker.max_chunk_size = max;
        self
    }

    /// Enable/disable ToC detection
    pub fn use_toc_detection(mut self, enabled: bool) -> Self {
        self.config.use_toc_detection = enabled;
        self
    }

    /// Enable/disable summarization
    pub fn generate_summaries(mut self, enabled: bool) -> Self {
        self.config.generate_summaries = enabled;
        self
    }

    /// Build the pipeline
    pub fn build(self) -> IngestPipeline<R>
    where
        R: ReasoningEngine,
    {
        IngestPipeline {
            config: self.config.clone(),
            extractor: SmartExtractor::new(),
            chunker: SemanticChunker::new(self.config.chunker),
            tree_builder: TreeBuilder::new(),
            reasoner: self.reasoner,
        }
    }
}

impl Default for PipelineBuilder<NoOpReasoner> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_text_ingestion() {
        let pipeline = IngestPipeline::<NoOpReasoner>::without_llm();

        let text = r#"
Chapter 1: Introduction

This is the introduction to our document. It contains important background information.

Chapter 2: Methods

Here we describe the methods used in our research. We employed several techniques.

Chapter 3: Conclusion

In conclusion, our findings suggest significant results.
"#;

        let result = pipeline.ingest_text("Test Document", "test-table", text).await.unwrap();

        assert_eq!(result.document.title, "Test Document");
        assert_eq!(result.document.table_id, "test-table");
        assert!(result.nodes.len() > 1);
        assert!(result.stats.chunks_created > 0);
    }

    #[test]
    fn test_pipeline_builder() {
        let pipeline: IngestPipeline<NoOpReasoner> = PipelineBuilder::<NoOpReasoner>::new()
            .chunk_size(1000, 200, 2000)
            .use_toc_detection(false)
            .generate_summaries(false)
            .build();

        assert_eq!(pipeline.config.chunker.target_chunk_size, 1000);
        assert!(!pipeline.config.use_toc_detection);
        assert!(!pipeline.config.generate_summaries);
    }
}
