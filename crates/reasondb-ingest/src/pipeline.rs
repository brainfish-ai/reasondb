//! Document ingestion pipeline
//!
//! Orchestrates the complete document ingestion process:
//! 1. Extract text/markdown from documents (via extractor plugins)
//! 2. Chunk into semantic segments
//! 3. Build hierarchical tree
//! 4. Generate summaries
//! 5. Store in database

use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};

use reasondb_core::llm::ReasoningEngine;
use reasondb_core::model::{Document, PageNode};
use reasondb_core::NodeStore;
use reasondb_plugin::PluginManager;

use crate::chunker::{ChunkerConfig, SemanticChunker};
use crate::error::{IngestError, Result};
use crate::extractor::{DocumentType, SmartExtractor};
use crate::summarizer::{BatchSummarizer, MockSummarizer, SummarizerConfig};
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
    plugin_manager: Option<Arc<PluginManager>>,
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
            plugin_manager: None,
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
            plugin_manager: None,
        }
    }

    /// Attach a plugin manager for plugin-based pipeline stages
    pub fn with_plugins(mut self, manager: Arc<PluginManager>) -> Self {
        self.extractor = self.extractor.with_plugin_manager(Arc::clone(&manager));
        self.plugin_manager = Some(manager);
        self
    }

    /// Ingest a file using registered extractor plugins.
    ///
    /// The `table_id` must reference an existing table in the database.
    pub async fn ingest_file<P: AsRef<Path>>(
        &self,
        path: P,
        table_id: &str,
    ) -> Result<IngestResult> {
        let path = path.as_ref();
        let start = std::time::Instant::now();
        let mut stats = IngestStats::default();

        let doc_type = DocumentType::from_path(path);
        info!(
            "Starting ingestion of {} file: {}",
            doc_type.name(),
            path.display()
        );

        // Extract document content — run in a blocking thread so the plugin's
        // subprocess polling loop doesn't starve the Tokio runtime.
        let extraction_start = std::time::Instant::now();
        let extractor = self.extractor.clone();
        let path_buf = path.to_path_buf();
        let extraction = tokio::task::spawn_blocking(move || extractor.extract(&path_buf))
            .await
            .map_err(|e| {
                IngestError::TextExtraction(format!("Extraction task panicked: {}", e))
            })??;
        stats.extraction_time_ms = extraction_start.elapsed().as_millis() as u64;
        stats.chars_extracted = extraction.char_count;
        stats.pages_extracted = 1; // MarkItDown doesn't give page counts

        debug!(
            "Extracted {} chars in {}ms",
            stats.chars_extracted, stats.extraction_time_ms
        );

        // Process the markdown content
        let result = self
            .process_markdown(
                &extraction.title,
                table_id,
                &extraction.markdown,
                &mut stats,
            )
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
            .process_markdown(
                &extraction.title,
                table_id,
                &extraction.markdown,
                &mut stats,
            )
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

    /// Process extracted markdown content into a document tree.
    ///
    /// Pipeline stages:
    /// 1. Post-processor plugins (chain, if any registered)
    /// 2. Chunk (plugin chunker or built-in SemanticChunker)
    /// 3. Build tree
    /// 4. Summarize (plugin summarizer or built-in LLM summarizer)
    async fn process_markdown(
        &self,
        title: &str,
        table_id: &str,
        markdown: &str,
        stats: &mut IngestStats,
    ) -> Result<(Document, Vec<PageNode>)> {
        let mut processed_markdown = Self::strip_frontmatter(markdown);

        // 1. Run post-processor plugins (chain) if any registered
        if let Some(ref pm) = self.plugin_manager {
            if pm.has_post_processors() {
                debug!("Running post-processor plugins");
                match pm.run_post_processors(&processed_markdown, &std::collections::HashMap::new())
                {
                    Ok(result) => {
                        processed_markdown = result.markdown;
                        debug!(
                            "Post-processing complete, {} chars",
                            processed_markdown.len()
                        );
                    }
                    Err(e) => {
                        warn!(
                            "Post-processor plugin failed, using original markdown: {}",
                            e
                        );
                    }
                }
            }
        }

        // 2. Chunk (plugin chunker or built-in SemanticChunker)
        let chunking_start = std::time::Instant::now();
        let chunks = if let Some(ref pm) = self.plugin_manager {
            if pm.has_chunker() {
                debug!("Using plugin chunker");
                let config = reasondb_plugin::ChunkConfig {
                    target_chunk_size: self.config.chunker.target_chunk_size,
                    min_chunk_size: self.config.chunker.min_chunk_size,
                    max_chunk_size: self.config.chunker.max_chunk_size,
                    overlap: 100,
                };
                match pm.chunk(&processed_markdown, &config) {
                    Ok(result) => result
                        .chunks
                        .into_iter()
                        .map(|c| {
                            let word_count = c.content.split_whitespace().count();
                            crate::chunker::TextChunk {
                                id: uuid::Uuid::new_v4().to_string(),
                                content: c.content,
                                heading: c.heading.map(|text| crate::chunker::DetectedHeading {
                                    text,
                                    level: c.level,
                                    offset: 0,
                                    page_number: None,
                                }),
                                char_count: c.char_count,
                                word_count,
                                start_page: None,
                                end_page: None,
                            }
                        })
                        .collect(),
                    Err(e) => {
                        warn!("Plugin chunker failed, falling back to built-in: {}", e);
                        self.chunker.chunk_text(&processed_markdown)?
                    }
                }
            } else {
                self.chunker.chunk_text(&processed_markdown)?
            }
        } else {
            self.chunker.chunk_text(&processed_markdown)?
        };
        stats.chunking_time_ms = chunking_start.elapsed().as_millis() as u64;
        stats.chunks_created = chunks.len();

        debug!(
            "Created {} chunks in {}ms",
            stats.chunks_created, stats.chunking_time_ms
        );

        // 3. Build tree
        let (document, mut nodes) = self.tree_builder.build(title, table_id, chunks)?;
        stats.nodes_created = nodes.len();

        // 4. Summarize (plugin summarizer or built-in LLM summarizer)
        if self.config.generate_summaries {
            let summarization_start = std::time::Instant::now();

            let mut used_plugin = false;
            if let Some(ref pm) = self.plugin_manager {
                if pm.has_summarizer() {
                    debug!("Using plugin summarizer");
                    for node in &mut nodes {
                        if let Some(ref content) = node.content {
                            let context = std::collections::HashMap::from([(
                                "title".to_string(),
                                node.title.clone(),
                            )]);
                            match pm.summarize(content, &context) {
                                Ok(result) => {
                                    node.summary = result.summary;
                                }
                                Err(e) => {
                                    warn!(
                                        "Plugin summarizer failed for node '{}': {}",
                                        node.title, e
                                    );
                                }
                            }
                        }
                    }
                    stats.summaries_generated = nodes.len();
                    used_plugin = true;
                }
            }

            if !used_plugin {
                if let Some(ref reasoner) = self.reasoner {
                    let summarizer =
                        BatchSummarizer::new(reasoner, 10, self.config.summarizer.max_concurrent);
                    summarizer.summarize_batch(&mut nodes).await?;
                    stats.summaries_generated = nodes.len();
                } else {
                    MockSummarizer::summarize_tree(&mut nodes);
                    stats.summaries_generated = nodes.len();
                }
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
    pub async fn ingest_text(
        &self,
        title: &str,
        table_id: &str,
        text: &str,
    ) -> Result<IngestResult> {
        let start = std::time::Instant::now();
        let mut stats = IngestStats::default();

        info!("Starting text ingestion: {}", title);
        stats.chars_extracted = text.chars().count();

        let result = self
            .process_markdown(title, table_id, text, &mut stats)
            .await?;

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
    /// Domain vocabulary extraction is spawned as a background task so this
    /// method returns as soon as the document is safely stored.
    pub async fn ingest_and_store<P: AsRef<Path>>(
        &self,
        path: P,
        table_id: &str,
        store: Arc<NodeStore>,
    ) -> Result<IngestResult>
    where
        R: Clone + Send + Sync + 'static,
    {
        let result = self.ingest_file(path, table_id).await?;

        if self.config.store_in_db {
            Self::store_result(&result, &store)?;
            self.spawn_vocab_update(&result, store);
        }

        Ok(result)
    }

    /// Ingest text and store in database
    ///
    /// The `table_id` must reference an existing table in the database.
    /// Domain vocabulary extraction is spawned as a background task.
    pub async fn ingest_text_and_store(
        &self,
        title: &str,
        table_id: &str,
        text: &str,
        store: Arc<NodeStore>,
    ) -> Result<IngestResult>
    where
        R: Clone + Send + Sync + 'static,
    {
        let result = self.ingest_text(title, table_id, text).await?;

        if self.config.store_in_db {
            Self::store_result(&result, &store)?;
            self.spawn_vocab_update(&result, store);
        }

        Ok(result)
    }

    /// Ingest URL and store in database
    ///
    /// The `table_id` must reference an existing table in the database.
    /// Domain vocabulary extraction is spawned as a background task.
    pub async fn ingest_url_and_store(
        &self,
        url: &str,
        table_id: &str,
        store: Arc<NodeStore>,
    ) -> Result<IngestResult>
    where
        R: Clone + Send + Sync + 'static,
    {
        let result = self.ingest_url(url, table_id).await?;

        if self.config.store_in_db {
            Self::store_result(&result, &store)?;
            self.spawn_vocab_update(&result, store);
        }

        Ok(result)
    }

    /// Fire-and-forget domain vocabulary extraction. The job completes immediately
    /// after storage; vocab enrichment finishes in the background.
    fn spawn_vocab_update(&self, result: &IngestResult, store: Arc<NodeStore>)
    where
        R: Clone + Send + Sync + 'static,
    {
        let Some(ref reasoner) = self.reasoner else {
            return;
        };
        let Some(root) = result.nodes.iter().find(|n| n.depth == 0) else {
            return;
        };
        if root.summary.is_empty() {
            return;
        }

        let reasoner = reasoner.clone();
        let table_id = result.document.table_id.clone();
        let root_summary = root.summary.clone();

        tokio::spawn(do_vocab_update(reasoner, table_id, root_summary, store));
    }

    /// Strip YAML frontmatter (`---` delimited block at the start of the file).
    fn strip_frontmatter(text: &str) -> String {
        let trimmed = text.trim_start();
        if !trimmed.starts_with("---") {
            return text.to_string();
        }
        // Find the closing `---` after the opening one
        if let Some(end) = trimmed[3..].find("\n---") {
            let after = &trimmed[3 + end + 4..]; // skip past closing `---`
            let result = after.trim_start().to_string();
            if result.is_empty() {
                return text.to_string();
            }
            debug!(
                "Stripped YAML frontmatter ({} chars removed)",
                text.len() - result.len()
            );
            result
        } else {
            text.to_string()
        }
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

/// Background task: extract domain vocabulary and persist it to the table metadata.
///
/// Spawned with `tokio::spawn` so ingestion jobs complete without waiting for
/// the LLM vocab call.
async fn do_vocab_update<R>(
    reasoner: R,
    table_id: String,
    root_summary: String,
    store: Arc<NodeStore>,
) where
    R: ReasoningEngine + Send + Sync + 'static,
{
    let existing_vocab: Vec<String> = store
        .get_table(&table_id)
        .ok()
        .flatten()
        .and_then(|t| t.metadata.get("domain_vocab").cloned())
        .and_then(|v| v.as_array().cloned())
        .map(|arr| {
            arr.into_iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    info!(
        table_id = %table_id,
        summary_len = root_summary.len(),
        existing_terms = existing_vocab.len(),
        "Extracting domain vocab from root node summary (background)"
    );

    match reasoner
        .extract_domain_vocab(&root_summary, &existing_vocab)
        .await
    {
        Ok(new_terms) if !new_terms.is_empty() => {
            let mut all_terms = existing_vocab;
            for term in new_terms {
                let is_new = !all_terms
                    .iter()
                    .any(|t| t.to_lowercase() == term.to_lowercase());
                if is_new {
                    all_terms.push(term);
                }
            }

            if let Ok(Some(mut table)) = store.get_table(&table_id) {
                table.metadata.insert(
                    "domain_vocab".to_string(),
                    serde_json::Value::Array(
                        all_terms
                            .into_iter()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                );
                if let Err(e) = store.update_table(&table) {
                    warn!("Failed to update domain vocab for table: {}", e);
                } else {
                    info!(table_id = %table_id, "Domain vocab updated (background)");
                }
            }
        }
        Ok(_) => {
            debug!(table_id = %table_id, "Domain vocab extraction returned no new terms");
        }
        Err(e) => warn!("Domain vocab extraction failed: {}", e),
    }
}

/// A no-op reasoner for when LLM is not needed
#[derive(Clone)]
pub struct NoOpReasoner;

#[async_trait::async_trait]
impl ReasoningEngine for NoOpReasoner {
    async fn decide_next_step(
        &self,
        _query: &str,
        _current_context: &str,
        _candidates: &[reasondb_core::llm::NodeSummary],
        _max_selections: usize,
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

    async fn decompose_query(
        &self,
        query: &str,
        _domain_context: Option<&reasondb_core::query_decomposer::DomainContext>,
    ) -> reasondb_core::Result<Vec<reasondb_core::query_decomposer::SubQuery>> {
        Ok(vec![reasondb_core::query_decomposer::SubQuery {
            text: query.to_string(),
            rationale: "no-op passthrough".to_string(),
        }])
    }

    async fn extract_domain_vocab(
        &self,
        _document_summary: &str,
        _existing_vocab: &[String],
    ) -> reasondb_core::Result<Vec<String>> {
        Ok(vec![])
    }

    fn name(&self) -> &str {
        "no-op"
    }
}

/// Builder for configuring the pipeline
pub struct PipelineBuilder<R: ReasoningEngine> {
    reasoner: Option<R>,
    config: PipelineConfig,
    plugin_manager: Option<Arc<PluginManager>>,
}

impl<R: ReasoningEngine> PipelineBuilder<R> {
    /// Start building a pipeline
    pub fn new() -> PipelineBuilder<NoOpReasoner> {
        PipelineBuilder {
            reasoner: None,
            config: PipelineConfig::default(),
            plugin_manager: None,
        }
    }

    /// Set the reasoning engine
    pub fn with_reasoner<R2: ReasoningEngine>(self, reasoner: R2) -> PipelineBuilder<R2> {
        PipelineBuilder {
            reasoner: Some(reasoner),
            config: self.config,
            plugin_manager: self.plugin_manager,
        }
    }

    /// Attach a plugin manager
    pub fn with_plugins(mut self, manager: Arc<PluginManager>) -> Self {
        self.plugin_manager = Some(manager);
        self
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
        let mut extractor = SmartExtractor::new();
        if let Some(ref pm) = self.plugin_manager {
            extractor = extractor.with_plugin_manager(Arc::clone(pm));
        }

        IngestPipeline {
            config: self.config.clone(),
            extractor,
            chunker: SemanticChunker::new(self.config.chunker),
            tree_builder: TreeBuilder::new(),
            reasoner: self.reasoner,
            plugin_manager: self.plugin_manager,
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

        let result = pipeline
            .ingest_text("Test Document", "test-table", text)
            .await
            .unwrap();

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
