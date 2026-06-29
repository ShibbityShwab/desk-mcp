//! Safety layer — confirmation gating, rate limiting, and action logging.
//!
//! ## Confirmation system
//! Tools flagged as `requires_confirmation` are blocked until the user
//! calls `approve(id)`. The agent uses `request_confirmation(message)` to
//! surface a blocking prompt; the user responds with `approve` or `deny`.
//!
//! ## Rate limiter
//! Per-tool token bucket prevents runaway agent loops. Default: 30
//! actions per minute per tool, burst of 5.
//!
//! ## Action log
//! Every tool invocation is written to
//! `~/.local/share/desk-mcp/actions.log` in JSONL format.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Confirmation
// ---------------------------------------------------------------------------

static PENDING: OnceLock<Mutex<Vec<Confirmation>>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Confirmation {
    pub id: String,
    pub tool: String,
    pub message: String,
    pub params: serde_json::Value,
    pub created: chrono::DateTime<chrono::Utc>,
}

fn pending() -> &'static Mutex<Vec<Confirmation>> {
    PENDING.get_or_init(|| Mutex::new(Vec::new()))
}

/// Register a tool invocation as requiring confirmation. Returns a
/// confirmation id the user must approve/deny.
pub fn request(tool: &str, message: &str, params: &serde_json::Value) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let conf = Confirmation {
        id: id.clone(),
        tool: tool.to_string(),
        message: message.to_string(),
        params: params.clone(),
        created: chrono::Utc::now(),
    };
    if let Ok(mut v) = pending().lock() {
        v.push(conf);
    }
    id
}

/// Approve a pending confirmation. Returns `Ok(())` if approved,
/// `Err(...)` if the id was not found.
///
/// Also increments the per-tool manual approval counter so that
/// `auto_approve_after` can take effect.
pub fn approve(id: &str) -> Result<(), String> {
    let mut v = pending().lock().map_err(|e| e.to_string())?;
    let pos = v.iter().position(|c| c.id == id);
    match pos {
        Some(i) => {
            let tool = v[i].tool.clone();
            v.remove(i);
            // Increment manual approval counter for this tool
            if let Ok(mut map) = MANUAL_APPROVALS.lock() {
                *map.entry(tool).or_insert(0) += 1;
            }
            Ok(())
        }
        None => Err(format!("no pending confirmation with id {id}")),
    }
}

/// Deny a pending confirmation.
pub fn deny(id: &str, _reason: &str) -> Result<(), String> {
    let mut v = pending().lock().map_err(|e| e.to_string())?;
    let pos = v.iter().position(|c| c.id == id);
    match pos {
        Some(i) => {
            v.remove(i);
            Ok(())
        }
        None => Err(format!("no pending confirmation with id {id}")),
    }
}

/// List all pending confirmations (for the `list_pending` tool).
pub fn list_pending() -> Vec<Confirmation> {
    pending().lock().map(|v| v.clone()).unwrap_or_default()
}

/// Returns `true` if the given tool is currently gated behind a pending
/// confirmation. The agent should call `request_confirmation` first, get
/// the id, then wait for the user to approve.
pub fn is_gated(tool: &str) -> bool {
    // Tools that always require confirmation before executing.
    matches!(
        tool,
        "shell_run"
            | "file_write"
            | "file_edit"
            | "code_run"
            | "code_build"
            | "browser_download"
            | "mouse_click"
            | "keyboard_type"
            | "open_app"
    )
}

// ---------------------------------------------------------------------------
// Auto-approval tracking (per-tool manual approval counters)
// ---------------------------------------------------------------------------

static MANUAL_APPROVALS: LazyLock<Mutex<HashMap<String, u32>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Returns `true` if the tool has been manually approved enough times
/// to qualify for auto-approval (per the `auto_approve_after` policy).
pub fn is_approved_for_session(tool: &str, _params: &serde_json::Value) -> bool {
    let threshold = match crate::policy::auto_approve_threshold(tool) {
        Some(n) => n,
        None => return false,
    };
    let map = MANUAL_APPROVALS.lock().unwrap_or_else(|e| e.into_inner());
    let count = map.get(tool).copied().unwrap_or(0);
    count >= threshold
}

// ---------------------------------------------------------------------------
// Rate limiter — token bucket per tool
// ---------------------------------------------------------------------------

const MAX_PER_MINUTE: u32 = 30;
const BURST: u32 = 5;

static RATE_STATE: OnceLock<Mutex<HashMap<String, RateBucket>>> = OnceLock::new();

#[derive(Debug)]
struct RateBucket {
    tokens: f64,
    last_refill: Instant,
}

fn rate_state() -> &'static Mutex<HashMap<String, RateBucket>> {
    RATE_STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns `true` if the action is allowed by the rate limiter.
/// Consumes one token on success.
pub fn check_rate(tool: &str) -> bool {
    let mut map = rate_state().lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    let bucket = map.entry(tool.to_string()).or_insert(RateBucket {
        tokens: BURST as f64,
        last_refill: now,
    });

    // Refill tokens at 30/min = 0.5/sec
    let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
    bucket.tokens = (bucket.tokens + elapsed * (MAX_PER_MINUTE as f64 / 60.0)).min(BURST as f64);
    bucket.last_refill = now;

    if bucket.tokens >= 1.0 {
        bucket.tokens -= 1.0;
        true
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// Action log
// ---------------------------------------------------------------------------

fn log_path() -> PathBuf {
    let mut p = dirs_next::data_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    p.push("desk-mcp");
    p.push("actions.log");
    p
}

/// Record a tool invocation to the action log (JSONL).
pub fn log_action(tool: &str, params: &serde_json::Value, success: bool, summary: &str) {
    let entry = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "tool": tool,
        "params": params,
        "success": success,
        "summary": summary,
    });

    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{entry}");
    }
}
