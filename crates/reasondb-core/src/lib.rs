//! # ReasonDB Core
//!
//! Core library for ReasonDB - a reasoning-native database for AI agents.
//!
//! This crate provides:
//! - Data models (`PageNode`, `Document`)
//! - Storage engine (`NodeStore`)
//! - Reasoning engine trait (`ReasoningEngine`)
//! - Search engine with beam search
//!
//! ## Example
//!
//! ```rust,no_run
//! use reasondb_core::{NodeStore, PageNode, Document};
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Open a database
//! let store = NodeStore::open("./my_database")?;
//!
//! // Create a document with nodes
//! let doc = Document::new("My Document".to_string());
//! store.insert_document(&doc)?;
//!
//! // Query nodes
//! let node = store.get_node("node_id")?;
//! # Ok(())
//! # }
//! ```

pub mod engine;
pub mod error;
pub mod llm;
pub mod model;
pub mod store;

// Re-export main types
pub use engine::{SearchConfig, SearchEngine, SearchResult};
pub use error::{ReasonError, Result};
pub use llm::{MockReasoner, OpenAIReasoner, ReasoningEngine};
pub use model::{Document, NodeId, NodeMetadata, PageNode};
pub use store::NodeStore;
