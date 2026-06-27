//! DeskMCP — Full Agentic Desktop Control MCP Server for Linux
//!
//! Single MCP server providing agentic control of a Linux system.
//! CPU-only, dual-mode: personal desktop + headless server.
//!
//! ## Architecture
//! - `providers/`  → Platform backends (KDE Wayland, headless)
//! - `discovery.rs` → Auto-detects environment at startup
//! - `tools/`       → All 42 tool implementations
//! - `response.rs`  → Unified `{ok, result, error}` contract

pub mod discovery;
pub mod ocr;
pub mod providers;
pub mod response;
pub mod tools;

use std::sync::LazyLock;
use providers::ComputerProvider;

/// Global provider instance — initialized lazily on first use
pub static PROVIDER: LazyLock<Box<dyn ComputerProvider + Send + Sync>> =
    LazyLock::new(|| providers::get_provider());

/// Global MCP server name
pub const SERVER_NAME: &str = "desk-mcp";
pub const SERVER_VERSION: &str = "0.1.0";
