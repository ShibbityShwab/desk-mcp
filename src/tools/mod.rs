//! Tool registry — defines all 50 MCP tools with JSON schemas.
//!
//! Each tool is registered with a name, description, and JSON Schema input spec.
//! The `dispatch` function routes tool calls to the appropriate handler.

pub mod computer;
pub mod browser;
pub mod code;

use crate::response::ToolResponse;
use serde::Serialize;

/// MCP tool definition
#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// Generate the full tool list (42 tools)
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
    t("screenshot", "Capture screen or region. Returns base64 PNG image.",
      r#"{"type":"object","properties":{"region":{"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4,"description":"[x,y,w,h] region to capture"}}}"#);

    t("get_screen_size", "Get the primary display dimensions in physical pixels.",
      r#"{"type":"object","properties":{}}"#);

    t("mouse_move", "Move cursor to (x,y) in screen pixels.",
      r#"{"type":"object","required":["x","y"],"properties":{"x":{"type":"integer"},"y":{"type":"integer"},"smooth":{"type":"boolean","default":false},"duration_ms":{"type":"integer","default":200,"minimum":50,"maximum":2000}}}"#);

    t("mouse_click", "Click at current position or specified coordinates.",
      r#"{"type":"object","properties":{"button":{"type":"string","enum":["left","right","middle"],"default":"left"},"x":{"type":"integer"},"y":{"type":"integer"},"clicks":{"type":"integer","default":1,"minimum":1,"maximum":3}}}"#);

    t("mouse_double_click", "Double-click at current position or coordinates.",
      r#"{"type":"object","properties":{"x":{"type":"integer"},"y":{"type":"integer"},"button":{"type":"string","enum":["left","right","middle"],"default":"left"}}}"#);

    t("mouse_scroll", "Scroll mouse wheel. Positive dy=down, negative dy=up.",
      r#"{"type":"object","properties":{"dx":{"type":"integer","default":0},"dy":{"type":"integer","default":0},"x":{"type":"integer"},"y":{"type":"integer"}}}"#);

    t("mouse_drag", "Click and drag from (x1,y1) to (x2,y2).",
      r#"{"type":"object","required":["x1","y1","x2","y2"],"properties":{"x1":{"type":"integer"},"y1":{"type":"integer"},"x2":{"type":"integer"},"y2":{"type":"integer"},"button":{"type":"string","enum":["left","right","middle"],"default":"left"},"duration_ms":{"type":"integer","default":500,"minimum":100,"maximum":5000}}}"#);

    t("keyboard_type", "Type a string of text. Supports Unicode.",
      r#"{"type":"object","required":["text"],"properties":{"text":{"type":"string"},"delay_ms":{"type":"integer","default":10,"minimum":0,"maximum":500}}}"#);

    t("key_press", "Press a key or combination. Examples: 'Return', 'ctrl+c', 'alt+Tab', 'ctrl+shift+t'.",
      r#"{"type":"object","required":["key"],"properties":{"key":{"type":"string"}}}"#);

    t("press_hotkey", "Press a key combination as an array. Example: ['ctrl', 'shift', 't'].",
      r#"{"type":"object","required":["keys"],"properties":{"keys":{"type":"array","items":{"type":"string"}}}}"#);

    t("click_on_text", "Find text on screen via OCR and click on it.",
      r#"{"type":"object","required":["text"],"properties":{"text":{"type":"string"},"button":{"type":"string","enum":["left","right","middle"],"default":"left"},"partial":{"type":"boolean","default":true},"region":{"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4}}}"#);

    t("wait_for_text", "Poll screen until text appears or timeout.",
      r#"{"type":"object","required":["text"],"properties":{"text":{"type":"string"},"timeout":{"type":"number","default":10,"minimum":0.5,"maximum":120},"partial":{"type":"boolean","default":true},"region":{"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4}}}"#);

    t("extract_text", "OCR the screen (or region) and return all visible text with bounding boxes.",
      r#"{"type":"object","properties":{"region":{"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4}}}"#);

    t("describe_screen", "Capture screen + OCR → structured description of visible UI elements.",
      r#"{"type":"object","properties":{"region":{"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4}}}"#);

    t("wait", "Sleep for N seconds.",
      r#"{"type":"object","required":["seconds"],"properties":{"seconds":{"type":"number","minimum":0,"maximum":300}}}"#);

    t("clipboard_get", "Read the system clipboard text contents.",
      r#"{"type":"object","properties":{}}"#);

    t("clipboard_set", "Set the system clipboard text contents.",
      r#"{"type":"object","required":["text"],"properties":{"text":{"type":"string"}}}"#);

    t("shell_run", "Execute a shell command. Requires ALLOW_SHELL=1 env var.",
      r#"{"type":"object","required":["command"],"properties":{"command":{"type":"string"},"timeout":{"type":"number","default":30,"minimum":1,"maximum":600}}}"#);

    t("list_windows", "List all top-level windows with titles, apps, and geometry.",
      r#"{"type":"object","properties":{}}"#);

    t("focus_window", "Bring a window to the foreground by matching title or app class.",
      r#"{"type":"object","required":["title"],"properties":{"title":{"type":"string"}}}"#);

    t("get_active_window", "Get the currently focused window information.",
      r#"{"type":"object","properties":{}}"#);

    t("open_app", "Launch or focus an application by name.",
      r#"{"type":"object","required":["name"],"properties":{"name":{"type":"string"}}}"#);

    t("notify", "Send a desktop notification.",
      r#"{"type":"object","required":["title","message"],"properties":{"title":{"type":"string"},"message":{"type":"string"},"urgency":{"type":"string","enum":["low","normal","critical"],"default":"normal"}}}"#);

    t("type_to_window", "Focus a window by title, then type text into it.",
      r#"{"type":"object","required":["title","text"],"properties":{"title":{"type":"string"},"text":{"type":"string"}}}"#);

    // ═══════════════ BROWSER USE (17 tools) ═══════════════
    t("browser_launch", "Launch or connect to a browser for web automation.",
      r#"{"type":"object","properties":{"mode":{"type":"string","enum":["auto","headless","desktop"],"default":"auto"}}}"#);

    t("browser_navigate", "Navigate the browser to a URL.",
      r#"{"type":"object","required":["url"],"properties":{"url":{"type":"string"},"wait_until":{"type":"string","enum":["load","domcontentloaded","networkidle"],"default":"domcontentloaded"}}}"#);

    t("browser_click", "Click an element by CSS selector, visible text, or coordinates.",
      r#"{"type":"object","properties":{"selector":{"type":"string"},"text":{"type":"string"},"x":{"type":"integer"},"y":{"type":"integer"}}}"#);

    t("browser_type", "Type text into an input element identified by CSS selector.",
      r#"{"type":"object","required":["selector","text"],"properties":{"selector":{"type":"string"},"text":{"type":"string"},"clear":{"type":"boolean","default":true},"press_enter":{"type":"boolean","default":false}}}"#);

    t("browser_screenshot", "Take a screenshot of the browser page or an element.",
      r#"{"type":"object","properties":{"selector":{"type":"string"},"full_page":{"type":"boolean","default":false}}}"#);

    t("browser_exec_js", "Execute JavaScript in the browser and return the result.",
      r#"{"type":"object","required":["code"],"properties":{"code":{"type":"string"}}}"#);

    t("browser_get_html", "Get the full HTML of the current page or a specific element.",
      r#"{"type":"object","properties":{"selector":{"type":"string"}}}"#);

    t("browser_get_text", "Get visible text content of the page or a specific element.",
      r#"{"type":"object","properties":{"selector":{"type":"string"}}}"#);

    t("browser_wait_for", "Wait for a selector to appear or text to be visible.",
      r#"{"type":"object","properties":{"selector":{"type":"string"},"text":{"type":"string"},"timeout":{"type":"number","default":30}}}"#);

    t("browser_tabs", "List all open browser tabs.",
      r#"{"type":"object","properties":{}}"#);

    t("browser_new_tab", "Open a new browser tab and optionally navigate to a URL.",
      r#"{"type":"object","properties":{"url":{"type":"string"}}}"#);

    t("browser_close_tab", "Close the current browser tab, or a specific one by index.",
      r#"{"type":"object","properties":{"index":{"type":"integer"}}}"#);

    t("browser_switch_tab", "Switch to a different browser tab by index or title.",
      r#"{"type":"object","properties":{"index":{"type":"integer"},"title":{"type":"string"}}}"#);

    t("browser_download", "Click a download link/button and wait for the file.",
      r#"{"type":"object","required":["selector"],"properties":{"selector":{"type":"string"},"save_dir":{"type":"string","default":"/tmp/mcp_downloads"},"timeout":{"type":"number","default":60}}}"#);

    t("browser_upload", "Upload one or more files via a file input element.",
      r#"{"type":"object","required":["selector","files"],"properties":{"selector":{"type":"string"},"files":{"type":"array","items":{"type":"string"}}}}"#);

    t("browser_cookies", "Get cookies for the current page.",
      r#"{"type":"object","properties":{"urls":{"type":"array","items":{"type":"string"}}}}"#);

    t("browser_console", "Get recent browser console messages.",
      r#"{"type":"object","properties":{"limit":{"type":"integer","default":50,"minimum":1,"maximum":500}}}"#);

    // ═══════════════ CODE MODE (8 tools) ═══════════════
    t("file_read", "Read file contents with line numbers. Supports offset and limit for large files.",
      r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string","description":"Absolute or workspace-relative path"},"offset":{"type":"integer","default":0},"limit":{"type":"integer","minimum":1,"maximum":10000}}}"#);

    t("file_write", "Create or overwrite a file. Creates parent directories as needed.",
      r#"{"type":"object","required":["path","content"],"properties":{"path":{"type":"string","description":"Absolute or workspace-relative path"},"content":{"type":"string"}}}"#);

    t("file_edit", "Perform exact string replacements in a file. Returns error if old_string is not unique (unless replace_all=true).",
      r#"{"type":"object","required":["path","old_string","new_string"],"properties":{"path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"},"replace_all":{"type":"boolean","default":false}}}"#);

    t("grep", "Search file contents using regex patterns. Uses ripgrep if available, grep otherwise.",
      r#"{"type":"object","required":["pattern"],"properties":{"pattern":{"type":"string","description":"Regex pattern to search for"},"path":{"type":"string","default":"."},"case_insensitive":{"type":"boolean","default":false},"glob":{"type":"string","description":"File filter e.g. '*.rs'"}}}"#);

    t("glob", "Find files matching a glob pattern. Returns sorted by modification time (newest first), max 100 results.",
      r#"{"type":"object","required":["pattern"],"properties":{"pattern":{"type":"string","description":"Glob pattern e.g. '**/*.rs'"},"path":{"type":"string","default":"."}}}"#);

    t("code_run", "Execute a code snippet. Requires ALLOW_CODE=1. Supports: python, bash, node, ruby, perl, php.",
      r#"{"type":"object","required":["language","code"],"properties":{"language":{"type":"string","enum":["python","py","bash","sh","node","javascript","js","ruby","rb","perl","pl","php"]},"code":{"type":"string"},"timeout":{"type":"integer","default":30,"minimum":1,"maximum":300},"cwd":{"type":"string","description":"Working directory (optional)"}}}"#);

    t("code_lint", "Run a linter on a file and return diagnostics. Supported: .rs (clippy), .py (ruff), .js/.ts (eslint), .sh (shellcheck), .go (go vet).",
      r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string"}}}"#);

    t("code_build", "Run a build command in a project directory. Auto-detects: cargo, npm, make, go, python.",
      r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string","description":"Project directory path"},"command":{"type":"string","default":"auto","description":"Custom build command or 'auto'"},"timeout":{"type":"integer","default":120,"minimum":5,"maximum":300}}}"#);

    // ═══════════════ DISCOVERY & STATUS (2 tools) ═══════════════
    t("discover", "Report all detected capabilities: display type, tools, browsers, environment.",
      r#"{"type":"object","properties":{}}"#);

    t("server_status", "Health check: uptime, memory, provider, tool availability.",
      r#"{"type":"object","properties":{}}"#);

    tools
}

/// Dispatch a tool call by name
pub async fn dispatch(name: &str, args: serde_json::Value) -> ToolResponse {
    tracing::debug!(tool = name, "dispatching tool");

    let result = match name {
        // Computer use
        "screenshot" | "get_screen_size" | "mouse_move" | "mouse_click"
        | "mouse_double_click" | "mouse_scroll" | "mouse_drag"
        | "keyboard_type" | "key_press" | "press_hotkey"
        | "click_on_text" | "wait_for_text" | "extract_text" | "describe_screen"
        | "wait" | "clipboard_get" | "clipboard_set" | "shell_run"
        | "list_windows" | "focus_window" | "get_active_window"
        | "open_app" | "notify" | "type_to_window" => {
            computer::handle(name, args).await
        }

        // Browser use
        "browser_launch" | "browser_navigate" | "browser_click" | "browser_type"
        | "browser_screenshot" | "browser_exec_js" | "browser_get_html"
        | "browser_get_text" | "browser_wait_for" | "browser_tabs"
        | "browser_new_tab" | "browser_close_tab" | "browser_switch_tab"
        | "browser_download" | "browser_upload" | "browser_cookies"
        | "browser_console" => {
            browser::handle(name, args).await
        }

        // Code mode
        "file_read" | "file_write" | "file_edit" | "grep" | "glob"
        | "code_run" | "code_lint" | "code_build" => {
            code::handle(name, args).await
        }

        // Discovery & status
        "discover" => {
            let caps = crate::discovery::detect();
            crate::response::ok(&caps)
        }
        "server_status" => {
            let caps = crate::discovery::detect();
            crate::response::ok(serde_json::json!({
                "server": crate::SERVER_NAME,
                "version": crate::SERVER_VERSION,
                "provider": caps.provider,
                "display_type": caps.display_type,
                "desktop": caps.desktop,
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

        _ => crate::response::err("UNKNOWN_TOOL", &format!("No tool named '{name}'")),
    };

    result
}
