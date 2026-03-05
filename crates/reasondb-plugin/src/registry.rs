//! Plugin registry
//!
//! Stores loaded plugin manifests and resolves which plugin should handle
//! a given request based on format, URL pattern, and priority.

use std::path::Path;
use tracing::{debug, info, warn};

use crate::error::{PluginError, Result};
use crate::manifest::{PluginKind, PluginManifest};

/// Stores all discovered plugin manifests and provides lookup methods.
#[derive(Debug, Default)]
pub struct PluginRegistry {
    plugins: Vec<PluginManifest>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Register a plugin manifest.
    pub fn register(&mut self, manifest: PluginManifest) {
        info!(
            plugin = %manifest.name,
            kind = %manifest.capabilities.kind,
            priority = manifest.capabilities.priority,
            "Registered plugin"
        );
        self.plugins.push(manifest);
        // Keep sorted by priority descending so higher-priority plugins are tried first
        self.plugins
            .sort_by(|a, b| b.capabilities.priority.cmp(&a.capabilities.priority));
    }

    /// Scan a directory for plugin manifests and register them.
    /// Each subdirectory containing a `plugin.toml` is treated as a plugin.
    pub fn discover(&mut self, plugins_dir: &Path) -> Result<usize> {
        if !plugins_dir.exists() {
            debug!(dir = %plugins_dir.display(), "Plugins directory does not exist, skipping");
            return Ok(0);
        }

        let mut count = 0;
        let entries = std::fs::read_dir(plugins_dir).map_err(|e| {
            PluginError::Manifest(format!(
                "Cannot read plugins directory {}: {}",
                plugins_dir.display(),
                e
            ))
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let manifest_path = path.join("plugin.toml");
            if !manifest_path.exists() {
                debug!(dir = %path.display(), "No plugin.toml found, skipping");
                continue;
            }

            match PluginManifest::load(&manifest_path) {
                Ok(manifest) => {
                    self.register(manifest);
                    count += 1;
                }
                Err(e) => {
                    warn!(
                        path = %manifest_path.display(),
                        error = %e,
                        "Failed to load plugin manifest"
                    );
                }
            }
        }

        info!(count, dir = %plugins_dir.display(), "Plugin discovery complete");
        Ok(count)
    }

    /// Find the best extractor plugin for a file extension.
    /// Returns plugins sorted by priority (highest first).
    pub fn find_extractor_for_format(&self, ext: &str) -> Option<&PluginManifest> {
        self.plugins
            .iter()
            .find(|p| p.capabilities.kind == PluginKind::Extractor && p.handles_format(ext))
    }

    /// Find the best extractor plugin for a URL.
    pub fn find_extractor_for_url(&self, url: &str) -> Option<&PluginManifest> {
        self.plugins
            .iter()
            .find(|p| p.capabilities.kind == PluginKind::Extractor && p.handles_url(url))
    }

    /// Get all plugins of a specific kind, sorted by priority.
    pub fn plugins_by_kind(&self, kind: PluginKind) -> Vec<&PluginManifest> {
        self.plugins
            .iter()
            .filter(|p| p.capabilities.kind == kind)
            .collect()
    }

    /// Get a plugin by name.
    pub fn get(&self, name: &str) -> Option<&PluginManifest> {
        self.plugins.iter().find(|p| p.name == name)
    }

    /// Get all registered plugins.
    pub fn all(&self) -> &[PluginManifest] {
        &self.plugins
    }

    /// Number of registered plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{PluginCapabilities, RunnerConfig};
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn make_manifest(
        name: &str,
        kind: PluginKind,
        formats: &[&str],
        priority: u32,
    ) -> PluginManifest {
        PluginManifest {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: String::new(),
            author: String::new(),
            license: String::new(),
            runner: RunnerConfig {
                command: "bash".to_string(),
                args: vec![],
                timeout_secs: 60,
                env: HashMap::new(),
                daemon: false,
            },
            capabilities: PluginCapabilities {
                kind,
                formats: formats.iter().map(|s| s.to_string()).collect(),
                handles_urls: kind == PluginKind::Extractor,
                url_patterns: vec!["*".to_string()],
                priority,
            },
            dir: std::path::PathBuf::from("/tmp"),
        }
    }

    #[test]
    fn test_register_and_find() {
        let mut reg = PluginRegistry::new();
        reg.register(make_manifest(
            "pdf-plugin",
            PluginKind::Extractor,
            &["pdf"],
            100,
        ));
        reg.register(make_manifest(
            "docx-plugin",
            PluginKind::Extractor,
            &["docx"],
            100,
        ));

        assert_eq!(reg.len(), 2);
        assert!(reg.find_extractor_for_format("pdf").is_some());
        assert!(reg.find_extractor_for_format("docx").is_some());
        assert!(reg.find_extractor_for_format("xlsx").is_none());
    }

    #[test]
    fn test_priority_ordering() {
        let mut reg = PluginRegistry::new();
        reg.register(make_manifest("low", PluginKind::Extractor, &["pdf"], 50));
        reg.register(make_manifest("high", PluginKind::Extractor, &["pdf"], 200));
        reg.register(make_manifest("mid", PluginKind::Extractor, &["pdf"], 100));

        let found = reg.find_extractor_for_format("pdf").unwrap();
        assert_eq!(found.name, "high");
    }

    #[test]
    fn test_plugins_by_kind() {
        let mut reg = PluginRegistry::new();
        reg.register(make_manifest("ext1", PluginKind::Extractor, &["pdf"], 100));
        reg.register(make_manifest("chunk1", PluginKind::Chunker, &[], 100));
        reg.register(make_manifest("ext2", PluginKind::Extractor, &["docx"], 100));

        let extractors = reg.plugins_by_kind(PluginKind::Extractor);
        assert_eq!(extractors.len(), 2);

        let chunkers = reg.plugins_by_kind(PluginKind::Chunker);
        assert_eq!(chunkers.len(), 1);
    }

    #[test]
    fn test_get_by_name() {
        let mut reg = PluginRegistry::new();
        reg.register(make_manifest(
            "my-plugin",
            PluginKind::Extractor,
            &["pdf"],
            100,
        ));

        assert!(reg.get("my-plugin").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn test_discover_from_directory() {
        let dir = tempdir().unwrap();
        let plugin_dir = dir.path().join("my-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            r#"
[plugin]
name = "discovered"
[plugin.runner]
command = "bash"
[plugin.capabilities]
kind = "extractor"
formats = ["txt"]
"#,
        )
        .unwrap();

        let mut reg = PluginRegistry::new();
        let count = reg.discover(dir.path()).unwrap();
        assert_eq!(count, 1);
        assert!(reg.get("discovered").is_some());
    }

    #[test]
    fn test_discover_skips_invalid() {
        let dir = tempdir().unwrap();

        // Valid plugin
        let valid = dir.path().join("valid");
        std::fs::create_dir_all(&valid).unwrap();
        std::fs::write(
            valid.join("plugin.toml"),
            r#"
[plugin]
name = "valid"
[plugin.runner]
command = "bash"
[plugin.capabilities]
kind = "extractor"
"#,
        )
        .unwrap();

        // Invalid plugin (bad TOML)
        let invalid = dir.path().join("invalid");
        std::fs::create_dir_all(&invalid).unwrap();
        std::fs::write(invalid.join("plugin.toml"), "not valid toml {{{{").unwrap();

        // Directory without plugin.toml
        let no_manifest = dir.path().join("empty");
        std::fs::create_dir_all(&no_manifest).unwrap();

        let mut reg = PluginRegistry::new();
        let count = reg.discover(dir.path()).unwrap();
        assert_eq!(count, 1);
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn test_discover_nonexistent_dir() {
        let mut reg = PluginRegistry::new();
        let count = reg.discover(Path::new("/nonexistent/path")).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_find_url_extractor() {
        let mut reg = PluginRegistry::new();
        reg.register(make_manifest("web", PluginKind::Extractor, &["html"], 100));

        assert!(reg.find_extractor_for_url("https://example.com").is_some());
    }
}
