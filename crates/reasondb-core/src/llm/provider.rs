//! Multi-provider LLM implementation
//!
//! This module provides a unified interface to multiple LLM providers:
//! - OpenAI (GPT-4o, GPT-4o-mini, etc.)
//! - Anthropic (Claude 3.5 Sonnet, Claude 3 Haiku, etc.)
//! - Google Gemini
//! - Cohere
//!
//! Uses structured output extraction via `schemars::JsonSchema`.

use async_trait::async_trait;
use rig::completion::Prompt;
use serde::Serialize;
use tracing::debug;

use super::{
    NodeSummary, ReasoningConfig, ReasoningEngine, SummarizationContext, TraversalDecision,
    TraversalDecisions, VerificationResult,
};
use crate::error::{ReasonError, Result};

/// Supported LLM providers
#[derive(Debug, Clone)]
pub enum LLMProvider {
    /// OpenAI GPT models
    OpenAI { api_key: String, model: String },
    /// Anthropic Claude models
    Anthropic { api_key: String, model: String },
    /// Google Gemini models
    Gemini { api_key: String, model: String },
    /// Cohere models
    Cohere { api_key: String, model: String },
}

impl LLMProvider {
    /// Create an OpenAI provider with GPT-4o-mini (fast, cheap)
    pub fn openai_mini(api_key: impl Into<String>) -> Self {
        Self::OpenAI {
            api_key: api_key.into(),
            model: "gpt-4o-mini".to_string(),
        }
    }

    /// Create an OpenAI provider with GPT-4o (powerful)
    pub fn openai(api_key: impl Into<String>) -> Self {
        Self::OpenAI {
            api_key: api_key.into(),
            model: "gpt-4o".to_string(),
        }
    }

    /// Create an Anthropic provider with Claude 3.5 Sonnet
    pub fn claude_sonnet(api_key: impl Into<String>) -> Self {
        Self::Anthropic {
            api_key: api_key.into(),
            model: "claude-3-5-sonnet-20241022".to_string(),
        }
    }

    /// Create an Anthropic provider with Claude 3 Haiku (fast)
    pub fn claude_haiku(api_key: impl Into<String>) -> Self {
        Self::Anthropic {
            api_key: api_key.into(),
            model: "claude-3-haiku-20240307".to_string(),
        }
    }

    /// Create a Gemini provider with Gemini 1.5 Flash
    pub fn gemini(api_key: impl Into<String>) -> Self {
        Self::Gemini {
            api_key: api_key.into(),
            model: "gemini-1.5-flash".to_string(),
        }
    }

    /// Create a Gemini provider with Gemini 1.5 Pro
    pub fn gemini_pro(api_key: impl Into<String>) -> Self {
        Self::Gemini {
            api_key: api_key.into(),
            model: "gemini-1.5-pro".to_string(),
        }
    }

    /// Create a Cohere provider
    pub fn cohere(api_key: impl Into<String>) -> Self {
        Self::Cohere {
            api_key: api_key.into(),
            model: "command-r-plus".to_string(),
        }
    }
}

/// Multi-provider reasoning engine.
///
/// Supports structured output extraction via `schemars::JsonSchema`.
///
/// # Example
///
/// ```rust,ignore
/// use reasondb_core::llm::{Reasoner, LLMProvider};
///
/// // Using OpenAI
/// let reasoner = Reasoner::new(LLMProvider::openai_mini("sk-..."));
///
/// // Using Claude
/// let reasoner = Reasoner::new(LLMProvider::claude_sonnet("sk-ant-..."));
///
/// // Using Gemini
/// let reasoner = Reasoner::new(LLMProvider::gemini("your-api-key"));
/// ```
#[derive(Clone)]
pub struct Reasoner {
    provider: LLMProvider,
    config: ReasoningConfig,
}

impl Reasoner {
    /// Create a new Reasoner with the specified provider
    pub fn new(provider: LLMProvider) -> Self {
        Self {
            provider,
            config: ReasoningConfig::default(),
        }
    }

    /// Set custom configuration
    pub fn with_config(mut self, config: ReasoningConfig) -> Self {
        self.config = config;
        self
    }

    /// Format candidates for the prompt
    fn format_candidates(&self, candidates: &[NodeSummary]) -> String {
        candidates
            .iter()
            .enumerate()
            .map(|(i, c)| {
                format!(
                    "{}. ID: \"{}\" | Title: \"{}\" | Summary: {}",
                    i + 1,
                    c.id,
                    c.title,
                    c.summary
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Execute a completion request and extract structured output
    async fn extract<T>(&self, prompt: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned + schemars::JsonSchema + Serialize + Send + Sync + 'static,
    {
        match &self.provider {
            LLMProvider::OpenAI { api_key, model } => {
                let client = rig::providers::openai::Client::new(api_key);
                let extractor = client.extractor::<T>(model).build();

                extractor.extract(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("OpenAI extraction error: {}", e))
                })
            }
            LLMProvider::Anthropic { api_key, model } => {
                let client = rig::providers::anthropic::ClientBuilder::new(api_key).build();
                let extractor = client.extractor::<T>(model).build();

                extractor.extract(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("Anthropic extraction error: {}", e))
                })
            }
            LLMProvider::Gemini { api_key, model } => {
                let client = rig::providers::gemini::Client::new(api_key);
                let extractor = client.extractor::<T>(model).build();

                extractor.extract(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("Gemini extraction error: {}", e))
                })
            }
            LLMProvider::Cohere { api_key, model } => {
                let client = rig::providers::cohere::Client::new(api_key);
                let extractor = client.extractor::<T>(model).build();

                extractor.extract(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("Cohere extraction error: {}", e))
                })
            }
        }
    }

    /// Execute a simple completion (for summarization)
    async fn complete(&self, prompt: &str) -> Result<String> {
        match &self.provider {
            LLMProvider::OpenAI { api_key, model } => {
                let client = rig::providers::openai::Client::new(api_key);
                let agent = client.agent(model).build();

                agent.prompt(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("OpenAI completion error: {}", e))
                })
            }
            LLMProvider::Anthropic { api_key, model } => {
                let client = rig::providers::anthropic::ClientBuilder::new(api_key).build();
                let agent = client.agent(model).build();

                agent.prompt(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("Anthropic completion error: {}", e))
                })
            }
            LLMProvider::Gemini { api_key, model } => {
                let client = rig::providers::gemini::Client::new(api_key);
                let agent = client.agent(model).build();

                agent.prompt(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("Gemini completion error: {}", e))
                })
            }
            LLMProvider::Cohere { api_key, model } => {
                let client = rig::providers::cohere::Client::new(api_key);
                let agent = client.agent(model).build();

                agent.prompt(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("Cohere completion error: {}", e))
                })
            }
        }
    }
}

#[async_trait]
impl ReasoningEngine for Reasoner {
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

        let prompt = format!(
            r#"You are a document navigation assistant. Select which sections are most likely to contain information relevant to the user's query.

Query: "{}"
{}
Available sections:
{}

Select up to {} sections most likely to contain the answer. For each selection, provide:
- node_id: The exact ID string from the list
- confidence: A score from 0.0 to 1.0
- reasoning: A brief explanation

Only include sections that are likely relevant. If none seem relevant, return an empty selections array."#,
            query,
            context_part,
            self.format_candidates(candidates),
            self.config.beam_width
        );

        debug!("Deciding next step with {} candidates", candidates.len());

        let result: TraversalDecisions = self.extract(&prompt).await?;

        Ok(result.selections)
    }

    async fn verify_answer(&self, query: &str, content: &str) -> Result<VerificationResult> {
        // Truncate content if too long
        let truncated_content: String = content.chars().take(4000).collect();

        let prompt = format!(
            r#"Determine if this content answers or is relevant to the user's query.

Query: "{}"

Content:
{}

Analyze the content and determine:
- is_relevant: true if the content answers or contains information relevant to the query
- confidence: A score from 0.0 to 1.0 indicating how confident you are
- extracted_answer: If relevant, provide a brief extracted answer (or null if not relevant)"#,
            query, truncated_content
        );

        debug!("Verifying answer for query: {}", query);

        self.extract(&prompt).await
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

        let prompt = format!(
            r#"{}Summarize this {} in 1-2 sentences. Focus on:
- What topics/concepts are covered
- Key facts, figures, or conclusions
- What questions this section could answer

{}

Provide only the summary, no additional commentary."#,
            title_hint, node_type, truncated_content
        );

        debug!("Summarizing content ({} chars)", content.len());

        self.complete(&prompt).await
    }

    fn name(&self) -> &str {
        match &self.provider {
            LLMProvider::OpenAI { model, .. } => model,
            LLMProvider::Anthropic { model, .. } => model,
            LLMProvider::Gemini { model, .. } => model,
            LLMProvider::Cohere { model, .. } => model,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_builders() {
        let openai = LLMProvider::openai_mini("test-key");
        assert!(matches!(openai, LLMProvider::OpenAI { model, .. } if model == "gpt-4o-mini"));

        let claude = LLMProvider::claude_sonnet("test-key");
        assert!(matches!(claude, LLMProvider::Anthropic { .. }));

        let gemini = LLMProvider::gemini("test-key");
        assert!(matches!(gemini, LLMProvider::Gemini { model, .. } if model == "gemini-1.5-flash"));

        let cohere = LLMProvider::cohere("test-key");
        assert!(matches!(cohere, LLMProvider::Cohere { model, .. } if model == "command-r-plus"));
    }

    #[test]
    fn test_format_candidates() {
        let reasoner = Reasoner::new(LLMProvider::openai_mini("test"));

        let candidates = vec![
            NodeSummary {
                id: "node_1".to_string(),
                title: "Chapter 1".to_string(),
                summary: "About finance".to_string(),
                depth: 1,
                is_leaf: false,
            },
            NodeSummary {
                id: "node_2".to_string(),
                title: "Chapter 2".to_string(),
                summary: "About technology".to_string(),
                depth: 1,
                is_leaf: false,
            },
        ];

        let formatted = reasoner.format_candidates(&candidates);
        assert!(formatted.contains("node_1"));
        assert!(formatted.contains("Chapter 1"));
        assert!(formatted.contains("node_2"));
        assert!(formatted.contains("Chapter 2"));
    }

    #[test]
    fn test_reasoner_config() {
        let config = ReasoningConfig {
            beam_width: 5,
            min_confidence: 0.5,
            ..Default::default()
        };

        let reasoner =
            Reasoner::new(LLMProvider::openai_mini("test")).with_config(config.clone());

        assert_eq!(reasoner.config.beam_width, 5);
        assert_eq!(reasoner.config.min_confidence, 0.5);
    }

    #[test]
    fn test_reasoner_name() {
        let openai = Reasoner::new(LLMProvider::openai_mini("test"));
        assert_eq!(openai.name(), "gpt-4o-mini");

        let claude = Reasoner::new(LLMProvider::claude_sonnet("test"));
        assert!(claude.name().contains("claude"));
    }
}
