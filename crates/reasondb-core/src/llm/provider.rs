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
//! - Google Vertex AI (Gemini via OpenAI-compatible endpoint)
//! - AWS Bedrock (Claude, etc. via Converse API)
//!
//! Uses structured output extraction via `schemars::JsonSchema`.

use async_trait::async_trait;
use rig::completion::Prompt;
use serde::Serialize;
use tracing::{debug, error, info, warn};

use super::{
    BatchSummaryResult, DecomposedQueryResult, DocumentRanking, DocumentRankings, DocumentSummary,
    DomainVocabResult, NodeSummary, ReasoningConfig, ReasoningEngine, SummarizationContext,
    TraversalDecision, TraversalDecisions, VerificationResult, VerificationResultRaw,
};
use crate::error::{ReasonError, Result};
use crate::query_decomposer::{DomainContext, SubQuery};

/// Extract valid JSON from an LLM response that may contain markdown fences or prose.
/// Tries (in order): raw parse, fence-stripped parse, brace/bracket extraction.
fn extract_json_from_response(response: &str) -> &str {
    let trimmed = response.trim();

    // Fast path: response is already valid-looking JSON
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return trimmed;
    }

    // Strip markdown code fences: ```json ... ``` (possibly with trailing prose)
    if let Some(after_fence) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
    {
        if let Some(end) = after_fence.find("```") {
            return after_fence[..end].trim();
        }
    }

    // Last resort: find the first { or [ and its matching close
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return &trimmed[start..=end];
            }
        }
    }
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            if end > start {
                return &trimmed[start..=end];
            }
        }
    }

    trimmed
}

/// Extract plain text from a Bedrock Converse response output (assistant message content).
#[cfg(feature = "bedrock")]
fn extract_bedrock_message_text(
    output: Option<&aws_sdk_bedrockruntime::types::ConverseOutput>,
) -> Result<String> {
    use aws_sdk_bedrockruntime::types::ContentBlock;
    let output =
        output.ok_or_else(|| ReasonError::Reasoning("Bedrock returned no output".into()))?;
    let msg = output
        .as_message()
        .map_err(|_| ReasonError::Reasoning("Bedrock output was not a message".into()))?;
    let text: String = msg
        .content()
        .iter()
        .filter_map(|block| {
            if let ContentBlock::Text(s) = block {
                Some(s.clone())
            } else {
                None
            }
        })
        .collect();
    if text.is_empty() {
        return Err(ReasonError::Reasoning(
            "Bedrock response contained no text content".into(),
        ));
    }
    Ok(text)
}

/// Attempt to repair malformed JSON where the LLM flattened multiple array
/// elements into a single object with duplicate keys.
///
/// Pattern detected:
/// `{"selections": [{"node_id":"a","confidence":0.9,"reasoning":"x","node_id":"b",...}]}`
///
/// Repaired to:
/// `{"selections": [{"node_id":"a","confidence":0.9,"reasoning":"x"},{"node_id":"b",...}]}`
fn repair_duplicate_key_json(json_str: &str, duplicate_field: &str) -> Option<String> {
    use regex::Regex;

    let pattern = format!(r#",\s*"{}"\s*:"#, regex::escape(duplicate_field));
    let split_re = Regex::new(&pattern).ok()?;

    let first_field_pattern = format!(r#""{}"\s*:"#, regex::escape(duplicate_field));
    let first_re = Regex::new(&first_field_pattern).ok()?;

    let all_matches: Vec<_> = first_re.find_iter(json_str).collect();
    if all_matches.len() < 2 {
        return None;
    }

    let repaired = split_re.replacen(
        json_str,
        all_matches.len() - 1,
        &format!(r#"}},{{"{}":"#, duplicate_field),
    );

    if serde_json::from_str::<serde_json::Value>(&repaired).is_ok() {
        debug!(
            "Repaired duplicate-key JSON for field '{}'",
            duplicate_field
        );
        Some(repaired.into_owned())
    } else {
        None
    }
}

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
    /// Google Vertex AI (Gemini via OpenAI-compatible endpoint)
    Vertex {
        base_url: String,
        api_key: String,
        model: String,
    },
    /// AWS Bedrock (Converse API; uses default credential chain)
    Bedrock { region: String, model: String },
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
            model: "gemini-2.5-flash".to_string(),
        }
    }

    /// Create a Gemini provider with Gemini 1.5 Pro
    pub fn gemini_pro(api_key: impl Into<String>) -> Self {
        Self::Gemini {
            api_key: api_key.into(),
            model: "gemini-2.5-pro".to_string(),
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

    /// Create a Vertex AI provider (base_url = Vertex openapi endpoint, api_key = Google Cloud token)
    pub fn vertex(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self::Vertex {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
        }
    }

    /// Create an AWS Bedrock provider (uses default credential chain)
    pub fn bedrock(region: impl Into<String>, model: impl Into<String>) -> Self {
        Self::Bedrock {
            region: region.into(),
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
            Self::Vertex { .. } => "vertex",
            Self::Bedrock { .. } => "bedrock",
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
            | Self::Ollama { model, .. }
            | Self::Vertex { model, .. }
            | Self::Bedrock { model, .. } => model,
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

    /// Build `additional_params` JSON from options (top_p, penalties).
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
        if map.is_empty() {
            None
        } else {
            Some(serde_json::Value::Object(map))
        }
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

    /// Like `extract` but caps output to `max_tokens` — used for compact
    /// scoring tasks where verbose reasoning preamble must be suppressed.
    async fn extract_compact<T>(&self, prompt: &str, max_tokens: u64) -> Result<T>
    where
        T: serde::de::DeserializeOwned + schemars::JsonSchema + Serialize + Send + Sync + 'static,
    {
        // Temporarily override max_tokens via a cloned reasoner options.
        let mut opts_override = self.options.clone();
        opts_override.max_tokens = Some(max_tokens);
        let overridden = Reasoner {
            provider: self.provider.clone(),
            config: self.config.clone(),
            options: opts_override,
        };
        overridden.extract::<T>(prompt).await
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

                extractor
                    .extract(prompt)
                    .await
                    .map_err(|e| ReasonError::Reasoning(format!("OpenAI extraction error: {}", e)))
            }
            LLMProvider::Anthropic { api_key, model } => {
                // rig-core 0.6.1's ExtractorBuilder doesn't expose max_tokens()
                // and its calculate_max_tokens doesn't know Claude 4.x models,
                // so we use agent + manual JSON parsing with a repair fallback.
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
                    "Extract the following information and return ONLY valid JSON matching this schema.\n\
                    IMPORTANT: When the schema has an array of objects, return EACH item as a SEPARATE object in the array.\n\n\
                    Schema:\n{}\n\nText:\n{}",
                    schema_json, prompt
                );

                let response = agent.prompt(&extraction_prompt).await.map_err(|e| {
                    error!("Anthropic API call failed: {}", e);
                    ReasonError::Reasoning(format!("Anthropic completion error: {}", e))
                })?;

                debug!(
                    response_len = response.len(),
                    "Anthropic raw response (first 500 chars): {}",
                    response.chars().take(500).collect::<String>()
                );

                let json_str = extract_json_from_response(&response);

                match serde_json::from_str(json_str) {
                    Ok(parsed) => Ok(parsed),
                    Err(e) => {
                        let err_msg = e.to_string();
                        if err_msg.contains("duplicate field") {
                            let field = err_msg
                                .strip_prefix("duplicate field `")
                                .and_then(|s| s.split('`').next())
                                .unwrap_or("node_id");
                            if let Some(repaired) = repair_duplicate_key_json(json_str, field) {
                                if let Ok(parsed) = serde_json::from_str(&repaired) {
                                    warn!(
                                        "Anthropic returned duplicate-key JSON (field '{}'), repaired successfully",
                                        field
                                    );
                                    return Ok(parsed);
                                }
                            }
                        }
                        warn!(
                            "Anthropic JSON parse failed: {}. Raw response: {}",
                            e,
                            json_str.chars().take(500).collect::<String>()
                        );
                        Err(ReasonError::Reasoning(format!(
                            "Failed to parse Anthropic JSON response: {}. Response was: {}",
                            e, json_str
                        )))
                    }
                }
            }
            LLMProvider::Gemini { api_key, model } => {
                let client = rig::providers::gemini::Client::new(api_key);
                let mut builder = client.extractor::<T>(model);
                if let Some(preamble) = &self.options.system_prompt {
                    builder = builder.preamble(preamble);
                }
                let extractor = builder.build();

                extractor
                    .extract(prompt)
                    .await
                    .map_err(|e| ReasonError::Reasoning(format!("Gemini extraction error: {}", e)))
            }
            LLMProvider::Cohere { api_key, model } => {
                let client = rig::providers::cohere::Client::new(api_key);
                let mut builder = client.extractor::<T>(model);
                if let Some(preamble) = &self.options.system_prompt {
                    builder = builder.preamble(preamble);
                }
                let extractor = builder.build();

                extractor
                    .extract(prompt)
                    .await
                    .map_err(|e| ReasonError::Reasoning(format!("Cohere extraction error: {}", e)))
            }
            LLMProvider::Glm { api_key, model } => {
                let client = rig::providers::openai::Client::from_url(
                    api_key,
                    "https://open.bigmodel.cn/api/paas/v4",
                );
                let mut builder = client.extractor::<T>(model);
                if let Some(preamble) = &self.options.system_prompt {
                    builder = builder.preamble(preamble);
                }
                let extractor = builder.build();

                extractor
                    .extract(prompt)
                    .await
                    .map_err(|e| ReasonError::Reasoning(format!("GLM extraction error: {}", e)))
            }
            LLMProvider::Kimi { api_key, model } => {
                let client =
                    rig::providers::openai::Client::from_url(api_key, "https://api.moonshot.ai/v1");
                let default_preamble = "You are a structured data extraction assistant. Extract the requested information accurately.";
                let mut builder = client
                    .extractor::<T>(model)
                    .preamble(self.effective_preamble(default_preamble));
                if let Some(preamble) = &self.options.system_prompt {
                    builder = builder.preamble(preamble);
                }
                let extractor = builder.build();

                extractor
                    .extract(prompt)
                    .await
                    .map_err(|e| ReasonError::Reasoning(format!("Kimi extraction error: {}", e)))
            }
            LLMProvider::Ollama { base_url, model } => {
                let client = rig::providers::openai::Client::from_url("ollama", base_url);
                let mut builder = client.extractor::<T>(model);
                if let Some(preamble) = &self.options.system_prompt {
                    builder = builder.preamble(preamble);
                }
                let extractor = builder.build();

                extractor
                    .extract(prompt)
                    .await
                    .map_err(|e| ReasonError::Reasoning(format!("Ollama extraction error: {}", e)))
            }
            LLMProvider::Vertex {
                base_url,
                api_key,
                model,
            } => {
                let client = rig::providers::openai::Client::from_url(api_key.as_str(), base_url);
                let mut builder = client.extractor::<T>(model);
                if let Some(preamble) = &self.options.system_prompt {
                    builder = builder.preamble(preamble);
                }
                let extractor = builder.build();

                extractor
                    .extract(prompt)
                    .await
                    .map_err(|e| ReasonError::Reasoning(format!("Vertex extraction error: {}", e)))
            }
            #[cfg(feature = "bedrock")]
            LLMProvider::Bedrock { region, model } => {
                let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .region(aws_sdk_bedrockruntime::config::Region::new(region.clone()))
                    .load()
                    .await;
                let client = aws_sdk_bedrockruntime::Client::new(&config);
                let default_preamble = "You are a JSON extraction assistant. Always respond with valid JSON only, no other text.";
                let system = vec![aws_sdk_bedrockruntime::types::SystemContentBlock::Text(
                    self.effective_preamble(default_preamble).to_string(),
                )];
                let schema = schemars::schema_for!(T);
                let schema_json = serde_json::to_string_pretty(&schema)
                    .map_err(|e| ReasonError::Reasoning(format!("Schema error: {}", e)))?;
                let extraction_prompt = format!(
                    "Extract the following information and return ONLY valid JSON matching this schema.\n\
                    IMPORTANT: When the schema has an array of objects, return EACH item as a SEPARATE object in the array.\n\n\
                    Schema:\n{}\n\nText:\n{}",
                    schema_json, prompt
                );
                let user_message = aws_sdk_bedrockruntime::types::Message::builder()
                    .role(aws_sdk_bedrockruntime::types::ConversationRole::User)
                    .content(aws_sdk_bedrockruntime::types::ContentBlock::Text(
                        extraction_prompt,
                    ))
                    .build();
                let inference_config =
                    aws_sdk_bedrockruntime::types::InferenceConfiguration::builder()
                        .max_tokens(self.effective_max_tokens(4096) as i32)
                        .build();
                let response = client
                    .converse()
                    .model_id(model)
                    .set_messages(Some(vec![user_message]))
                    .set_system(Some(system))
                    .set_inference_config(Some(inference_config))
                    .send()
                    .await
                    .map_err(|e| {
                        error!("Bedrock Converse failed: {}", e);
                        ReasonError::Reasoning(format!("Bedrock extraction error: {}", e))
                    })?;
                let response_text = extract_bedrock_message_text(response.output())?;
                let json_str = extract_json_from_response(&response_text);
                match serde_json::from_str(json_str) {
                    Ok(parsed) => Ok(parsed),
                    Err(e) => {
                        let err_msg = e.to_string();
                        if err_msg.contains("duplicate field") {
                            let field = err_msg
                                .strip_prefix("duplicate field `")
                                .and_then(|s| s.split('`').next())
                                .unwrap_or("node_id");
                            if let Some(repaired) = repair_duplicate_key_json(json_str, field) {
                                if let Ok(parsed) = serde_json::from_str(&repaired) {
                                    warn!(
                                        "Bedrock returned duplicate-key JSON (field '{}'), repaired successfully",
                                        field
                                    );
                                    return Ok(parsed);
                                }
                            }
                        }
                        Err(ReasonError::Reasoning(format!(
                            "Failed to parse Bedrock JSON response: {}. Response was: {}",
                            e, json_str
                        )))
                    }
                }
            }
            #[cfg(not(feature = "bedrock"))]
            LLMProvider::Bedrock { .. } => Err(ReasonError::Reasoning(
                "AWS Bedrock support is not compiled in. Rebuild with --features bedrock.".into(),
            )),
        }
    }

    /// Fast extraction for ranking: lower max_tokens, terse preamble.
    /// Keeps all other provider options but overrides token budget and system prompt.
    async fn extract_lean<T>(&self, prompt: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned + schemars::JsonSchema + Serialize + Send + Sync + 'static,
    {
        let mut lean = self.clone();
        lean.options.max_tokens = Some(1024);
        lean.options.system_prompt = Some("Return ONLY valid JSON. No explanation.".into());
        lean.extract(prompt).await
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

                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e| ReasonError::Reasoning(format!("OpenAI completion error: {}", e)))
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

                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e| ReasonError::Reasoning(format!("Gemini completion error: {}", e)))
            }
            LLMProvider::Cohere { api_key, model } => {
                let client = rig::providers::cohere::Client::new(api_key);
                let agent = self.apply_agent_options(client.agent(model)).build();

                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e| ReasonError::Reasoning(format!("Cohere completion error: {}", e)))
            }
            LLMProvider::Glm { api_key, model } => {
                let client = rig::providers::openai::Client::from_url(
                    api_key,
                    "https://open.bigmodel.cn/api/paas/v4",
                );
                let agent = self.apply_agent_options(client.agent(model)).build();

                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e| ReasonError::Reasoning(format!("GLM completion error: {}", e)))
            }
            LLMProvider::Kimi { api_key, model } => {
                let client =
                    rig::providers::openai::Client::from_url(api_key, "https://api.moonshot.ai/v1");
                let mut builder = client.agent(model);
                if self.options.system_prompt.is_none() {
                    builder = builder.preamble("You are a helpful assistant.");
                }
                let agent = self.apply_agent_options(builder).build();

                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e| ReasonError::Reasoning(format!("Kimi completion error: {}", e)))
            }
            LLMProvider::Ollama { base_url, model } => {
                let client = rig::providers::openai::Client::from_url("ollama", base_url);
                let agent = self.apply_agent_options(client.agent(model)).build();

                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e| ReasonError::Reasoning(format!("Ollama completion error: {}", e)))
            }
            LLMProvider::Vertex {
                base_url,
                api_key,
                model,
            } => {
                let client = rig::providers::openai::Client::from_url(api_key.as_str(), base_url);
                let agent = self.apply_agent_options(client.agent(model)).build();

                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e| ReasonError::Reasoning(format!("Vertex completion error: {}", e)))
            }
            #[cfg(feature = "bedrock")]
            LLMProvider::Bedrock { region, model } => {
                let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .region(aws_sdk_bedrockruntime::config::Region::new(region.clone()))
                    .load()
                    .await;
                let client = aws_sdk_bedrockruntime::Client::new(&config);
                let user_message = aws_sdk_bedrockruntime::types::Message::builder()
                    .role(aws_sdk_bedrockruntime::types::ConversationRole::User)
                    .content(aws_sdk_bedrockruntime::types::ContentBlock::Text(
                        prompt.to_string(),
                    ))
                    .build();
                let inference_config =
                    aws_sdk_bedrockruntime::types::InferenceConfiguration::builder()
                        .max_tokens(self.effective_max_tokens(4096) as i32)
                        .build();
                let response = client
                    .converse()
                    .model_id(model)
                    .set_messages(Some(vec![user_message]))
                    .set_inference_config(Some(inference_config))
                    .send()
                    .await
                    .map_err(|e| {
                        error!("Bedrock Converse failed: {}", e);
                        ReasonError::Reasoning(format!("Bedrock completion error: {}", e))
                    })?;
                extract_bedrock_message_text(response.output())
            }
            #[cfg(not(feature = "bedrock"))]
            LLMProvider::Bedrock { .. } => Err(ReasonError::Reasoning(
                "AWS Bedrock support is not compiled in. Rebuild with --features bedrock.".into(),
            )),
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
        max_selections: usize,
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
            r#"You are a document navigation assistant. Select sections that directly address the user's query.

Query: "{}"
{}
Available sections:
{}

Select up to {} sections most likely to contain a direct answer.
- A section that merely mentions a query keyword in a footnote or unrelated context is NOT worth exploring.
- For queries comparing multiple entities (e.g. "Compare Apple, Tesla, and Microsoft"): sections covering the queried metrics for ANY ONE of those entities are highly worth exploring — include them.
- Only skip a section if its summary clearly shows it is about something entirely different from the query topic.

Return JSON with this EXACT structure (each selection is a SEPARATE object in the array):
{{"selections": [{{"node_id": "exact_id_from_list", "confidence": 0.9, "reasoning": "revenue data FY2023"}}, {{"node_id": "another_id", "confidence": 0.7, "reasoning": "disability benefit rules"}}]}}

"reasoning" MUST be 3-6 words only — e.g. "revenue figures FY2023" or "disability benefit waiting period".

If none seem relevant, return: {{"selections": []}}"#,
            query,
            context_part,
            self.format_candidates(candidates),
            max_selections
        );

        debug!("Deciding next step with {} candidates", candidates.len());

        let result: TraversalDecisions = self.extract(&prompt).await?;

        Ok(result.selections)
    }

    async fn verify_answer(&self, query: &str, content: &str) -> Result<VerificationResult> {
        // Truncate content if too long
        let truncated_content: String = content.chars().take(4000).collect();

        let prompt = format!(
            r#"Does this content directly answer or provide substantive information needed to answer the user's query?

Query: "{}"

Content:
{}

Rules:
- is_relevant: true if the content directly answers the query OR provides key information needed to answer any part of it
- For queries comparing multiple entities (e.g. "Compare Apple, Tesla, and Microsoft revenue"): content about ONE of those entities is highly relevant if it covers the queried metric for that entity — mark is_relevant: true
- A section that merely mentions a query keyword in passing (e.g. a Docker command in a monitoring setup guide when the query is about Docker support) is NOT relevant — set is_relevant: false
- relevance_score: rate on an INTEGER scale from 1 to 10. Each level is distinct — choose carefully:
   10 = comprehensive, self-contained answer covering all aspects of the query
    9 = directly and completely answers the query with minor gaps
    8 = strong answer with clear, actionable information, or fully covers one entity in a multi-entity comparison
    7 = good answer but missing some context, or covers most of one entity's data in a comparison
    6 = partially answers — provides useful info but leaves key questions open
    5 = related content that gives helpful context without answering directly
    4 = mentions the topic but doesn't provide a useful answer
    3 = only loosely related, mostly about something else
    2 = barely related — superficial keyword overlap only
    1 = not relevant at all"#,
            query, truncated_content
        );

        debug!("Verifying relevance for query: {}", query);

        let raw: VerificationResultRaw = self.extract(&prompt).await?;

        Ok(raw.into())
    }

    async fn batch_verify_answers(
        &self,
        query: &str,
        candidates: &[crate::llm::BatchVerifyInput],
    ) -> Result<Vec<VerificationResult>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        // Single-item — reuse the well-tuned single verify_answer prompt
        // to avoid spending tokens on batch scaffolding.
        if candidates.len() == 1 {
            return Ok(vec![
                self.verify_answer(query, &candidates[0].content).await?,
            ]);
        }

        // Build a compact section list; cap each entry at 120 chars to keep
        // the total prompt small — the content is just a summary hint and
        // doesn't need to be complete for relevance scoring.
        let sections: String = candidates
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let snippet: String = c.content.chars().take(180).collect();
                format!("[{}] {}", i, snippet)
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Sparse-output format: only return sections with score >= 5 (moderate
        // relevance). Score 5 means "contains relevant info", 7+ means "directly
        // answers". Using 5 prevents false-negative misses on clause-embedded
        // topics (e.g. hazardous activity exclusions, cancellation provisions).
        let prompt = format!(
            r#"Score sections for relevance to the query. Return JSON only.

Query: "{}"

Sections:
{}

Return ONLY sections with score >= 5:
{{"relevant": [{{"index": 0, "score": 8}}, {{"index": 3, "score": 5}}]}}

Rules:
- score: INTEGER 1-10 (10 = perfectly on-topic, 1 = completely unrelated)
- Only list sections where score >= 5
- Sections not listed are treated as irrelevant (score < 5)
- Return empty array if nothing scores >= 5: {{"relevant": []}}"#,
            query, sections,
        );

        debug!(
            "Batch-verifying {} candidates (sparse output) for query: {}",
            candidates.len(),
            query
        );

        // Use compact mode (max 768 tokens) to prevent verbose reasoning text
        // before the JSON — with threshold=5 more entries appear, ~30 tokens each.
        let batch: crate::llm::BatchVerifyResponse = self.extract_compact(&prompt, 768).await?;

        // Sparse response: unlisted = not relevant (confidence = 0).
        let mut out = vec![
            VerificationResult {
                is_relevant: false,
                confidence: 0.0,
            };
            candidates.len()
        ];
        for score in batch.relevant {
            if score.index < out.len() {
                let raw = VerificationResultRaw {
                    is_relevant: score.score >= 5,
                    relevance_score: score.score,
                };
                out[score.index] = raw.into();
            }
        }
        Ok(out)
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
                let node_type = if ctx.is_leaf {
                    "content"
                } else {
                    "section summaries"
                };
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

    async fn summarize_batch_with_refs(
        &self,
        items: &[(String, String, SummarizationContext)],
    ) -> Result<Vec<(String, String, Vec<String>)>> {
        if items.is_empty() {
            return Ok(Vec::new());
        }

        if items.len() == 1 {
            let (id, content, ctx) = &items[0];
            let summary = self.summarize(content, ctx).await?;
            return Ok(vec![(id.clone(), summary, vec![])]);
        }

        info!(
            provider = self.provider.provider_name(),
            model = self.provider.model(),
            batch_size = items.len(),
            "LLM batch summarize+cross-ref request"
        );

        let nodes_formatted: String = items
            .iter()
            .map(|(node_id, content, ctx)| {
                let truncated: String = content.chars().take(2000).collect();
                let node_type = if ctx.is_leaf {
                    "content"
                } else {
                    "section summaries"
                };
                let title = ctx.title.as_deref().unwrap_or("Untitled");
                format!(
                    "[node_id: \"{}\"] Title: \"{}\" ({})\n{}",
                    node_id, title, node_type, truncated
                )
            })
            .collect::<Vec<_>>()
            .join("\n---\n");

        let prompt = format!(
            r#"Summarize each section and identify any explicit cross-references to other sections.

Sections:
{nodes_formatted}

Return a JSON object with a "summaries" array. Each element must have:
- "node_id": the exact node_id from the section header
- "summary": a 1-2 sentence summary covering topics, key facts, and what questions it answers
- "references": list of verbatim cross-reference strings as they appear in the text
  (e.g. "Section 10.2", "the Exclusions section", "Chapter 5", "Appendix A").
  Copy the text EXACTLY as it appears. Use [] if no explicit references found.

Only include EXPLICIT textual references (e.g. "see Section X", "refer to the Definitions clause") —
NOT general topic overlaps. Return [] when a section has no such references.

Return summaries for ALL {count} sections."#,
            count = items.len()
        );

        let result: BatchSummaryResult = self.extract(&prompt).await?;

        Ok(result
            .summaries
            .into_iter()
            .map(|item| (item.node_id, item.summary, item.references))
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

        const MAX_SUMMARY_CHARS: usize = 120;
        const MAX_SNIPPET_CHARS: usize = 80;
        const MAX_SECTIONS: usize = 3;

        let docs_formatted: String = documents
            .iter()
            .enumerate()
            .map(|(i, doc)| {
                let summary: &str = if doc.summary.len() > MAX_SUMMARY_CHARS {
                    &doc.summary[..MAX_SUMMARY_CHARS]
                } else {
                    &doc.summary
                };
                let mut entry = format!("{}. [{}] \"{}\" - {}", i + 1, doc.id, doc.title, summary);
                if !doc.matched_sections.is_empty() {
                    let sections: Vec<&str> = doc
                        .matched_sections
                        .iter()
                        .take(MAX_SECTIONS)
                        .map(|s| s.as_str())
                        .collect();
                    entry.push_str(&format!(" | sections: {}", sections.join(", ")));
                }
                if let Some(ref snippet) = doc.best_snippet {
                    let snip: &str = if snippet.len() > MAX_SNIPPET_CHARS {
                        &snippet[..MAX_SNIPPET_CHARS]
                    } else {
                        snippet
                    };
                    entry.push_str(&format!(" | match: {}", snip));
                }
                entry
            })
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"Rank documents by relevance to the query. Return ONLY JSON.

Query: "{}"

Documents:
{}

Return top {} relevant docs (relevance > 0.3), highest first.
Format: {{"rankings": [{{"document_id": "...", "relevance": 0.9}}]}}"#,
            query, docs_formatted, top_k
        );

        debug!("Ranking {} documents for query: {}", documents.len(), query);

        let result: DocumentRankings = self.extract_lean(&prompt).await?;

        let mut rankings = result.rankings;
        rankings.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        rankings.truncate(top_k);

        Ok(rankings)
    }

    async fn decompose_query(
        &self,
        query: &str,
        domain_context: Option<&DomainContext>,
    ) -> Result<Vec<SubQuery>> {
        let description_line = domain_context
            .and_then(|ctx| ctx.description.as_ref())
            .map(|d| format!("Collection description: {}\n", d))
            .unwrap_or_default();

        let vocab_line = domain_context
            .filter(|ctx| !ctx.vocab_hints.is_empty())
            .map(|ctx| {
                format!(
                    "Known domain terms (use these when appropriate): {}\n",
                    ctx.vocab_hints.join(", ")
                )
            })
            .unwrap_or_default();

        let table_name_line = domain_context
            .map(|ctx| format!("Document collection: \"{}\"\n", ctx.table_name))
            .unwrap_or_default();

        let prompt = format!(
            r#"You are a search query expert. A user is searching a document collection and may not know the domain-specific terminology used in the documents.

{table_name_line}{description_line}{vocab_line}
User query: "{query}"

Generate 3 alternative search queries that:
1. Preserve the original intent
2. Use domain-specific terminology and jargon from the collection
3. Cover different aspects or phrasings of the question
4. Would improve recall against technical documents

Include at least one query that closely mirrors the original phrasing.

Return JSON:
{{"sub_queries": [{{"text": "alternative query text", "rationale": "why this phrasing helps"}}, ...]}}

If the original query already uses precise domain terminology, return fewer alternatives."#,
        );

        debug!("Decomposing query: {}", query);

        let result: DecomposedQueryResult = self.extract(&prompt).await.unwrap_or_else(|e| {
            tracing::warn!("Query decomposition failed, using passthrough: {}", e);
            DecomposedQueryResult {
                sub_queries: vec![super::SubQueryItem {
                    text: query.to_string(),
                    rationale: "Fallback to original query".to_string(),
                }],
            }
        });

        Ok(result
            .sub_queries
            .into_iter()
            .map(|item| SubQuery {
                text: item.text,
                rationale: item.rationale,
            })
            .collect())
    }

    async fn extract_domain_vocab(
        &self,
        document_summary: &str,
        existing_vocab: &[String],
    ) -> Result<Vec<String>> {
        let truncated: String = document_summary.chars().take(3000).collect();

        let existing_line = if existing_vocab.is_empty() {
            String::new()
        } else {
            format!(
                "\nAlready known terms (do NOT include these): {}\n",
                existing_vocab.join(", ")
            )
        };

        let prompt = format!(
            r#"Extract domain-specific technical terms and jargon from this document summary.

Focus on:
- Technical product names and abbreviations (e.g. "TPD", "IBR", "PDS")
- Domain-specific concepts a general user would not know to search for
- Policy or procedural terms used by domain experts
- Acronyms and their full forms

Document summary:
{truncated}{existing_line}

Return up to 15 new unique terms. Return an empty array if the summary contains no domain-specific terminology.

Return JSON:
{{"terms": ["term1", "term2", ...]}}"#,
        );

        debug!("Extracting domain vocab from document summary");

        let result: DomainVocabResult = self.extract(&prompt).await.unwrap_or_else(|e| {
            tracing::warn!("Domain vocab extraction failed: {}", e);
            DomainVocabResult { terms: vec![] }
        });

        Ok(result.terms)
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
        assert!(matches!(gemini, LLMProvider::Gemini { model, .. } if model == "gemini-2.5-flash"));

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
        assert!(
            matches!(kimi_long, LLMProvider::Kimi { model, .. } if model == "moonshot-v1-128k")
        );

        let ollama = LLMProvider::ollama("llama3.3");
        assert!(matches!(ollama, LLMProvider::Ollama { ref model, .. } if model == "llama3.3"));
        assert_eq!(ollama.provider_name(), "ollama");

        let ollama_custom = LLMProvider::ollama_from_url("http://remote:11434/v1", "qwen2.5");
        assert!(
            matches!(ollama_custom, LLMProvider::Ollama { base_url, model } if base_url == "http://remote:11434/v1" && model == "qwen2.5")
        );

        let vertex = LLMProvider::vertex(
            "https://us-central1-aiplatform.googleapis.com/v1/projects/p/locations/us-central1/endpoints/openapi",
            "token",
            "gemini-2.0-flash-001",
        );
        assert!(
            matches!(vertex, LLMProvider::Vertex { ref model, .. } if model == "gemini-2.0-flash-001")
        );
        assert_eq!(vertex.provider_name(), "vertex");

        let bedrock = LLMProvider::bedrock("us-east-1", "anthropic.claude-3-sonnet-20240229-v1:0");
        assert!(
            matches!(bedrock, LLMProvider::Bedrock { ref region, ref model, .. } if region == "us-east-1" && model == "anthropic.claude-3-sonnet-20240229-v1:0")
        );
        assert_eq!(bedrock.provider_name(), "bedrock");
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
    fn test_repair_duplicate_key_json() {
        let malformed = r#"{"selections": [{"node_id": "aaa", "confidence": 0.95, "reasoning": "first reason", "node_id": "bbb", "confidence": 0.75, "reasoning": "second reason"}]}"#;

        let repaired =
            repair_duplicate_key_json(malformed, "node_id").expect("repair should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&repaired).expect("repaired JSON should parse");
        let selections = parsed["selections"].as_array().unwrap();
        assert_eq!(selections.len(), 2);
        assert_eq!(selections[0]["node_id"], "aaa");
        assert_eq!(selections[1]["node_id"], "bbb");
    }

    #[test]
    fn test_repair_duplicate_reasoning_field() {
        let malformed = r#"{"selections": [{"node_id": "aaa", "confidence": 0.95, "reasoning": "first", "reasoning": "second"}]}"#;

        let repaired =
            repair_duplicate_key_json(malformed, "reasoning").expect("repair should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&repaired).expect("repaired JSON should parse");
        let selections = parsed["selections"].as_array().unwrap();
        assert_eq!(selections.len(), 2);
    }

    #[test]
    fn test_repair_no_duplicates_returns_none() {
        let valid =
            r#"{"selections": [{"node_id": "aaa", "confidence": 0.95, "reasoning": "ok"}]}"#;
        assert!(repair_duplicate_key_json(valid, "node_id").is_none());
    }

    #[test]
    fn test_reasoner_config() {
        let config = ReasoningConfig {
            beam_width: 5,
            min_confidence: 0.5,
            ..Default::default()
        };

        let reasoner = Reasoner::new(LLMProvider::openai_mini("test")).with_config(config.clone());

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
