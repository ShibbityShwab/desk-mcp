//! Unified response contract — every tool returns `{ok, result, error}`.
//!
//! Success: `{"ok": true, "result": {...}, "error": null}`
//! Failure: `{"ok": false, "result": null, "error": {"code": "...", "message": "..."}}`

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Standard tool response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ToolError>,
}

/// Structured error info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Build a success response
pub fn ok(value: impl Serialize) -> ToolResponse {
    ToolResponse {
        ok: true,
        result: Some(serde_json::to_value(value).unwrap_or(Value::Null)),
        error: None,
    }
}

/// Build an error response
pub fn err(code: &str, message: &str) -> ToolResponse {
    ToolResponse {
        ok: false,
        result: None,
        error: Some(ToolError {
            code: code.to_string(),
            message: message.to_string(),
            detail: None,
        }),
    }
}

/// Build an error with detail
pub fn err_detail(code: &str, message: &str, detail: &str) -> ToolResponse {
    ToolResponse {
        ok: false,
        result: None,
        error: Some(ToolError {
            code: code.to_string(),
            message: message.to_string(),
            detail: Some(detail.to_string()),
        }),
    }
}

/// Dependency missing error
pub fn dep_missing(tool: &str, dep: &str, hint: &str) -> ToolResponse {
    err_detail(
        "DEPENDENCY_MISSING",
        &format!("{dep} not found. Install: {hint}"),
        &format!("Tool '{tool}' requires {dep}"),
    )
}

/// Not available in this environment
pub fn not_available(tool: &str) -> ToolResponse {
    err_detail(
        "NOT_IMPLEMENTED",
        &format!("'{tool}' is not available in this environment"),
        "",
    )
}

/// Timeout error
pub fn timeout(tool: &str, seconds: f64) -> ToolResponse {
    err(
        "TIMEOUT",
        &format!("'{tool}' timed out after {seconds}s"),
    )
}
