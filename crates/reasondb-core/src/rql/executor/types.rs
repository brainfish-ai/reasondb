//! Query result types and statistics
//!
//! This module contains all the data structures returned from query execution.

use crate::engine::ReasoningStep;
use crate::model::Document;
use serde::Serialize;

/// Result of executing a query.
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// Matched documents (for regular SELECT queries)
    pub documents: Vec<DocumentMatch>,
    /// Total count (before pagination)
    pub total_count: usize,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Query execution statistics
    pub stats: QueryStats,
    /// Aggregate results (for COUNT/SUM/AVG/etc. queries)
    pub aggregates: Option<Vec<AggregateResult>>,
    /// Query plan (for EXPLAIN queries)
    pub explain: Option<QueryPlan>,
    /// Trace ID for this query (only set for REASON queries when tracing is enabled)
    pub trace_id: Option<String>,
}

/// A node selected during REASON traversal with full context.
#[derive(Debug, Clone)]
pub struct MatchedNode {
    /// Node ID
    pub node_id: String,
    /// Node title
    pub title: String,
    /// The actual content of the node
    pub content: String,
    /// Path from root to this node (titles)
    pub path: Vec<String>,
    /// Confidence score for this match
    pub confidence: f32,
    /// The reasoning trace showing decisions that led here
    pub reasoning_trace: Vec<ReasoningStep>,
}

/// A document match with relevance info.
#[derive(Debug, Clone)]
pub struct DocumentMatch {
    /// The matched document
    pub document: Document,
    /// Relevance score (for search queries)
    pub score: Option<f32>,
    /// Nodes that matched the query (full details for REASON queries)
    pub matched_nodes: Vec<MatchedNode>,
    /// Highlighted text snippets
    pub highlights: Vec<String>,
    /// Confidence score from LLM (for REASON queries)
    pub confidence: Option<f32>,
}

/// Query execution statistics for analysis and optimization.
#[derive(Debug, Clone, Default)]
pub struct QueryStats {
    /// Index used for initial filtering
    pub index_used: Option<String>,
    /// Total rows scanned
    pub rows_scanned: usize,
    /// Rows returned after filtering
    pub rows_returned: usize,
    /// Whether SEARCH clause was executed
    pub search_executed: bool,
    /// Whether REASON clause was executed
    pub reason_executed: bool,
    /// Number of LLM calls made (for REASON)
    pub llm_calls: usize,
}

// ==================== Mutation Result ====================

/// Result of executing an UPDATE or DELETE statement.
#[derive(Debug, Clone)]
pub struct MutationResult {
    /// Number of documents affected
    pub rows_affected: usize,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
}

// ==================== REASON Progress Types ====================

/// Phase of the REASON query execution pipeline.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReasonPhase {
    Candidates,
    Filtering,
    Ranking,
    Reasoning,
}

/// Status within a phase.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReasonPhaseStatus {
    Started,
    Progress,
    Completed,
}

/// Progress event emitted during REASON query execution.
#[derive(Debug, Clone, Serialize)]
pub struct ReasonProgress {
    pub phase: ReasonPhase,
    pub status: ReasonPhaseStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

// ==================== Aggregate Types ====================

/// Result of an aggregate function
#[derive(Debug, Clone)]
pub struct AggregateResult {
    /// Alias or function name
    pub name: String,
    /// Computed value
    pub value: AggregateValue,
    /// Group key (for GROUP BY queries)
    pub group_key: Option<Vec<(String, serde_json::Value)>>,
}

/// Value types for aggregate results
#[derive(Debug, Clone)]
pub enum AggregateValue {
    /// Integer count
    Count(usize),
    /// Floating point sum/avg/min/max
    Float(f64),
    /// Null (when no rows match)
    Null,
}

// ==================== Query Plan Types ====================

/// Query execution plan (for EXPLAIN)
#[derive(Debug, Clone)]
pub struct QueryPlan {
    /// Steps in the execution plan
    pub steps: Vec<PlanStep>,
    /// Estimated row count
    pub estimated_rows: usize,
    /// Indexes that would be used
    pub indexes_used: Vec<String>,
}

/// A single step in the query plan
#[derive(Debug, Clone)]
pub struct PlanStep {
    /// Step type (e.g., "TableScan", "IndexScan", "Filter", "Aggregate")
    pub step_type: String,
    /// Description of what this step does
    pub description: String,
    /// Estimated cost (0-100)
    pub estimated_cost: u32,
}
