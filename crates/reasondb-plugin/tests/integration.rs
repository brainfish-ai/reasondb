//! Integration tests for the plugin system.
//!
//! These tests create real temporary plugins and invoke them end-to-end.

use reasondb_plugin::*;
use std::collections::HashMap;
use tempfile::tempdir;

fn write_script(dir: &std::path::Path, name: &str, content: &str) {
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn write_manifest(dir: &std::path::Path, toml_content: &str) {
    std::fs::write(dir.join("plugin.toml"), toml_content).unwrap();
}

// ---------------------------------------------------------------------------
// End-to-end: extractor plugin
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_extractor_plugin() {
    let dir = tempdir().unwrap();
    let plugin_dir = dir.path().join("test-ext");
    std::fs::create_dir_all(&plugin_dir).unwrap();

    write_manifest(
        &plugin_dir,
        r#"
[plugin]
name = "test-extractor"
version = "1.0.0"
description = "Test extractor"
[plugin.runner]
command = "bash"
args = ["extract.sh"]
timeout_secs = 10
[plugin.capabilities]
kind = "extractor"
formats = ["txt", "md"]
handles_urls = false
priority = 150
"#,
    );

    write_script(
        &plugin_dir,
        "extract.sh",
        "#!/bin/bash\nread input\necho '{\"version\":1,\"status\":\"ok\",\"result\":{\"title\":\"Extracted\",\"markdown\":\"Content here\",\"metadata\":{}}}'",
    );

    let mut manager = PluginManager::new(dir.path());
    let count = manager.discover().unwrap();
    assert_eq!(count, 1);
    assert!(manager.has_extractor_for_format("txt"));
    assert!(manager.has_extractor_for_format("md"));
    assert!(!manager.has_extractor_for_format("pdf"));

    let result = manager.extract_file("/tmp/test.txt").unwrap();
    assert_eq!(result.title, "Extracted");
    assert_eq!(result.markdown, "Content here");
}

// ---------------------------------------------------------------------------
// End-to-end: post-processor plugin
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_post_processor_plugin() {
    let dir = tempdir().unwrap();
    let plugin_dir = dir.path().join("test-proc");
    std::fs::create_dir_all(&plugin_dir).unwrap();

    write_manifest(
        &plugin_dir,
        r#"
[plugin]
name = "test-processor"
[plugin.runner]
command = "bash"
args = ["process.sh"]
timeout_secs = 10
[plugin.capabilities]
kind = "post_processor"
priority = 100
"#,
    );

    write_script(
        &plugin_dir,
        "process.sh",
        "#!/bin/bash\nread input\necho '{\"version\":1,\"status\":\"ok\",\"result\":{\"markdown\":\"PROCESSED\",\"metadata\":{}}}'",
    );

    let mut manager = PluginManager::new(dir.path());
    manager.discover().unwrap();

    assert!(manager.has_post_processors());

    let result = manager
        .run_post_processors("original markdown", &HashMap::new())
        .unwrap();
    assert_eq!(result.markdown, "PROCESSED");
}

// ---------------------------------------------------------------------------
// End-to-end: chunker plugin
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_chunker_plugin() {
    let dir = tempdir().unwrap();
    let plugin_dir = dir.path().join("test-chunker");
    std::fs::create_dir_all(&plugin_dir).unwrap();

    write_manifest(
        &plugin_dir,
        r#"
[plugin]
name = "test-chunker"
[plugin.runner]
command = "bash"
args = ["chunk.sh"]
timeout_secs = 10
[plugin.capabilities]
kind = "chunker"
priority = 100
"#,
    );

    write_script(
        &plugin_dir,
        "chunk.sh",
        "#!/bin/bash\nread input\necho '{\"version\":1,\"status\":\"ok\",\"result\":{\"chunks\":[{\"content\":\"chunk1\",\"heading\":\"Intro\",\"level\":1,\"char_count\":6},{\"content\":\"chunk2\",\"char_count\":6,\"level\":2}]}}'",
    );

    let mut manager = PluginManager::new(dir.path());
    manager.discover().unwrap();

    assert!(manager.has_chunker());

    let config = ChunkConfig::default();
    let result = manager.chunk("some markdown", &config).unwrap();
    assert_eq!(result.chunks.len(), 2);
    assert_eq!(result.chunks[0].content, "chunk1");
    assert_eq!(result.chunks[0].heading.as_deref(), Some("Intro"));
    assert_eq!(result.chunks[1].content, "chunk2");
}

// ---------------------------------------------------------------------------
// End-to-end: summarizer plugin
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_summarizer_plugin() {
    let dir = tempdir().unwrap();
    let plugin_dir = dir.path().join("test-summarizer");
    std::fs::create_dir_all(&plugin_dir).unwrap();

    write_manifest(
        &plugin_dir,
        r#"
[plugin]
name = "test-summarizer"
[plugin.runner]
command = "bash"
args = ["summarize.sh"]
timeout_secs = 10
[plugin.capabilities]
kind = "summarizer"
priority = 100
"#,
    );

    write_script(
        &plugin_dir,
        "summarize.sh",
        "#!/bin/bash\nread input\necho '{\"version\":1,\"status\":\"ok\",\"result\":{\"summary\":\"A brief summary\"}}'",
    );

    let mut manager = PluginManager::new(dir.path());
    manager.discover().unwrap();

    assert!(manager.has_summarizer());

    let result = manager
        .summarize("Long content here...", &HashMap::new())
        .unwrap();
    assert_eq!(result.summary, "A brief summary");
}

// ---------------------------------------------------------------------------
// Multiple plugins: priority ordering
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_priority_ordering() {
    let dir = tempdir().unwrap();

    // Low-priority extractor
    let low = dir.path().join("low-pri");
    std::fs::create_dir_all(&low).unwrap();
    write_manifest(
        &low,
        r#"
[plugin]
name = "low-priority"
[plugin.runner]
command = "bash"
args = ["ext.sh"]
[plugin.capabilities]
kind = "extractor"
formats = ["txt"]
priority = 10
"#,
    );
    write_script(
        &low,
        "ext.sh",
        "#!/bin/bash\nread input\necho '{\"version\":1,\"status\":\"ok\",\"result\":{\"title\":\"LOW\",\"markdown\":\"low\",\"metadata\":{}}}'",
    );

    // High-priority extractor
    let high = dir.path().join("high-pri");
    std::fs::create_dir_all(&high).unwrap();
    write_manifest(
        &high,
        r#"
[plugin]
name = "high-priority"
[plugin.runner]
command = "bash"
args = ["ext.sh"]
[plugin.capabilities]
kind = "extractor"
formats = ["txt"]
priority = 200
"#,
    );
    write_script(
        &high,
        "ext.sh",
        "#!/bin/bash\nread input\necho '{\"version\":1,\"status\":\"ok\",\"result\":{\"title\":\"HIGH\",\"markdown\":\"high\",\"metadata\":{}}}'",
    );

    let mut manager = PluginManager::new(dir.path());
    manager.discover().unwrap();
    assert_eq!(manager.plugin_count(), 2);

    let result = manager.extract_file("/tmp/test.txt").unwrap();
    assert_eq!(result.title, "HIGH");
}

// ---------------------------------------------------------------------------
// Disabled plugin system
// ---------------------------------------------------------------------------

#[test]
fn test_disabled_plugin_system() {
    let manager = PluginManager::disabled();

    assert!(!manager.is_enabled());
    assert!(!manager.has_extractor_for_format("pdf"));
    assert!(!manager.has_chunker());
    assert!(!manager.has_summarizer());
    assert!(!manager.has_post_processors());
    assert!(!manager.has_formatter());
    assert_eq!(manager.plugin_count(), 0);
    assert!(manager.list_plugins().is_empty());

    // Operations should return pass-through or errors
    assert!(manager.extract_file("/tmp/test.pdf").is_err());
    assert!(manager.extract_url("https://example.com").is_err());
    assert!(manager.chunk("text", &ChunkConfig::default()).is_err());
    assert!(manager.summarize("text", &HashMap::new()).is_err());

    let pp = manager
        .run_post_processors("markdown", &HashMap::new())
        .unwrap();
    assert_eq!(pp.markdown, "markdown");
}

// ---------------------------------------------------------------------------
// Manifest validation edge cases
// ---------------------------------------------------------------------------

#[test]
fn test_manifest_with_env_vars() {
    let toml = r#"
[plugin]
name = "env-test"
[plugin.runner]
command = "bash"
timeout_secs = 30
[plugin.runner.env]
MY_VAR = "hello"
ANOTHER = "world"
[plugin.capabilities]
kind = "extractor"
formats = ["pdf"]
"#;
    let manifest = PluginManifest::parse(toml, std::path::Path::new("/tmp/plugin.toml")).unwrap();
    assert_eq!(manifest.runner.env.len(), 2);
    assert_eq!(manifest.runner.env.get("MY_VAR").unwrap(), "hello");
}

#[test]
fn test_manifest_all_kinds() {
    for kind_str in &[
        "extractor",
        "post_processor",
        "postprocessor",
        "chunker",
        "summarizer",
        "formatter",
    ] {
        let toml = format!(
            r#"
[plugin]
name = "test-{}"
[plugin.runner]
command = "bash"
[plugin.capabilities]
kind = "{}"
"#,
            kind_str, kind_str
        );
        let result = PluginManifest::parse(&toml, std::path::Path::new("/tmp/plugin.toml"));
        assert!(result.is_ok(), "Failed for kind: {}", kind_str);
    }
}

// ---------------------------------------------------------------------------
// Protocol roundtrip tests
// ---------------------------------------------------------------------------

#[test]
fn test_protocol_full_roundtrip() {
    let request = PluginRequest::extract(ExtractParams {
        source_type: SourceType::File,
        path: Some("/tmp/test.pdf".to_string()),
        url: None,
        config: HashMap::from([("key".to_string(), serde_json::json!("value"))]),
    });

    let json = serde_json::to_string(&request).unwrap();
    let parsed: PluginRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.operation, "extract");
    assert_eq!(parsed.version, PROTOCOL_VERSION);

    let params: ExtractParams = serde_json::from_value(parsed.params).unwrap();
    assert_eq!(params.source_type, SourceType::File);
    assert_eq!(params.path.as_deref(), Some("/tmp/test.pdf"));
}

#[test]
fn test_response_roundtrip() {
    let response = PluginResponse::ok(serde_json::json!({
        "title": "Test",
        "markdown": "content"
    }));
    let json = serde_json::to_string(&response).unwrap();
    let parsed: PluginResponse = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_ok());
    assert_eq!(parsed.result.unwrap()["title"], "Test");

    let error = PluginResponse::err("fail");
    let json = serde_json::to_string(&error).unwrap();
    let parsed: PluginResponse = serde_json::from_str(&json).unwrap();
    assert!(!parsed.is_ok());
    assert_eq!(parsed.error.unwrap(), "fail");
}

// ---------------------------------------------------------------------------
// Real example plugin test (uses the Python extractor example)
// ---------------------------------------------------------------------------

#[test]
fn test_discover_real_plugin_manifests() {
    let plugins_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("plugins");

    if !plugins_dir.exists() {
        return;
    }

    let mut registry = PluginRegistry::new();
    let count = registry.discover(&plugins_dir).unwrap();

    assert!(count >= 1, "Expected at least 1 plugin (markitdown)");
    assert!(registry.get("markitdown").is_some());

    let markitdown = registry.get("markitdown").unwrap();
    assert_eq!(markitdown.capabilities.kind, PluginKind::Extractor);
    assert!(markitdown.handles_format("pdf"));
    assert!(markitdown.handles_format("docx"));
    assert!(markitdown.handles_url("https://example.com"));
}

#[test]
fn test_manager_with_real_plugins_dir() {
    let plugins_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("plugins");

    if !plugins_dir.exists() {
        return;
    }

    let mut manager = PluginManager::new(&plugins_dir);
    let count = manager.discover().unwrap();

    assert!(count >= 1);
    assert!(manager.has_extractor_for_format("pdf"));

    let plugins = manager.list_plugins();
    assert!(!plugins.is_empty());

    let markitdown = manager.get_plugin("markitdown");
    assert!(markitdown.is_some());
}
