//! Plugin manager
//!
//! Top-level facade for the plugin system. Handles discovery, routing
//! requests to the correct plugin, and invoking them via the runner.

use std::path::{Path, PathBuf};
use tracing::{debug, info};

use crate::error::{PluginError, Result};
use crate::manifest::{PluginKind, PluginManifest};
use crate::protocol::*;
use crate::registry::PluginRegistry;
use crate::runner::PluginRunner;

/// Central coordinator for all plugins.
pub struct PluginManager {
    registry: PluginRegistry,
    plugins_dir: PathBuf,
    enabled: bool,
}

impl PluginManager {
    /// Create a new plugin manager. Call `discover()` to scan for plugins.
    pub fn new(plugins_dir: &Path) -> Self {
        Self {
            registry: PluginRegistry::new(),
            plugins_dir: plugins_dir.to_path_buf(),
            enabled: true,
        }
    }

    /// Create a disabled plugin manager (no-op for all operations).
    pub fn disabled() -> Self {
        Self {
            registry: PluginRegistry::new(),
            plugins_dir: PathBuf::new(),
            enabled: false,
        }
    }

    /// Whether the plugin system is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Scan the plugins directory and load all valid manifests.
    /// Returns the number of plugins discovered.
    pub fn discover(&mut self) -> Result<usize> {
        if !self.enabled {
            return Ok(0);
        }
        info!(dir = %self.plugins_dir.display(), "Discovering plugins");
        self.registry.discover(&self.plugins_dir)
    }

    // -----------------------------------------------------------------------
    // Extractor operations
    // -----------------------------------------------------------------------

    /// Check if any plugin can extract the given file format.
    pub fn has_extractor_for_format(&self, ext: &str) -> bool {
        self.enabled && self.registry.find_extractor_for_format(ext).is_some()
    }

    /// Check if any plugin can extract from the given URL.
    pub fn has_extractor_for_url(&self, url: &str) -> bool {
        self.enabled && self.registry.find_extractor_for_url(url).is_some()
    }

    /// Extract content from a file using the best matching plugin.
    pub fn extract_file(&self, path: &str) -> Result<ExtractResult> {
        if !self.enabled {
            return Err(PluginError::NoHandler("Plugins disabled".to_string()));
        }

        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let manifest = self
            .registry
            .find_extractor_for_format(ext)
            .ok_or_else(|| PluginError::NoHandler(format!("No extractor for .{}", ext)))?;

        let request = PluginRequest::extract(ExtractParams {
            source_type: SourceType::File,
            path: Some(path.to_string()),
            url: None,
            config: Default::default(),
        });

        let response = PluginRunner::invoke(manifest, &request)?;
        Self::parse_result::<ExtractResult>(response)
    }

    /// Extract content from a URL using the best matching plugin.
    pub fn extract_url(&self, url: &str) -> Result<ExtractResult> {
        if !self.enabled {
            return Err(PluginError::NoHandler("Plugins disabled".to_string()));
        }

        let manifest = self
            .registry
            .find_extractor_for_url(url)
            .ok_or_else(|| PluginError::NoHandler(format!("No extractor for URL: {}", url)))?;

        let request = PluginRequest::extract(ExtractParams {
            source_type: SourceType::Url,
            path: None,
            url: Some(url.to_string()),
            config: Default::default(),
        });

        let response = PluginRunner::invoke(manifest, &request)?;
        Self::parse_result::<ExtractResult>(response)
    }

    // -----------------------------------------------------------------------
    // Post-processor operations
    // -----------------------------------------------------------------------

    /// Check if any post-processor plugins are registered.
    pub fn has_post_processors(&self) -> bool {
        self.enabled
            && !self
                .registry
                .plugins_by_kind(PluginKind::PostProcessor)
                .is_empty()
    }

    /// Run all registered post-processor plugins in priority order (chained).
    /// Each plugin receives the output of the previous one.
    pub fn run_post_processors(
        &self,
        markdown: &str,
        metadata: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<ProcessResult> {
        if !self.enabled {
            return Ok(ProcessResult {
                markdown: markdown.to_string(),
                metadata: metadata.clone(),
            });
        }

        let processors = self.registry.plugins_by_kind(PluginKind::PostProcessor);
        if processors.is_empty() {
            return Ok(ProcessResult {
                markdown: markdown.to_string(),
                metadata: metadata.clone(),
            });
        }

        let mut current_md = markdown.to_string();
        let mut current_meta = metadata.clone();

        for manifest in processors {
            debug!(plugin = %manifest.name, "Running post-processor");
            let request = PluginRequest::process(ProcessParams {
                markdown: current_md.clone(),
                metadata: current_meta.clone(),
                config: Default::default(),
            });

            let response = PluginRunner::invoke(manifest, &request)?;
            let result: ProcessResult = Self::parse_result(response)?;
            current_md = result.markdown;
            current_meta = result.metadata;
        }

        Ok(ProcessResult {
            markdown: current_md,
            metadata: current_meta,
        })
    }

    // -----------------------------------------------------------------------
    // Chunker operations
    // -----------------------------------------------------------------------

    /// Check if a chunker plugin is registered.
    pub fn has_chunker(&self) -> bool {
        self.enabled
            && !self
                .registry
                .plugins_by_kind(PluginKind::Chunker)
                .is_empty()
    }

    /// Chunk markdown using the highest-priority chunker plugin.
    pub fn chunk(&self, markdown: &str, config: &ChunkConfig) -> Result<ChunkResult> {
        if !self.enabled {
            return Err(PluginError::NoHandler("Plugins disabled".to_string()));
        }

        let manifest = self
            .registry
            .plugins_by_kind(PluginKind::Chunker)
            .into_iter()
            .next()
            .ok_or_else(|| PluginError::NoHandler("No chunker plugin".to_string()))?;

        let request = PluginRequest::chunk(ChunkParams {
            markdown: markdown.to_string(),
            config: config.clone(),
        });

        let response = PluginRunner::invoke(manifest, &request)?;
        Self::parse_result::<ChunkResult>(response)
    }

    // -----------------------------------------------------------------------
    // Summarizer operations
    // -----------------------------------------------------------------------

    /// Check if a summarizer plugin is registered.
    pub fn has_summarizer(&self) -> bool {
        self.enabled
            && !self
                .registry
                .plugins_by_kind(PluginKind::Summarizer)
                .is_empty()
    }

    /// Summarize content using the highest-priority summarizer plugin.
    pub fn summarize(
        &self,
        content: &str,
        context: &std::collections::HashMap<String, String>,
    ) -> Result<SummarizeResult> {
        if !self.enabled {
            return Err(PluginError::NoHandler("Plugins disabled".to_string()));
        }

        let manifest = self
            .registry
            .plugins_by_kind(PluginKind::Summarizer)
            .into_iter()
            .next()
            .ok_or_else(|| PluginError::NoHandler("No summarizer plugin".to_string()))?;

        let request = PluginRequest::summarize(SummarizeParams {
            content: content.to_string(),
            context: context.clone(),
        });

        let response = PluginRunner::invoke(manifest, &request)?;
        Self::parse_result::<SummarizeResult>(response)
    }

    // -----------------------------------------------------------------------
    // Formatter operations
    // -----------------------------------------------------------------------

    /// Check if a formatter plugin is registered.
    pub fn has_formatter(&self) -> bool {
        self.enabled
            && !self
                .registry
                .plugins_by_kind(PluginKind::Formatter)
                .is_empty()
    }

    /// Format nodes using the highest-priority formatter plugin.
    pub fn format(
        &self,
        nodes: Vec<FormatNode>,
        config: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<FormatResult> {
        if !self.enabled {
            return Err(PluginError::NoHandler("Plugins disabled".to_string()));
        }

        let manifest = self
            .registry
            .plugins_by_kind(PluginKind::Formatter)
            .into_iter()
            .next()
            .ok_or_else(|| PluginError::NoHandler("No formatter plugin".to_string()))?;

        let request = PluginRequest::format(FormatParams {
            nodes,
            config: config.clone(),
        });

        let response = PluginRunner::invoke(manifest, &request)?;
        Self::parse_result::<FormatResult>(response)
    }

    // -----------------------------------------------------------------------
    // Query / admin
    // -----------------------------------------------------------------------

    /// List all registered plugins.
    pub fn list_plugins(&self) -> &[PluginManifest] {
        self.registry.all()
    }

    /// Get a plugin by name.
    pub fn get_plugin(&self, name: &str) -> Option<&PluginManifest> {
        self.registry.get(name)
    }

    /// Number of registered plugins.
    pub fn plugin_count(&self) -> usize {
        self.registry.len()
    }

    /// The directory being scanned for plugins.
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn parse_result<T: serde::de::DeserializeOwned>(response: PluginResponse) -> Result<T> {
        let value = response
            .result
            .ok_or_else(|| PluginError::Protocol("Plugin response has no result".to_string()))?;

        serde_json::from_value(value)
            .map_err(|e| PluginError::Protocol(format!("Cannot parse plugin result: {}", e)))
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::disabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_disabled_manager() {
        let mgr = PluginManager::disabled();
        assert!(!mgr.is_enabled());
        assert!(!mgr.has_extractor_for_format("pdf"));
        assert!(!mgr.has_chunker());
        assert!(!mgr.has_summarizer());
        assert_eq!(mgr.plugin_count(), 0);
    }

    #[test]
    fn test_discover_empty_dir() {
        let dir = tempdir().unwrap();
        let mut mgr = PluginManager::new(dir.path());
        let count = mgr.discover().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_discover_with_plugin() {
        let dir = tempdir().unwrap();
        let plugin_dir = dir.path().join("test-ext");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            r#"
[plugin]
name = "test-ext"
[plugin.runner]
command = "bash"
[plugin.capabilities]
kind = "extractor"
formats = ["pdf", "docx"]
handles_urls = true
url_patterns = ["*"]
"#,
        )
        .unwrap();

        let mut mgr = PluginManager::new(dir.path());
        let count = mgr.discover().unwrap();
        assert_eq!(count, 1);
        assert!(mgr.has_extractor_for_format("pdf"));
        assert!(mgr.has_extractor_for_format("docx"));
        assert!(!mgr.has_extractor_for_format("xlsx"));
        assert!(mgr.has_extractor_for_url("https://example.com"));
    }

    #[test]
    fn test_no_handler_error() {
        let dir = tempdir().unwrap();
        let mgr = PluginManager::new(dir.path());
        let err = mgr.extract_file("/tmp/test.xyz");
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("No extractor"));
    }

    #[test]
    fn test_post_processors_passthrough_when_none() {
        let dir = tempdir().unwrap();
        let mgr = PluginManager::new(dir.path());
        let result = mgr
            .run_post_processors("# Hello", &std::collections::HashMap::new())
            .unwrap();
        assert_eq!(result.markdown, "# Hello");
    }

    #[test]
    fn test_list_plugins() {
        let dir = tempdir().unwrap();
        let plugin_dir = dir.path().join("plug1");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            r#"
[plugin]
name = "plug1"
[plugin.runner]
command = "bash"
[plugin.capabilities]
kind = "chunker"
"#,
        )
        .unwrap();

        let mut mgr = PluginManager::new(dir.path());
        mgr.discover().unwrap();

        assert_eq!(mgr.list_plugins().len(), 1);
        assert!(mgr.get_plugin("plug1").is_some());
        assert!(mgr.get_plugin("nope").is_none());
    }
}
