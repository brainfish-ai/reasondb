//! Persistent plugin daemon pool
//!
//! When a plugin manifest declares `daemon = true` in its `[plugin.runner]`
//! section, the plugin process is started once and kept alive between
//! invocations instead of being re-spawned per request.
//!
//! # Protocol
//!
//! The daemon plugin reads newline-delimited JSON from stdin and writes
//! one newline-delimited JSON response per request to stdout:
//!
//! ```python
//! import sys, json
//!
//! for line in sys.stdin:
//!     line = line.strip()
//!     if not line:
//!         continue
//!     req = json.loads(line)
//!     # ... process ...
//!     print(json.dumps(result), flush=True)
//! ```
//!
//! # Timeout handling
//!
//! If the caller's timeout fires before the daemon responds, the daemon
//! process is killed and restarted on the next invocation to restore
//! protocol consistency (avoids mismatched request/response pairs).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::error::{PluginError, Result};
use crate::manifest::PluginManifest;
use crate::protocol::{PluginRequest, PluginResponse};

// ---------------------------------------------------------------------------
// Global daemon pool
// ---------------------------------------------------------------------------

type RequestPayload = (PluginRequest, mpsc::Sender<Result<PluginResponse>>);

/// A handle to a single persistent daemon process.
///
/// Internally the process is owned by a background thread. Callers communicate
/// via a synchronous channel: one request in, one response out.
pub struct DaemonHandle {
    /// Send a (request, reply_channel) pair to the daemon thread.
    request_tx: mpsc::SyncSender<RequestPayload>,
}

impl DaemonHandle {
    /// Start a background thread that owns the daemon process.
    fn start(manifest: &PluginManifest) -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel::<RequestPayload>(0); // rendezvous: one in-flight at a time

        let manifest = manifest.clone();
        let plugin_name = manifest.name.clone();
        std::thread::Builder::new()
            .name(format!("plugin-daemon-{}", plugin_name))
            .spawn(move || {
                daemon_thread(&manifest, rx);
            })
            .map_err(|e| {
                PluginError::Invocation(format!(
                    "Failed to spawn daemon thread for '{}': {}",
                    plugin_name, e
                ))
            })?;

        Ok(Self { request_tx: tx })
    }

    /// Send a request to the daemon and wait for a response (with timeout).
    pub fn invoke(&self, request: PluginRequest, timeout: Duration) -> Result<PluginResponse> {
        let (reply_tx, reply_rx) = mpsc::channel();

        self.request_tx.send((request, reply_tx)).map_err(|_| {
            PluginError::Invocation("Plugin daemon thread has shut down".to_string())
        })?;

        reply_rx.recv_timeout(timeout).map_err(|_| {
            // Timeout: the process will be killed by the daemon thread (it detects the
            // dropped reply_tx) and restarted on the next invocation.
            PluginError::Timeout(timeout.as_secs())
        })?
    }
}

/// Background thread: owns one plugin process, restarts it on failure.
fn daemon_thread(manifest: &PluginManifest, rx: mpsc::Receiver<RequestPayload>) {
    info!(plugin = %manifest.name, "Plugin daemon thread started");

    loop {
        // Attempt to start (or restart) the plugin process.
        let process = match spawn_daemon_process(manifest) {
            Ok(p) => p,
            Err(e) => {
                warn!(plugin = %manifest.name, error = %e, "Failed to start plugin daemon; giving up");
                // Drain remaining requests with an error so callers don't block.
                for (_, reply_tx) in rx {
                    reply_tx
                        .send(Err(PluginError::Invocation(format!(
                            "Plugin daemon '{}' failed to start: {}",
                            manifest.name, e
                        ))))
                        .ok();
                }
                return;
            }
        };

        info!(plugin = %manifest.name, pid = process.child_pid, "Plugin daemon process started");

        let mut state = process;

        // Process requests until the daemon dies or the channel closes.
        loop {
            let (request, reply_tx) = match rx.recv() {
                Ok(payload) => payload,
                Err(_) => {
                    // Channel closed — server is shutting down.
                    info!(plugin = %manifest.name, "Plugin daemon channel closed; shutting down");
                    let _ = state.kill();
                    return;
                }
            };

            let result = invoke_via_daemon(&mut state, &request);
            let errored = result.is_err();

            if reply_tx.send(result).is_err() {
                // Caller timed out (reply channel was dropped). Kill the process
                // to reset protocol state before the next request arrives.
                warn!(
                    plugin = %manifest.name,
                    "Caller timed out; killing daemon process to reset protocol state"
                );
                let _ = state.kill();
                break; // restart loop
            }

            if errored {
                warn!(plugin = %manifest.name, "Daemon invocation error; restarting process");
                let _ = state.kill();
                break; // restart loop
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Process lifetime
// ---------------------------------------------------------------------------

struct DaemonProcess {
    child: Child,
    child_pid: u32,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl DaemonProcess {
    fn kill(&mut self) -> std::io::Result<()> {
        self.child.kill()
    }
}

fn spawn_daemon_process(manifest: &PluginManifest) -> Result<DaemonProcess> {
    let mut cmd = Command::new(&manifest.runner.command);

    for arg in &manifest.runner.args {
        cmd.arg(arg);
    }

    cmd.current_dir(&manifest.dir);

    for (key, value) in &manifest.runner.env {
        cmd.env(key, value);
    }

    // Signal daemon mode so the plugin script can enter its request loop.
    cmd.env("REASONDB_DAEMON", "1");

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit()); // let plugin log to server stderr

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            PluginError::Invocation(format!(
                "Plugin '{}' command not found: {}",
                manifest.name, manifest.runner.command
            ))
        } else {
            PluginError::Invocation(format!(
                "Failed to spawn daemon process for '{}': {}",
                manifest.name, e
            ))
        }
    })?;

    let pid = child.id();
    let stdin = BufWriter::new(
        child
            .stdin
            .take()
            .ok_or_else(|| PluginError::Invocation("No stdin on daemon process".to_string()))?,
    );
    let stdout = BufReader::new(
        child
            .stdout
            .take()
            .ok_or_else(|| PluginError::Invocation("No stdout on daemon process".to_string()))?,
    );

    Ok(DaemonProcess {
        child,
        child_pid: pid,
        stdin,
        stdout,
    })
}

// ---------------------------------------------------------------------------
// Single request/response over the persistent process
// ---------------------------------------------------------------------------

fn invoke_via_daemon(state: &mut DaemonProcess, request: &PluginRequest) -> Result<PluginResponse> {
    let line = serde_json::to_string(request)
        .map_err(|e| PluginError::Protocol(format!("Failed to serialize request: {}", e)))?;

    debug!(operation = %request.operation, "Sending request to daemon");

    // Write one JSON line to stdin.
    state
        .stdin
        .write_all(line.as_bytes())
        .and_then(|_| state.stdin.write_all(b"\n"))
        .and_then(|_| state.stdin.flush())
        .map_err(|e| PluginError::Invocation(format!("Failed to write to daemon stdin: {}", e)))?;

    // Read one JSON line from stdout.
    let mut response_line = String::new();
    state
        .stdout
        .read_line(&mut response_line)
        .map_err(|e| PluginError::Invocation(format!("Failed to read daemon response: {}", e)))?;

    if response_line.is_empty() {
        return Err(PluginError::Invocation(
            "Daemon process closed stdout (likely crashed)".to_string(),
        ));
    }

    let trimmed = response_line.trim();
    let response: PluginResponse = serde_json::from_str(trimmed).map_err(|e| {
        PluginError::Protocol(format!(
            "Daemon returned invalid JSON: {} (output: {})",
            e,
            &trimmed[..trimmed.len().min(200)]
        ))
    })?;

    if !response.is_ok() {
        return Err(PluginError::PluginResponse(
            response
                .error
                .unwrap_or_else(|| "Unknown plugin error".to_string()),
        ));
    }

    Ok(response)
}

// ---------------------------------------------------------------------------
// Global pool
// ---------------------------------------------------------------------------

static POOL: OnceLock<Mutex<HashMap<String, Arc<DaemonHandle>>>> = OnceLock::new();

fn global_pool() -> &'static Mutex<HashMap<String, Arc<DaemonHandle>>> {
    POOL.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Invoke a daemon-capable plugin, starting the daemon process on first use.
///
/// The daemon is identified by its plugin name. The first call starts a
/// background thread and process; subsequent calls reuse the same handle.
pub fn invoke_daemon(manifest: &PluginManifest, request: &PluginRequest) -> Result<PluginResponse> {
    let timeout = Duration::from_secs(manifest.runner.timeout_secs);
    let key = manifest.name.clone();

    // Fast path: handle already exists.
    {
        let pool = global_pool()
            .lock()
            .map_err(|_| PluginError::Invocation("Daemon pool lock poisoned".to_string()))?;
        if let Some(handle) = pool.get(&key) {
            let h = handle.clone();
            drop(pool); // release lock before blocking on invoke
            return h.invoke(request.clone(), timeout);
        }
    }

    // Slow path: start a new daemon.
    let handle = Arc::new(DaemonHandle::start(manifest)?);
    {
        let mut pool = global_pool()
            .lock()
            .map_err(|_| PluginError::Invocation("Daemon pool lock poisoned".to_string()))?;
        // Insert only if another thread hasn't already done it.
        let handle = pool.entry(key).or_insert_with(|| handle).clone();
        drop(pool);
        handle.invoke(request.clone(), timeout)
    }
}
