//! Error types for ReasonDB
//!
//! This module defines all error types used throughout the ReasonDB core library.

use thiserror::Error;

/// Main error type for ReasonDB operations
#[derive(Error, Debug)]
pub enum ReasonError {
    /// Storage-related errors
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    /// Serialization/deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Node not found
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    /// Document not found
    #[error("Document not found: {0}")]
    DocumentNotFound(String),

    /// Table not found
    #[error("Table not found: {0}")]
    TableNotFound(String),

    /// Invalid operation
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// LLM-related errors
    #[error("Reasoning error: {0}")]
    Reasoning(String),

    /// Generic internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Storage-specific errors
#[derive(Error, Debug)]
pub enum StorageError {
    /// Database open/create error
    #[error("Failed to open database: {0}")]
    OpenError(String),

    /// Transaction error
    #[error("Transaction error: {0}")]
    TransactionError(String),

    /// Table operation error
    #[error("Table error: {0}")]
    TableError(String),

    /// Table already exists (by ID)
    #[error("Table already exists: {0}")]
    TableAlreadyExists(String),

    /// Table name already exists (by slug)
    #[error("Table name already exists: {0}")]
    TableNameExists(String),

    /// Table not empty (has documents)
    #[error("Table not empty: {0}")]
    TableNotEmpty(String),

    /// I/O error
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Convert redb errors to our storage error
impl From<redb::Error> for StorageError {
    fn from(err: redb::Error) -> Self {
        StorageError::OpenError(err.to_string())
    }
}

impl From<redb::DatabaseError> for StorageError {
    fn from(err: redb::DatabaseError) -> Self {
        StorageError::OpenError(err.to_string())
    }
}

impl From<redb::TableError> for StorageError {
    fn from(err: redb::TableError) -> Self {
        StorageError::TableError(err.to_string())
    }
}

impl From<redb::TransactionError> for StorageError {
    fn from(err: redb::TransactionError) -> Self {
        StorageError::TransactionError(err.to_string())
    }
}

impl From<redb::CommitError> for StorageError {
    fn from(err: redb::CommitError) -> Self {
        StorageError::TransactionError(err.to_string())
    }
}

impl From<redb::StorageError> for StorageError {
    fn from(err: redb::StorageError) -> Self {
        StorageError::OpenError(err.to_string())
    }
}

/// Convert bincode errors
impl From<bincode::Error> for ReasonError {
    fn from(err: bincode::Error) -> Self {
        ReasonError::Serialization(err.to_string())
    }
}

/// Result type alias for ReasonDB operations
pub type Result<T> = std::result::Result<T, ReasonError>;
