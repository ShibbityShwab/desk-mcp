//! DeskMCP — Full Desktop Control MCP Server
//!
//! Gives AI agents full desktop control: screenshots, mouse, keyboard,
//! OCR, browser automation, and code tools — all through a single MCP server.
//!
//! ## Architecture
//! - `providers/` — Pluggable desktop backends (KDE Wayland, headless, etc.)
//! - `tools/` — 61 MCP tools across computer use, browser use, code mode, a11y, safety, and recipes
//! - `safety.rs` — Confirmation, rate limiting, action logging
//! - `vision.rs` — Screen state analysis, clickable region detection
//! - `discovery.rs` — Environment detection (cached for performance)
//! - `response.rs` — Unified tool response contract
//! - `ocr.rs` — OCR via ocrs (pure Rust)
//! - `error.rs` — Error types
//! - `transport.rs` — JSON-RPC dispatch + HTTP/SSE server

pub mod a11y;
pub mod audit;
pub mod auth;
pub mod dashboard;
pub mod discovery;
pub mod error;
pub mod ocr;
pub mod plugin;
pub mod policy;
pub mod providers;
pub mod recipes;
pub mod record;
pub mod resolution;
pub mod response;
pub mod safety;
pub mod session;
pub mod tools;
pub mod transport;
pub mod vision;

/// Global provider — initialized once at startup
pub static PROVIDER: std::sync::LazyLock<Box<dyn providers::ComputerProvider + Send + Sync>> =
    std::sync::LazyLock::new(|| providers::get_provider());

pub const SERVER_NAME: &str = "desk-mcp";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
