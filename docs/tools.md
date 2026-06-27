---
layout: default
title: Tool Reference — desk-mcp
---

# Tool Reference

desk-mcp exposes 50 tools across three categories: Computer Use, Browser Use, and Code Mode.

## Computer Use (24 tools)

### `screenshot`
Capture screen as base64-encoded PNG.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `format` | string | No | `"png"` or `"jpeg"` (default: `"png"`) |
| `quality` | integer | No | JPEG quality 1-100 |

### `describe_screen`
Screenshot + OCR = text description of what's on screen.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `lang` | string | No | OCR language code (default: `"eng"`) |
| `find` | string[] | No | Words to search for, return positions |

### `find_text`
Find text on screen and return its bounding box.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `text` | string | Yes | Text to find |
| `screen` | integer | No | Screen index (multi-monitor) |

### `mouse_move`
Move mouse to absolute X,Y coordinates.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `x` | integer | Yes | X coordinate |
| `y` | integer | Yes | Y coordinate |

### `click`, `double_click`, `right_click`
Click at a position or at current cursor location.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `x` | integer | No | X coordinate |
| `y` | integer | No | Y coordinate |

### `mouse_drag`
Click, drag, and release.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `x1`, `y1` | integer | Yes | Start position |
| `x2`, `y2` | integer | Yes | End position |
| `duration` | float | No | Drag duration in seconds |

### `type_text`
Type text at the currently focused input field.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `text` | string | Yes | Text to type |

### `key_down`, `key_up`, `press_key`
Press, release, or press-and-release individual keys.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `key` | string | Yes | Key name (`"a"`, `"Enter"`, `"Escape"`, etc.) |

### `key_combo`
Hold modifiers and press a key.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `modifiers` | string[] | Yes | E.g., `["ctrl", "shift"]` |
| `key` | string | Yes | The key to press |

### `shell_run`
Execute a shell command. Requires `ALLOW_SHELL=1` env var.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `command` | string | Yes | Shell command to execute |
| `timeout` | integer | No | Timeout in seconds (default: 30, max: 300) |

### `window_list`, `window_focus`, `window_resize`, `window_close`
Window management tools. Parameters vary by tool.

### `clipboard_read`, `clipboard_write`
Read or write the system clipboard.

### `notify`
Send a desktop notification.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `title` | string | Yes | Notification title |
| `body` | string | No | Notification body |

### `discover`
Return environment info, installed tools, and capabilities.

### `server_status`
Server health check — returns version, uptime, available tools.

## Browser Use (17 tools)

All browser tools require a browser to be launched first via `browser_launch`.

### `browser_launch`
Launch headless Chromium or connect to a running desktop Chrome.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `mode` | string | No | `"auto"` (default), `"desktop"`, or `"headless"` |

### `browser_navigate`
Navigate to a URL.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `url` | string | Yes | Full URL to navigate to |

### `browser_click`
Click an element by CSS selector or X,Y coordinates.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `selector` | string | No | CSS selector |
| `x` | integer | No | X coordinate |
| `y` | integer | No | Y coordinate |

### `browser_type`
Type text into an input field.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `selector` | string | Yes | CSS selector for the input |
| `text` | string | Yes | Text to type |

### `browser_screenshot`
Screenshot the page or a specific element.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `selector` | string | No | CSS selector to screenshot |

### `browser_exec_js`
Execute JavaScript in the page context.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `code` | string | Yes | JavaScript code to execute |

### `browser_get_html`, `browser_get_text`
Get page HTML or visible text (optionally scoped to a selector).

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `selector` | string | No | CSS selector to scope to |

### `browser_wait_for`
Wait for a CSS selector or text to appear. Polls every 300ms.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `selector` | string | No | CSS selector to wait for |
| `text` | string | No | Text to wait for |
| `timeout` | float | No | Timeout in seconds (default: 30) |

### `browser_tabs`
List all open browser tabs (uses read lock — no serialization).

### `browser_new_tab`, `browser_close_tab`, `browser_switch_tab`
Tab management operations.

### `browser_cookies`, `browser_console`
Get cookies or console messages from the current page.

## Code Mode (8 tools)

All file paths are validated against `DESKMCP_WORKSPACE` (default: `$HOME/Projects`).

### `file_read`
Read a file with line numbers.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | Yes | File path (relative or absolute) |
| `offset` | integer | No | Line offset (0-based) |
| `limit` | integer | No | Max lines to return |

### `file_write`
Write or overwrite a file.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | Yes | File path |
| `content` | string | Yes | File content |

### `file_edit`
Replace an exact string in a file. Fails if the string is not unique.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | Yes | File path |
| `old_string` | string | Yes | Exact string to replace |
| `new_string` | string | Yes | Replacement string |
| `replace_all` | boolean | No | Replace all occurrences (default: false) |

### `grep`
Search files with regex patterns using ripgrep (falls back to grep).

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `pattern` | string | Yes | Regex pattern |
| `path` | string | No | Directory or file to search (default: `.`) |
| `glob` | string | No | File filter (e.g., `"*.rs"`) |
| `case_insensitive` | boolean | No | Case-insensitive search |

### `glob`
Find files matching a glob pattern. Uses native Rust glob (no subprocess).

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `pattern` | string | Yes | Glob pattern (e.g., `"**/*.rs"`) |
| `path` | string | No | Base directory (default: `.`) |

### `code_run`
Execute code in a supported language. Requires `ALLOW_CODE=1`.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `language` | string | Yes | `python`, `bash`, `node`, `ruby`, `perl`, `php` |
| `code` | string | Yes | Source code to execute |
| `timeout` | integer | No | Timeout in seconds (default: 30, max: 300) |
| `cwd` | string | No | Working directory |

### `code_lint`
Lint a file with the appropriate linter.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | Yes | File to lint |

Supported languages: `.rs` (Clippy), `.py` (Ruff), `.js/.ts` (ESLint), `.sh` (ShellCheck), `.go` (go vet)

### `code_build`
Build a project (auto-detects build system).

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | Yes | Project directory |
| `command` | string | No | Custom build command (default: auto-detect) |
| `timeout` | integer | No | Timeout in seconds (default: 120) |

Auto-detects: Cargo, npm, Make, Go, Python setup.py/pyproject.toml
