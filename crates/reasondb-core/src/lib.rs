//! # ReasonDB Core
//!
//! Core library for ReasonDB - a reasoning-native database for AI agents.
//!
//! This crate provides:
//! - Data models (`Table`, `Document`, `PageNode`)
//! - Storage engine (`NodeStore`)
//! - Reasoning engine trait (`ReasoningEngine`)
//! - Search engine with beam search
//! - Filtering with `SearchFilter`
//! - RQL query language
//! - Authentication & API key management
//!
//! ## Example
//!
//! ```rust,no_run
//! use reasondb_core::{NodeStore, PageNode, Document, Table, SearchFilter};
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Open a database
//! let store = NodeStore::open("./my_database")?;
//!
//! // Create a table first (required for documents)
//! let table = Table::new("Legal Contracts".to_string());
//! store.insert_table(&table)?;
//!
//! // Create a document in the table
//! let mut doc = Document::new("NDA Agreement".to_string(), &table.id);
//! doc.tags = vec!["nda".to_string(), "confidential".to_string()];
//! store.insert_document(&doc)?;
//!
//! // Filter documents
//! let filter = SearchFilter::new()
//!     .with_table_id(&table.id)
//!     .with_tags(vec!["nda"]);
//! let docs = store.find_documents(&filter)?;
//!
//! // Or use RQL query language
//! use reasondb_core::rql::Query;
//! let query = Query::parse("SELECT * FROM legal_contracts WHERE author = 'Alice'")?;
//! let results = store.execute_rql(&query)?;
//! # Ok(())
//! # }
//! ```

pub mod auth;
pub mod backup;
pub mod cache;
pub mod cluster;
pub mod engine;
pub mod error;
pub mod llm;
pub mod model;
pub mod query_decomposer;
pub mod query_filter;
pub mod ratelimit;
pub mod rql;
pub mod shard;
pub mod store;
pub mod text_index;
pub mod trace;
pub mod tree_grep;

// Re-export main types
pub use auth::{ApiKey, ApiKeyId, ApiKeyMetadata, ApiKeyStore, KeyPrefix, Permission, Permissions};
pub use backup::{
    BackupInfo, BackupManager, BackupOptions, BackupType, ExportFormat, ExportOptions, ExportScope,
    Exporter, ImportOptions, ImportResult, Importer, RestoreOptions, Snapshot, SnapshotMetadata,
};
pub use cache::{
    CachedDocSummary, CachedMatch, CachedMatchedNode, CachedQueryResult, QueryCache,
    QueryCacheStats, SummaryCache,
};
pub use cluster::{
    ClusterConfig, ClusterNode, ClusterState, ClusterStateMachine, LogEntry, LogEntryType,
    NetworkClient, NetworkMessage, NodeConfig, NodeId as ClusterNodeId, NodeRole, NodeStatus,
    RaftNode,
};
pub use engine::{SearchConfig, SearchEngine, SearchResult};
pub use error::{ReasonDBError, ReasonError, Result};
pub use llm::{
    DynamicReasoner, LLMProvider, LlmModelConfig, LlmOptions, LlmSettings, MockReasoner, Reasoner,
    ReasoningEngine,
};
pub use model::{
    Document, DocumentId, DocumentRelation, NodeId, NodeMetadata, PageNode, RelationBuilder,
    RelationType, SearchFilter, Table, TableId,
};
pub use query_decomposer::{DecomposedQuery, DomainContext, SubQuery};
pub use ratelimit::{RateLimitConfig, RateLimitResult, RateLimitStore, RateLimitTier, RateLimiter};
pub use shard::{ScatterGatherResult, ShardMap, ShardRouter};
pub use store::{NodeStore, StoreStats};
pub use text_index::{TextIndex, TextSearchResult};
pub use trace::{QueryTrace, QueryTraceSummary};
