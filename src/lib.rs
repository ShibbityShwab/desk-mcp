//! AionUI Unified MCP Server — Full Computer Use + Browser Use
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
pub const SERVER_NAME: &str = "aionui-unified";
pub const SERVER_VERSION: &str = "0.1.0";
