//! OpenAI implementation of ReasoningEngine
//!
//! This module provides integration with OpenAI's API for LLM-based reasoning.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{
    NodeSummary, ReasoningConfig, ReasoningEngine, SummarizationContext, TraversalDecision,
    VerificationResult,
};
use crate::error::{ReasonError, Result};

/// OpenAI-based reasoning engine.
///
/// Uses the OpenAI Chat Completions API for decision making.
///
/// # Example
///
/// ```rust,ignore
/// use reasondb_core::llm::OpenAIReasoner;
///
/// let reasoner = OpenAIReasoner::new("your-api-key")
///     .with_model("gpt-4o-mini");
/// ```
pub struct OpenAIReasoner {
    api_key: String,
    model: String,
    config: ReasoningConfig,
    client: reqwest::Client,
    base_url: String,
}

impl OpenAIReasoner {
    /// Create a new OpenAI reasoner with the given API key
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: "gpt-4o-mini".to_string(),
            config: ReasoningConfig::default(),
            client: reqwest::Client::new(),
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set the reasoning configuration
    pub fn with_config(mut self, config: ReasoningConfig) -> Self {
        self.config = config;
        self
    }

    /// Set a custom base URL (for proxies or compatible APIs)
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Make a chat completion request
    async fn chat_completion(&self, messages: Vec<ChatMessage>) -> Result<String> {
        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages,
            temperature: self.config.temperature,
            max_tokens: Some(1024),
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ReasonError::Reasoning(format!("HTTP error: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ReasonError::Reasoning(format!(
                "OpenAI API error ({}): {}",
                status, body
            )));
        }

        let completion: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| ReasonError::Reasoning(format!("JSON parse error: {}", e)))?;

        completion
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| ReasonError::Reasoning("No response from OpenAI".to_string()))
    }

    /// Format candidates for the prompt
    fn format_candidates(&self, candidates: &[NodeSummary]) -> String {
        candidates
            .iter()
            .enumerate()
            .map(|(i, c)| {
                format!(
                    "{}. ID: {} | Title: \"{}\" | Summary: {}",
                    i + 1,
                    c.id,
                    c.title,
                    c.summary
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Parse decision response from LLM
    fn parse_decisions(&self, response: &str, candidates: &[NodeSummary]) -> Vec<TraversalDecision> {
        // Try to parse as JSON first
        if let Ok(decisions) = serde_json::from_str::<DecisionResponse>(response) {
            return decisions.selections;
        }

        // Fallback: Look for IDs mentioned in the response
        let mut decisions = Vec::new();
        for candidate in candidates {
            if response.contains(&candidate.id) {
                decisions.push(TraversalDecision {
                    node_id: candidate.id.clone(),
                    confidence: 0.7,
                    reasoning: format!("ID {} found in response", candidate.id),
                });
            }
        }

        // If still empty, check for numbers/indices
        if decisions.is_empty() {
            for (i, candidate) in candidates.iter().enumerate() {
                let patterns = [
                    format!("{}.", i + 1),
                    format!("#{}", i + 1),
                    format!("option {}", i + 1),
                ];
                if patterns.iter().any(|p| response.to_lowercase().contains(p)) {
                    decisions.push(TraversalDecision {
                        node_id: candidate.id.clone(),
                        confidence: 0.6,
                        reasoning: format!("Index {} found in response", i + 1),
                    });
                }
            }
        }

        decisions.truncate(self.config.beam_width);
        decisions
    }
}

#[async_trait]
impl ReasoningEngine for OpenAIReasoner {
    async fn decide_next_step(
        &self,
        query: &str,
        current_context: &str,
        candidates: &[NodeSummary],
    ) -> Result<Vec<TraversalDecision>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let context_part = if current_context.is_empty() {
            String::new()
        } else {
            format!("\nCurrent location: {}\n", current_context)
        };

        let system_prompt = "You are a document navigation assistant. Your task is to select which sections are most likely to contain information relevant to the user's query. Return your selections as JSON.";

        let user_prompt = format!(
            r#"Query: "{}"
{}
Available sections:
{}

Select up to {} sections most likely to contain the answer.
Return JSON: {{"selections": [{{"node_id": "...", "confidence": 0.0-1.0, "reasoning": "..."}}]}}
Only include sections that are likely relevant. If none seem relevant, return an empty array."#,
            query,
            context_part,
            self.format_candidates(candidates),
            self.config.beam_width
        );

        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt,
            },
        ];

        let response = self.chat_completion(messages).await?;
        let decisions = self.parse_decisions(&response, candidates);

        Ok(decisions)
    }

    async fn verify_answer(&self, query: &str, content: &str) -> Result<VerificationResult> {
        // Truncate content if too long
        let truncated_content: String = content.chars().take(4000).collect();

        let system_prompt = "You are a relevance verification assistant. Determine if the given content answers or is relevant to the user's query. Return your assessment as JSON.";

        let user_prompt = format!(
            r#"Query: "{}"

Content:
{}

Does this content answer or contain information relevant to the query?
Return JSON: {{"is_relevant": true/false, "confidence": 0.0-1.0, "extracted_answer": "brief answer or null"}}"#,
            query, truncated_content
        );

        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt,
            },
        ];

        let response = self.chat_completion(messages).await?;

        // Try to parse JSON response
        if let Ok(result) = serde_json::from_str::<VerificationResult>(&response) {
            return Ok(result);
        }

        // Fallback: Simple keyword detection
        let is_relevant = response.to_lowercase().contains("true")
            || response.to_lowercase().contains("relevant")
            || response.to_lowercase().contains("yes");

        Ok(VerificationResult {
            is_relevant,
            confidence: if is_relevant { 0.7 } else { 0.3 },
            extracted_answer: if is_relevant {
                Some(truncated_content.chars().take(500).collect())
            } else {
                None
            },
        })
    }

    async fn summarize(&self, content: &str, context: &SummarizationContext) -> Result<String> {
        let truncated_content: String = content.chars().take(8000).collect();

        let title_hint = context
            .title
            .as_ref()
            .map(|t| format!("Section title: \"{}\"\n", t))
            .unwrap_or_default();

        let node_type = if context.is_leaf {
            "content"
        } else {
            "section summaries"
        };

        let system_prompt = format!(
            "You are a document summarization assistant. Create a concise summary ({}) that captures the key topics and information. The summary should help someone decide if this section contains what they're looking for.",
            self.config.max_summary_tokens
        );

        let user_prompt = format!(
            r#"{}Summarize this {} in 1-2 sentences. Focus on:
- What topics/concepts are covered
- Key facts, figures, or conclusions
- What questions this section could answer

{}

Provide only the summary, no additional commentary."#,
            title_hint, node_type, truncated_content
        );

        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt,
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt,
            },
        ];

        self.chat_completion(messages).await
    }

    fn name(&self) -> &str {
        "OpenAIReasoner"
    }
}

// OpenAI API types

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct DecisionResponse {
    selections: Vec<TraversalDecision>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_candidates() {
        let reasoner = OpenAIReasoner::new("test-key");

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
        ];

        let formatted = reasoner.format_candidates(&candidates);
        assert!(formatted.contains("Chapter 1"));
        assert!(formatted.contains("Chapter 2"));
        assert!(formatted.contains("ID: 1"));
        assert!(formatted.contains("ID: 2"));
    }

    #[test]
    fn test_parse_decisions_json() {
        let reasoner = OpenAIReasoner::new("test-key");

        let response = r#"{"selections": [{"node_id": "abc", "confidence": 0.9, "reasoning": "test"}]}"#;
        let candidates = vec![NodeSummary {
            id: "abc".to_string(),
            title: "Test".to_string(),
            summary: "Test".to_string(),
            depth: 1,
            is_leaf: false,
        }];

        let decisions = reasoner.parse_decisions(response, &candidates);
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].node_id, "abc");
        assert_eq!(decisions[0].confidence, 0.9);
    }

    #[test]
    fn test_parse_decisions_fallback() {
        let reasoner = OpenAIReasoner::new("test-key");

        let response = "I think section abc123 is most relevant";
        let candidates = vec![NodeSummary {
            id: "abc123".to_string(),
            title: "Test".to_string(),
            summary: "Test".to_string(),
            depth: 1,
            is_leaf: false,
        }];

        let decisions = reasoner.parse_decisions(response, &candidates);
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].node_id, "abc123");
    }

    #[test]
    fn test_config_builder() {
        let reasoner = OpenAIReasoner::new("test-key")
            .with_model("gpt-4o")
            .with_config(ReasoningConfig {
                beam_width: 5,
                ..Default::default()
            });

        assert_eq!(reasoner.model, "gpt-4o");
        assert_eq!(reasoner.config.beam_width, 5);
    }
}
