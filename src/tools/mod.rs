//! Tool registry — defines all 61 MCP tools with JSON schemas.
//!
//! Each tool is registered with a name, description, and JSON Schema input spec.
//! The `dispatch` function routes tool calls to the appropriate handler.

pub mod a11y;
pub mod browser;
pub mod browser_cdp;
pub mod code;
pub mod computer;

use crate::response::ToolResponse;
use serde::Serialize;
use std::sync::atomic::AtomicBool;

/// Recipe recursion guard — prevents recipe→recipe nesting.
static RECIPE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// MCP tool definition
#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// Generate the full tool list (61 tools)
pub fn all_tools() -> Vec<ToolDef> {
    let mut tools = Vec::new();

    let mut t = |name: &str, desc: &str, schema_json: &str| {
        tools.push(ToolDef {
            name: name.into(),
            description: desc.into(),
            input_schema: serde_json::from_str(schema_json).unwrap_or_else(|e| {
                panic!("Invalid JSON schema for tool '{name}': {e}");
            }),
        });
    };

    // ═══════════════ COMPUTER USE (24 tools) ═══════════════
    t(
        "screenshot",
        "Capture screen or region. Returns base64 PNG image.",
        r#"{"type":"object","properties":{"region":{"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4,"description":"[x,y,w,h] region to capture"}}}"#,
    );

    t(
        "get_screen_size",
        "Get the primary display dimensions in physical pixels.",
        r#"{"type":"object","properties":{}}"#,
    );

    t(
        "mouse_move",
        "Move cursor to (x,y) in screen pixels.",
        r#"{"type":"object","required":["x","y"],"properties":{"x":{"type":"integer"},"y":{"type":"integer"},"smooth":{"type":"boolean","default":false},"duration_ms":{"type":"integer","default":200,"minimum":50,"maximum":2000}}}"#,
    );

    t(
        "mouse_click",
        "Click at current position or specified coordinates.",
        r#"{"type":"object","properties":{"button":{"type":"string","enum":["left","right","middle"],"default":"left"},"x":{"type":"integer"},"y":{"type":"integer"},"clicks":{"type":"integer","default":1,"minimum":1,"maximum":3}}}"#,
    );

    t(
        "mouse_double_click",
        "Double-click at current position or coordinates.",
        r#"{"type":"object","properties":{"x":{"type":"integer"},"y":{"type":"integer"},"button":{"type":"string","enum":["left","right","middle"],"default":"left"}}}"#,
    );

    t(
        "mouse_scroll",
        "Scroll mouse wheel. Positive dy=down, negative dy=up.",
        r#"{"type":"object","properties":{"dx":{"type":"integer","default":0},"dy":{"type":"integer","default":0},"x":{"type":"integer"},"y":{"type":"integer"}}}"#,
    );

    t(
        "mouse_drag",
        "Click and drag from (x1,y1) to (x2,y2).",
        r#"{"type":"object","required":["x1","y1","x2","y2"],"properties":{"x1":{"type":"integer"},"y1":{"type":"integer"},"x2":{"type":"integer"},"y2":{"type":"integer"},"button":{"type":"string","enum":["left","right","middle"],"default":"left"},"duration_ms":{"type":"integer","default":500,"minimum":100,"maximum":5000}}}"#,
    );

    t(
        "keyboard_type",
        "Type a string of text. Supports Unicode.",
        r#"{"type":"object","required":["text"],"properties":{"text":{"type":"string"},"delay_ms":{"type":"integer","default":10,"minimum":0,"maximum":500}}}"#,
    );

    t(
        "key_press",
        "Press a key or combination. Examples: 'Return', 'ctrl+c', 'alt+Tab', 'ctrl+shift+t'.",
        r#"{"type":"object","required":["key"],"properties":{"key":{"type":"string"}}}"#,
    );

    t(
        "press_hotkey",
        "Press a key combination as an array. Example: ['ctrl', 'shift', 't'].",
        r#"{"type":"object","required":["keys"],"properties":{"keys":{"type":"array","items":{"type":"string"}}}}"#,
    );

    t(
        "click_on_text",
        "Find text on screen via OCR and click on it.",
        r#"{"type":"object","required":["text"],"properties":{"text":{"type":"string"},"button":{"type":"string","enum":["left","right","middle"],"default":"left"},"partial":{"type":"boolean","default":true},"region":{"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4}}}"#,
    );

    t(
        "wait_for_text",
        "Poll screen until text appears or timeout.",
        r#"{"type":"object","required":["text"],"properties":{"text":{"type":"string"},"timeout":{"type":"number","default":10,"minimum":0.5,"maximum":120},"partial":{"type":"boolean","default":true},"region":{"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4}}}"#,
    );

    t(
        "extract_text",
        "OCR the screen (or region) and return all visible text with bounding boxes.",
        r#"{"type":"object","properties":{"region":{"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4}}}"#,
    );

    t(
        "describe_screen",
        "Capture screen + OCR → structured description of visible UI elements.",
        r#"{"type":"object","properties":{"region":{"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4}}}"#,
    );

    t(
        "wait",
        "Sleep for N seconds.",
        r#"{"type":"object","required":["seconds"],"properties":{"seconds":{"type":"number","minimum":0,"maximum":300}}}"#,
    );

    t(
        "clipboard_get",
        "Read the system clipboard text contents.",
        r#"{"type":"object","properties":{}}"#,
    );

    t(
        "clipboard_set",
        "Set the system clipboard text contents.",
        r#"{"type":"object","required":["text"],"properties":{"text":{"type":"string"}}}"#,
    );

    t(
        "env_get",
        "Read an environment variable value by name. Returns empty string if not set.",
        r#"{"type":"object","required":["name"],"properties":{"name":{"type":"string","description":"Environment variable name (e.g. HOME, PATH, ALLOW_CODE)"}}}"#,
    );

    t(
        "shell_run",
        "Execute a shell command. Requires ALLOW_SHELL=1 env var.",
        r#"{"type":"object","required":["command"],"properties":{"command":{"type":"string"},"timeout":{"type":"number","default":30,"minimum":1,"maximum":600}}}"#,
    );

    t(
        "list_windows",
        "List all top-level windows with titles, apps, and geometry.",
        r#"{"type":"object","properties":{}}"#,
    );

    t(
        "focus_window",
        "Bring a window to the foreground by matching title or app class.",
        r#"{"type":"object","required":["title"],"properties":{"title":{"type":"string"}}}"#,
    );

    t(
        "get_active_window",
        "Get the currently focused window information.",
        r#"{"type":"object","properties":{}}"#,
    );

    t(
        "open_app",
        "Launch or focus an application by name.",
        r#"{"type":"object","required":["name"],"properties":{"name":{"type":"string"},"app":{"type":"string"}}}"#,
    );

    t(
        "notify",
        "Send a desktop notification.",
        r#"{"type":"object","required":["title","message"],"properties":{"title":{"type":"string"},"message":{"type":"string"},"urgency":{"type":"string","enum":["low","normal","critical"],"default":"normal"}}}"#,
    );

    t("get_window_state", "Get structured UI element tree for the active window via AT-SPI accessibility. Returns window info and all interactive elements with exact bounds, roles, labels, and states. Use this instead of screenshot+OCR for precision targeting of buttons, text fields, menus, etc.",
      r#"{"type":"object","properties":{"window_title":{"type":"string","description":"Optional: focus a window by title before getting state. If omitted, uses active window."}}}"#);

    t(
        "type_to_window",
        "Focus a window by title, then type text into it.",
        r#"{"type":"object","required":["title","text"],"properties":{"title":{"type":"string"},"text":{"type":"string"}}}"#,
    );

    // ═══════════════ BROWSER USE (17 tools) ═══════════════
    t(
        "browser_launch",
        "Launch or connect to a browser for web automation.",
        r#"{"type":"object","properties":{"mode":{"type":"string","enum":["auto","headless","desktop"],"default":"auto"}}}"#,
    );

    t(
        "browser_navigate",
        "Navigate the browser to a URL.",
        r#"{"type":"object","required":["url"],"properties":{"url":{"type":"string"},"wait_until":{"type":"string","enum":["load","domcontentloaded","networkidle"],"default":"domcontentloaded"}}}"#,
    );

    t(
        "browser_click",
        "Click an element by CSS selector, visible text, or coordinates.",
        r#"{"type":"object","properties":{"selector":{"type":"string"},"text":{"type":"string"},"x":{"type":"integer"},"y":{"type":"integer"}}}"#,
    );

    t(
        "browser_type",
        "Type text into an input element identified by CSS selector.",
        r#"{"type":"object","required":["selector","text"],"properties":{"selector":{"type":"string"},"text":{"type":"string"},"clear":{"type":"boolean","default":true},"press_enter":{"type":"boolean","default":false}}}"#,
    );

    t(
        "browser_screenshot",
        "Take a screenshot of the browser page or an element.",
        r#"{"type":"object","properties":{"selector":{"type":"string"},"full_page":{"type":"boolean","default":false}}}"#,
    );

    t(
        "browser_exec_js",
        "Execute JavaScript in the browser and return the result.",
        r#"{"type":"object","required":["code"],"properties":{"code":{"type":"string"}}}"#,
    );

    t(
        "browser_get_html",
        "Get the full HTML of the current page or a specific element.",
        r#"{"type":"object","properties":{"selector":{"type":"string"}}}"#,
    );

    t(
        "browser_get_text",
        "Get visible text content of the page or a specific element.",
        r#"{"type":"object","properties":{"selector":{"type":"string"}}}"#,
    );

    t(
        "browser_wait_for",
        "Wait for a selector to appear or text to be visible.",
        r#"{"type":"object","properties":{"selector":{"type":"string"},"text":{"type":"string"},"timeout":{"type":"number","default":30}}}"#,
    );

    t(
        "browser_tabs",
        "List all open browser tabs.",
        r#"{"type":"object","properties":{}}"#,
    );

    t(
        "browser_new_tab",
        "Open a new browser tab and optionally navigate to a URL.",
        r#"{"type":"object","properties":{"url":{"type":"string"}}}"#,
    );

    t(
        "browser_close_tab",
        "Close the current browser tab, or a specific one by index.",
        r#"{"type":"object","properties":{"index":{"type":"integer"}}}"#,
    );

    t(
        "browser_switch_tab",
        "Switch to a different browser tab by index or title.",
        r#"{"type":"object","properties":{"index":{"type":"integer"},"title":{"type":"string"}}}"#,
    );

    t(
        "browser_download",
        "Click a download link/button and wait for the file.",
        r#"{"type":"object","required":["selector"],"properties":{"selector":{"type":"string"},"save_dir":{"type":"string","default":"/tmp/mcp_downloads"},"timeout":{"type":"number","default":60}}}"#,
    );

    t(
        "browser_upload",
        "Upload one or more files via a file input element.",
        r#"{"type":"object","required":["selector","files"],"properties":{"selector":{"type":"string"},"files":{"type":"array","items":{"type":"string"}}}}"#,
    );

    t(
        "browser_cookies",
        "Get cookies for the current page.",
        r#"{"type":"object","properties":{"urls":{"type":"array","items":{"type":"string"}}}}"#,
    );

    t(
        "browser_console",
        "Get recent browser console messages.",
        r#"{"type":"object","properties":{"limit":{"type":"integer","default":50,"minimum":1,"maximum":500}}}"#,
    );

    t("browser_refresh", "Refresh browser discovery cache and return updated browser list. Use when a browser was launched outside desk-mcp.",
      r#"{"type":"object","properties":{}}"#);

    // ═══════════════ CODE MODE (8 tools) ═══════════════
    t(
        "file_read",
        "Read file contents with line numbers. Supports offset and limit for large files.",
        r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string","description":"Absolute or workspace-relative path"},"offset":{"type":"integer","default":0},"limit":{"type":"integer","minimum":1,"maximum":10000}}}"#,
    );

    t(
        "file_write",
        "Create or overwrite a file. Creates parent directories as needed.",
        r#"{"type":"object","required":["path","content"],"properties":{"path":{"type":"string","description":"Absolute or workspace-relative path"},"content":{"type":"string"}}}"#,
    );

    t("file_edit", "Perform exact string replacements in a file. Returns error if old_string is not unique (unless replace_all=true).",
      r#"{"type":"object","required":["path","old_string","new_string"],"properties":{"path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"},"replace_all":{"type":"boolean","default":false}}}"#);

    t(
        "grep",
        "Search file contents using regex patterns. Uses ripgrep if available, grep otherwise.",
        r#"{"type":"object","required":["pattern"],"properties":{"pattern":{"type":"string","description":"Regex pattern to search for"},"path":{"type":"string","default":"."},"case_insensitive":{"type":"boolean","default":false},"glob":{"type":"string","description":"File filter e.g. '*.rs'"}}}"#,
    );

    t("glob", "Find files matching a glob pattern. Returns sorted by modification time (newest first), max 100 results.",
      r#"{"type":"object","required":["pattern"],"properties":{"pattern":{"type":"string","description":"Glob pattern e.g. '**/*.rs'"},"path":{"type":"string","default":"."}}}"#);

    t("code_run", "Execute a code snippet. Requires ALLOW_CODE=1. Supports: python, bash, node, ruby, perl, php.",
      r#"{"type":"object","required":["language","code"],"properties":{"language":{"type":"string","enum":["python","py","bash","sh","node","javascript","js","ruby","rb","perl","pl","php"]},"code":{"type":"string"},"timeout":{"type":"integer","default":30,"minimum":1,"maximum":300},"cwd":{"type":"string","description":"Working directory (optional)"}}}"#);

    t("code_lint", "Run a linter on a file and return diagnostics. Supported: .rs (clippy), .py (ruff), .js/.ts (eslint), .sh (shellcheck), .go (go vet).",
      r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string"}}}"#);

    t(
        "code_build",
        "Run a build command in a project directory. Auto-detects: cargo, npm, make, go, python.",
        r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string","description":"Project directory path"},"command":{"type":"string","default":"auto","description":"Custom build command or 'auto'"},"timeout":{"type":"integer","default":120,"minimum":5,"maximum":300}}}"#,
    );

    // ═══════════════ ACCESSIBILITY (4 tools) ═══════════════
    t("find_elements", "Search the accessibility tree for elements matching a role and/or name. Returns elements with index, role, name, text, bounds, and actions. Use the returned index with get_element_text or click_element.",
      r#"{"type":"object","properties":{"role":{"type":"string","description":"Accessibility role filter (e.g. 'push button', 'text', 'menu item')"},"name_contains":{"type":"string","description":"Filter elements whose name contains this string (case-insensitive)"},"max_results":{"type":"integer","default":20,"minimum":1,"maximum":100}}}"#);

    t(
        "get_element_text",
        "Get detailed text and metadata for an accessibility element by its index path.",
        r#"{"type":"object","required":["path"],"properties":{"path":{"type":"integer","description":"Element index from find_elements results"}}}"#,
    );

    t("click_element", "Activate (click) an accessibility element by its index path. Clicks the center of the element's bounding box.",
      r#"{"type":"object","required":["path"],"properties":{"path":{"type":"integer","description":"Element index from find_elements results"}}}"#);

    t("get_window_tree", "Get the full accessibility tree for the active window. Heavy — opt-in only. Use max_depth to limit response size.",
      r#"{"type":"object","properties":{"max_depth":{"type":"integer","default":3,"minimum":1,"maximum":6,"description":"Maximum tree depth to return"}}}"#);

    // ═══════════════ STATUS (1 tool) ═══════════════
    // Note: "discover" was removed in Phase 5 — server_status provides
    // the same information without a separate tool.
    t(
        "server_status",
        "Health check: uptime, memory, provider, tool availability.",
        r#"{"type":"object","properties":{}}"#,
    );

    // ═══════════════ SAFETY & CONFIRMATION (4 tools) ═══════════════
    t(
        "request_confirmation",
        "Request user confirmation before a gated action. Returns a confirmation ID.",
        r#"{"type":"object","required":["tool","message"],"properties":{"tool":{"type":"string","description":"Tool name being gated"},"message":{"type":"string","description":"Why confirmation is needed"},"params":{"type":"object","description":"Tool params being held"}}}"#,
    );

    t(
        "approve",
        "Approve a pending confirmation by ID, unblocking the tool.",
        r#"{"type":"object","required":["id"],"properties":{"id":{"type":"string","description":"Confirmation ID from request_confirmation"}}}"#,
    );

    t(
        "deny",
        "Deny a pending confirmation by ID, cancelling the tool.",
        r#"{"type":"object","required":["id"],"properties":{"id":{"type":"string","description":"Confirmation ID from request_confirmation"},"reason":{"type":"string","description":"Optional deny reason"}}}"#,
    );

    t(
        "list_pending",
        "List all pending confirmations awaiting approval/denial.",
        r#"{"type":"object","properties":{}}"#,
    );

    // ═══════════════ RECIPES (dynamic, loaded from disk) ═══════════════
    for recipe in crate::recipes::load_all_recipes() {
        let schema = crate::recipes::recipe_input_schema(&recipe);
        tools.push(ToolDef {
            name: recipe.name,
            description: recipe.description,
            input_schema: schema,
        });
    }

    tools
}

/// Dispatch a tool call by name
pub async fn dispatch(
    name: &str,
    args: serde_json::Value,
    session_id: Option<&str>,
) -> ToolResponse {
    tracing::debug!(tool = name, "dispatching tool");
    let start = std::time::Instant::now();

    // ── Policy check ──
    match crate::policy::evaluate(name, &args) {
        crate::policy::PolicyDecision::Deny { reason } => {
            let resp = crate::response::err("POLICY_DENIED", &reason);
            crate::audit::log(name, &args, false, Some("policy_denied"), start);
            return resp;
        }
        crate::policy::PolicyDecision::RequireConfirmation { message, params } => {
            if !crate::safety::is_approved_for_session(name, &params) {
                let id = crate::safety::request(name, &message, &params);
                return crate::response::err(
                    "CONFIRMATION_REQUIRED",
                    &format!(
                        "Tool '{}' requires confirmation. Use approve('{}') to proceed. {}",
                        name, id, message
                    ),
                );
            }
        }
        crate::policy::PolicyDecision::Allow => {} // proceed
    }

    // ── Rate limiting ──
    // Prefer per-session rate bucket when available; fall back to global.
    // (record_action is handled by transport::handle_request after dispatch.)
    let rate_limited = if let Some(sid) = session_id {
        match crate::session::SESSIONS.get_session(&sid.to_string()) {
            Some(session) => !session.check_rate(),
            None => !crate::safety::check_rate(name),
        }
    } else {
        !crate::safety::check_rate(name)
    };

    if rate_limited {
        let resp = crate::response::err(
            "RATE_LIMITED",
            &format!("Rate limit reached for '{name}'. Wait a moment before retrying."),
        );
        crate::audit::log(name, &args, false, Some("rate_limited"), start);
        return resp;
    }

    // Clone args for audit — each handler consumes its own args
    let audit_args = args.clone();

    // ── Recipe dispatch (checked before main dispatch to avoid async recursion) ──
    if let Some(recipe) = crate::recipes::find_recipe(name) {
        // Guard against recipe→recipe recursion (recipes cannot call other recipes in v1)
        if RECIPE_ACTIVE.swap(true, std::sync::atomic::Ordering::AcqRel) {
            RECIPE_ACTIVE.store(false, std::sync::atomic::Ordering::Release);
            let resp = crate::response::err(
                "RECIPE_RECURSION",
                "Recipes cannot call other recipes (nesting limit: 1). Use individual tool calls instead.",
            );
            crate::audit::log(name, &args, false, Some("recipe_recursion"), start);
            return resp;
        }

        let mut params: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        if let Some(obj) = args.as_object() {
            for (k, v) in obj {
                match v {
                    serde_json::Value::String(s) => {
                        params.insert(k.clone(), s.clone());
                    }
                    other => {
                        params.insert(k.clone(), other.to_string());
                    }
                }
            }
        }
        let mut last_result: Option<crate::response::ToolResponse> = None;
        for step in &recipe.steps {
            // Skip recipe→recipe steps (they would recurse)
            if crate::recipes::find_recipe(&step.tool).is_some() {
                continue;
            }
            let step_params = crate::recipes::substitute_params(&step.params, &params);
            let resp = Box::pin(crate::tools::dispatch(&step.tool, step_params, session_id)).await;
            if !resp.ok {
                RECIPE_ACTIVE.store(false, std::sync::atomic::Ordering::Release);
                crate::audit::log(name, &args, false, Some("recipe_step_failed"), start);
                return resp;
            }
            last_result = Some(resp);
        }
        RECIPE_ACTIVE.store(false, std::sync::atomic::Ordering::Release);
        let final_resp = last_result.unwrap_or_else(|| {
            crate::response::ok(serde_json::json!({
                "recipe": name, "steps_completed": recipe.steps.len(),
            }))
        });
        crate::audit::log(name, &args, final_resp.ok, None, start);
        return final_resp;
    }

    // ── Run tool handler with a hard 60s deadline ──
    // Some tool handlers (browser_launch in particular) can hang indefinitely
    // if the CDP WebSocket handshake stalls. This timeout ensures the server
    // always responds — even if the handler future never yields.
    let dispatch_future = async {
        // ── Resolution-routed tools (three-tier resolver) ──
        // These tools first try resolution via the target field when present,
        // falling back to the direct handler when only raw coordinates are provided.
        let resolution_routed = matches!(
            name,
            "mouse_click"
                | "mouse_double_click"
                | "keyboard_type"
                | "click_on_text"
                | "browser_click"
                | "browser_type"
        );
        if resolution_routed {
            if let Ok(resp) = crate::resolution::dispatch_resolve(name, &args).await {
                return resp;
            }
            // If resolution fails (e.g. no target fields), fall through to
            // the existing direct handler below.
        }

        match name {
            // Computer use
            "screenshot" | "get_screen_size" | "mouse_move" | "mouse_click"
            | "mouse_double_click" | "mouse_scroll" | "mouse_drag" | "keyboard_type"
            | "key_press" | "press_hotkey" | "click_on_text" | "wait_for_text" | "extract_text"
            | "describe_screen" | "wait" | "clipboard_get" | "clipboard_set" | "env_get"
            | "shell_run" | "list_windows" | "focus_window" | "get_active_window" | "open_app"
            | "notify" | "get_window_state" | "type_to_window" => {
                computer::handle(name, args).await
            }

            // Browser use
            "browser_launch" | "browser_navigate" | "browser_click" | "browser_type"
            | "browser_screenshot" | "browser_exec_js" | "browser_get_html"
            | "browser_get_text" | "browser_wait_for" | "browser_tabs" | "browser_new_tab"
            | "browser_close_tab" | "browser_switch_tab" | "browser_download"
            | "browser_upload" | "browser_cookies" | "browser_console" => {
                browser::handle(name, args, session_id).await
            }
            "browser_refresh" => {
                let fresh_browsers = crate::discovery::refresh_browsers();
                let caps = crate::discovery::detect();
                crate::response::ok(serde_json::json!({
                    "discovered_browsers": fresh_browsers.iter()
                        .map(|b| serde_json::json!({"binary": b.binary, "port": b.debugging_port, "pid": b.pid}))
                        .collect::<Vec<_>>(),
                    "browser_automation": caps.browser_automation,
                    "installed_browsers": caps.installed_browsers,
                }))
            }

            // Code mode
            "file_read" | "file_write" | "file_edit" | "grep" | "glob" | "code_run"
            | "code_lint" | "code_build" => code::handle(name, args).await,

            // Status
            "server_status" => {
                let caps = crate::discovery::detect();
                crate::response::ok(serde_json::json!({
                    "server": crate::SERVER_NAME,
                    "version": crate::SERVER_VERSION,
                    "provider": caps.provider,
                    "display_type": caps.display_type,
                    "desktop": caps.desktop,
                    "screenshot_tool": caps.screenshot_tool,
                    "input_tool": caps.input_tool,
                    "window_tool": caps.window_tool,
                    "available": {
                        "screenshot": caps.screenshot,
                        "mouse": caps.mouse,
                        "keyboard": caps.keyboard,
                        "windows": caps.windows,
                        "clipboard": caps.clipboard,
                        "notify": caps.notify,
                        "ocr": caps.ocr,
                        "browser_automation": caps.browser_automation,
                    },
                    "installed_browsers": caps.installed_browsers.len(),
                    "discovered_browsers": caps.discovered_browsers.iter()
                        .map(|b| serde_json::json!({"binary": b.binary, "port": b.debugging_port, "pid": b.pid}))
                        .collect::<Vec<_>>(),
                }))
            }

            // Accessibility
            "find_elements" | "get_element_text" | "click_element" | "get_window_tree" => {
                a11y::handle(name, args).await
            }

            // Safety & confirmation
            "request_confirmation" => {
                let tool = args
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let message = args
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Action requires confirmation");
                let params = args
                    .get("params")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let id = crate::safety::request(tool, message, &params);
                crate::response::ok(serde_json::json!({"id": id, "status": "pending"}))
            }
            "approve" => {
                if let Some(id) = args.get("id").and_then(|v| v.as_str()) {
                    match crate::safety::approve(id) {
                        Ok(()) => {
                            crate::response::ok(serde_json::json!({"id": id, "status": "approved"}))
                        }
                        Err(e) => crate::response::err("NOT_FOUND", &e),
                    }
                } else {
                    crate::response::err("INVALID_ARGS", "Missing 'id' parameter")
                }
            }
            "deny" => {
                if let Some(id) = args.get("id").and_then(|v| v.as_str()) {
                    let reason = args
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Denied by user");
                    match crate::safety::deny(id, reason) {
                        Ok(()) => {
                            crate::response::ok(serde_json::json!({"id": id, "status": "denied"}))
                        }
                        Err(e) => crate::response::err("NOT_FOUND", &e),
                    }
                } else {
                    crate::response::err("INVALID_ARGS", "Missing 'id' parameter")
                }
            }
            "list_pending" => {
                let pending = crate::safety::list_pending();
                crate::response::ok(serde_json::json!({
                    "pending": pending.iter().map(|p| serde_json::json!({
                        "id": p.id,
                        "tool": p.tool,
                        "message": p.message,
                        "params": p.params,
                        "created": p.created
                    })).collect::<Vec<_>>()
                }))
            }

            _ => {
                // Suggest closest matching tool names
                let tools = all_tools();
                let name_lower = name.to_lowercase();
                let mut scored: Vec<(&str, usize)> = tools
                    .iter()
                    .map(|t| {
                        let t_name = t.name.as_str();
                        let score = if t_name == name_lower {
                            0 // exact match after lowercasing
                        } else if t_name.contains(&name_lower) || name_lower.contains(t_name) {
                            1
                        } else {
                            // Levenshtein-ish: count common prefix chars
                            t_name
                                .chars()
                                .zip(name_lower.chars())
                                .take_while(|(a, b)| a == b)
                                .count()
                        };
                        (t_name, score)
                    })
                    .collect();
                scored.sort_by_key(|(_, s)| std::cmp::Reverse(*s));
                let suggestions: Vec<&str> = scored
                    .iter()
                    .filter(|(_, s)| *s > 0)
                    .take(3)
                    .map(|(n, _)| *n)
                    .collect();

                let msg = if suggestions.is_empty() {
                    format!("No tool named '{name}'. Use tools/list to see available tools.")
                } else {
                    format!(
                    "No tool named '{name}'. Did you mean: {}? Use tools/list to see all {} tools.",
                    suggestions.join(", "),
                    tools.len()
                )
                };
                crate::response::err("UNKNOWN_TOOL", &msg)
            }
        }
    };

    // Wrap with a 60-second hard deadline so a stuck handler never
    // blocks the server permanently.
    let result =
        match tokio::time::timeout(std::time::Duration::from_secs(60), dispatch_future).await {
            Ok(tool_response) => tool_response,
            Err(_elapsed) => {
                tracing::error!(tool = name, "tool handler timed out after 60s");
                crate::response::err(
                    "TIMEOUT",
                    &format!("Tool '{name}' timed out after 60 seconds"),
                )
            }
        };

    // Audit log
    let ok = result.ok;
    let _err_msg = result.error.as_ref().map(|e| e.code.as_str());
    crate::audit::log(name, &audit_args, ok, _err_msg, start);

    result
}
