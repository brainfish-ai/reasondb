//! LLM Interface for ReasonDB
//!
//! This module defines the `ReasoningEngine` trait that abstracts LLM interactions.
//! Different providers (OpenAI, Anthropic, local models) can implement this trait.

pub mod mock;
pub mod openai;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::PageNode;

/// A summary of a node for LLM decision making
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSummary {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub depth: u8,
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

/// Decision made by the LLM about which branches to explore
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalDecision {
    /// ID of the node to explore
    pub node_id: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Explanation for why this branch was chosen
    pub reasoning: String,
}

/// Result of verifying if a leaf node answers the query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether the content is relevant to the query
    pub is_relevant: bool,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Extracted answer from the content (if relevant)
    pub extracted_answer: Option<String>,
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
///     let decisions = reasoner.decide_next_step(query, "", candidates).await.unwrap();
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
    ) -> Result<Vec<TraversalDecision>>;

    /// Verify if a leaf node's content answers the query.
    ///
    /// When we reach a leaf node, this method determines if the content
    /// is actually relevant and can answer the query.
    ///
    /// # Arguments
    ///
    /// * `query` - The user's search query
    /// * `content` - The actual content of the leaf node
    ///
    /// # Returns
    ///
    /// A verification result indicating relevance and optionally extracting the answer.
    async fn verify_answer(
        &self,
        query: &str,
        content: &str,
    ) -> Result<VerificationResult>;

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
    async fn summarize(
        &self,
        content: &str,
        context: &SummarizationContext,
    ) -> Result<String>;

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

// Re-export for convenience
pub use mock::MockReasoner;
pub use openai::OpenAIReasoner;
