//! Mock implementation of ReasoningEngine for testing
//!
//! This module provides a mock reasoner that can be configured with
//! predetermined responses for testing the search engine.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::{
    NodeSummary, ReasoningEngine, SummarizationContext, TraversalDecision, VerificationResult,
};
use crate::error::Result;
use crate::query_decomposer::{DomainContext, SubQuery};

/// A mock reasoning engine for testing.
///
/// Can be configured with predetermined responses or use simple heuristics.
///
/// # Example
///
/// ```rust
/// use reasondb_core::llm::{MockReasoner, ReasoningEngine};
///
/// # async fn example() {
/// let reasoner = MockReasoner::new()
///     .with_always_select_first(true);
///
/// // Use in tests...
/// # }
/// ```
#[derive(Clone)]
pub struct MockReasoner {
    /// Always select the first N candidates
    select_first_n: usize,
    /// Always verify as relevant
    always_relevant: bool,
    /// Fixed confidence score to return
    fixed_confidence: f32,
    /// Keyword matching for smarter mock behavior
    keywords: Vec<String>,
    /// Track calls for verification in tests
    call_log: Arc<Mutex<Vec<MockCall>>>,
    /// Predetermined decisions for specific queries
    predetermined_decisions: HashMap<String, Vec<TraversalDecision>>,
}

/// Log entry for mock calls
#[derive(Debug, Clone)]
pub enum MockCall {
    DecideNextStep {
        query: String,
        num_candidates: usize,
    },
    VerifyAnswer {
        query: String,
        content_len: usize,
    },
    Summarize {
        content_len: usize,
    },
}

impl MockReasoner {
    /// Create a new mock reasoner with default settings
    pub fn new() -> Self {
        Self {
            select_first_n: 2,
            always_relevant: true,
            fixed_confidence: 0.8,
            keywords: Vec::new(),
            call_log: Arc::new(Mutex::new(Vec::new())),
            predetermined_decisions: HashMap::new(),
        }
    }

    /// Configure to always select the first candidate
    pub fn with_always_select_first(mut self, first_only: bool) -> Self {
        self.select_first_n = if first_only { 1 } else { 2 };
        self
    }

    /// Set how many candidates to select
    pub fn with_select_count(mut self, count: usize) -> Self {
        self.select_first_n = count;
        self
    }

    /// Set whether to always verify as relevant
    pub fn with_always_relevant(mut self, relevant: bool) -> Self {
        self.always_relevant = relevant;
        self
    }

    /// Set a fixed confidence score
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.fixed_confidence = confidence;
        self
    }

    /// Add keywords for smarter matching
    pub fn with_keywords(mut self, keywords: Vec<String>) -> Self {
        self.keywords = keywords;
        self
    }

    /// Add predetermined decisions for a specific query
    pub fn with_predetermined_decisions(
        mut self,
        query: &str,
        decisions: Vec<TraversalDecision>,
    ) -> Self {
        self.predetermined_decisions
            .insert(query.to_lowercase(), decisions);
        self
    }

    /// Get the call log for test verification
    pub fn get_call_log(&self) -> Vec<MockCall> {
        self.call_log.lock().unwrap().clone()
    }

    /// Clear the call log
    pub fn clear_call_log(&self) {
        self.call_log.lock().unwrap().clear();
    }

    /// Check if a candidate matches any keywords
    fn matches_keywords(&self, candidate: &NodeSummary) -> bool {
        if self.keywords.is_empty() {
            return true;
        }

        let text = format!("{} {}", candidate.title, candidate.summary).to_lowercase();
        self.keywords
            .iter()
            .any(|kw| text.contains(&kw.to_lowercase()))
    }

    /// Log a call
    fn log_call(&self, call: MockCall) {
        self.call_log.lock().unwrap().push(call);
    }
}

impl Default for MockReasoner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ReasoningEngine for MockReasoner {
    async fn decide_next_step(
        &self,
        query: &str,
        _current_context: &str,
        candidates: &[NodeSummary],
        _max_selections: usize,
    ) -> Result<Vec<TraversalDecision>> {
        self.log_call(MockCall::DecideNextStep {
            query: query.to_string(),
            num_candidates: candidates.len(),
        });

        // Check for predetermined decisions
        if let Some(decisions) = self.predetermined_decisions.get(&query.to_lowercase()) {
            return Ok(decisions.clone());
        }

        // Smart selection based on keywords
        let mut decisions: Vec<TraversalDecision> = candidates
            .iter()
            .filter(|c| self.matches_keywords(c))
            .take(self.select_first_n)
            .enumerate()
            .map(|(i, candidate)| TraversalDecision {
                node_id: candidate.id.clone(),
                confidence: self.fixed_confidence - (i as f32 * 0.1),
                reasoning: format!("Mock selected: {}", candidate.title),
            })
            .collect();

        // If no keyword matches, fall back to first N
        if decisions.is_empty() && !candidates.is_empty() {
            decisions = candidates
                .iter()
                .take(self.select_first_n)
                .enumerate()
                .map(|(i, candidate)| TraversalDecision {
                    node_id: candidate.id.clone(),
                    confidence: self.fixed_confidence - (i as f32 * 0.1),
                    reasoning: format!("Mock fallback selected: {}", candidate.title),
                })
                .collect();
        }

        Ok(decisions)
    }

    async fn verify_answer(&self, query: &str, content: &str) -> Result<VerificationResult> {
        self.log_call(MockCall::VerifyAnswer {
            query: query.to_string(),
            content_len: content.len(),
        });

        // Simple keyword matching for relevance
        let is_relevant = if self.always_relevant {
            true
        } else {
            let query_lower = query.to_lowercase();
            let content_lower = content.to_lowercase();
            query_lower
                .split_whitespace()
                .any(|word| content_lower.contains(word))
        };

        Ok(VerificationResult {
            is_relevant,
            confidence: if is_relevant {
                self.fixed_confidence
            } else {
                0.2
            },
        })
    }

    async fn summarize(&self, content: &str, context: &SummarizationContext) -> Result<String> {
        self.log_call(MockCall::Summarize {
            content_len: content.len(),
        });

        // Generate a mock summary
        let title_part = context
            .title
            .as_ref()
            .map(|t| format!("Section '{}': ", t))
            .unwrap_or_default();

        let preview: String = content
            .chars()
            .take(100)
            .collect::<String>()
            .replace('\n', " ");

        Ok(format!("{}{}...", title_part, preview.trim()))
    }

    async fn summarize_batch(
        &self,
        items: &[(String, String, SummarizationContext)],
    ) -> Result<Vec<(String, String)>> {
        items
            .iter()
            .map(|(node_id, content, context)| {
                self.log_call(MockCall::Summarize {
                    content_len: content.len(),
                });

                let title_part = context
                    .title
                    .as_ref()
                    .map(|t| format!("Section '{}': ", t))
                    .unwrap_or_default();

                let preview: String = content
                    .chars()
                    .take(100)
                    .collect::<String>()
                    .replace('\n', " ");

                Ok((
                    node_id.clone(),
                    format!("{}{}...", title_part, preview.trim()),
                ))
            })
            .collect()
    }

    async fn decompose_query(
        &self,
        query: &str,
        _domain_context: Option<&DomainContext>,
    ) -> Result<Vec<SubQuery>> {
        Ok(vec![SubQuery {
            text: query.to_string(),
            rationale: "Mock passthrough".to_string(),
        }])
    }

    async fn extract_domain_vocab(
        &self,
        _document_summary: &str,
        _existing_vocab: &[String],
    ) -> Result<Vec<String>> {
        Ok(vec![])
    }

    fn name(&self) -> &str {
        "MockReasoner"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_decide_next_step() {
        let reasoner = MockReasoner::new().with_select_count(2);

        let candidates = vec![
            NodeSummary {
                id: "1".to_string(),
                title: "Chapter 1".to_string(),
                summary: "About finance".to_string(),
                depth: 1,
                is_leaf: false,
            },
            NodeSummary {
                id: "2".to_string(),
                title: "Chapter 2".to_string(),
                summary: "About technology".to_string(),
                depth: 1,
                is_leaf: false,
            },
            NodeSummary {
                id: "3".to_string(),
                title: "Chapter 3".to_string(),
                summary: "About marketing".to_string(),
                depth: 1,
                is_leaf: false,
            },
        ];

        let decisions = reasoner
            .decide_next_step("test query", "", &candidates, 3)
            .await
            .unwrap();

        assert_eq!(decisions.len(), 2);
        assert_eq!(decisions[0].node_id, "1");
        assert_eq!(decisions[1].node_id, "2");
    }

    #[tokio::test]
    async fn test_mock_with_keywords() {
        let reasoner = MockReasoner::new()
            .with_keywords(vec!["finance".to_string()])
            .with_select_count(3);

        let candidates = vec![
            NodeSummary {
                id: "1".to_string(),
                title: "Marketing".to_string(),
                summary: "About marketing".to_string(),
                depth: 1,
                is_leaf: false,
            },
            NodeSummary {
                id: "2".to_string(),
                title: "Finance".to_string(),
                summary: "About financial data".to_string(),
                depth: 1,
                is_leaf: false,
            },
        ];

        let decisions = reasoner
            .decide_next_step("query", "", &candidates, 3)
            .await
            .unwrap();

        // Should only select the finance chapter
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].node_id, "2");
    }

    #[tokio::test]
    async fn test_mock_verify_answer() {
        let reasoner = MockReasoner::new().with_always_relevant(true);

        let result = reasoner
            .verify_answer("What is revenue?", "The revenue was $4.2 billion")
            .await
            .unwrap();

        assert!(result.is_relevant);
        assert!(result.confidence > 0.0);
    }

    #[tokio::test]
    async fn test_mock_summarize() {
        let reasoner = MockReasoner::new();

        let context = SummarizationContext {
            title: Some("Introduction".to_string()),
            ..Default::default()
        };

        let summary = reasoner
            .summarize("This is a long document about various topics...", &context)
            .await
            .unwrap();

        assert!(summary.contains("Introduction"));
    }

    #[tokio::test]
    async fn test_call_logging() {
        let reasoner = MockReasoner::new();

        let _ = reasoner.verify_answer("test", "content").await.unwrap();

        let log = reasoner.get_call_log();
        assert_eq!(log.len(), 1);

        match &log[0] {
            MockCall::VerifyAnswer { query, .. } => {
                assert_eq!(query, "test");
            }
            _ => panic!("Wrong call type"),
        }
    }

    #[tokio::test]
    async fn test_predetermined_decisions() {
        let reasoner = MockReasoner::new().with_predetermined_decisions(
            "specific query",
            vec![TraversalDecision {
                node_id: "predetermined_node".to_string(),
                confidence: 0.99,
                reasoning: "Predetermined".to_string(),
            }],
        );

        let candidates = vec![NodeSummary {
            id: "other".to_string(),
            title: "Other".to_string(),
            summary: "Other".to_string(),
            depth: 1,
            is_leaf: false,
        }];

        let decisions = reasoner
            .decide_next_step("specific query", "", &candidates, 3)
            .await
            .unwrap();

        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].node_id, "predetermined_node");
    }
}
