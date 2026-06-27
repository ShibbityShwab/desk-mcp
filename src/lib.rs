//! DeskMCP — Full Desktop Control MCP Server
//!
//! Gives AI agents full desktop control: screenshots, mouse, keyboard,
//! OCR, browser automation, and code tools — all through a single MCP server.
//!
//! ## Architecture
//! - `providers/` — Pluggable desktop backends (KDE Wayland, headless, etc.)
//! - `tools/` — 50 MCP tools across computer use, browser use, and code mode
//! - `discovery.rs` — Environment detection (cached for performance)
//! - `response.rs` — Unified tool response contract
//! - `ocr.rs` — OCR via tesseract

pub mod providers;
pub mod tools;
pub mod discovery;
pub mod response;
pub mod ocr;
pub mod error;

/// Global provider — initialized once at startup
pub static PROVIDER: std::sync::LazyLock<
    Box<dyn providers::ComputerProvider + Send + Sync>,
> = std::sync::LazyLock::new(|| providers::get_provider());

pub const SERVER_NAME: &str = "desk-mcp";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
