# Desk-MCP Error Codes

Audit date: 2025-07-01  
Source: `src/error.rs`, `src/response.rs`, `src/tools/*.rs`  

This document catalogs every error code that desk-mcp tools can emit, along with what triggers it and how to resolve it.

---

## Table of Contents

1. [Structured error types (McpError)](#1-structured-error-types-mcperror)
2. [Hardcoded string codes](#2-hardcoded-string-codes)
3. [Inconsistencies observed](#3-inconsistencies-observed)

---

## 1. Structured error types (McpError)

Defined in `src/error.rs` and rendered by `response::from_mcp_error()` or the `From<McpError>` tuple conversion. These are the canonical, typed error codes.

| Code | Variant | Trigger | Remediation |
|------|---------|---------|-------------|
| `DEPENDENCY_MISSING` | `McpError::DependencyMissing` | A required system dependency (e.g. tesseract, ydotool) is not installed. | Install the missing dependency and ensure it is on `PATH`. |
| `NOT_IMPLEMENTED` | `McpError::NotAvailable` | The tool or feature is not available in the current environment (e.g. no display server). | Check environment requirements; run on a desktop session with the needed capabilities. |
| `TIMEOUT` | `McpError::Timeout` | An operation exceeded its configured timeout. | Retry with a longer timeout or reduce input size. |
| `BROWSER_NOT_LAUNCHED` | `McpError::BrowserNotLaunched` | A browser tool was called before `browser_launch`. | Call `browser_launch` first. |
| `BROWSER_ERROR` | `McpError::BrowserLaunchFailed` | Browser launch failed (e.g. Chrome binary not found, port conflict). | Check Chrome installation and that the debug port is free. |
| `IO_ERROR` | `McpError::Io` | A filesystem I/O operation failed on a specific path. | Check file permissions and disk space. |
| `FILE_ERROR` | `McpError::FileOp` | A file operation (read, write, edit) failed — e.g. file not found, permission denied. | Verify the file path and permissions. |
| `PATH_ERROR` | `McpError::PathOutsideWorkspace` | A file path resolved outside the allowed workspace root. | Supply a path within the workspace. |
| `SHELL_NOT_ALLOWED` | `McpError::ShellNotAllowed` | `shell_run` was called without `ALLOW_SHELL=1` environment variable. | Set `ALLOW_SHELL=1` in the environment. |
| `CODE_NOT_ALLOWED` | `McpError::CodeNotAllowed` | `code_run` was called without `ALLOW_CODE=1` environment variable. | Set `ALLOW_CODE=1` in the environment. |
| `UNKNOWN_TOOL` | `McpError::UnknownTool` | The dispatched tool name does not match any registered tool. | Use `tools/list` to see available tools. |
| `TOOL_ERROR` | `McpError::ToolError` | Catch-all for generic tool failures (also wraps `anyhow::Error` and bare `std::io::Error`). | Inspect the message for details. |
| `JSON_ERROR` | `McpError::JsonError` | JSON parsing or serialization failure. | Check that input is valid JSON. |

---

## 2. Hardcoded string codes

These are emitted via direct `response::err("CODE", "...")` calls or `Err(("CODE".into(), ...))` tuples in tool handlers. They do *not* pass through the `McpError` enum.

### 2a. Dispatch layer (`src/tools/mod.rs`)

| Code | Source line(s) | Trigger | Remediation |
|------|----------------|---------|-------------|
| `POLICY_DENIED` | 421 | The policy engine denied the tool invocation (e.g. policy rules blocked `shell_run`). | Adjust policy configuration or use an allowed tool. |
| `CONFIRMATION_REQUIRED` | 428-434 | The tool requires user confirmation before execution (safety gate). | Call `approve(id)` to confirm, or `deny(id)` to reject. |
| `RATE_LIMITED` | 453-456 | Per-session or global rate limit was exceeded. | Wait and retry. |
| `RECIPE_RECURSION` | 469-472 | A recipe attempted to call another recipe (nesting limit = 1). | Use individual tool calls instead of nested recipes. |
| `TIMEOUT` | 725-728 | The tokio `timeout` wrapper fired (hard 60-second deadline per tool invocation). | Optimize the operation or split into smaller steps. Note: this is a separate emission from `McpError::Timeout` — same string code, different code path. |
| `UNKNOWN_TOOL` | 701, 713 | The tool name did not match any known handler after recipe/confirmation dispatch. | Check tool name; suggestions are included in the message. |
| `NOT_FOUND` | 626-627 (`approve`), 642-643 (`deny`) | The confirmation ID passed to `approve` or `deny` was not found (already expired or never existed). | Supply a valid confirmation ID from `request_confirmation`. |
| `INVALID_ARGS` | 629, 641, 645 | Missing required `id` parameter for `approve` or `deny`. | Include the `id` parameter. |

### 2b. Browser tools (`src/tools/browser.rs`)

| Code | Source line | Trigger | Remediation |
|------|-------------|---------|-------------|
| `BROWSER_ERROR` | 56 | Catch-all for any browser tool failure (bad args, navigation error, element not found, etc.). | Check the message for details; ensure browser is launched and the page is loaded. |

> **Note**: `BROWSER_ERROR` overlaps with `McpError::BrowserLaunchFailed` (which also emits `BROWSER_ERROR`). The typed variant is used only during `browser_launch`; the hardcoded string is used for all other browser tools.

### 2c. Computer use tools (`src/tools/computer.rs`)

| Code | Source line(s) | Trigger | Remediation |
|------|----------------|---------|-------------|
| `COMPUTER_ERROR` | 64 (macro), 83 | Generic computer-tool failure (screenshot, mouse move, key press, etc., when the provider returns an error). | Check the message for details. |
| `TEXT_NOT_FOUND` | 303-306 | `wait_for_text` timed out without finding the target text on screen. | Verify the text is visible; increase timeout. |
| `SHELL_DISABLED` | 412-416 | `shell_run` called without `ALLOW_SHELL=1` (duplicate of typed `SHELL_NOT_ALLOWED`). | Set `ALLOW_SHELL=1`. |
| `NO_ACTIVE_WINDOW` | 469 | `focus_window` could not determine the currently active window. | Ensure at least one window is open. |
| `WINDOW_NOT_FOUND` | 521-525 | `focus_window` / `type_to_window` — no window matched the given title. | Check the window title. |
| `UNKNOWN_TOOL` | 537 | The tool name did not match any computer-use handler. | Check tool name. |

### 2d. Accessibility tools (`src/tools/a11y.rs`)

| Code | Source line | Trigger | Remediation |
|------|-------------|---------|-------------|
| `A11Y_ERROR` | 17 | Catch-all for any accessibility tool failure (AT-SPI not available, element not found, etc.). | Ensure a desktop environment with AT-SPI is running. |

### 2e. Code tools (`src/tools/code.rs`)

| Code | Source line | Trigger | Remediation |
|------|-------------|---------|-------------|
| `CODE_ERROR` | 160 | Catch-all for any code/file tool failure (file not found, permission denied, lint/build failure, etc.). | Check the message for details. |

### 2f. Search tools (`src/tools/search.rs`)

| Code | Source line(s) | Trigger | Remediation |
|------|----------------|---------|-------------|
| `web_fetch` | 124, 128, 146, 151, 165 | Validation/HTTP failures in `web_fetch` — missing/ invalid URL, request failure, read error. | Supply a valid HTTP(S) URL; check network connectivity. |
| `web_search` | 257, 284 | Validation/search failure in `web_search` — missing query or DuckDuckGo error. | Supply a query string; check network connectivity. |

> ⚠️ **Inconsistency**: The search tool uses **lowercase, tool-name codes** (`"web_fetch"`, `"web_search"`) instead of the `SCREAMING_SNAKE_CASE` convention used everywhere else. See [§3 Inconsistencies](#3-inconsistencies-observed).

---

## 3. Inconsistencies observed

| # | Issue | Severity | Details |
|---|-------|----------|---------|
| 1 | **Search tool uses tool-name codes** | Medium | `response::err("web_fetch", ...)` and `response::err("web_search", ...)` use lowercase tool names as error codes. Every other module uses `SCREAMING_SNAKE_CASE`. Suggestion: change to `WEB_FETCH_ERROR` / `WEB_SEARCH_ERROR`. |
| 2 | **`SHELL_DISABLED` vs `SHELL_NOT_ALLOWED`** | Medium | The `computer.rs` handler emits `SHELL_DISABLED` for the same condition that the typed `McpError::ShellNotAllowed` emits `SHELL_NOT_ALLOWED`. Two different codes for the same state. Suggestion: unify on `SHELL_NOT_ALLOWED`. |
| 3 | **`TIMEOUT` double-sourced** | Low | Both `McpError::Timeout` and `tools/mod.rs:725` emit `"TIMEOUT"`. The typed version carries per-tool seconds info; the dispatch wrapper uses a fixed 60s. The string is the same, but the code paths differ. |
| 4 | **`BROWSER_ERROR` double-sourced** | Low | Both `McpError::BrowserLaunchFailed` and `browser.rs:56` emit `"BROWSER_ERROR"`. The typed variant is used for launch failure; the string variant is a catch-all for all other browser operations. Same code, different semantics. |
| 5 | **`UNKNOWN_TOOL` double-sourced** | Low | Both `McpError::UnknownTool` and `mod.rs:701` emit `"UNKNOWN_TOOL"`. The typed variant is used when a non-existent tool name is given to `dispatch()`; the hardcoded string in `mod.rs:701` is returned from the catch-all branch. They live in the same function, so this is more of a code-organization concern. |
| 6 | **`COMPUTER_ERROR` not in McpError enum** | Low | `COMPUTER_ERROR` is emitted by the `map_err!` macro and direct `Err(...)` tuples but is not represented in the `McpError` enum. It is the most-used code in `computer.rs`. |

---

## 4. Coverage summary

| Source file | Codes used |
|-------------|------------|
| `src/error.rs` (McpError) | `DEPENDENCY_MISSING`, `NOT_IMPLEMENTED`, `TIMEOUT`, `BROWSER_NOT_LAUNCHED`, `BROWSER_ERROR`, `IO_ERROR`, `FILE_ERROR`, `PATH_ERROR`, `SHELL_NOT_ALLOWED`, `CODE_NOT_ALLOWED`, `UNKNOWN_TOOL`, `TOOL_ERROR`, `JSON_ERROR` |
| `src/tools/mod.rs` (dispatch) | `POLICY_DENIED`, `CONFIRMATION_REQUIRED`, `RATE_LIMITED`, `RECIPE_RECURSION`, `TIMEOUT`, `UNKNOWN_TOOL`, `NOT_FOUND`, `INVALID_ARGS` |
| `src/tools/browser.rs` | `BROWSER_ERROR` |
| `src/tools/computer.rs` | `COMPUTER_ERROR`, `TEXT_NOT_FOUND`, `SHELL_DISABLED`, `NO_ACTIVE_WINDOW`, `WINDOW_NOT_FOUND`, `UNKNOWN_TOOL` |
| `src/tools/a11y.rs` | `A11Y_ERROR` |
| `src/tools/code.rs` | `CODE_ERROR` |
| `src/tools/search.rs` | `web_fetch`, `web_search` |

**Total distinct error codes**: 28  
**Canonical (McpError)**: 13  
**Hardcoded string codes**: 15 (including the 2 tool-name-style codes)
