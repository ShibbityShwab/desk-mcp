# DeskMCP Architecture

## 1. System Overview

```
┌──────────────────────────────────────────────────────────────────┐
│                         DeskMCP (Rust)                            │
│                                                                  │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐      │
│  │  Computer Use  │  │  Browser Use   │  │   Discovery     │      │
│  │  24 tools      │  │  17 tools      │  │   2 tools       │      │
│  └───────┬────────┘  └───────┬────────┘  └───────┬────────┘      │
│          │                   │                    │               │
│  ┌───────┴───────────────────┴────────────────────┴────────┐     │
│  │                Provider Layer (trait)                     │     │
│  │  ┌─────────────────────┐  ┌──────────────────┐          │     │
│  │  │ KDE Wayland         │  │ Headless          │          │     │
│  │  │ spectacle, ydotool, │  │ graceful          │          │     │
│  │  │ kdotool, wl-paste   │  │ degradation       │          │     │
│  │  └─────────────────────┘  └──────────────────┘          │     │
│  └─────────────────────────────┬───────────────────────────┘     │
│                                │                                  │
│  ┌─────────────────────────────┴───────────────────────────┐     │
│  │          Auto-Discovery Engine                            │     │
│  │  Detects: display type, desktop env, browser CDP ports,  │     │
│  │  installed binaries, available capabilities              │     │
│  └───────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘
```

## 2. Transport: MCP JSON-RPC 2.0 over stdio

DeskMCP implements the Model Context Protocol (MCP) with a custom JSON-RPC 2.0 handler over stdin/stdout. No third-party MCP crate — just tokio, serde_json, and a `Content-Length` header parser.

```
Client                        DeskMCP
  │                              │
  │  Content-Length: 123\r\n     │
  │  \r\n                        │
  │  {"jsonrpc":"2.0",...}      │
  │ ──────────────────────────►  │  stdin reader thread
  │                              │  → mpsc channel → async handler
  │                 Content-Length: 456\r\n
  │                 \r\n
  │                 {"jsonrpc":"2.0","result":{...}}
  │  ◄──────────────────────────  stdout writer
```

**Methods**: `initialize`, `tools/list`, `tools/call`, `ping`, `notifications/*`

## 3. Provider Pattern

Rust `trait ComputerProvider` with two implementations:

```rust
trait ComputerProvider {
    fn screenshot(&self, region: Option<(i32,i32,u32,u32)>) -> Result<Vec<u8>>;
    fn get_screen_size(&self) -> Result<ScreenSize>;
    fn mouse_move(&self, x: i32, y: i32, smooth: bool, duration_ms: u64) -> Result<()>;
    fn mouse_click(&self, button: &str, x: Option<i32>, y: Option<i32>, clicks: u32) -> Result<()>;
    fn mouse_scroll(&self, dx: i32, dy: i32, x: Option<i32>, y: Option<i32>) -> Result<()>;
    fn mouse_drag(&self, x1, y1, x2, y2, button, duration_ms) -> Result<()>;
    fn keyboard_type(&self, text: &str, delay_ms: u64) -> Result<()>;
    fn key_press(&self, key: &str) -> Result<()>;
    fn clipboard_get(&self) -> Result<String>;
    fn clipboard_set(&self, text: &str) -> Result<()>;
    fn shell_run(&self, command: &str, timeout: u64) -> Result<ShellResult>;
    fn list_windows(&self) -> Result<Vec<WindowInfo>>;
    fn focus_window(&self, title: &str) -> Result<WindowMatch>;
    fn get_active_window(&self) -> Result<Option<WindowInfo>>;
    fn open_app(&self, name: &str) -> Result<()>;
    fn notify(&self, title: &str, message: &str, urgency: &str) -> Result<()>;
}
```

Provider is selected once at startup via `std::sync::LazyLock` and never changes.

## 4. Display / Screenshot Stack

### KDE Wayland (personal desktop)
- **spectacle CLI** — `spectacle -b -n -f -o /tmp/file.png` (~220ms, 2560×1440)
- Screenshot bytes are loaded into memory, optionally base64-encoded for MCP response
- Region capture supported: `spectacle -b -n -r {x},{y},{w},{h} -o /tmp/file.png`

### Headless
- All screenshot calls return `DEPENDENCY_MISSING` error
- Browser screenshots still work via chromiumoxide CDP (renders internally)

## 5. Input Stack

| Tool | Wayland | Headless | Notes |
|------|---------|----------|-------|
| **ydotool** | ✅ via `/dev/uinput` | ❌ | Mouse move, click, scroll, keyboard |
| **kdotool** | ✅ KWin D-Bus | ❌ | Window management |
| **wl-paste/wl-copy** | ✅ | ❌ | Clipboard |
| **Key code mapping** | 139 entries | — | Linux input key codes → ydotool |

## 6. Browser Stack

### chromiumoxide (pure Rust CDP)

```
  Browser::connect(url) → (Browser, Handler)   // connect to running Chrome
  Browser::launch(config) → (Browser, Handler)  // headless Chromium
  Page::goto(NavigateParams)
  Page::screenshot(ScreenshotParams) → Vec<u8>
  Page::evaluate(expr) → EvaluationResult
  Page::find_element(selector) → Element
  Element::click(), .type_str(), .press_key()
```

- **Desktop mode**: `Browser::connect("http://localhost:{port}")` — finds Chrome CDP port from `/proc` scanning
- **Headless mode**: `Browser::launch(BrowserConfig::new_headless_mode())` — fresh headless Chromium
- Handler event stream spawned as background tokio task

## 7. OCR Stack

```
Screenshot bytes → tesseract stdin stdout --psm 6 -l eng tsv
                        ↓
                  Parse TSV output
                        ↓
            Vec<OcrItem> { text, confidence, x, y, w, h }
                        ↓
            find_text() → click_on_text / wait_for_text
```

Tesseract TSV provides word-level bounding boxes at level 5. Confidence filtering available for low-quality matches.

## 8. Tool Dispatch

```
tools/call {"name":"screenshot","arguments":{...}}
        │
        ▼
  dispatch(name, args)
        │
        ├─ "screenshot".."type_to_window" → computer::handle()
        │                                      │
        │                                      └─ PROVIDER.method()
        │
        ├─ "browser_launch".."browser_console" → browser::handle()
        │                                           │
        │                                           └─ chromiumoxide Page/Element
        │
        └─ "discover" / "server_status" → inline detection
```

## 9. Response Contract

Every tool returns:

```json
{"ok": true, "result": {...}}

{"ok": false, "result": null, "error": {"code": "COMPUTER_ERROR", "message": "..."}}
```

Error codes: `COMPUTER_ERROR`, `BROWSER_ERROR`, `UNKNOWN_TOOL`, `DEPENDENCY_MISSING`, `NOT_AVAILABLE`

## 10. Project Structure

```
src/
├── main.rs             # Entry point, JSON-RPC 2.0 stdio server
├── lib.rs              # Library root, provider singleton, constants
├── response.rs         # ToolResponse struct + helpers
├── discovery.rs        # Environment auto-detection
├── ocr.rs              # Tesseract TSV parser
├── tools/
│   ├── mod.rs          # 42 tool definitions with JSON schemas, dispatch
│   ├── computer.rs     # 24 computer use handlers
│   └── browser.rs      # 17 browser use handlers (chromiumoxide)
└── providers/
    ├── mod.rs          # ComputerProvider trait + factory
    ├── kde_wayland.rs  # KDE Wayland (spectacle, ydotool, kdotool)
    └── headless.rs     # Headless graceful degradation
```

## 11. Dependencies

| Crate | Purpose |
|-------|---------|
| `tokio` | Async runtime |
| `serde` / `serde_json` | JSON-RPC messages, tool args/results |
| `chromiumoxide` | Chrome DevTools Protocol client |
| `enigo` | Keyboard simulation (fallback) |
| `image` | PNG decoding for OCR prep |
| `base64` | Screenshot encoding |
| `arboard` | Clipboard access |
| `tracing` / `tracing-subscriber` | Structured logging to stderr |
| `which` | Binary detection |
| `libc` | UID check |
| `async-trait` | Async trait methods |
| `anyhow` / `thiserror` | Error handling |
| `futures` | StreamExt for CDP handler events |

## 12. Performance

| Operation | Time |
|-----------|------|
| Startup + detection | <5ms |
| Screenshot (spectacle CLI) | ~220ms |
| OCR (Tesseract TSV, 1440p) | ~50ms |
| Mouse click (ydotool) | ~2ms |
| Keyboard type (ydotool) | ~10ms per char |
| Browser navigate + DOM ready | 500ms–2s |
| Browser screenshot (CDP) | ~100ms |

## 13. Future Roadmap

- [ ] PipeWire screencast helper (sub-50ms persistent capture)
- [ ] RapidOCR ONNX via `ort` crate (replaces tesseract subprocess)
- [ ] ACP transport alongside MCP
- [ ] Wayland non-KDE provider (wlroots, GNOME)
- [ ] X11 provider (xdotool)
- [ ] Firefox CDP support
