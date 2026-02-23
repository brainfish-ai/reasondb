//! JSON protocol for plugin communication
//!
//! Plugins receive a `PluginRequest` on stdin and write a `PluginResponse` to stdout.
//! All communication is JSON, one object per invocation (one-shot model).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const PROTOCOL_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Envelope types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRequest {
    pub version: u32,
    pub operation: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResponse {
    pub version: u32,
    pub status: ResponseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ResponseStatus {
    Ok,
    Error,
}

impl PluginRequest {
    pub fn extract(params: ExtractParams) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            operation: "extract".to_string(),
            params: serde_json::to_value(params).unwrap_or_default(),
        }
    }

    pub fn process(params: ProcessParams) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            operation: "process".to_string(),
            params: serde_json::to_value(params).unwrap_or_default(),
        }
    }

    pub fn chunk(params: ChunkParams) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            operation: "chunk".to_string(),
            params: serde_json::to_value(params).unwrap_or_default(),
        }
    }

    pub fn summarize(params: SummarizeParams) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            operation: "summarize".to_string(),
            params: serde_json::to_value(params).unwrap_or_default(),
        }
    }

    pub fn format(params: FormatParams) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            operation: "format".to_string(),
            params: serde_json::to_value(params).unwrap_or_default(),
        }
    }
}

impl PluginResponse {
    pub fn ok(result: serde_json::Value) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            status: ResponseStatus::Ok,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            status: ResponseStatus::Error,
            result: None,
            error: Some(message.into()),
        }
    }

    pub fn is_ok(&self) -> bool {
        self.status == ResponseStatus::Ok
    }
}

// ---------------------------------------------------------------------------
// Extractor types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractParams {
    pub source_type: SourceType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    File,
    Url,
    Bytes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractResult {
    pub title: String,
    pub markdown: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_type: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Post-processor types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessParams {
    pub markdown: String,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessResult {
    pub markdown: String,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Chunker types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkParams {
    pub markdown: String,
    #[serde(default)]
    pub config: ChunkConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChunkConfig {
    #[serde(default = "default_target_size")]
    pub target_chunk_size: usize,
    #[serde(default = "default_min_size")]
    pub min_chunk_size: usize,
    #[serde(default = "default_max_size")]
    pub max_chunk_size: usize,
    #[serde(default = "default_overlap")]
    pub overlap: usize,
}

fn default_target_size() -> usize {
    1500
}
fn default_min_size() -> usize {
    500
}
fn default_max_size() -> usize {
    3000
}
fn default_overlap() -> usize {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkResult {
    pub chunks: Vec<ChunkOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkOutput {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    #[serde(default)]
    pub level: u8,
    pub char_count: usize,
}

// ---------------------------------------------------------------------------
// Summarizer types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummarizeParams {
    pub content: String,
    #[serde(default)]
    pub context: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummarizeResult {
    pub summary: String,
}

// ---------------------------------------------------------------------------
// Formatter types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatParams {
    pub nodes: Vec<FormatNode>,
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatNode {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub depth: usize,
    pub children: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatResult {
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = PluginRequest::extract(ExtractParams {
            source_type: SourceType::File,
            path: Some("/tmp/doc.pdf".to_string()),
            url: None,
            config: HashMap::new(),
        });
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"operation\":\"extract\""));
        assert!(json.contains("\"version\":1"));

        let parsed: PluginRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.operation, "extract");
    }

    #[test]
    fn test_response_ok() {
        let resp = PluginResponse::ok(serde_json::json!({"title": "Test"}));
        assert!(resp.is_ok());
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"status\":\"ok\""));
    }

    #[test]
    fn test_response_error() {
        let resp = PluginResponse::err("something broke");
        assert!(!resp.is_ok());
        assert_eq!(resp.error.as_deref(), Some("something broke"));
    }

    #[test]
    fn test_extract_params_roundtrip() {
        let params = ExtractParams {
            source_type: SourceType::Url,
            path: None,
            url: Some("https://example.com".to_string()),
            config: HashMap::new(),
        };
        let json = serde_json::to_value(&params).unwrap();
        let parsed: ExtractParams = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.source_type, SourceType::Url);
        assert_eq!(parsed.url.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn test_chunk_config_defaults() {
        let config: ChunkConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config.target_chunk_size, 1500);
        assert_eq!(config.min_chunk_size, 500);
        assert_eq!(config.max_chunk_size, 3000);
        assert_eq!(config.overlap, 100);
    }

    #[test]
    fn test_all_operations() {
        let extract = PluginRequest::extract(ExtractParams {
            source_type: SourceType::File,
            path: Some("/tmp/f.pdf".to_string()),
            url: None,
            config: HashMap::new(),
        });
        assert_eq!(extract.operation, "extract");

        let process = PluginRequest::process(ProcessParams {
            markdown: "# Hi".to_string(),
            metadata: HashMap::new(),
            config: HashMap::new(),
        });
        assert_eq!(process.operation, "process");

        let chunk = PluginRequest::chunk(ChunkParams {
            markdown: "# Hi\nworld".to_string(),
            config: ChunkConfig::default(),
        });
        assert_eq!(chunk.operation, "chunk");

        let summarize = PluginRequest::summarize(SummarizeParams {
            content: "Long text".to_string(),
            context: HashMap::new(),
        });
        assert_eq!(summarize.operation, "summarize");

        let format = PluginRequest::format(FormatParams {
            nodes: vec![],
            config: HashMap::new(),
        });
        assert_eq!(format.operation, "format");
    }
}
