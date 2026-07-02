//! Declarative YAML-based security policy engine.
//!
//! Evaluates every tool call before dispatch. Supports allow/deny lists,
//! confirmation requirements, conditional deny rules (dangerous shell
//! commands, domain allowlists, param filters), capability caps, and
//! session limits.
//!
//! ## Config
//! - `~/.config/desk-mcp/policy.yaml` — user-defined policy
//! - Falls back to a built-in default (permissive: allow reads, confirm writes)
//!
//! ## Evaluation order (single-pass)
//! Per rule, in priority order:
//! 1. Explicit deny — immediate return (always wins)
//! 2. `deny_unless` conditions — immediate deny on failure
//! 3. Capability caps — immediate deny on violation
//! 4. Explicit allow — recorded, later deny in another rule overrides
//! 5. `require_confirmation` — recorded, later deny/allow overrides
//! 6. Default action — when no rule matches

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Policy types
// ---------------------------------------------------------------------------

fn default_version() -> String {
    "1.0".into()
}
fn default_max_actions() -> u32 {
    500
}
fn default_max_duration() -> u32 {
    30
}
fn default_idle_timeout() -> u32 {
    15
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolicyConfig {
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub default: DefaultAction,
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
    #[serde(default)]
    pub session: SessionPolicy,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DefaultAction {
    #[default]
    Allow,
    Deny,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolicyRule {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
    #[serde(default)]
    pub require_confirmation: Vec<String>,
    #[serde(default)]
    pub auto_approve_after: Option<u32>,
    #[serde(default)]
    pub deny_unless: Vec<DenyCondition>,
    #[serde(default)]
    pub cap: Vec<CapRule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DenyCondition {
    pub tool: String,
    pub condition: DenyConditionType,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DenyConditionType {
    /// Deny if the shell command contains any of these substrings.
    CommandNotContains(Vec<String>),
    /// Deny if a named param field matches any of the given values.
    ParamsContains { field: String, values: Vec<String> },
    /// Deny if the browser URL domain is not in this allowlist.
    DomainNotIn(Vec<String>),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CapRule {
    pub tool: String,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub max_duration_secs: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SessionPolicy {
    #[serde(default = "default_max_actions")]
    pub max_actions: u32,
    #[serde(default = "default_max_duration")]
    pub max_duration_minutes: u32,
    #[serde(default = "default_idle_timeout")]
    pub require_reauth_after_idle_minutes: u32,
}

// ---------------------------------------------------------------------------
// Policy decision
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum PolicyDecision {
    Allow,
    Deny {
        reason: String,
    },
    RequireConfirmation {
        message: String,
        params: serde_json::Value,
    },
}

// ---------------------------------------------------------------------------
// Session tracking
// ---------------------------------------------------------------------------

static SESSION_ACTIONS: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(0));
static SESSION_START: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Increment the per-session action counter.
pub fn increment_session_action() {
    if let Ok(mut count) = SESSION_ACTIONS.lock() {
        *count += 1;
    }
}

/// Check session limits (max actions, max duration). Returns `Err(reason)`
/// if a limit is exceeded.
pub fn check_session_limits() -> Result<(), String> {
    check_session_limits_with_config(load_config())
}

/// Check session limits against a specific policy config (for testing).
pub(crate) fn check_session_limits_with_config(config: &PolicyConfig) -> Result<(), String> {

    // Max actions
    let actions = SESSION_ACTIONS.lock().map_err(|e| e.to_string())?;
    if *actions >= config.session.max_actions {
        return Err(format!(
            "Session action limit reached ({} actions). Restart desk-mcp to reset.",
            config.session.max_actions
        ));
    }

    // Max duration
    let elapsed = SESSION_START.elapsed().as_secs() / 60;
    if elapsed >= config.session.max_duration_minutes as u64 {
        return Err(format!(
            "Session duration limit reached ({} min). Restart desk-mcp to reset.",
            config.session.max_duration_minutes
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Config loading (cached)
// ---------------------------------------------------------------------------

static CONFIG: OnceLock<PolicyConfig> = OnceLock::new();

fn config_path() -> PathBuf {
    let mut p = dirs_next::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"));
    p.push("desk-mcp");
    p.push("policy.yaml");
    p
}

/// Load the policy config, caching it for the lifetime of the process.
/// Set `DESKMCP_FORCE_DEFAULT_POLICY=1` to bypass user config and use the
/// built-in default (used by integration tests).
fn load_config() -> &'static PolicyConfig {
    CONFIG.get_or_init(|| {
        if std::env::var("DESKMCP_FORCE_DEFAULT_POLICY").as_deref() == Ok("1") {
            tracing::info!("DESKMCP_FORCE_DEFAULT_POLICY=1 — using built-in default policy.");
            return default_config();
        }
        let path = config_path();
        match std::fs::read_to_string(&path) {
            Ok(yaml) => match serde_yaml::from_str::<PolicyConfig>(&yaml) {
                Ok(cfg) => {
                    tracing::info!("Loaded policy from {}", path.display());
                    cfg
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse {}: {e}. Using default policy.",
                        path.display()
                    );
                    default_config()
                }
            },
            Err(_) => {
                tracing::info!(
                    "No policy file at {} — using built-in default policy.",
                    path.display()
                );
                default_config()
            }
        }
    })
}

/// Built-in default policy: allow reads, confirm writes, block dangerous
/// shell patterns.
pub fn default_config() -> PolicyConfig {
    PolicyConfig {
        version: "1.0".into(),
        default: DefaultAction::Allow,
        rules: vec![
            PolicyRule {
                require_confirmation: vec![
                    "shell_run".into(),
                    "file_write".into(),
                    "file_edit".into(),
                    "code_run".into(),
                    "browser_download".into(),
                ],
                auto_approve_after: Some(5),
                ..Default::default()
            },
            PolicyRule {
                deny_unless: vec![DenyCondition {
                    tool: "shell_run".into(),
                    condition: DenyConditionType::CommandNotContains(vec![
                        "rm -rf".into(),
                        "sudo".into(),
                        "mkfs".into(),
                        "dd if=".into(),
                        "> /dev/".into(),
                        "chmod 777".into(),
                        ":(){ :|:& };:".into(),
                    ]),
                    reason: Some("Dangerous shell command blocked by policy".into()),
                }],
                ..Default::default()
            },
        ],
        session: SessionPolicy {
            max_actions: 500,
            max_duration_minutes: 30,
            require_reauth_after_idle_minutes: 15,
        },
    }
}

/// Re-implemented Default for PolicyRule since we can't derive it
/// (Vec fields need explicit defaults).
impl Default for PolicyRule {
    fn default() -> Self {
        Self {
            allow: Vec::new(),
            deny: Vec::new(),
            require_confirmation: Vec::new(),
            auto_approve_after: None,
            deny_unless: Vec::new(),
            cap: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Policy evaluation
// ---------------------------------------------------------------------------

/// Evaluate a tool call against the loaded policy.
///
/// Returns a `PolicyDecision`:
/// - `Deny` if the tool is explicitly denied or a deny condition matches.
/// - `RequireConfirmation` if the tool needs user approval.
/// - `Allow` otherwise.
pub fn evaluate(tool: &str, args: &serde_json::Value) -> PolicyDecision {
    evaluate_with_config(tool, args, load_config())
}

/// Evaluate a tool call against a specific policy config (for testing).
pub(crate) fn evaluate_with_config(
    tool: &str,
    args: &serde_json::Value,
    config: &PolicyConfig,
) -> PolicyDecision {

    let mut best: Option<PolicyDecision> = None;

    for rule in &config.rules {
        // 1. Explicit deny — immediate return (deny always wins)
        if rule.deny.iter().any(|t| t == tool) {
            return PolicyDecision::Deny {
                reason: format!("Tool '{tool}' is denied by policy."),
            };
        }

        // 2. Deny conditions — immediate return
        for dc in &rule.deny_unless {
            if dc.tool == tool && !check_condition(&dc.condition, args) {
                let reason = dc
                    .reason
                    .clone()
                    .unwrap_or_else(|| format!("Tool '{tool}' denied by conditional rule."));
                return PolicyDecision::Deny { reason };
            }
        }

        // 3. Cap checks — immediate deny if cap fails
        for cap in &rule.cap {
            if cap.tool == tool {
                if let Some(reason) = check_caps(cap, args) {
                    return PolicyDecision::Deny { reason };
                }
            }
        }

        // 4. Explicit allow — record but keep scanning (deny in later rules wins)
        if rule.allow.iter().any(|t| t == tool) && best.is_none() {
            best = Some(PolicyDecision::Allow);
        }

        // 5. Require confirmation — record only if no allow found yet
        if rule.require_confirmation.iter().any(|t| t == tool) && best.is_none() {
            best = Some(PolicyDecision::RequireConfirmation {
                message: format!("Tool '{tool}' requires explicit user confirmation."),
                params: args.clone(),
            });
        }
    }

    // Fall back to best found, then default
    best.unwrap_or_else(|| match config.default {
        DefaultAction::Allow => PolicyDecision::Allow,
        DefaultAction::Deny => PolicyDecision::Deny {
            reason: format!(
                "Tool '{tool}' is not explicitly allowed and the default policy is 'deny'."
            ),
        },
    })
}

/// Return the `auto_approve_after` threshold for a tool, if configured.
///
/// Used by the safety layer to decide whether to auto-approve after N
/// manual approvals.
pub fn auto_approve_threshold(tool: &str) -> Option<u32> {
    let config = load_config();
    for rule in &config.rules {
        if rule.require_confirmation.iter().any(|t| t == tool) {
            return rule.auto_approve_after;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Condition checking helpers
// ---------------------------------------------------------------------------

/// Returns `true` if the condition passes (tool is *allowed* under this
/// condition). Called inside a `deny_unless` check: we deny unless the
/// condition holds.
fn check_condition(cond: &DenyConditionType, args: &serde_json::Value) -> bool {
    match cond {
        DenyConditionType::CommandNotContains(blocked) => {
            // For shell_run: check the "command" field for blocked substrings.
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
            !blocked.iter().any(|b| cmd.contains(b.as_str()))
        }
        DenyConditionType::ParamsContains { field, values } => {
            // Deny if the named field contains any of the listed values.
            let field_val = args
                .get(field.as_str())
                .and_then(|v| v.as_str())
                .unwrap_or("");
            !values.iter().any(|v| field_val == v.as_str())
        }
        DenyConditionType::DomainNotIn(allowed) => {
            // For browser_navigate: extract domain from URL and check allowlist.
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let domain = extract_domain(url);
            if domain.is_empty() {
                // If we can't parse the URL, deny it (fail closed).
                return false;
            }
            allowed
                .iter()
                .any(|a| domain == a.as_str() || domain.ends_with(&format!(".{a}")))
        }
    }
}

/// Crude domain extraction from a URL (no `url` crate needed).
fn extract_domain(url: &str) -> &str {
    let url = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("file://");
    // Take everything before the first '/' or ':' or '?' or '#'
    url.split(['/', ':', '?', '#']).next().unwrap_or("")
}

/// Check capability caps for a tool. Returns `Some(reason)` if the cap is
/// violated, `None` if the call is within bounds.
fn check_caps(cap: &CapRule, args: &serde_json::Value) -> Option<String> {
    // Domain allowlist
    if !cap.domains.is_empty() {
        let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let domain = extract_domain(url);
        if domain.is_empty()
            || !cap
                .domains
                .iter()
                .any(|d| domain == d.as_str() || domain.ends_with(&format!(".{d}")))
        {
            return Some(format!(
                "Domain '{}' is not in the allowed list for this tool.",
                domain
            ));
        }
    }

    // Path allowlist
    if !cap.paths.is_empty() {
        let path = args
            .get("path")
            .or_else(|| args.get("file"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if path.is_empty() || !cap.paths.iter().any(|p| path.starts_with(p.as_str())) {
            return Some(format!(
                "Path '{}' is not in the allowed prefix list for this tool.",
                path
            ));
        }
    }

    // Max duration
    if let Some(max_secs) = cap.max_duration_secs {
        let timeout = args.get("timeout").and_then(|v| v.as_f64()).unwrap_or(30.0);
        if timeout > max_secs as f64 {
            return Some(format!(
                "Timeout {timeout}s exceeds maximum allowed {max_secs}s for this tool."
            ));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: evaluate against the built-in default policy config,
    /// not the user's overridden policy.yaml.
    fn default_eval(tool: &str, args: &serde_json::Value) -> PolicyDecision {
        evaluate_with_config(tool, args, &default_config())
    }

    #[test]
    fn test_default_policy_allows_reads() {
        let decision = default_eval("screenshot", &serde_json::json!({}));
        assert_eq!(decision, PolicyDecision::Allow);
    }

    #[test]
    fn test_default_policy_confirms_writes() {
        let decision = default_eval("shell_run", &serde_json::json!({"command": "ls -la"}));
        assert!(matches!(
            decision,
            PolicyDecision::RequireConfirmation { .. }
        ));

        let decision = default_eval("file_write", &serde_json::json!({"path": "/tmp/test"}));
        assert!(matches!(
            decision,
            PolicyDecision::RequireConfirmation { .. }
        ));
    }

    #[test]
    fn test_default_policy_blocks_dangerous_commands() {
        let decision = default_eval(
            "shell_run",
            &serde_json::json!({"command": "sudo rm -rf /"}),
        );
        assert!(matches!(decision, PolicyDecision::Deny { .. }));

        let decision = default_eval(
            "shell_run",
            &serde_json::json!({"command": "chmod 777 /etc/passwd"}),
        );
        assert!(matches!(decision, PolicyDecision::Deny { .. }));
    }

    #[test]
    fn test_default_policy_allows_safe_commands_but_confirms() {
        let decision = default_eval("shell_run", &serde_json::json!({"command": "cargo build"}));
        // Not in deny_unless blocklist → should be RequireConfirmation
        assert!(matches!(
            decision,
            PolicyDecision::RequireConfirmation { .. }
        ));
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(extract_domain("https://example.com/path"), "example.com");
        assert_eq!(
            extract_domain("http://sub.example.com:8080/x"),
            "sub.example.com"
        );
        assert_eq!(extract_domain("file:///tmp/foo"), "");
        assert_eq!(extract_domain("example.com/path"), "example.com");
    }

    #[test]
    fn test_domain_not_in_condition() {
        let cond = DenyConditionType::DomainNotIn(vec!["example.com".into()]);
        // Allowed domain
        assert!(check_condition(
            &cond,
            &serde_json::json!({"url": "https://example.com/page"})
        ));
        // Subdomain of allowed domain
        assert!(check_condition(
            &cond,
            &serde_json::json!({"url": "https://sub.example.com/page"})
        ));
        // Not allowed
        assert!(!check_condition(
            &cond,
            &serde_json::json!({"url": "https://evil.com"})
        ));
    }

    #[test]
    fn test_params_contains_condition() {
        let cond = DenyConditionType::ParamsContains {
            field: "mode".into(),
            values: vec!["headless".into(), "stealth".into()],
        };
        // Mode not in blocklist
        assert!(check_condition(
            &cond,
            &serde_json::json!({"mode": "desktop"})
        ));
        // Mode in blocklist → condition fails
        assert!(!check_condition(
            &cond,
            &serde_json::json!({"mode": "headless"})
        ));
    }

    #[test]
    fn test_session_limits() {
        // Use built-in default config (not user's overridden policy.yaml)
        let cfg = default_config();

        // After reset, session should be within limits
        *SESSION_ACTIONS.lock().unwrap() = 0;
        assert!(check_session_limits_with_config(&cfg).is_ok());

        // Exceed action limit (default is 500)
        *SESSION_ACTIONS.lock().unwrap() = 500;
        assert!(check_session_limits_with_config(&cfg).is_err());
    }
}
