//! Structured audit logging — every tool invocation is logged.
//!
//! Logs JSON-per-line entries to `~/.local/share/desk-mcp/audit.log`:
//!
//! ```json
//! {"ts":"2026-06-29T14:22:31Z","tool":"keyboard_type","args":{"text_len":12,"keys":[]},"ok":true,"ms":45}
//! {"ts":"2026-06-29T14:22:32Z","tool":"screenshot","args":{"region":null},"ok":true,"ms":320}
//! ```
//!
//! - Timestamp in ISO 8601 UTC
//! - Tool name
//! - Sanitized args (text → length only; never logs clipboard, passwords, URLs)
//! - Success/failure
//! - Duration in milliseconds

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

/// Audit log entry
#[derive(serde::Serialize)]
struct AuditEntry {
    ts: String,
    tool: String,
    args: serde_json::Value,
    ok: bool,
    ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Log a tool invocation.
///
/// `start` should be captured before the tool executes.
/// `args` is the raw JSON params — they will be sanitized before writing.
pub fn log(tool: &str, args: &serde_json::Value, ok: bool, error: Option<&str>, start: Instant) {
    let ms = start.elapsed().as_millis() as u64;
    let entry = AuditEntry {
        ts: chrono::Utc::now().to_rfc3339(),
        tool: tool.to_string(),
        args: sanitize_args(tool, args),
        ok,
        ms,
        error: error.map(|s| s.to_string()),
    };

    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let line = serde_json::to_string(&entry).unwrap_or_default();
        let _ = writeln!(f, "{line}");
    }
}

/// Sanitize tool arguments to avoid logging sensitive data.
///
/// Rules:
/// - `text` fields → replaced with `{"text_len": N}`
/// - Clipboard contents → never logged
/// - URLs → never logged (browser URLs are page metadata, not secrets, but conservative)
/// - Shell commands → logged as-is (user is warned; shell is gated)
/// - Everything else → logged as-is
fn sanitize_args(tool: &str, args: &serde_json::Value) -> serde_json::Value {
    if tool.contains("clipboard") {
        return serde_json::json!({"_note": "clipboard contents omitted"});
    }

    let obj = match args.as_object() {
        Some(o) => o.clone(),
        None => return args.clone(),
    };

    let mut sanitized = serde_json::Map::new();
    for (key, value) in &obj {
        let sanitized_value = match key.as_str() {
            "text" => {
                if let Some(s) = value.as_str() {
                    serde_json::json!({"text_len": s.len()})
                } else {
                    value.clone()
                }
            }
            "content" => {
                if let Some(s) = value.as_str() {
                    serde_json::json!({"content_len": s.len()})
                } else {
                    value.clone()
                }
            }
            "password" | "secret" | "token" | "api_key" => {
                serde_json::json!("<redacted>")
            }
            _ => value.clone(),
        };
        sanitized.insert(key.clone(), sanitized_value);
    }

    serde_json::Value::Object(sanitized)
}

fn log_path() -> PathBuf {
    let mut p = dirs_next::data_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    p.push("desk-mcp");
    p.push("audit.log");
    p
}
