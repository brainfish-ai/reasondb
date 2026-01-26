//! # ReasonDB Ingest
//!
//! Document ingestion pipeline for ReasonDB.
//!
//! This crate provides:
//! - Multi-format document extraction (via [MarkItDown](https://github.com/microsoft/markitdown))
//! - Semantic text chunking with ToC detection
//! - Hierarchical tree building
//! - LLM-based summarization
//!
//! ## Supported Formats
//!
//! With MarkItDown installed (`pip install 'markitdown[all]'`):
//! - PDF, Word (.docx), PowerPoint (.pptx), Excel (.xlsx)
//! - Images (JPEG, PNG) with OCR
//! - Audio (WAV, MP3) with transcription
//! - HTML, CSV, JSON, XML
//! - EPUB, ZIP files
//! - YouTube URLs
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use reasondb_ingest::{IngestPipeline, PipelineBuilder};
//! use reasondb_core::llm::{Reasoner, LLMProvider};
//!
//! // With LLM summarization
//! let reasoner = Reasoner::new(LLMProvider::openai_mini("sk-..."));
//! let pipeline = IngestPipeline::new(reasoner);
//! let result = pipeline.ingest_file("document.pdf").await?;
//! let result = pipeline.ingest_file("presentation.pptx").await?;
//! let result = pipeline.ingest_url("https://youtube.com/watch?v=...").await?;
//!
//! // Without LLM (mock summaries)
//! let pipeline = IngestPipeline::without_llm();
//! let result = pipeline.ingest_file("document.docx").await?;
//! ```
//!
//! ## Pipeline Stages
//!
//! 1. **Extraction** - Convert documents to Markdown using MarkItDown
//! 2. **Chunking** - Split into semantic chunks with heading detection
//! 3. **Tree Building** - Organize into a hierarchical tree structure
//! 4. **Summarization** - Generate summaries using LLM (bottom-up)
//! 5. **Storage** - Store in ReasonDB for searching

pub mod chunker;
pub mod error;
pub mod extractor;
pub mod pipeline;
pub mod summarizer;
pub mod tree_builder;

// Legacy PDF module (for backward compatibility)
#[doc(hidden)]
pub mod pdf;

// Re-export main types
pub use chunker::{ChunkerConfig, DetectedHeading, SemanticChunker, TextChunk, TocExtractor};
pub use error::{IngestError, Result};
pub use extractor::{DocumentType, ExtractionResult, MarkItDownExtractor, SmartExtractor};
pub use pipeline::{IngestPipeline, IngestResult, IngestStats, NoOpReasoner, PipelineBuilder, PipelineConfig};
pub use summarizer::{MockSummarizer, NodeSummarizer, SummarizerConfig};
pub use tree_builder::TreeBuilder;
