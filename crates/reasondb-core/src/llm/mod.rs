//! LLM Interface for ReasonDB
//!
//! This module defines the `ReasoningEngine` trait that abstracts LLM interactions.
//! Supports multiple providers: OpenAI, Anthropic, Gemini, Cohere, GLM (Zhipu AI),
//! Kimi (Moonshot AI), and Ollama (local models).

pub mod config;
pub mod dynamic;
pub mod mock;
pub mod provider;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::PageNode;
use crate::query_decomposer::{DomainContext, SubQuery};

/// A summary of a node for LLM decision making
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NodeSummary {
    /// Unique identifier for the node
    pub id: String,
    /// Human-readable title
    pub title: String,
    /// Brief summary of the node's content
    pub summary: String,
    /// Depth level in the tree
    pub depth: u8,
    /// Whether this is a leaf node
    pub is_leaf: bool,
}

impl From<&PageNode> for NodeSummary {
    fn from(node: &PageNode) -> Self {
        Self {
            id: node.id.clone(),
            title: node.title.clone(),
            summary: node.summary.clone(),
            depth: node.depth,
            is_leaf: node.is_leaf(),
        }
    }
}

/// Decision made by the LLM about which branches to explore.
/// Uses JsonSchema for structured output extraction.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TraversalDecision {
    /// ID of the node to explore
    pub node_id: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Explanation for why this branch was chosen
    pub reasoning: String,
}

/// Wrapper for multiple traversal decisions
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TraversalDecisions {
    /// List of selected nodes to explore
    pub selections: Vec<TraversalDecision>,
}

/// Raw LLM output for leaf verification — uses an integer scale for better score variance.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct VerificationResultRaw {
    /// Whether the content is relevant to the query
    pub is_relevant: bool,
    /// Relevance score from 1 (not relevant at all) to 10 (perfect, comprehensive answer)
    pub relevance_score: u8,
}

/// Result of verifying if a leaf node is relevant to the query.
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether the content is relevant to the query
    pub is_relevant: bool,
    /// Confidence score (0.0 - 1.0), derived from the LLM's 1-10 rating
    pub confidence: f32,
}

impl From<VerificationResultRaw> for VerificationResult {
    fn from(raw: VerificationResultRaw) -> Self {
        let clamped = raw.relevance_score.clamp(1, 10);
        Self {
            is_relevant: raw.is_relevant,
            confidence: clamped as f32 / 10.0,
        }
    }
}

/// A document summary for quick LLM scanning/ranking
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocumentSummary {
    /// Document ID
    pub id: String,
    /// Document title
    pub title: String,
    /// Combined summary of document content (from root node)
    pub summary: String,
    /// Tags for context
    pub tags: Vec<String>,
    /// Titles of nodes that matched query terms (from tree-grep pre-filter)
    #[serde(default)]
    pub matched_sections: Vec<String>,
    /// Snippet from the highest-scoring matched node
    #[serde(default)]
    pub best_snippet: Option<String>,
}

/// Result of ranking a document's relevance to a query
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocumentRanking {
    /// Document ID
    pub document_id: String,
    /// Relevance score (0.0 - 1.0)
    pub relevance: f32,
    /// Optional brief explanation (omitted in fast-rank mode to save tokens)
    #[serde(default)]
    pub reasoning: String,
}

/// Wrapper for multiple document rankings
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocumentRankings {
    /// Ranked list of documents (highest relevance first)
    pub rankings: Vec<DocumentRanking>,
}

/// A single item in a batch summarization response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BatchSummaryItem {
    /// The node ID this summary belongs to
    pub node_id: String,
    /// The generated summary
    pub summary: String,
}

/// Wrapper for batch summarization results (structured output extraction)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BatchSummaryResult {
    /// Summaries for each node in the batch
    pub summaries: Vec<BatchSummaryItem>,
}

/// A single sub-query item returned by the LLM during decomposition.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubQueryItem {
    /// The expanded/rephrased search query text
    pub text: String,
    /// Brief explanation of why this phrasing helps retrieval
    pub rationale: String,
}

/// Wrapper for the decompose_query LLM response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DecomposedQueryResult {
    /// List of alternative search queries
    pub sub_queries: Vec<SubQueryItem>,
}

/// Wrapper for the extract_domain_vocab LLM response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DomainVocabResult {
    /// Extracted domain-specific terms
    pub terms: Vec<String>,
}

/// One leaf node to score in a batch verification call.
#[derive(Debug, Clone)]
pub struct BatchVerifyInput {
    /// Leaf node ID (echoed back in the response for alignment)
    pub node_id: String,
    /// Content to evaluate — already truncated / summary-prefixed by the caller
    pub content: String,
}

/// Score returned for a single entry in a batch verification call.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BatchVerifyScore {
    /// 0-based index matching the request array position
    pub index: usize,
    /// Whether this section is relevant to the query
    pub is_relevant: bool,
    /// Relevance on a 1-10 scale (same scale as verify_answer)
    pub relevance_score: u8,
}

/// Structured LLM response for batch verification.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BatchVerifyResponse {
    /// One score per input section (must cover every index)
    pub results: Vec<BatchVerifyScore>,
}

/// Context for summarization during ingestion
#[derive(Debug, Clone, Default)]
pub struct SummarizationContext {
    /// Title of the section being summarized
    pub title: Option<String>,
    /// Parent section's summary for context
    pub parent_summary: Option<String>,
    /// Depth in the tree
    pub depth: u8,
    /// Whether this is a leaf node (actual content) or internal (children summaries)
    pub is_leaf: bool,
}

/// The core trait for LLM-based reasoning.
///
/// This trait abstracts the LLM interaction, allowing different providers
/// (OpenAI, Anthropic, local models) to be used interchangeably.
///
/// # Example
///
/// ```rust,ignore
/// use reasondb_core::llm::{ReasoningEngine, NodeSummary};
///
/// async fn search_with_reasoning<R: ReasoningEngine>(
///     reasoner: &R,
///     query: &str,
///     candidates: &[NodeSummary],
/// ) {
///     let decisions = reasoner.decide_next_step(query, "", candidates, 4).await.unwrap();
///     for decision in decisions {
///         println!("Explore {} (confidence: {})", decision.node_id, decision.confidence);
///     }
/// }
/// ```
#[async_trait]
pub trait ReasoningEngine: Send + Sync {
    /// Decide which branches to explore next.
    ///
    /// Given a query and a list of candidate nodes (children of the current node),
    /// the LLM decides which branches are most likely to contain relevant information.
    ///
    /// # Arguments
    ///
    /// * `query` - The user's search query
    /// * `current_context` - Summary of the current node (breadcrumb context)
    /// * `candidates` - List of child nodes to choose from
    ///
    /// # Returns
    ///
    /// A list of decisions indicating which nodes to explore, with confidence scores.
    async fn decide_next_step(
        &self,
        query: &str,
        current_context: &str,
        candidates: &[NodeSummary],
        max_selections: usize,
    ) -> Result<Vec<TraversalDecision>>;

    /// Verify if a leaf node's content is relevant to the query.
    ///
    /// When we reach a leaf node, this method determines if the content
    /// is actually relevant to the query.
    ///
    /// # Arguments
    ///
    /// * `query` - The user's search query
    /// * `content` - The actual content of the leaf node
    ///
    /// # Returns
    ///
    /// A verification result indicating relevance and confidence.
    async fn verify_answer(&self, query: &str, content: &str) -> Result<VerificationResult>;

    /// Verify multiple leaf nodes in a **single** LLM call.
    ///
    /// This is the primary verification path: instead of issuing one
    /// `verify_answer` round-trip per leaf (17 calls × 4 docs = 68 total),
    /// the engine accumulates all leaves from a traversal level and calls
    /// this method once, receiving scores for the whole batch.
    ///
    /// The default implementation falls back to sequential `verify_answer`
    /// calls so every provider is automatically correct even before adding
    /// an optimised override.
    async fn batch_verify_answers(
        &self,
        query: &str,
        candidates: &[BatchVerifyInput],
    ) -> Result<Vec<VerificationResult>> {
        let mut out = Vec::with_capacity(candidates.len());
        for c in candidates {
            out.push(self.verify_answer(query, &c.content).await?);
        }
        Ok(out)
    }

    /// Generate a summary for a node during ingestion.
    ///
    /// This is called during document ingestion to create summaries
    /// that will be used for navigation decisions during search.
    ///
    /// # Arguments
    ///
    /// * `content` - The content to summarize (raw text for leaves, child summaries for internals)
    /// * `context` - Additional context about the node
    ///
    /// # Returns
    ///
    /// A concise summary suitable for LLM-guided navigation.
    async fn summarize(&self, content: &str, context: &SummarizationContext) -> Result<String>;

    /// Summarize multiple nodes in a single LLM request.
    ///
    /// Reduces API round-trips during ingestion by batching summaries.
    /// The default implementation falls back to individual `summarize()` calls.
    ///
    /// # Arguments
    ///
    /// * `items` - Tuples of (node_id, content, context) for each node to summarize
    ///
    /// # Returns
    ///
    /// A list of (node_id, summary) pairs.
    async fn summarize_batch(
        &self,
        items: &[(String, String, SummarizationContext)],
    ) -> Result<Vec<(String, String)>> {
        let mut results = Vec::with_capacity(items.len());
        for (node_id, content, context) in items {
            let summary = self.summarize(content, context).await?;
            results.push((node_id.clone(), summary));
        }
        Ok(results)
    }

    /// Rank documents by relevance to a query based on their summaries.
    ///
    /// This is used for "agentic search" - quickly scanning many document
    /// summaries to find the most relevant ones before deep reasoning.
    ///
    /// # Arguments
    ///
    /// * `query` - The user's search query
    /// * `documents` - List of document summaries to rank
    /// * `top_k` - Maximum number of documents to return
    ///
    /// # Returns
    ///
    /// Ranked list of documents with relevance scores.
    async fn rank_documents(
        &self,
        _query: &str,
        documents: &[DocumentSummary],
        top_k: usize,
    ) -> Result<Vec<DocumentRanking>> {
        // Default implementation: return all documents with equal relevance
        // Providers can override this with actual LLM-based ranking
        Ok(documents
            .iter()
            .take(top_k)
            .map(|doc| DocumentRanking {
                document_id: doc.id.clone(),
                relevance: 0.5,
                reasoning: "Default ranking".to_string(),
            })
            .collect())
    }

    /// Decompose a user query into multiple domain-aligned sub-queries.
    ///
    /// For domain-specific corpora (e.g. insurance policies), users write natural
    /// language queries that have no BM25 overlap with the document content.
    /// This method expands the query into 3–5 semantically richer alternatives
    /// using the table's domain context (description + known vocabulary).
    ///
    /// # Arguments
    ///
    /// * `query` - The original user query
    /// * `domain_context` - Optional domain context built from `Table.description`
    ///   and `Table.metadata["domain_vocab"]`
    ///
    /// # Returns
    ///
    /// A list of sub-queries. The original query is always included.
    /// Default implementation returns only the original query (no decomposition).
    async fn decompose_query(
        &self,
        query: &str,
        _domain_context: Option<&DomainContext>,
    ) -> Result<Vec<SubQuery>> {
        Ok(vec![SubQuery {
            text: query.to_string(),
            rationale: "Original query (decomposition not implemented)".to_string(),
        }])
    }

    /// Extract domain-specific vocabulary terms from a document summary.
    ///
    /// Called after ingestion to automatically populate `Table.metadata["domain_vocab"]`
    /// with jargon and technical terms that users are unlikely to search for directly
    /// but that appear throughout the document collection.
    ///
    /// # Arguments
    ///
    /// * `document_summary` - The root node summary of the newly ingested document
    /// * `existing_vocab` - Terms already in `Table.metadata["domain_vocab"]`
    ///   (to avoid duplicates)
    ///
    /// # Returns
    ///
    /// New unique terms to append to the table vocabulary.
    /// Default implementation returns an empty list.
    async fn extract_domain_vocab(
        &self,
        _document_summary: &str,
        _existing_vocab: &[String],
    ) -> Result<Vec<String>> {
        Ok(vec![])
    }

    /// Get the name of this reasoning engine (for logging/debugging)
    fn name(&self) -> &str;
}

/// Configuration for the reasoning engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningConfig {
    /// Maximum number of branches to explore at each level (beam width)
    pub beam_width: usize,
    /// Minimum confidence threshold for branch selection
    pub min_confidence: f32,
    /// Maximum tokens for summarization
    pub max_summary_tokens: usize,
    /// Temperature for LLM responses (0.0 - 1.0)
    pub temperature: f32,
}

impl Default for ReasoningConfig {
    fn default() -> Self {
        Self {
            beam_width: 3,
            min_confidence: 0.3,
            max_summary_tokens: 150,
            temperature: 0.1,
        }
    }
}

// Re-export for convenience
pub use config::{LlmModelConfig, LlmOptions, LlmSettings};
pub use dynamic::DynamicReasoner;
pub use mock::MockReasoner;
pub use provider::{LLMProvider, Reasoner};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_summary_from_page_node() {
        let node = PageNode::new(
            "doc_1".to_string(),
            "Test".to_string(),
            Some("Summary".to_string()),
            1,
        );

        let summary = NodeSummary::from(&node);
        assert_eq!(summary.id, node.id);
        assert_eq!(summary.title, "Test");
        assert_eq!(summary.summary, "Summary");
        assert_eq!(summary.depth, 1);
    }

    #[test]
    fn test_default_config() {
        let config = ReasoningConfig::default();
        assert_eq!(config.beam_width, 3);
        assert_eq!(config.min_confidence, 0.3);
    }
}
