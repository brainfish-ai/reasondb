//! Plugin process runner
//!
//! Spawns a plugin as a child process, sends a JSON request on stdin,
//! reads the JSON response from stdout, and enforces a timeout.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use tracing::{debug, warn};

use crate::daemon::invoke_daemon;
use crate::error::{PluginError, Result};
use crate::manifest::PluginManifest;
use crate::protocol::{PluginRequest, PluginResponse};

pub struct PluginRunner;

impl PluginRunner {
    /// Invoke a plugin with the given request.
    ///
    /// If the manifest has `daemon = true`, the request is routed to a
    /// persistent process via the global daemon pool (eliminating per-call
    /// cold-start overhead). Otherwise a new subprocess is spawned per request.
    pub fn invoke(manifest: &PluginManifest, request: &PluginRequest) -> Result<PluginResponse> {
        if manifest.runner.daemon {
            return invoke_daemon(manifest, request);
        }
        Self::invoke_subprocess(manifest, request)
    }

    /// Spawn a fresh subprocess for this single request.
    fn invoke_subprocess(
        manifest: &PluginManifest,
        request: &PluginRequest,
    ) -> Result<PluginResponse> {
        let timeout = Duration::from_secs(manifest.runner.timeout_secs);

        let request_json =
            serde_json::to_string(request).map_err(|e| PluginError::Protocol(e.to_string()))?;

        debug!(
            plugin = %manifest.name,
            operation = %request.operation,
            "Invoking plugin"
        );

        let mut cmd = Command::new(&manifest.runner.command);

        for arg in &manifest.runner.args {
            cmd.arg(arg);
        }

        // Set working directory to plugin dir
        cmd.current_dir(&manifest.dir);

        // Inject plugin-specific env vars
        for (key, value) in &manifest.runner.env {
            cmd.env(key, value);
        }

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                PluginError::Invocation(format!(
                    "Plugin '{}' command not found: {}",
                    manifest.name, manifest.runner.command
                ))
            } else {
                PluginError::Invocation(format!(
                    "Failed to spawn plugin '{}': {}",
                    manifest.name, e
                ))
            }
        })?;

        // Write request to stdin and close it (signals EOF to the plugin).
        if let Some(mut stdin) = child.stdin.take() {
            // Ignore broken-pipe errors: the child may have already exited before
            // reading stdin (e.g. due to a startup error).  We still want to fall
            // through and inspect the exit status, which surfaces the real error.
            if let Err(e) = stdin.write_all(request_json.as_bytes()) {
                if e.kind() != std::io::ErrorKind::BrokenPipe {
                    return Err(PluginError::Invocation(format!(
                        "Failed to write to plugin stdin: {}",
                        e
                    )));
                }
                warn!(plugin = %manifest.name, "Broken pipe writing to plugin stdin (child likely exited early)");
            }
            // stdin dropped here → EOF
        }

        // Use a background thread to call wait_with_output(), which reads stdout/stderr
        // into memory while waiting for the child to exit.
        //
        // The old try_wait() + wait_with_output() loop caused a pipe deadlock:
        // if the plugin writes more than the OS pipe buffer (~64 KB) to stdout,
        // the child blocks waiting for the parent to read, while the parent blocks
        // waiting for the child to exit — neither side makes progress.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            tx.send(child.wait_with_output()).ok();
        });

        let output = match rx.recv_timeout(timeout) {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => {
                return Err(PluginError::Invocation(format!(
                    "Failed to read plugin output: {}",
                    e
                )));
            }
            Err(_elapsed) => {
                // Child was moved into the thread; the process will be cleaned up
                // when the thread eventually unblocks or the container restarts.
                return Err(PluginError::Timeout(manifest.runner.timeout_secs));
            }
        };

        // Log stderr if non-empty
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            warn!(
                plugin = %manifest.name,
                stderr = %stderr.trim(),
                "Plugin stderr output"
            );
        }

        if !output.status.success() {
            return Err(PluginError::Invocation(format!(
                "Plugin '{}' exited with status {}: {}",
                manifest.name,
                output.status,
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            return Err(PluginError::Protocol(format!(
                "Plugin '{}' returned empty stdout",
                manifest.name
            )));
        }

        let response: PluginResponse = serde_json::from_str(stdout.trim()).map_err(|e| {
            PluginError::Protocol(format!(
                "Plugin '{}' returned invalid JSON: {} (output: {})",
                manifest.name,
                e,
                &stdout[..stdout.len().min(200)]
            ))
        })?;

        debug!(
            plugin = %manifest.name,
            status = ?response.status,
            "Plugin invocation complete"
        );

        if !response.is_ok() {
            return Err(PluginError::PluginResponse(
                response
                    .error
                    .unwrap_or_else(|| "Unknown plugin error".to_string()),
            ));
        }

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::RunnerConfig;
    use crate::protocol::{ExtractParams, SourceType};
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn create_script_plugin(dir: &std::path::Path, script: &str) -> PluginManifest {
        let script_path = dir.join("plugin.sh");
        std::fs::write(&script_path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        PluginManifest {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: String::new(),
            author: String::new(),
            license: String::new(),
            runner: RunnerConfig {
                command: "bash".to_string(),
                args: vec!["plugin.sh".to_string()],
                timeout_secs: 5,
                env: HashMap::new(),
                daemon: false,
            },
            capabilities: crate::manifest::PluginCapabilities {
                kind: crate::manifest::PluginKind::Extractor,
                formats: vec!["pdf".to_string()],
                handles_urls: false,
                url_patterns: vec![],
                priority: 100,
            },
            dir: dir.to_path_buf(),
        }
    }

    #[test]
    fn test_invoke_simple_plugin() {
        let dir = tempdir().unwrap();
        let script = "#!/bin/bash\nread input\necho '{\"version\":1,\"status\":\"ok\",\"result\":{\"title\":\"Test\",\"markdown\":\"Hello World\"}}'";
        let manifest = create_script_plugin(dir.path(), script);

        let request = PluginRequest::extract(ExtractParams {
            source_type: SourceType::File,
            path: Some("/tmp/test.pdf".to_string()),
            url: None,
            config: HashMap::new(),
        });

        let response = PluginRunner::invoke(&manifest, &request).unwrap();
        assert!(response.is_ok());
        let result = response.result.unwrap();
        assert_eq!(result["title"], "Test");
        assert_eq!(result["markdown"], "Hello World");
    }

    #[test]
    fn test_invoke_plugin_error_response() {
        let dir = tempdir().unwrap();
        let manifest = create_script_plugin(
            dir.path(),
            r#"#!/bin/bash
read input
echo '{"version":1,"status":"error","error":"File not supported"}'
"#,
        );

        let request = PluginRequest::extract(ExtractParams {
            source_type: SourceType::File,
            path: Some("/tmp/bad.xyz".to_string()),
            url: None,
            config: HashMap::new(),
        });

        let err = PluginRunner::invoke(&manifest, &request).unwrap_err();
        assert!(err.to_string().contains("File not supported"));
    }

    #[test]
    fn test_invoke_plugin_bad_json() {
        let dir = tempdir().unwrap();
        let manifest = create_script_plugin(
            dir.path(),
            r#"#!/bin/bash
read input
echo 'not json'
"#,
        );

        let request = PluginRequest::extract(ExtractParams {
            source_type: SourceType::File,
            path: Some("/tmp/test.pdf".to_string()),
            url: None,
            config: HashMap::new(),
        });

        let err = PluginRunner::invoke(&manifest, &request).unwrap_err();
        assert!(err.to_string().contains("invalid JSON"));
    }

    #[test]
    fn test_invoke_plugin_not_found() {
        let dir = tempdir().unwrap();
        let manifest = PluginManifest {
            name: "missing".to_string(),
            version: String::new(),
            description: String::new(),
            author: String::new(),
            license: String::new(),
            runner: RunnerConfig {
                command: "nonexistent_binary_12345".to_string(),
                args: vec![],
                timeout_secs: 5,
                env: HashMap::new(),
                daemon: false,
            },
            capabilities: crate::manifest::PluginCapabilities {
                kind: crate::manifest::PluginKind::Extractor,
                formats: vec![],
                handles_urls: false,
                url_patterns: vec![],
                priority: 100,
            },
            dir: dir.path().to_path_buf(),
        };

        let request = PluginRequest::extract(ExtractParams {
            source_type: SourceType::File,
            path: None,
            url: None,
            config: HashMap::new(),
        });

        let err = PluginRunner::invoke(&manifest, &request).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_invoke_plugin_nonzero_exit() {
        let dir = tempdir().unwrap();
        let manifest = create_script_plugin(
            dir.path(),
            r#"#!/bin/bash
echo "crash" >&2
exit 1
"#,
        );

        let request = PluginRequest::extract(ExtractParams {
            source_type: SourceType::File,
            path: None,
            url: None,
            config: HashMap::new(),
        });

        let err = PluginRunner::invoke(&manifest, &request).unwrap_err();
        assert!(err.to_string().contains("exited with status"));
    }
}
