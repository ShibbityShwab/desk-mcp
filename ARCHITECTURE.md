# Architecture — desk-mcp

## Overview

desk-mcp is a pure-Rust MCP (Model Context Protocol) server that wraps desktop
automation primitives behind a clean, versioned JSON-RPC API. AI agents
call tools by name with JSON arguments and receive `{ok, result, error}` responses.

## Layer Stack

```
┌──────────────────────────────────────┐
│          MCP Client (AI agent)        │
├──────────────────────────────────────┤
│         stdin/stdout JSON-RPC        │
│         (MCP protocol transport)      │
├──────────────────────────────────────┤
│            tools/mod.rs              │
│  ┌──────────┬──────────┬──────────┐  │
│  │ computer │ browser  │  code    │  │
│  │ (24)     │ (17)     │ (8)      │  │
│  └──────────┴──────────┴──────────┘  │
├──────────────────────────────────────┤
│       response.rs  ←  error.rs       │
│  (unified {ok, result, error})       │
├──────────────────────────────────────┤
│         discovery.rs (cached)        │
│     Environment & capability scan    │
├──────────────────────────────────────┤
│          providers/                  │
│  ┌────────────┬──────────────┬──────┐│
│  │kwin_dbus   │kde_wayland   │browsr││
│  │(native KWin│(wdotool-core │_ext  ││
│  │ D-Bus)     │ + ydotool)   │(WS)  ││
│  ├────────────┼──────────────┼──────┤│
│  │  headless  │  mock        │macOS ││
│  │(no display)│(testing)     │/Win  ││
│  └────────────┴──────────────┴──────┘│
├──────────────────────────────────────┤
│           System tools               │
│  spectacle, grim, ydotool, xdotool,  │
│  tesseract, chromium                 │
└──────────────────────────────────────┘
```

## Key Modules

### `main.rs` — Entry Point
- Parses `--version`, `--help`, and optional port
- Initializes tracing subscriber
- Starts the MCP server on stdin/stdout

### `lib.rs` — Crate Root
- Re-exports all public modules
- Exposes `SERVER_NAME` and `SERVER_VERSION` constants

### `tools/mod.rs` — Tool Dispatch
- Receives `{name, arguments}` from MCP transport
- Routes to the appropriate handler module
- Returns `ToolResponse` (ok or error)

### `tools/computer.rs` — Computer Use (24 tools)
- Screenshot, OCR, mouse, keyboard, shell execution
- Window management, clipboard, notifications
- Discovery and server status endpoints
- All desktop interaction goes through the provider abstraction

### `tools/browser.rs` — Browser Use (17 tools)
- Chromium automation via chromiumoxide (CDP)
- Connect to running desktop Chrome or launch headless
- Page navigation, element interaction, JS evaluation
- Tab management, downloads, cookies, console

**Lock Architecture (optimized for concurrency):**
- `BROWSER: RwLock<Option<BrowserState>>` — global browser handle
- `get_page()` → `.read().await` — shared, non-blocking
- `browser_tabs()`, `browser_cookies()` → `.read().await`
- `browser_launch()`, `browser_new_tab()`, `browser_close_tab()`, `browser_switch_tab()` → `.write().await`
- Per-page network calls (title, URL) happen **outside** the lock

### `tools/code.rs` — Code Mode (8 tools)
- File I/O (read, write, edit with exact string replacement)
- Search (grep via ripgrep/grep, glob via native Rust `glob` crate)
- Code execution (Python, Bash, Node, Ruby, Perl, PHP)
- Linting (Ruff, Clippy, ESLint, ShellCheck, go vet)
- Build (auto-detect Cargo, npm, Make, Go, Python)

**Performance notes:**
- All file I/O uses `tokio::fs` (async, non-blocking)
- `glob_search` uses the native Rust `glob` crate (no `find` subprocess)
- Failover to `find` only on glob pattern parse errors

### `discovery.rs` — Environment Detection (cached)
- Scans for installed tools (`which`)
- Detects running browsers via `/proc`
- Identifies display type, desktop environment, and capabilities
- Cached via `OnceLock` — runs once, O(1) thereafter
- `refresh_discovery()` for manual re-scan

### `response.rs` — Unified Response Contract
```json
{
  "ok": true,
  "result": {...},
  "error": null
}
```
```json
{
  "ok": false,
  "result": null,
  "error": {
    "code": "DEPENDENCY_MISSING",
    "message": "screenshot requires spectacle: pacman -S spectacle",
    "detail": null
  }
}
```

### `error.rs` — Typed Error Handling
- `McpError` enum with 13 structured variants
- Each variant has a stable error code (e.g., `DEPENDENCY_MISSING`, `TIMEOUT`)
- `From` impls for `serde_json::Error`, `anyhow::Error`, `std::io::Error`
- Convenience `.code()` method for response formatting

### `ocr.rs` — OCR via Tesseract
- Pipes image bytes to `tesseract` subprocess
- Parses TSV output into structured `OcrItem` structs
- `find_text()` for substring matching with confidence threshold

## Provider Plugins

Each provider implements the platform-specific primitives:
- **kde_wayland.rs** — Uses spectacle (screenshots), kdotool/ydotool (input)
- **headless.rs** — Returns env vars and filesystem info only
- **x11.rs** (planned) — xdotool-based
- **wlr.rs** (planned) — grim+ydotool for wlroots-based compositors

## Data Flow

```
Tool Call: "screenshot"
  → tools/mod.rs routes to computer::handle("screenshot", args)
  → computer::handle dispatches to providers::take_screenshot()
  → provider runs spectacle/grim, reads PNG bytes
  → base64-encodes PNG
  → response::ok({image_base64: "...", format: "png"})
  → serialized to JSON-RPC response
```

## Release Profile

```
[profile.release]
lto = "fat"           # Full LTO across all crates
codegen-units = 1     # Single codegen unit for max optimization
opt-level = 3         # Aggressive optimizations
strip = true          # Strip debug symbols
panic = "abort"       # Abort on panic (smaller binary, no unwind tables)
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| `chromiumoxide` | Chrome DevTools Protocol (pure Rust) |
| `serde` + `serde_json` | Serialization |
| `tokio` | Async runtime |
| `base64` | Image encoding |
| `glob` | Native file globbing |
| `which` | Binary discovery |
| `anyhow` | Error handling |
| `thiserror` | Typed error derivation |
| `tracing` | Structured logging |
| `tempfile` | Temporary file/dir creation |
| `wdotool-core` | Wayland input via libei |
| `zbus` | KWin D-Bus window management |
| `atspi` | Pure-Rust AT-SPI client (stub) |
| `dashmap` | Concurrent session registry |
| `serde_yaml` | Policy engine config |
| `libloading` | Dynamic plugin loading |
| `uuid` | Session identifiers |
| `dirs-next` | XDG config directory paths |

## Concurrency Model

desk-mcp uses `tokio` with the multi-threaded runtime. Each MCP tool call is
handled as a separate async task. Long-running operations (browser launch,
shell execution) have configurable timeouts.

**Browser state** is protected by a `tokio::sync::RwLock` to allow concurrent
read access — multiple browser tools can operate on the same page simultaneously
without serializing on the lock.
