//! Multi-provider LLM implementation
//!
//! This module provides a unified interface to multiple LLM providers:
//! - OpenAI (GPT-4o, GPT-4o-mini, etc.)
//! - Anthropic (Claude 3.5 Sonnet, Claude 3 Haiku, etc.)
//! - Google Gemini
//! - Cohere
//! - GLM (Zhipu AI — GLM-4, GLM-4-Flash, etc.)
//! - Kimi (Moonshot AI — moonshot-v1-8k, moonshot-v1-128k, etc.)
//! - Ollama (local models — Llama, Qwen, Mistral, etc.)
//!
//! Uses structured output extraction via `schemars::JsonSchema`.

use async_trait::async_trait;
use rig::completion::Prompt;
use serde::Serialize;
use tracing::{debug, error, info, warn};

use super::{
    BatchSummaryResult, DocumentRanking, DocumentRankings, DocumentSummary, NodeSummary,
    ReasoningConfig, ReasoningEngine, SummarizationContext, TraversalDecision, TraversalDecisions,
    VerificationResult,
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
    /// Zhipu AI GLM models (OpenAI-compatible API)
    Glm { api_key: String, model: String },
    /// Moonshot AI Kimi models (OpenAI-compatible API)
    Kimi { api_key: String, model: String },
    /// Ollama local models (OpenAI-compatible API, no API key needed)
    Ollama { base_url: String, model: String },
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

    /// Create an Anthropic provider with Claude 4.5 Sonnet (powerful reasoning)
    pub fn claude_sonnet(api_key: impl Into<String>) -> Self {
        Self::Anthropic {
            api_key: api_key.into(),
            model: "claude-sonnet-4-5-20250929".to_string(),
        }
    }

    /// Create an Anthropic provider with Claude 4.5 Haiku (fast, cost-effective)
    pub fn claude_haiku(api_key: impl Into<String>) -> Self {
        Self::Anthropic {
            api_key: api_key.into(),
            model: "claude-haiku-4-5-20250929".to_string(),
        }
    }

    /// Create an Anthropic provider with a custom model name
    pub fn anthropic_custom(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::Anthropic {
            api_key: api_key.into(),
            model: model.into(),
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

    /// Create a GLM provider with GLM-4-Flash (fast, cost-effective)
    pub fn glm(api_key: impl Into<String>) -> Self {
        Self::Glm {
            api_key: api_key.into(),
            model: "glm-4-flash".to_string(),
        }
    }

    /// Create a GLM provider with GLM-4-Plus (powerful)
    pub fn glm_plus(api_key: impl Into<String>) -> Self {
        Self::Glm {
            api_key: api_key.into(),
            model: "glm-4-plus".to_string(),
        }
    }

    /// Create a Kimi provider with moonshot-v1-8k
    pub fn kimi(api_key: impl Into<String>) -> Self {
        Self::Kimi {
            api_key: api_key.into(),
            model: "moonshot-v1-8k".to_string(),
        }
    }

    /// Create a Kimi provider with moonshot-v1-128k (long context)
    pub fn kimi_long(api_key: impl Into<String>) -> Self {
        Self::Kimi {
            api_key: api_key.into(),
            model: "moonshot-v1-128k".to_string(),
        }
    }

    /// Create an Ollama provider with the default local endpoint
    pub fn ollama(model: impl Into<String>) -> Self {
        Self::Ollama {
            base_url: "http://localhost:11434/v1".to_string(),
            model: model.into(),
        }
    }

    /// Create an Ollama provider with a custom base URL
    pub fn ollama_from_url(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self::Ollama {
            base_url: base_url.into(),
            model: model.into(),
        }
    }

    /// The provider name (e.g. "openai", "anthropic")
    pub fn provider_name(&self) -> &str {
        match self {
            Self::OpenAI { .. } => "openai",
            Self::Anthropic { .. } => "anthropic",
            Self::Gemini { .. } => "gemini",
            Self::Cohere { .. } => "cohere",
            Self::Glm { .. } => "glm",
            Self::Kimi { .. } => "kimi",
            Self::Ollama { .. } => "ollama",
        }
    }

    /// The model identifier (e.g. "gpt-4o", "claude-sonnet-4-5-20250929")
    pub fn model(&self) -> &str {
        match self {
            Self::OpenAI { model, .. }
            | Self::Anthropic { model, .. }
            | Self::Gemini { model, .. }
            | Self::Cohere { model, .. }
            | Self::Glm { model, .. }
            | Self::Kimi { model, .. }
            | Self::Ollama { model, .. } => model,
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
    options: super::config::LlmOptions,
}

impl Reasoner {
    /// Create a new Reasoner with the specified provider
    pub fn new(provider: LLMProvider) -> Self {
        Self {
            provider,
            config: ReasoningConfig::default(),
            options: super::config::LlmOptions::default(),
        }
    }

    /// Set custom configuration
    pub fn with_config(mut self, config: ReasoningConfig) -> Self {
        self.config = config;
        self
    }

    /// Set LLM options (temperature, max_tokens, etc.)
    pub fn with_options(mut self, options: super::config::LlmOptions) -> Self {
        self.options = options;
        self
    }

    /// Build `additional_params` JSON from options (top_p, penalties, disable_thinking).
    fn additional_params_json(&self) -> Option<serde_json::Value> {
        let mut map = serde_json::Map::new();
        if let Some(top_p) = self.options.top_p {
            map.insert("top_p".into(), serde_json::json!(top_p));
        }
        if let Some(fp) = self.options.frequency_penalty {
            map.insert("frequency_penalty".into(), serde_json::json!(fp));
        }
        if let Some(pp) = self.options.presence_penalty {
            map.insert("presence_penalty".into(), serde_json::json!(pp));
        }
        if self.options.disable_thinking {
            map.insert("disable_thinking".into(), serde_json::json!(true));
        }
        if map.is_empty() { None } else { Some(serde_json::Value::Object(map)) }
    }

    /// Get the effective preamble: options override, or fallback to the provided default.
    fn effective_preamble<'a>(&'a self, default: &'a str) -> &'a str {
        self.options.system_prompt.as_deref().unwrap_or(default)
    }

    /// Get effective max_tokens (options override or given default).
    fn effective_max_tokens(&self, default: u64) -> u64 {
        self.options.max_tokens.unwrap_or(default)
    }

    /// Get effective temperature as f64.
    fn effective_temperature(&self) -> Option<f64> {
        self.options.temperature.map(|t| t as f64)
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
        info!(
            provider = self.provider.provider_name(),
            model = self.provider.model(),
            "LLM extraction request"
        );
        match &self.provider {
            LLMProvider::OpenAI { api_key, model } => {
                let client = rig::providers::openai::Client::new(api_key);
                let mut builder = client.extractor::<T>(model);
                if let Some(preamble) = &self.options.system_prompt {
                    builder = builder.preamble(preamble);
                }
                let extractor = builder.build();

                extractor.extract(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("OpenAI extraction error: {}", e))
                })
            }
            LLMProvider::Anthropic { api_key, model } => {
                let client = rig::providers::anthropic::ClientBuilder::new(api_key).build();
                let default_preamble = "You are a JSON extraction assistant. Always respond with valid JSON only, no other text.";
                let mut builder = client
                    .agent(model)
                    .max_tokens(self.effective_max_tokens(4096))
                    .preamble(self.effective_preamble(default_preamble));
                builder = self.apply_agent_options(builder);
                let agent = builder.build();

                let schema = schemars::schema_for!(T);
                let schema_json = serde_json::to_string_pretty(&schema)
                    .map_err(|e| ReasonError::Reasoning(format!("Schema error: {}", e)))?;

                let extraction_prompt = format!(
                    "Extract the following information from the text and return ONLY valid JSON matching this schema:\n\nSchema:\n{}\n\nText to extract from:\n{}",
                    schema_json, prompt
                );

                let response = agent.prompt(&extraction_prompt).await.map_err(|e| {
                    error!("Anthropic API call failed: {}", e);
                    ReasonError::Reasoning(format!("Anthropic completion error: {}", e))
                })?;

                debug!(
                    response_len = response.len(),
                    "Anthropic raw response (first 500 chars): {}",
                    &response[..response.len().min(500)]
                );

                let json_str = response
                    .trim()
                    .strip_prefix("```json")
                    .or_else(|| response.trim().strip_prefix("```"))
                    .unwrap_or(&response)
                    .trim()
                    .strip_suffix("```")
                    .unwrap_or(&response)
                    .trim();

                serde_json::from_str(json_str).map_err(|e| {
                    warn!(
                        "Anthropic JSON parse failed: {}. Raw response: {}",
                        e,
                        &json_str[..json_str.len().min(500)]
                    );
                    ReasonError::Reasoning(format!("Failed to parse Anthropic JSON response: {}. Response was: {}", e, json_str))
                })
            }
            LLMProvider::Gemini { api_key, model } => {
                let client = rig::providers::gemini::Client::new(api_key);
                let mut builder = client.extractor::<T>(model);
                if let Some(preamble) = &self.options.system_prompt {
                    builder = builder.preamble(preamble);
                }
                let extractor = builder.build();

                extractor.extract(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("Gemini extraction error: {}", e))
                })
            }
            LLMProvider::Cohere { api_key, model } => {
                let client = rig::providers::cohere::Client::new(api_key);
                let mut builder = client.extractor::<T>(model);
                if let Some(preamble) = &self.options.system_prompt {
                    builder = builder.preamble(preamble);
                }
                let extractor = builder.build();

                extractor.extract(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("Cohere extraction error: {}", e))
                })
            }
            LLMProvider::Glm { api_key, model } => {
                let client = rig::providers::openai::Client::from_url(api_key, "https://open.bigmodel.cn/api/paas/v4");
                let mut builder = client.extractor::<T>(model);
                if let Some(preamble) = &self.options.system_prompt {
                    builder = builder.preamble(preamble);
                }
                let extractor = builder.build();

                extractor.extract(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("GLM extraction error: {}", e))
                })
            }
            LLMProvider::Kimi { api_key, model } => {
                let client = rig::providers::openai::Client::from_url(api_key, "https://api.moonshot.ai/v1");
                let default_preamble = "You are a structured data extraction assistant. Extract the requested information accurately.";
                let mut builder = client.extractor::<T>(model)
                    .preamble(self.effective_preamble(default_preamble));
                if let Some(preamble) = &self.options.system_prompt {
                    builder = builder.preamble(preamble);
                }
                let extractor = builder.build();

                extractor.extract(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("Kimi extraction error: {}", e))
                })
            }
            LLMProvider::Ollama { base_url, model } => {
                let client = rig::providers::openai::Client::from_url("ollama", base_url);
                let mut builder = client.extractor::<T>(model);
                if let Some(preamble) = &self.options.system_prompt {
                    builder = builder.preamble(preamble);
                }
                let extractor = builder.build();

                extractor.extract(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("Ollama extraction error: {}", e))
                })
            }
        }
    }

    /// Apply LlmOptions to a rig AgentBuilder.
    fn apply_agent_options<M: rig::completion::CompletionModel>(
        &self,
        mut builder: rig::agent::AgentBuilder<M>,
    ) -> rig::agent::AgentBuilder<M> {
        if let Some(temp) = self.effective_temperature() {
            builder = builder.temperature(temp);
        }
        if let Some(max) = self.options.max_tokens {
            builder = builder.max_tokens(max);
        }
        if let Some(preamble) = &self.options.system_prompt {
            builder = builder.preamble(preamble);
        }
        if let Some(params) = self.additional_params_json() {
            builder = builder.additional_params(params);
        }
        builder
    }

    /// Execute a simple completion (for summarization)
    async fn complete(&self, prompt: &str) -> Result<String> {
        info!(
            provider = self.provider.provider_name(),
            model = self.provider.model(),
            "LLM completion request"
        );
        match &self.provider {
            LLMProvider::OpenAI { api_key, model } => {
                let client = rig::providers::openai::Client::new(api_key);
                let agent = self.apply_agent_options(client.agent(model)).build();

                agent.prompt(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("OpenAI completion error: {}", e))
                })
            }
            LLMProvider::Anthropic { api_key, model } => {
                let client = rig::providers::anthropic::ClientBuilder::new(api_key).build();
                let mut builder = client
                    .agent(model)
                    .max_tokens(self.effective_max_tokens(4096));
                builder = self.apply_agent_options(builder);

                let agent = builder.build();
                agent.prompt(prompt).await.map_err(|e| {
                    error!("Anthropic completion call failed: {}", e);
                    ReasonError::Reasoning(format!("Anthropic completion error: {}", e))
                })
            }
            LLMProvider::Gemini { api_key, model } => {
                let client = rig::providers::gemini::Client::new(api_key);
                let agent = self.apply_agent_options(client.agent(model)).build();

                agent.prompt(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("Gemini completion error: {}", e))
                })
            }
            LLMProvider::Cohere { api_key, model } => {
                let client = rig::providers::cohere::Client::new(api_key);
                let agent = self.apply_agent_options(client.agent(model)).build();

                agent.prompt(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("Cohere completion error: {}", e))
                })
            }
            LLMProvider::Glm { api_key, model } => {
                let client = rig::providers::openai::Client::from_url(api_key, "https://open.bigmodel.cn/api/paas/v4");
                let agent = self.apply_agent_options(client.agent(model)).build();

                agent.prompt(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("GLM completion error: {}", e))
                })
            }
            LLMProvider::Kimi { api_key, model } => {
                let client = rig::providers::openai::Client::from_url(api_key, "https://api.moonshot.ai/v1");
                let mut builder = client.agent(model);
                if self.options.system_prompt.is_none() {
                    builder = builder.preamble("You are a helpful assistant.");
                }
                let agent = self.apply_agent_options(builder).build();

                agent.prompt(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("Kimi completion error: {}", e))
                })
            }
            LLMProvider::Ollama { base_url, model } => {
                let client = rig::providers::openai::Client::from_url("ollama", base_url);
                let agent = self.apply_agent_options(client.agent(model)).build();

                agent.prompt(prompt).await.map_err(|e| {
                    ReasonError::Reasoning(format!("Ollama completion error: {}", e))
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

    async fn summarize_batch(
        &self,
        items: &[(String, String, SummarizationContext)],
    ) -> Result<Vec<(String, String)>> {
        if items.is_empty() {
            return Ok(Vec::new());
        }

        // For a single item, fall back to the regular summarize path
        if items.len() == 1 {
            let (id, content, ctx) = &items[0];
            let summary = self.summarize(content, ctx).await?;
            return Ok(vec![(id.clone(), summary)]);
        }

        info!(
            provider = self.provider.provider_name(),
            model = self.provider.model(),
            batch_size = items.len(),
            "LLM batch summarization request"
        );

        let nodes_formatted: String = items
            .iter()
            .map(|(node_id, content, ctx)| {
                let truncated: String = content.chars().take(2000).collect();
                let node_type = if ctx.is_leaf { "content" } else { "section summaries" };
                let title = ctx.title.as_deref().unwrap_or("Untitled");
                format!(
                    "[node_id: \"{}\"] Title: \"{}\" ({})\n{}",
                    node_id, title, node_type, truncated
                )
            })
            .collect::<Vec<_>>()
            .join("\n---\n");

        let prompt = format!(
            r#"Summarize each of the following sections in 1-2 sentences. For each section, focus on:
- What topics/concepts are covered
- Key facts, figures, or conclusions
- What questions this section could answer

Sections to summarize:
{nodes_formatted}

Return a JSON object with a "summaries" array. Each element must have:
- "node_id": the exact node_id from the section header
- "summary": a 1-2 sentence summary

Return summaries for ALL {count} sections."#,
            count = items.len()
        );

        debug!("Batch summarizing {} nodes", items.len());

        let result: BatchSummaryResult = self.extract(&prompt).await?;

        Ok(result
            .summaries
            .into_iter()
            .map(|item| (item.node_id, item.summary))
            .collect())
    }

    async fn rank_documents(
        &self,
        query: &str,
        documents: &[DocumentSummary],
        top_k: usize,
    ) -> Result<Vec<DocumentRanking>> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        // Format documents for the prompt
        let docs_formatted: String = documents
            .iter()
            .enumerate()
            .map(|(i, doc)| {
                format!(
                    "{}. [ID: {}] \"{}\" - {}\n   Tags: {:?}",
                    i + 1, doc.id, doc.title, doc.summary, doc.tags
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"You are a document ranking assistant. Rank the following documents by their relevance to the user's query.

Query: "{}"

Documents to rank:
{}

Return the top {} most relevant documents. For each document, provide:
- document_id: The ID of the document (from the [ID: ...] field)
- relevance: A score from 0.0 to 1.0 indicating relevance
- reasoning: A brief explanation of why this document is relevant

Return as JSON array in the format:
{{"rankings": [{{"document_id": "...", "relevance": 0.9, "reasoning": "..."}}]}}

Only include documents that are actually relevant to the query (relevance > 0.3).
Order by relevance, highest first."#,
            query, docs_formatted, top_k
        );

        debug!("Ranking {} documents for query: {}", documents.len(), query);

        let result: DocumentRankings = self.extract(&prompt).await?;

        // Sort by relevance (highest first) and take top_k
        let mut rankings = result.rankings;
        rankings.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap_or(std::cmp::Ordering::Equal));
        rankings.truncate(top_k);

        Ok(rankings)
    }

    fn name(&self) -> &str {
        self.provider.model()
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

        let glm = LLMProvider::glm("test-key");
        assert!(matches!(glm, LLMProvider::Glm { ref model, .. } if model == "glm-4-flash"));
        assert_eq!(glm.provider_name(), "glm");

        let glm_plus = LLMProvider::glm_plus("test-key");
        assert!(matches!(glm_plus, LLMProvider::Glm { model, .. } if model == "glm-4-plus"));

        let kimi = LLMProvider::kimi("test-key");
        assert!(matches!(kimi, LLMProvider::Kimi { ref model, .. } if model == "moonshot-v1-8k"));
        assert_eq!(kimi.provider_name(), "kimi");

        let kimi_long = LLMProvider::kimi_long("test-key");
        assert!(matches!(kimi_long, LLMProvider::Kimi { model, .. } if model == "moonshot-v1-128k"));

        let ollama = LLMProvider::ollama("llama3.3");
        assert!(matches!(ollama, LLMProvider::Ollama { ref model, .. } if model == "llama3.3"));
        assert_eq!(ollama.provider_name(), "ollama");

        let ollama_custom = LLMProvider::ollama_from_url("http://remote:11434/v1", "qwen2.5");
        assert!(matches!(ollama_custom, LLMProvider::Ollama { base_url, model } if base_url == "http://remote:11434/v1" && model == "qwen2.5"));
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
