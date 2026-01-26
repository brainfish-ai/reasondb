//! # ReasonDB Ingest
//!
//! Document ingestion pipeline for ReasonDB.
//!
//! This crate provides:
//! - PDF text extraction
//! - Semantic text chunking with ToC detection
//! - Hierarchical tree building
//! - LLM-based summarization
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
//! let result = pipeline.ingest_pdf("document.pdf").await?;
//!
//! // Without LLM (mock summaries)
//! let pipeline = IngestPipeline::without_llm();
//! let result = pipeline.ingest_pdf("document.pdf").await?;
//!
//! // Using builder
//! let pipeline = PipelineBuilder::new()
//!     .chunk_size(1500, 500, 3000)
//!     .use_toc_detection(true)
//!     .build();
//! ```
//!
//! ## Pipeline Stages
//!
//! 1. **PDF Extraction** - Extract text from PDF files using `pdf-extract` or `lopdf`
//! 2. **Chunking** - Split text into semantic chunks with heading detection
//! 3. **Tree Building** - Organize chunks into a hierarchical tree structure
//! 4. **Summarization** - Generate summaries using LLM (bottom-up)
//! 5. **Storage** - Store in ReasonDB for searching

pub mod chunker;
pub mod error;
pub mod pdf;
pub mod pipeline;
pub mod summarizer;
pub mod tree_builder;

// Re-export main types
pub use chunker::{ChunkerConfig, DetectedHeading, SemanticChunker, TextChunk, TocExtractor};
pub use error::{IngestError, Result};
pub use pdf::{ExtractedPage, PdfExtraction, PdfExtractor};
pub use pipeline::{IngestPipeline, IngestResult, IngestStats, NoOpReasoner, PipelineBuilder, PipelineConfig};
pub use summarizer::{MockSummarizer, NodeSummarizer, SummarizerConfig};
pub use tree_builder::TreeBuilder;
