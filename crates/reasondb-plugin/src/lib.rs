//! ReasonDB Plugin System
//!
//! Process-based plugin architecture where plugins are external executables
//! communicating via JSON over stdin/stdout. Every pipeline stage (extraction,
//! post-processing, chunking, summarization, formatting) is pluggable.
//!
//! # Architecture
//!
//! ```text
//! Plugin Directory (~/.reasondb/plugins/)
//! ├── markitdown/
//! │   ├── plugin.toml      ← manifest declaring capabilities
//! │   └── wrapper.py       ← executable receiving JSON on stdin
//! ├── pii-redactor/
//! │   ├── plugin.toml
//! │   └── process.js
//! └── ...
//! ```
//!
//! # Protocol
//!
//! Plugins are one-shot: spawned per request, receive a JSON `PluginRequest`
//! on stdin, write a JSON `PluginResponse` to stdout, then exit.

pub mod daemon;
pub mod error;
pub mod manager;
pub mod manifest;
pub mod protocol;
pub mod registry;
pub mod runner;

pub use error::{PluginError, Result};
pub use manager::PluginManager;
pub use manifest::{PluginCapabilities, PluginKind, PluginManifest, RunnerConfig};
pub use protocol::*;
pub use registry::PluginRegistry;
pub use runner::PluginRunner;
