//! Error types for the ingestion pipeline

use thiserror::Error;

/// Errors that can occur during document ingestion
#[derive(Error, Debug)]
pub enum IngestError {
    /// PDF parsing failed
    #[error("PDF parsing error: {0}")]
    PdfParse(String),

    /// File I/O error
    #[error("File error: {0}")]
    FileIO(#[from] std::io::Error),

    /// Text extraction failed
    #[error("Text extraction error: {0}")]
    TextExtraction(String),

    /// Chunking failed
    #[error("Chunking error: {0}")]
    Chunking(String),

    /// Summarization failed
    #[error("Summarization error: {0}")]
    Summarization(String),

    /// Tree building failed
    #[error("Tree building error: {0}")]
    TreeBuilding(String),

    /// Storage error
    #[error("Storage error: {0}")]
    Storage(#[from] reasondb_core::ReasonError),

    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

/// Result type for ingestion operations
pub type Result<T> = std::result::Result<T, IngestError>;
