# AGENTS.md — desk-mcp Tool Reference for AI Agents

> **For AI agents only.** This document is designed to be loaded into your
> system prompt. It contains schema-first tool definitions and worked
> JSON-RPC examples. Human readers should see [docs/tools.md](tools.md).

## Quick Reference

All 63 tools with required parameters and return types.

| Tool | Category | Required Params | Returns |
|------|----------|----------------|---------|
| `screenshot` | computer | — | `{image_base64, width, height, format}` |
| `get_screen_size` | computer | — | `{width, height}` |
| `mouse_move` | computer | `x`, `y` | `{x, y}` |
| `mouse_click` | computer | — | `{button, x, y, clicks}` |
| `mouse_double_click` | computer | — | `{button, x, y}` |
| `mouse_scroll` | computer | — | `{dx, dy}` |
| `mouse_drag` | computer | `x1`, `y1`, `x2`, `y2` | `{from, to, duration_ms}` |
| `keyboard_type` | computer | `text` | `{text_len}` |
| `key_press` | computer | `key` | `{key}` |
| `press_hotkey` | computer | `keys` | `{keys}` |
| `click_on_text` | computer | `text` | `{text, bounds, clicked_at}` |
| `wait_for_text` | computer | `text` | `{text, found_at}` |
| `extract_text` | computer | — | `{items: [{text, bounds, confidence}]}` |
| `describe_screen` | computer | — | `{description, elements: [...]}` |
| `wait` | computer | `seconds` | `{elapsed}` |
| `clipboard_get` | computer | — | `{text}` |
| `clipboard_set` | computer | `text` | `{set: true}` |
| `env_get` | computer | `name` | `{name, value}` |
| `shell_run` | computer | `command` | `{returncode, stdout, stderr}` |
| `list_windows` | computer | — | `{windows: [{id, title, app, pid, geometry}]}` |
| `focus_window` | computer | `title` | `{matched, id, title, app}` |
| `get_active_window` | computer | — | `{id, title, app, pid, geometry}` |
| `open_app` | computer | `name` | `{launched, name}` |
| `notify` | computer | `title`, `message` | `{sent: true}` |
| `get_window_state` | computer | — | `{window, elements: [...], element_count}` |
| `type_to_window` | computer | `title`, `text` | `{focused, typed}` |
| `browser_launch` | browser | — | `{mode, ws_url, pid}` |
| `browser_navigate` | browser | `url` | `{url, title, loaded}` |
| `browser_click` | browser | — | `{selector, text, clicked}` |
| `browser_type` | browser | `selector`, `text` | `{selector, text_len}` |
| `browser_screenshot` | browser | — | `{image_base64, width, height}` |
| `browser_exec_js` | browser | `code` | `{result}` |
| `browser_get_html` | browser | — | `{html, length}` |
| `browser_get_text` | browser | — | `{text, length}` |
| `browser_wait_for` | browser | — | `{found, selector, text}` |
| `browser_tabs` | browser | — | `{tabs: [{index, title, url, active}]}` |
| `browser_new_tab` | browser | — | `{index, url}` |
| `browser_close_tab` | browser | — | `{closed, remaining}` |
| `browser_switch_tab` | browser | — | `{index, title, url}` |
| `browser_download` | browser | `selector` | `{file_path, size}` |
| `browser_upload` | browser | `selector`, `files` | `{uploaded: [paths]}` |
| `browser_cookies` | browser | — | `{cookies: [{name, value, domain}]}` |
| `browser_console` | browser | — | `{messages: [{level, text}]}` |
| `browser_refresh` | browser | — | `{discovered_browsers, browser_automation}` |
| `file_read` | code | `path` | `{path, lines: [{num, text}], total}` |
| `file_write` | code | `path`, `content` | `{path, bytes_written}` |
| `file_edit` | code | `path`, `old`, `new` | `{path, replaced, count}` |
| `grep` | code | `pattern` | `{matches: [{file, line, text}], count}` |
| `glob` | code | `pattern` | `{files: [paths], count}` |
| `code_run` | code | `language`, `code` | `{stdout, stderr, returncode}` |
| `code_lint` | code | `language`, `code` | `{issues: [{line, message, severity}]}` |
| `code_build` | code | — | `{success, output}` |
| `find_elements` | a11y | — | `{elements: [{index, role, name, bounds}], count}` |
| `get_element_text` | a11y | `path` or `index` | `{index, role, name, text, children}` |
| `click_element` | a11y | `path` or `index` | `{clicked, index, role, position}` |
| `get_window_tree` | a11y | — | `{tree, element_count, max_depth}` |
| `web_search` | web | `query` | `{results: [{title, url, snippet}], count}` |
| `web_fetch` | web | `url` | `{url, status, content_type, content, truncated}` |
| `request_confirmation` | safety | `tool`, `message` | `{id, status: "pending"}` |
| `approve` | safety | `id` | `{id, status: "approved"}` |
| `deny` | safety | `id` | `{id, status: "denied"}` |
| `list_pending` | safety | — | `{pending: [{id, tool, message, created}]}` |
| `server_status` | status | — | `{server, version, provider, available: {...}}` |

## Resolution Tiers

desk-mcp resolves UI targets through three tiers. When you call a tool like
`mouse_click` at semantic coordinates, the resolution router determines which
tier provided them:

| Tier | Mechanism | Latency | Used When | Output |
|------|-----------|---------|-----------|--------|
| 1. AT-SPI | Linux accessibility tree (`atspi` D-Bus) | ~8 ms | Native Linux apps (GTK, Qt with `QT_ACCESSIBILITY=1`) | Exact element bounds, role, label, enabled/focused state |
| 2. CDP | Chrome DevTools Protocol (`chromiumoxide`) | ~12 ms | Browser content (any web page) | CSS selector, DOM bounding rect, text content |
| 3. OCR | Tesseract + Sobel edge detection (`leptess`) | ~500 ms | Fallback for legacy and Electron apps | Text with bounding boxes, clickable region candidates |

**Routing logic:**

- `click_on_text` / `extract_text` → always uses OCR (Tier 3), unless a
  browser is active and the target is in web content (falls through to CDP)
- `find_elements` / `click_element` → always uses AT-SPI (Tier 1)
- `browser_click` / `browser_type` → always uses CDP (Tier 2)
- `get_window_state` → AT-SPI (Tier 1) with OCR fallback for element text

**Tip:** Before clicking at coordinates, call `describe_screen` or
`find_elements` to get structured element data. Avoid OCR-only workflows
when AT-SPI or CDP is available — they are 50-100× faster.

## JSON-RPC 2.0 Protocol

All communication uses JSON-RPC 2.0 over stdio (default) or HTTP POST
`/mcp` (when `--http` is active).

### Request format

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "mouse_click",
    "arguments": {"x": 400, "y": 300}
  }
}
```

### Response format (success)

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [{
      "type": "text",
      "text": "{\"ok\":true,\"result\":{\"button\":\"left\",\"x\":400,\"y\":300,\"clicks\":1},\"error\":null}"
    }]
  }
}
```

The inner `text` field is always a JSON string encoding the `ToolResponse`:

```json
{
  "ok": true,
  "result": { /* tool-specific output */ },
  "error": null
}
```

### Response format (error)

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [{
      "type": "text",
      "text": "{\"ok\":false,\"result\":null,\"error\":{\"code\":\"RATE_LIMITED\",\"message\":\"Rate limit reached for 'mouse_click'. Wait a moment before retrying.\"}}"
    }]
  }
}
```

### Initialization

Before calling tools, send `initialize`:

```json
{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}
```

Response:
```json
{"jsonrpc":"2.0","id":0,"result":{"protocolVersion":"2024-11-05","serverInfo":{"name":"desk-mcp","version":"0.5.0"},"capabilities":{"tools":{}}}}
```

## Example: Click a Button

Find a button in a native app and click it using AT-SPI (Tier 1).

**Step 1: Find the button**
```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"find_elements","arguments":{"role":"push button","name_contains":"Submit"}}}
```

Response:
```json
{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"{\"ok\":true,\"result\":{\"elements\":[{\"index\":7,\"role\":\"push button\",\"name\":\"Submit\",\"bounds\":{\"x\":540,\"y\":380,\"width\":120,\"height\":36},\"enabled\":true,\"focused\":false,\"actions\":[\"click\",\"press\"]}],\"count\":1,\"total\":42,\"window\":{\"title\":\"Registration Form\",\"app\":\"gtk4-demo\"}},\"error\":null}"}]}
```

**Step 2: Click it**
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"click_element","arguments":{"path":7}}}
```

Response:
```json
{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"{\"ok\":true,\"result\":{\"clicked\":true,\"index\":7,\"role\":\"push button\",\"name\":\"Submit\",\"position\":{\"x\":600,\"y\":398}},\"error\":null}"}]}
```

## Example: Fill a Web Form

Use CDP (Tier 2) to interact with a web page.

**Step 1: Launch browser**
```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"browser_launch","arguments":{"mode":"headless"}}}
```

**Step 2: Navigate**
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"browser_navigate","arguments":{"url":"https://example.com/login"}}}
```

**Step 3: Fill fields**
```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"browser_type","arguments":{"selector":"#email","text":"user@example.com","clear":true}}}
```
```json
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"browser_type","arguments":{"selector":"#password","text":"s3cret","press_enter":true}}}
```

**Step 4: Verify result**
```json
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"browser_get_text","arguments":{"selector":".dashboard-title"}}}
```

Response:
```json
{"jsonrpc":"2.0","id":5,"result":{"content":[{"type":"text","text":"{\"ok\":true,\"result\":{\"text\":\"Welcome, User\",\"length\":13},\"error\":null}"}]}
```

## Example: Read a Native App Window

Use AT-SPI (Tier 1) to inspect a native app without screenshots.

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_window_state","arguments":{}}}
```

Response (abbreviated):
```json
{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"{\"ok\":true,\"result\":{\"window\":{\"id\":\"0x3a00007\",\"title\":\"Settings\",\"app\":\"systemsettings\",\"pid\":12345,\"geometry\":{\"x\":100,\"y\":50,\"width\":900,\"height\":600}},\"elements\":[{\"index\":0,\"role\":\"frame\",\"name\":\"Settings\",\"bounds\":{\"x\":100,\"y\":50,\"width\":900,\"height\":600},\"enabled\":true,\"actions\":[]},{\"index\":1,\"role\":\"page tab list\",\"name\":\"\",\"bounds\":{\"x\":110,\"y\":60,\"width\":880,\"height\":40},\"enabled\":true,\"children\":[2,3,4]},{\"index\":2,\"role\":\"page tab\",\"name\":\"Appearance\",\"bounds\":{\"x\":110,\"y\":60,\"width\":120,\"height\":40},\"enabled\":true,\"focused\":true,\"actions\":[\"click\"]}],\"element_count\":87},\"error\":null}"}]}
```

If you need text from a specific element, query it directly:
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"find_elements","arguments":{"role":"label","max_results":10}}}
```

## Example: Search the Web

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"web_search","arguments":{"query":"desk-mcp GitHub","max_results":3}}}
```

Response:
```json
{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"{\"ok\":true,\"result\":{\"results\":[{\"title\":\"ShibbityShwab/desk-mcp\",\"url\":\"https://github.com/ShibbityShwab/desk-mcp\",\"snippet\":\"Full desktop control MCP server — screenshots, mouse, keyboard, OCR, browser automation, code tools\"}],\"count\":1},\"error\":null}"}]}
```

To read a result page:
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"web_fetch","arguments":{"url":"https://github.com/ShibbityShwab/desk-mcp","format":"text","max_bytes":10000}}}
```

## Example: Confirmation Flow

Nine tools require user confirmation before executing. You must call
`request_confirmation`, receive the `id`, then wait for `approve`.

**Step 1: Request confirmation**
```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"request_confirmation","arguments":{"tool":"shell_run","message":"About to execute: rm -rf /tmp/test-build","params":{"command":"rm -rf /tmp/test-build","timeout":10}}}}
```

Response:
```json
{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"{\"ok\":true,\"result\":{\"id\":\"a1b2c3d4-e5f6-7890-abcd-ef1234567890\",\"status\":\"pending\"},\"error\":null}"}]}
```

**Step 2: Wait for approval**

The user (human) must issue:
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"approve","arguments":{"id":"a1b2c3d4-e5f6-7890-abcd-ef1234567890"}}}
```

Response:
```json
{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"{\"ok\":true,\"result\":{\"id\":\"a1b2c3d4-e5f6-7890-abcd-ef1234567890\",\"status\":\"approved\"},\"error\":null}"}]}
```

**Step 3: Execute the gated tool**

Now you can call the actual tool (the confirmation was a gate, not a proxy):
```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"shell_run","arguments":{"command":"rm -rf /tmp/test-build"}}}
```

**Gated tools:** `shell_run`, `file_write`, `file_edit`, `code_run`,
`code_build`, `browser_download`, `mouse_click`, `keyboard_type`, `open_app`.

To check for pending confirmations:
```json
{"jsonrpc":"2.0","id":99,"method":"tools/call","params":{"name":"list_pending","arguments":{}}}
```

## Error Codes

All errors follow the structure `{"ok": false, "error": {"code": "...", "message": "..."}}`.

| Code | Meaning | When |
|------|---------|------|
| `DEPENDENCY_MISSING` | Required system tool not installed | `spectacle`, `tesseract`, `ydotool`, `chromium` not found |
| `NOT_IMPLEMENTED` | Feature not available in this environment | Calling window management in headless mode |
| `TIMEOUT` | Operation exceeded deadline | Tool handler took >60s, or sub-operation timed out |
| `BROWSER_NOT_LAUNCHED` | Browser must be launched first | Calling `browser_navigate` before `browser_launch` |
| `BROWSER_ERROR` | Browser operation failed | CDP connection lost, invalid selector, page crash |
| `IO_ERROR` | Filesystem operation failed | Disk full, permission denied, file not found |
| `FILE_ERROR` | Invalid file operation | Writing outside workspace, binary file, path traversal |
| `PATH_ERROR` | Path outside workspace | `DESKMCP_WORKSPACE` constraint violated |
| `SHELL_NOT_ALLOWED` | `ALLOW_SHELL=1` required | Calling `shell_run` without the env var |
| `CODE_NOT_ALLOWED` | `ALLOW_CODE=1` required | Calling `code_run` without the env var |
| `UNKNOWN_TOOL` | Tool name not recognized | Typo or wrong tool name; response includes suggestions |
| `TOOL_ERROR` | Generic tool failure | Catch-all for unexpected errors |
| `JSON_ERROR` | Malformed JSON in arguments | Invalid JSON, missing required fields, type mismatch |
| `RATE_LIMITED` | Per-tool rate limit exceeded | More than 30 calls/min for a single tool |
| `NOT_FOUND` | Confirmation ID not found | Approving/denying an expired or unknown confirmation |
| `INVALID_ARGS` | Missing or invalid parameters | Required parameter omitted, wrong type |
| `A11Y_ERROR` | AT-SPI accessibility query failed | No accessibility bus, app doesn't expose tree |

## Rate Limits & Timeouts

- **Rate limit:** 30 calls per minute per tool, burst of 5. The token bucket
  refills at 0.5 tokens/second. When depleted, calls return `RATE_LIMITED`
  immediately — no queuing.

- **Hard timeout:** 60 seconds per tool call. If a handler (especially
  `browser_launch` during CDP WebSocket handshake) takes longer, the call
  is cancelled and returns `TIMEOUT`.

- **Sub-timeouts:**
  - `shell_run`: 30 s default, 600 s max
  - `code_run`: 30 s default, 600 s max
  - `browser_launch`: 25 s for CDP WebSocket
  - `browser_wait_for`: 30 s default
  - `web_search` / `web_fetch`: 15 s / 20 s HTTP timeout

- **Workspace sandbox:** All `file_read`, `file_write`, `file_edit`, `grep`,
  `glob` operations are confined to `DESKMCP_WORKSPACE` (default
  `$HOME/Projects`). Path traversal (`../../etc/passwd`) is blocked via
  canonical path resolution.

- **HTTP auth:** When using `--http`, every request must include
  `Authorization: Bearer <token>` or `?token=<token>`. The token is
  auto-generated on first start (saved to `~/.config/desk-mcp/token`) and
  printed to stderr. Without a valid token, the server returns HTTP 401
  with JSON-RPC error code `-32001`.

## Transport Modes

desk-mcp listens on one transport per process:

| Mode | Flag | Default | For |
|------|------|---------|-----|
| stdio | *(default)* | stdin/stdout | Local MCP clients (Claude Desktop, Cline) |
| HTTP/SSE | `--http [addr]` | `127.0.0.1:9273` | Remote agents, Docker, headless servers |

Both modes use the same JSON-RPC 2.0 dispatch. The HTTP mode adds bearer
token auth and CORS headers.
