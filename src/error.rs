//! Typed error handling for desk-mcp.
//!
//! Replaces ad-hoc `Result<T, String>` usage with structured errors
//! that preserve context and compose properly.

use thiserror::Error;

/// Top-level error enum for desk-mcp operations.
#[derive(Debug, Error)]
pub enum McpError {
    /// Dependency not installed (tesseract, ydotool, etc.)
    #[error("{tool} requires {dep}: {hint}")]
    DependencyMissing {
        tool: String,
        dep: String,
        hint: String,
    },

    /// Feature not available in this environment
    #[error("'{tool}' is not available in this environment")]
    NotAvailable { tool: String },

    /// Operation timed out
    #[error("'{tool}' timed out after {seconds}s")]
    Timeout { tool: String, seconds: f64 },

    /// Browser not launched
    #[error("Browser not launched. Call browser_launch first.")]
    BrowserNotLaunched,

    /// Browser launch failed
    #[error("Browser launch failed: {0}")]
    BrowserLaunchFailed(String),

    /// IO error with context
    #[error("IO error on '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// File operation error
    #[error("Cannot {op} '{path}': {detail}")]
    FileOp {
        op: String,
        path: String,
        detail: String,
    },

    /// Path outside workspace
    #[error("Path '{path}' is outside workspace root '{root}'")]
    PathOutsideWorkspace { path: String, root: String },

    /// Shell execution not allowed
    #[error("ALLOW_SHELL=1 is required for shell commands")]
    ShellNotAllowed,

    /// Code execution not allowed
    #[error("ALLOW_CODE=1 is required for code execution")]
    CodeNotAllowed,

    /// Unknown tool name
    #[error("No tool named '{name}'")]
    UnknownTool { name: String },

    /// Generic tool error
    #[error("{0}")]
    ToolError(String),

    /// JSON parse error
    #[error("Invalid JSON: {0}")]
    JsonError(#[from] serde_json::Error),
}

impl McpError {
    /// Get the error code for this error
    pub fn code(&self) -> &'static str {
        match self {
            McpError::DependencyMissing { .. } => "DEPENDENCY_MISSING",
            McpError::NotAvailable { .. } => "NOT_IMPLEMENTED",
            McpError::Timeout { .. } => "TIMEOUT",
            McpError::BrowserNotLaunched => "BROWSER_NOT_LAUNCHED",
            McpError::BrowserLaunchFailed(_) => "BROWSER_ERROR",
            McpError::Io { .. } => "IO_ERROR",
            McpError::FileOp { .. } => "FILE_ERROR",
            McpError::PathOutsideWorkspace { .. } => "PATH_ERROR",
            McpError::ShellNotAllowed => "SHELL_NOT_ALLOWED",
            McpError::CodeNotAllowed => "CODE_NOT_ALLOWED",
            McpError::UnknownTool { .. } => "UNKNOWN_TOOL",
            McpError::ToolError(_) => "TOOL_ERROR",
            McpError::JsonError(_) => "JSON_ERROR",
        }
    }
}

/// Convenience: Convert McpError into a response-friendly error tuple
impl From<McpError> for (String, String) {
    fn from(e: McpError) -> (String, String) {
        (e.code().to_string(), e.to_string())
    }
}

/// Convenience: Convert anyhow::Error into McpError
impl From<anyhow::Error> for McpError {
    fn from(e: anyhow::Error) -> Self {
        McpError::ToolError(e.to_string())
    }
}

/// Convenience: Convert std::io::Error (without path) into McpError
impl From<std::io::Error> for McpError {
    fn from(e: std::io::Error) -> Self {
        McpError::ToolError(e.to_string())
    }
}
