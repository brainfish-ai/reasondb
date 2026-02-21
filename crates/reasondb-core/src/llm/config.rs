//! LLM configuration types
//!
//! Serializable configuration for LLM providers, stored in the database
//! and updatable at runtime via the API.

use serde::{Deserialize, Serialize};

use super::provider::LLMProvider;
use crate::error::{ReasonError, Result};

/// Provider-specific and general LLM options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmOptions {
    /// Temperature (0.0 = deterministic, 1.0 = creative). None = provider default.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub temperature: Option<f32>,
    /// Max output tokens. None = provider default.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_tokens: Option<u64>,
    /// System prompt override. None = use built-in prompts.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub system_prompt: Option<String>,
    /// Top-p nucleus sampling. None = provider default.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub top_p: Option<f32>,
    /// Frequency penalty (-2.0 to 2.0). None = provider default.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub frequency_penalty: Option<f32>,
    /// Presence penalty (-2.0 to 2.0). None = provider default.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub presence_penalty: Option<f32>,
    /// Disable extended thinking (Kimi K2.5, future thinking models).
    #[serde(default)]
    pub disable_thinking: bool,
}

impl Default for LlmOptions {
    fn default() -> Self {
        Self {
            temperature: None,
            max_tokens: None,
            system_prompt: None,
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            disable_thinking: false,
        }
    }
}

/// Configuration for a single LLM model (provider + credentials + options)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModelConfig {
    /// Provider name: "openai", "anthropic", "gemini", "cohere", "glm", "kimi", "ollama"
    pub provider: String,
    /// API key (not required for Ollama)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub api_key: Option<String>,
    /// Model name override. None = provider default.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub model: Option<String>,
    /// Base URL override (primarily for Ollama)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub base_url: Option<String>,
    /// LLM options (temperature, max_tokens, etc.)
    #[serde(default)]
    pub options: LlmOptions,
}

impl LlmModelConfig {
    /// Convert this config into an `LLMProvider` enum variant.
    pub fn to_provider(&self) -> Result<LLMProvider> {
        let model_or = |default: &str| -> String {
            self.model.clone().unwrap_or_else(|| default.to_string())
        };

        match self.provider.to_lowercase().as_str() {
            "openai" => {
                let key = self.api_key.clone().ok_or_else(|| {
                    ReasonError::Config("OpenAI requires an API key".into())
                })?;
                Ok(LLMProvider::OpenAI {
                    api_key: key,
                    model: model_or("gpt-4o"),
                })
            }
            "anthropic" => {
                let key = self.api_key.clone().ok_or_else(|| {
                    ReasonError::Config("Anthropic requires an API key".into())
                })?;
                Ok(LLMProvider::Anthropic {
                    api_key: key,
                    model: model_or("claude-sonnet-4-6"),
                })
            }
            "gemini" => {
                let key = self.api_key.clone().ok_or_else(|| {
                    ReasonError::Config("Gemini requires an API key".into())
                })?;
                Ok(LLMProvider::Gemini {
                    api_key: key,
                    model: model_or("gemini-1.5-flash"),
                })
            }
            "cohere" => {
                let key = self.api_key.clone().ok_or_else(|| {
                    ReasonError::Config("Cohere requires an API key".into())
                })?;
                Ok(LLMProvider::Cohere {
                    api_key: key,
                    model: model_or("command-r-plus"),
                })
            }
            "glm" => {
                let key = self.api_key.clone().ok_or_else(|| {
                    ReasonError::Config("GLM requires an API key".into())
                })?;
                Ok(LLMProvider::Glm {
                    api_key: key,
                    model: model_or("glm-4-flash"),
                })
            }
            "kimi" => {
                let key = self.api_key.clone().ok_or_else(|| {
                    ReasonError::Config("Kimi requires an API key".into())
                })?;
                Ok(LLMProvider::Kimi {
                    api_key: key,
                    model: model_or("moonshot-v1-8k"),
                })
            }
            "ollama" => {
                let base_url = self
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "http://localhost:11434/v1".to_string());
                Ok(LLMProvider::Ollama {
                    base_url,
                    model: model_or("llama3.3"),
                })
            }
            other => Err(ReasonError::Config(format!(
                "Unknown LLM provider: '{}'. Supported: openai, anthropic, gemini, cohere, glm, kimi, ollama",
                other
            ))),
        }
    }

    /// Mask the API key for display (first 4 + last 4 chars).
    pub fn masked(&self) -> Self {
        let mut copy = self.clone();
        copy.api_key = copy.api_key.map(|k| mask_key(&k));
        copy
    }
}

impl From<&LLMProvider> for LlmModelConfig {
    fn from(provider: &LLMProvider) -> Self {
        match provider {
            LLMProvider::OpenAI { api_key, model } => Self {
                provider: "openai".into(),
                api_key: Some(api_key.clone()),
                model: Some(model.clone()),
                base_url: None,
                options: LlmOptions::default(),
            },
            LLMProvider::Anthropic { api_key, model } => Self {
                provider: "anthropic".into(),
                api_key: Some(api_key.clone()),
                model: Some(model.clone()),
                base_url: None,
                options: LlmOptions::default(),
            },
            LLMProvider::Gemini { api_key, model } => Self {
                provider: "gemini".into(),
                api_key: Some(api_key.clone()),
                model: Some(model.clone()),
                base_url: None,
                options: LlmOptions::default(),
            },
            LLMProvider::Cohere { api_key, model } => Self {
                provider: "cohere".into(),
                api_key: Some(api_key.clone()),
                model: Some(model.clone()),
                base_url: None,
                options: LlmOptions::default(),
            },
            LLMProvider::Glm { api_key, model } => Self {
                provider: "glm".into(),
                api_key: Some(api_key.clone()),
                model: Some(model.clone()),
                base_url: None,
                options: LlmOptions::default(),
            },
            LLMProvider::Kimi { api_key, model } => Self {
                provider: "kimi".into(),
                api_key: Some(api_key.clone()),
                model: Some(model.clone()),
                base_url: None,
                options: LlmOptions::default(),
            },
            LLMProvider::Ollama { base_url, model } => Self {
                provider: "ollama".into(),
                api_key: None,
                model: Some(model.clone()),
                base_url: Some(base_url.clone()),
                options: LlmOptions::default(),
            },
        }
    }
}

/// Top-level LLM settings with separate ingestion and retrieval configs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmSettings {
    /// Model used for ingestion (summarization)
    pub ingestion: LlmModelConfig,
    /// Model used for retrieval (search reasoning)
    pub retrieval: LlmModelConfig,
}

impl LlmSettings {
    /// Create settings that use the same provider for both ingestion and retrieval.
    pub fn from_single(provider: &LLMProvider) -> Self {
        let config = LlmModelConfig::from(provider);
        Self {
            ingestion: config.clone(),
            retrieval: config,
        }
    }

    /// Return a copy with API keys masked for display.
    pub fn masked(&self) -> Self {
        Self {
            ingestion: self.ingestion.masked(),
            retrieval: self.retrieval.masked(),
        }
    }
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        "****".to_string()
    } else {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip_openai() {
        let provider = LLMProvider::openai("sk-test-key");
        let config = LlmModelConfig::from(&provider);
        assert_eq!(config.provider, "openai");
        assert_eq!(config.api_key.as_deref(), Some("sk-test-key"));
        assert_eq!(config.model.as_deref(), Some("gpt-4o"));

        let back = config.to_provider().unwrap();
        assert_eq!(back.provider_name(), "openai");
        assert_eq!(back.model(), "gpt-4o");
    }

    #[test]
    fn test_round_trip_ollama() {
        let provider = LLMProvider::ollama("llama3.3");
        let config = LlmModelConfig::from(&provider);
        assert_eq!(config.provider, "ollama");
        assert!(config.api_key.is_none());
        assert_eq!(config.base_url.as_deref(), Some("http://localhost:11434/v1"));

        let back = config.to_provider().unwrap();
        assert_eq!(back.provider_name(), "ollama");
    }

    #[test]
    fn test_mask_key() {
        assert_eq!(mask_key("sk-1234567890abcdef"), "sk-1...cdef");
        assert_eq!(mask_key("short"), "****");
    }

    #[test]
    fn test_unknown_provider_error() {
        let config = LlmModelConfig {
            provider: "unknown".into(),
            api_key: None,
            model: None,
            base_url: None,
            options: LlmOptions::default(),
        };
        assert!(config.to_provider().is_err());
    }

    #[test]
    fn test_settings_from_single() {
        let provider = LLMProvider::openai_mini("sk-test");
        let settings = LlmSettings::from_single(&provider);
        assert_eq!(settings.ingestion.provider, "openai");
        assert_eq!(settings.retrieval.provider, "openai");
    }

    #[test]
    fn test_options_default() {
        let opts = LlmOptions::default();
        assert!(opts.temperature.is_none());
        assert!(opts.max_tokens.is_none());
        assert!(!opts.disable_thinking);
    }
}
