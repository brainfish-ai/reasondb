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

use futures::stream::{FuturesUnordered, StreamExt};
use tokio::sync::Semaphore;

use reasondb_core::llm::ReasoningEngine;
use reasondb_core::model::{Document, PageNode};
use reasondb_core::NodeStore;
use reasondb_plugin::PluginManager;

use crate::chunker::{ChunkStrategy, ChunkerConfig, DetectedHeading, SemanticChunker, TextChunk};
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
#[derive(Debug, Default, Clone)]
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
    /// Called with the new document ID after the early flush (doc + nodes written
    /// to DB before summarization). Allows the job layer to record a checkpoint
    /// so a restart can resume summarization rather than re-chunking.
    checkpoint_callback: Option<Arc<dyn Fn(String) + Send + Sync + 'static>>,
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
            checkpoint_callback: None,
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
            checkpoint_callback: None,
        }
    }

    /// Attach a plugin manager for plugin-based pipeline stages
    pub fn with_plugins(mut self, manager: Arc<PluginManager>) -> Self {
        self.extractor = self.extractor.with_plugin_manager(Arc::clone(&manager));
        self.plugin_manager = Some(manager);
        self
    }

    /// Register a callback invoked with the document ID after the early DB flush
    /// (doc + nodes stored before summarization begins). The job layer uses this
    /// to persist a resume checkpoint so a server restart can skip re-chunking.
    pub fn with_checkpoint_callback(mut self, f: impl Fn(String) + Send + Sync + 'static) -> Self {
        self.checkpoint_callback = Some(Arc::new(f));
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

        // 2. Chunk (plugin chunker → agentic LLM → built-in MarkdownAware)
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
                            TextChunk {
                                id: uuid::Uuid::new_v4().to_string(),
                                content: c.content,
                                heading: c.heading.map(|text| DetectedHeading {
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
                self.run_chunking_strategy(&processed_markdown).await?
            }
        } else {
            self.run_chunking_strategy(&processed_markdown).await?
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

    /// Select the correct chunking path based on `config.chunker.strategy`.
    ///
    /// - `Agentic` — calls the LLM to group numbered lines; falls back to
    ///   `MarkdownAware` when no reasoner is configured.
    /// - `MarkdownAware` — deterministic CommonMark boundary splitting.
    async fn run_chunking_strategy(&self, markdown: &str) -> Result<Vec<TextChunk>> {
        match self.config.chunker.strategy {
            ChunkStrategy::MarkdownAware => self.chunker.chunk_text(markdown),
            ChunkStrategy::Agentic => {
                if let Some(ref reasoner) = self.reasoner {
                    self.agentic_chunk(markdown, reasoner).await
                } else {
                    warn!(
                        "Agentic chunking requires an LLM; no reasoner configured — \
                         falling back to MarkdownAware"
                    );
                    self.chunker.chunk_text(markdown)
                }
            }
        }
    }

    /// Call the LLM to chunk `markdown` into semantically coherent groups,
    /// processing the document in overlapping windows of `agentic_window_size` lines.
    /// Windows are dispatched concurrently (bounded by `agentic_concurrency`) so that
    /// large documents do not block on sequential LLM round-trips.
    async fn agentic_chunk(&self, markdown: &str, reasoner: &R) -> Result<Vec<TextChunk>> {
        let all_lines: Vec<String> = markdown.lines().map(|l| l.to_string()).collect();
        let total_lines = all_lines.len();
        let window_size = self.config.chunker.agentic_window_size;
        let concurrency = self.config.chunker.agentic_concurrency.max(1);

        if total_lines == 0 {
            return Ok(vec![]);
        }

        // Pre-compute all windows with their offsets and cutoff values.
        let overlap = (window_size / 10).max(5);
        let mut windows: Vec<(usize, Vec<String>, usize, usize)> = Vec::new(); // (idx, lines, window_offset, cutoff)
        let mut offset = 0usize;

        while offset < total_lines {
            let end = (offset + window_size).min(total_lines);
            let window_offset = offset + 1; // 1-based line numbers for the LLM
            let cutoff = if offset == 0 {
                window_offset
            } else {
                window_offset + overlap
            };
            windows.push((
                windows.len(),
                all_lines[offset..end].to_vec(),
                window_offset,
                cutoff,
            ));

            if end >= total_lines {
                break;
            }
            offset += window_size.saturating_sub(overlap);
        }

        info!(
            "Agentic chunking: {} windows, concurrency={}",
            windows.len(),
            concurrency
        );

        // Dispatch all windows concurrently, bounded by semaphore.
        let semaphore = Arc::new(Semaphore::new(concurrency));
        let mut futures = FuturesUnordered::new();

        for (idx, window_lines, window_offset, cutoff) in windows {
            let permit = semaphore.clone();
            futures.push(async move {
                let _permit = permit.acquire().await.unwrap();
                // reasoner is &R (Copy), so it's implicitly copied into each async block.
                reasoner
                    .chunk_document(&window_lines, window_offset)
                    .await
                    .map_err(|e| IngestError::Chunking(e.to_string()))
                    .map(|res| (idx, cutoff, res.groups))
            });
        }

        // Collect results preserving per-window cutoff metadata.
        let mut raw: Vec<(usize, usize, Vec<reasondb_core::llm::ChunkGroup>)> = Vec::new();
        while let Some(result) = futures.next().await {
            raw.push(result?);
        }

        // Sort by window index so cutoff filtering is deterministic.
        raw.sort_by_key(|(idx, _, _)| *idx);

        let mut all_groups: Vec<reasondb_core::llm::ChunkGroup> = Vec::new();
        for (idx, cutoff, groups) in raw {
            for g in groups {
                if idx == 0 || g.start_line >= cutoff {
                    all_groups.push(g);
                }
            }
        }

        if all_groups.is_empty() {
            warn!("Agentic chunking returned no groups; falling back to MarkdownAware");
            return self.chunker.chunk_text(markdown);
        }

        // Sort by start_line (LLM may occasionally return out-of-order groups).
        all_groups.sort_by_key(|g| g.start_line);

        // Build TextChunks from the line groups.
        let mut chunks = Vec::with_capacity(all_groups.len());
        for (i, group) in all_groups.iter().enumerate() {
            let start_idx = group.start_line.saturating_sub(1);
            let end_idx = group.end_line.min(total_lines);

            if start_idx >= end_idx {
                continue;
            }

            let content = all_lines[start_idx..end_idx].join("\n");
            let content = content.trim().to_string();
            if content.is_empty() {
                continue;
            }

            let heading = group.heading.as_ref().map(|text| DetectedHeading {
                text: text.clone(),
                level: 1,
                offset: start_idx,
                page_number: None,
            });

            let char_count = content.chars().count();
            let word_count = content.split_whitespace().count();

            chunks.push(TextChunk {
                id: format!("chunk_{}", i),
                content,
                heading,
                char_count,
                word_count,
                start_page: None,
                end_page: None,
            });
        }

        if chunks.is_empty() {
            warn!("Agentic chunking produced no valid chunks; falling back to MarkdownAware");
            return self.chunker.chunk_text(markdown);
        }

        debug!(
            "Agentic chunking produced {} chunks from {} lines",
            chunks.len(),
            total_lines
        );

        Ok(chunks)
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
        if !self.config.store_in_db {
            return self.ingest_file(path, table_id).await;
        }

        let path = path.as_ref();
        let start = std::time::Instant::now();
        let mut stats = IngestStats::default();

        let doc_type = crate::extractor::DocumentType::from_path(path);
        info!(
            "Starting ingestion of {} file: {}",
            doc_type.name(),
            path.display()
        );

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
        stats.pages_extracted = 1;

        let result = self
            .ingest_checkpointed(
                &extraction.title,
                table_id,
                &extraction.markdown,
                store,
                &mut stats,
            )
            .await?;

        stats.total_time_ms = start.elapsed().as_millis() as u64;
        Ok(IngestResult { stats, ..result })
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
        if !self.config.store_in_db {
            return self.ingest_text(title, table_id, text).await;
        }

        let start = std::time::Instant::now();
        let mut stats = IngestStats {
            chars_extracted: text.chars().count(),
            ..Default::default()
        };

        info!("Starting text ingestion: {}", title);

        let result = self
            .ingest_checkpointed(title, table_id, text, store, &mut stats)
            .await?;

        stats.total_time_ms = start.elapsed().as_millis() as u64;
        Ok(IngestResult { stats, ..result })
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
        if !self.config.store_in_db {
            return self.ingest_url(url, table_id).await;
        }

        let start = std::time::Instant::now();
        let mut stats = IngestStats::default();

        info!("Starting URL ingestion: {}", url);

        let extraction = self
            .extractor
            .extract_url(url)
            .map_err(|e| IngestError::TextExtraction(e.to_string()))?;
        stats.chars_extracted = extraction.char_count;

        let result = self
            .ingest_checkpointed(
                &extraction.title,
                table_id,
                &extraction.markdown,
                store,
                &mut stats,
            )
            .await?;

        stats.total_time_ms = start.elapsed().as_millis() as u64;
        Ok(IngestResult { stats, ..result })
    }

    /// Core checkpointed ingestion: chunk + build tree → early DB flush → summarize with
    /// incremental writes. The checkpoint callback is fired after the early flush so the
    /// job layer can record the doc ID before summarization begins.
    async fn ingest_checkpointed(
        &self,
        title: &str,
        table_id: &str,
        markdown: &str,
        store: Arc<NodeStore>,
        stats: &mut IngestStats,
    ) -> Result<IngestResult>
    where
        R: Clone + Send + Sync + 'static,
    {
        // Phase 1: chunk + build tree (no summarization yet)
        let (document, mut nodes) = self
            .chunk_and_build(title, table_id, markdown, stats)
            .await?;

        // Phase 1 flush: write doc + nodes (empty summaries) to DB immediately.
        // This protects the chunking work — a restart can resume summarization.
        store
            .insert_document(&document)
            .map_err(IngestError::Storage)?;
        store.insert_nodes(&nodes).map_err(IngestError::Storage)?;
        info!(
            "Checkpoint flush: stored doc {} ({} nodes) before summarization",
            document.id,
            nodes.len()
        );

        // Notify caller (job layer) so it can persist the checkpoint doc ID.
        if let Some(ref cb) = self.checkpoint_callback {
            cb(document.id.clone());
        }

        // Phase 2: summarize with incremental DB writes after each depth level.
        self.summarize_nodes(&mut nodes, stats, Some(store.clone()))
            .await?;

        // The summarizer already called update_node for each summarized node.
        // We only need to update nodes that were skipped (e.g., plugin or mock path).
        // For the LLM path with a store, update_node was already called per depth level.

        self.spawn_vocab_update(
            &IngestResult {
                document: document.clone(),
                nodes: nodes.clone(),
                stats: stats.clone(),
            },
            store,
        );

        Ok(IngestResult {
            document,
            nodes,
            stats: stats.clone(),
        })
    }

    /// Resume summarization for a document that was partially ingested.
    ///
    /// Called after a server restart when `checkpoint_doc_id` is set on the job.
    /// Reads all nodes from the DB, identifies those with empty summaries, runs
    /// the summarizer on the full node set (so parent nodes see child summaries),
    /// and writes the new summaries back incrementally.
    pub async fn resume_summarization(
        &self,
        doc_id: &str,
        store: Arc<NodeStore>,
    ) -> Result<IngestResult>
    where
        R: Clone + Send + Sync + 'static,
    {
        let document = store
            .get_document(doc_id)
            .map_err(IngestError::Storage)?
            .ok_or_else(|| {
                IngestError::InvalidInput(format!("Checkpoint doc {} not found in DB", doc_id))
            })?;

        let mut nodes = store
            .get_nodes_for_document(doc_id)
            .map_err(IngestError::Storage)?;

        let pending = nodes.iter().filter(|n| n.summary.is_empty()).count();
        info!(
            "Resuming summarization for doc {} ({}/{} nodes still pending)",
            doc_id,
            pending,
            nodes.len()
        );

        if pending == 0 {
            info!("All nodes already summarized — nothing to resume");
            let stats = IngestStats {
                nodes_created: nodes.len(),
                summaries_generated: nodes.len(),
                ..Default::default()
            };
            return Ok(IngestResult {
                document,
                nodes,
                stats,
            });
        }

        let mut stats = IngestStats {
            nodes_created: nodes.len(),
            ..Default::default()
        };

        // Run summarization on all nodes (the summarizer handles depth ordering;
        // already-summarized nodes keep their existing non-empty summaries).
        self.summarize_nodes(&mut nodes, &mut stats, Some(store.clone()))
            .await?;

        self.spawn_vocab_update(
            &IngestResult {
                document: document.clone(),
                nodes: nodes.clone(),
                stats: stats.clone(),
            },
            store,
        );

        Ok(IngestResult {
            document,
            nodes,
            stats,
        })
    }

    /// Chunk the markdown and build the document tree without running summarization.
    /// Returns `(Document, Vec<PageNode>)` with empty summaries ready for early flush.
    async fn chunk_and_build(
        &self,
        title: &str,
        table_id: &str,
        markdown: &str,
        stats: &mut IngestStats,
    ) -> Result<(Document, Vec<PageNode>)> {
        let mut processed = Self::strip_frontmatter(markdown);

        if let Some(ref pm) = self.plugin_manager {
            if pm.has_post_processors() {
                match pm.run_post_processors(&processed, &std::collections::HashMap::new()) {
                    Ok(result) => processed = result.markdown,
                    Err(e) => warn!("Post-processor failed, using original: {}", e),
                }
            }
        }

        let chunking_start = std::time::Instant::now();
        let chunks = if let Some(ref pm) = self.plugin_manager {
            if pm.has_chunker() {
                let config = reasondb_plugin::ChunkConfig {
                    target_chunk_size: self.config.chunker.target_chunk_size,
                    min_chunk_size: self.config.chunker.min_chunk_size,
                    max_chunk_size: self.config.chunker.max_chunk_size,
                    overlap: 100,
                };
                match pm.chunk(&processed, &config) {
                    Ok(result) => result
                        .chunks
                        .into_iter()
                        .map(|c| {
                            let word_count = c.content.split_whitespace().count();
                            TextChunk {
                                id: uuid::Uuid::new_v4().to_string(),
                                content: c.content,
                                heading: c.heading.map(|text| DetectedHeading {
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
                        self.run_chunking_strategy(&processed).await?
                    }
                }
            } else {
                self.run_chunking_strategy(&processed).await?
            }
        } else {
            self.run_chunking_strategy(&processed).await?
        };
        stats.chunking_time_ms = chunking_start.elapsed().as_millis() as u64;
        stats.chunks_created = chunks.len();

        let (document, nodes) = self.tree_builder.build(title, table_id, chunks)?;
        stats.nodes_created = nodes.len();

        Ok((document, nodes))
    }

    /// Run summarization on `nodes`, optionally flushing results to `store` after each
    /// depth-level batch so that progress survives a server restart.
    async fn summarize_nodes(
        &self,
        nodes: &mut [PageNode],
        stats: &mut IngestStats,
        store: Option<Arc<NodeStore>>,
    ) -> Result<()>
    where
        R: Clone + Send + Sync + 'static,
    {
        if !self.config.generate_summaries {
            return Ok(());
        }

        let summarization_start = std::time::Instant::now();
        let mut used_plugin = false;

        if let Some(ref pm) = self.plugin_manager {
            if pm.has_summarizer() {
                for node in nodes.iter_mut() {
                    if let Some(ref content) = node.content {
                        let context = std::collections::HashMap::from([(
                            "title".to_string(),
                            node.title.clone(),
                        )]);
                        match pm.summarize(content, &context) {
                            Ok(result) => node.summary = result.summary,
                            Err(e) => warn!("Plugin summarizer failed for '{}': {}", node.title, e),
                        }
                    }
                }
                // Flush plugin summaries to DB if store provided
                if let Some(ref s) = store {
                    for node in nodes.iter() {
                        if let Err(e) = s.update_node(node) {
                            warn!("Checkpoint flush failed for node {}: {}", node.id, e);
                        }
                    }
                }
                stats.summaries_generated = nodes.len();
                used_plugin = true;
            }
        }

        if !used_plugin {
            if let Some(ref reasoner) = self.reasoner {
                let mut summarizer =
                    BatchSummarizer::new(reasoner, 10, self.config.summarizer.max_concurrent);
                if let Some(s) = store {
                    summarizer = summarizer.with_store(s);
                }
                summarizer.summarize_batch(nodes).await?;
                stats.summaries_generated = nodes.len();
            } else {
                MockSummarizer::summarize_tree(nodes);
                // Flush mock summaries if store provided
                stats.summaries_generated = nodes.len();
            }
        }

        stats.summarization_time_ms = summarization_start.elapsed().as_millis() as u64;
        Ok(())
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
            checkpoint_callback: None,
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
