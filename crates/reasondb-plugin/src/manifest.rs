//! Plugin manifest parsing (plugin.toml)
//!
//! Each plugin ships a `plugin.toml` in its directory that declares its
//! identity, how to run it, and what capabilities it provides.

use crate::error::{PluginError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Top-level manifest wrapper matching TOML structure `[plugin]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestFile {
    plugin: ManifestInner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestInner {
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    license: String,
    runner: RunnerConfig,
    capabilities: CapabilitiesRaw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CapabilitiesRaw {
    kind: String,
    #[serde(default)]
    formats: Vec<String>,
    #[serde(default)]
    handles_urls: bool,
    #[serde(default)]
    url_patterns: Vec<String>,
    #[serde(default = "default_priority")]
    priority: u32,
}

fn default_priority() -> u32 {
    100
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// The kind of pipeline stage a plugin handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Extractor,
    PostProcessor,
    Chunker,
    Summarizer,
    Formatter,
}

impl std::str::FromStr for PluginKind {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "extractor" => Ok(Self::Extractor),
            "post_processor" | "postprocessor" => Ok(Self::PostProcessor),
            "chunker" => Ok(Self::Chunker),
            "summarizer" => Ok(Self::Summarizer),
            "formatter" => Ok(Self::Formatter),
            other => Err(format!("Unknown plugin kind '{}'", other)),
        }
    }
}

impl PluginKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Extractor => "extractor",
            Self::PostProcessor => "post_processor",
            Self::Chunker => "chunker",
            Self::Summarizer => "summarizer",
            Self::Formatter => "formatter",
        }
    }
}

impl std::fmt::Display for PluginKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Supported plugin runtimes.
///
/// Plugins must use one of these commands so the Docker image ships the
/// required runtime. Compiled binaries (Rust, Go, C++) use a relative
/// path like `"./my-binary"`.
const SUPPORTED_COMMANDS: &[&str] = &["python3", "python", "node", "bash", "sh"];

/// How to invoke the plugin process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Run as a persistent daemon instead of spawning per request.
    ///
    /// When `true`, the plugin process is started once and kept alive. It must
    /// read newline-delimited JSON requests from stdin in a loop and write one
    /// JSON response line per request to stdout.
    ///
    /// Set the env var `REASONDB_DAEMON=1` to help the plugin detect daemon
    /// mode and enter its request loop.
    #[serde(default)]
    pub daemon: bool,
}

impl RunnerConfig {
    /// Whether the command is a known supported runtime or a local binary path.
    pub fn is_supported(&self) -> bool {
        let cmd = self.command.as_str();
        SUPPORTED_COMMANDS.contains(&cmd) || cmd.starts_with("./") || cmd.starts_with('/')
    }
}

fn default_timeout() -> u64 {
    120
}

/// What the plugin can handle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCapabilities {
    pub kind: PluginKind,
    pub formats: Vec<String>,
    pub handles_urls: bool,
    pub url_patterns: Vec<String>,
    pub priority: u32,
}

/// A fully-parsed, validated plugin manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub runner: RunnerConfig,
    pub capabilities: PluginCapabilities,
    /// Absolute path to the plugin directory (set at load time).
    pub dir: PathBuf,
}

impl PluginManifest {
    /// Load and validate a manifest from a `plugin.toml` file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| PluginError::Manifest(format!("Cannot read {}: {}", path.display(), e)))?;
        Self::parse(&content, path)
    }

    /// Parse manifest from TOML string. `file_path` is used to resolve the
    /// plugin directory.
    pub fn parse(toml_str: &str, file_path: &Path) -> Result<Self> {
        let file: ManifestFile = toml::from_str(toml_str)
            .map_err(|e| PluginError::Manifest(format!("Invalid TOML: {}", e)))?;

        let inner = file.plugin;
        let kind: PluginKind = inner.capabilities.kind.parse().map_err(|_| {
            PluginError::Manifest(format!(
                "Unknown plugin kind '{}'. Expected: extractor, post_processor, chunker, summarizer, formatter",
                inner.capabilities.kind
            ))
        })?;

        if inner.name.is_empty() {
            return Err(PluginError::Manifest("Plugin name is required".to_string()));
        }

        if !inner.runner.is_supported() {
            return Err(PluginError::Manifest(format!(
                "Unsupported plugin command '{}'. Supported runtimes: python3, node, bash/sh, or a local binary (./path)",
                inner.runner.command
            )));
        }

        let dir = file_path.parent().unwrap_or(Path::new(".")).to_path_buf();

        Ok(Self {
            name: inner.name,
            version: inner.version,
            description: inner.description,
            author: inner.author,
            license: inner.license,
            runner: inner.runner,
            capabilities: PluginCapabilities {
                kind,
                formats: inner.capabilities.formats,
                handles_urls: inner.capabilities.handles_urls,
                url_patterns: inner.capabilities.url_patterns,
                priority: inner.capabilities.priority,
            },
            dir,
        })
    }

    /// Whether this plugin can handle a given file extension.
    pub fn handles_format(&self, ext: &str) -> bool {
        let ext_lower = ext.to_lowercase();
        self.capabilities
            .formats
            .iter()
            .any(|f| f.eq_ignore_ascii_case(&ext_lower))
    }

    /// Whether this plugin can handle URLs.
    pub fn handles_url(&self, url: &str) -> bool {
        if !self.capabilities.handles_urls {
            return false;
        }
        if self.capabilities.url_patterns.is_empty()
            || self.capabilities.url_patterns.contains(&"*".to_string())
        {
            return true;
        }
        self.capabilities
            .url_patterns
            .iter()
            .any(|pat| url.contains(pat))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MANIFEST: &str = r#"
[plugin]
name = "test-extractor"
version = "1.0.0"
description = "A test extractor plugin"
author = "Test Author"
license = "MIT"

[plugin.runner]
command = "python3"
args = ["extract.py"]
timeout_secs = 60

[plugin.capabilities]
kind = "extractor"
formats = ["pdf", "docx", "html"]
handles_urls = true
url_patterns = ["*"]
priority = 200
"#;

    #[test]
    fn test_parse_manifest() {
        let manifest =
            PluginManifest::parse(SAMPLE_MANIFEST, Path::new("/tmp/test/plugin.toml")).unwrap();
        assert_eq!(manifest.name, "test-extractor");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.capabilities.kind, PluginKind::Extractor);
        assert_eq!(manifest.capabilities.formats.len(), 3);
        assert!(manifest.capabilities.handles_urls);
        assert_eq!(manifest.capabilities.priority, 200);
        assert_eq!(manifest.runner.command, "python3");
        assert_eq!(manifest.runner.args, vec!["extract.py"]);
        assert_eq!(manifest.runner.timeout_secs, 60);
        assert_eq!(manifest.dir, Path::new("/tmp/test"));
    }

    #[test]
    fn test_handles_format() {
        let manifest =
            PluginManifest::parse(SAMPLE_MANIFEST, Path::new("/tmp/plugin.toml")).unwrap();
        assert!(manifest.handles_format("pdf"));
        assert!(manifest.handles_format("PDF"));
        assert!(manifest.handles_format("docx"));
        assert!(!manifest.handles_format("xlsx"));
    }

    #[test]
    fn test_handles_url_wildcard() {
        let manifest =
            PluginManifest::parse(SAMPLE_MANIFEST, Path::new("/tmp/plugin.toml")).unwrap();
        assert!(manifest.handles_url("https://example.com"));
        assert!(manifest.handles_url("https://youtube.com/watch?v=123"));
    }

    #[test]
    fn test_handles_url_patterns() {
        let toml = r#"
[plugin]
name = "youtube"
[plugin.runner]
command = "python3"
args = ["yt.py"]
[plugin.capabilities]
kind = "extractor"
handles_urls = true
url_patterns = ["youtube.com", "youtu.be"]
"#;
        let manifest = PluginManifest::parse(toml, Path::new("/tmp/plugin.toml")).unwrap();
        assert!(manifest.handles_url("https://youtube.com/watch?v=abc"));
        assert!(manifest.handles_url("https://youtu.be/abc"));
        assert!(!manifest.handles_url("https://example.com"));
    }

    #[test]
    fn test_invalid_kind() {
        let toml = r#"
[plugin]
name = "bad"
[plugin.runner]
command = "bash"
[plugin.capabilities]
kind = "invalid_kind"
"#;
        let err = PluginManifest::parse(toml, Path::new("/tmp/plugin.toml"));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("Unknown plugin kind"));
    }

    #[test]
    fn test_missing_name() {
        let toml = r#"
[plugin]
name = ""
[plugin.runner]
command = "bash"
[plugin.capabilities]
kind = "extractor"
"#;
        let err = PluginManifest::parse(toml, Path::new("/tmp/plugin.toml"));
        assert!(err.is_err());
    }

    #[test]
    fn test_defaults() {
        let toml = r#"
[plugin]
name = "minimal"
[plugin.runner]
command = "bash"
args = ["run.sh"]
[plugin.capabilities]
kind = "chunker"
"#;
        let manifest = PluginManifest::parse(toml, Path::new("/tmp/plugin.toml")).unwrap();
        assert_eq!(manifest.capabilities.priority, 100);
        assert_eq!(manifest.runner.timeout_secs, 120);
        assert!(!manifest.capabilities.handles_urls);
        assert!(manifest.capabilities.formats.is_empty());
    }

    #[test]
    fn test_plugin_kind_display() {
        assert_eq!(PluginKind::Extractor.to_string(), "extractor");
        assert_eq!(PluginKind::PostProcessor.to_string(), "post_processor");
        assert_eq!(PluginKind::Chunker.to_string(), "chunker");
    }

    #[test]
    fn test_unsupported_command() {
        let toml = r#"
[plugin]
name = "bad-runtime"
[plugin.runner]
command = "ruby"
[plugin.capabilities]
kind = "extractor"
"#;
        let err = PluginManifest::parse(toml, Path::new("/tmp/plugin.toml"));
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("Unsupported plugin command"));
    }

    #[test]
    fn test_local_binary_command_allowed() {
        let toml = r#"
[plugin]
name = "compiled-plugin"
[plugin.runner]
command = "./my-binary"
[plugin.capabilities]
kind = "extractor"
"#;
        let manifest = PluginManifest::parse(toml, Path::new("/tmp/plugin.toml"));
        assert!(manifest.is_ok());
    }

    #[test]
    fn test_post_processor_alias() {
        assert_eq!(
            "post_processor".parse::<PluginKind>(),
            Ok(PluginKind::PostProcessor)
        );
        assert_eq!(
            "postprocessor".parse::<PluginKind>(),
            Ok(PluginKind::PostProcessor)
        );
    }
}
