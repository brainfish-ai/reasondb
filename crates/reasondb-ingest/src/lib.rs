//! # ReasonDB Ingest
//!
//! Document ingestion pipeline for ReasonDB.
//!
//! This crate provides:
//! - Plugin-based document extraction (files and URLs)
//! - Semantic text chunking with ToC detection
//! - Hierarchical tree building
//! - LLM-based summarization
//!
//! ## Extraction via Plugins
//!
//! File and URL extraction is handled entirely by the plugin system.
//! The built-in `markitdown` plugin (ships in the Docker image) covers
//! PDF, Word, PowerPoint, Excel, Images (OCR), Audio, HTML, CSV, JSON,
//! XML, EPUB, ZIP, and YouTube URLs.
//!
//! Custom extractor plugins can be added under `$REASONDB_PLUGINS_DIR`.
//! Supported runtimes: Python, Node.js, Bash, compiled binaries.
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use reasondb_ingest::{IngestPipeline, PipelineBuilder};
//! use reasondb_core::llm::{Reasoner, LLMProvider};
//!
//! let reasoner = Reasoner::new(LLMProvider::openai_mini("sk-..."));
//! let pipeline = IngestPipeline::new(reasoner);
//! let result = pipeline.ingest_text("Title", "table-id", "# Hello\nworld").await?;
//! ```
//!
//! ## Pipeline Stages
//!
//! 1. **Extraction** — Convert documents to Markdown via extractor plugins
//! 2. **Post-processing** — Optional markdown transforms via post-processor plugins
//! 3. **Chunking** — Split into semantic chunks (plugin or built-in)
//! 4. **Tree Building** — Organize into a hierarchical tree structure
//! 5. **Summarization** — Generate summaries using LLM or summarizer plugin
//! 6. **Storage** — Store in ReasonDB for searching

pub mod chunker;
pub mod error;
pub mod extractor;
pub mod pipeline;
pub mod summarizer;
pub mod tree_builder;

// Re-export main types
pub use chunker::{ChunkerConfig, DetectedHeading, SemanticChunker, TextChunk, TocExtractor};
pub use error::{IngestError, Result};
pub use extractor::{DocumentType, ExtractionResult, SmartExtractor};
pub use pipeline::{
    IngestPipeline, IngestResult, IngestStats, NoOpReasoner, PipelineBuilder, PipelineConfig,
};
pub use summarizer::{BatchSummarizer, MockSummarizer, NodeSummarizer, SummarizerConfig};
pub use tree_builder::TreeBuilder;
