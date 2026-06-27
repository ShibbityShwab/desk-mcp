# DeskMCP — Full Agentic Desktop Control MCP Server

Give any LLM full control of your Linux desktop. Screenshots, mouse, keyboard, OCR, window management, clipboard, shell, notifications, and Chrome CDP browser automation — all through a single MCP server. Written in Rust. CPU-only. 42 tools.

[![Rust](https://img.shields.io/badge/rust-1.96+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Linux-1793D1.svg)](#)

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    DeskMCP (Rust)                        │
│                                                         │
│  Computer Use (24 tools)    Browser Use (16 tools)      │
│  screenshot, mouse,         navigate, click, type,      │
│  keyboard, OCR, clipboard,  screenshot, JS exec,        │
│  windows, shell, notify     cookies, tabs, downloads    │
│                                                         │
│               Discovery + Status (2 tools)              │
│               auto-detect environment                   │
└──────────────────────┬──────────────────────────────────┘
                       │
┌──────────────────────┴──────────────────────────────────┐
│         Provider Layer (strategy pattern)                │
│  KDE Wayland  │  Headless fallback                      │
└─────────────────────────────────────────────────────────┘
```

## Quick Start

```bash
# Install system dependencies (Arch/CachyOS)
sudo pacman -S ydotool spectacle wl-clipboard tesseract tesseract-data-eng chromium

# Enable ydotool daemon (required for mouse/keyboard)
systemctl --user enable --now ydotoold

# Build
cargo build --release

# Run
./target/release/desk-mcp
```

## Dual Mode

| Mode | Display | Input | Browser | Use case |
|------|---------|-------|---------|----------|
| **Desktop (KDE Wayland)** | Real display via spectacle | ydotool + kdotool | Connect to running Chrome CDP | Personal computer automation |
| **Headless** | No display | ❌ mouse/keyboard unavailable | Headless Chromium via chromiumoxide | Server-side browser automation |

## Requirements

- **Linux** (KDE Wayland, or headless)
- **Rust 1.96+** (to build) or use the prebuilt binary
- **ytool daemon** (`systemctl --user enable --now ydotoold`)
- **spectacle** for screenshots, **tesseract** for OCR
- **chromium** for browser automation

## MCP Clients

Works with any MCP client — no vendor lock-in:

- **Claude Desktop** — native MCP support
- **Cline / Roo Code** — VS Code extensions
- **Continue.dev** — open-source AI code assistant
- **Cody (Sourcegraph)** — MCP integration
- **OpenHands / OpenDevin** — agentic coding
- **Goose (Block)** — MCP agent

Add to your MCP client config:

```json
{
  "mcpServers": {
    "desk-mcp": {
      "command": "/path/to/desk-mcp/target/release/desk-mcp",
      "args": [],
      "env": {
        "ALLOW_SHELL": "0"
      }
    }
  }
}
```

## Tools (42 total)

### Computer Use (24)
`screenshot` `get_screen_size` `mouse_move` `mouse_click` `mouse_double_click` `mouse_scroll` `mouse_drag` `keyboard_type` `key_press` `press_hotkey` `click_on_text` `wait_for_text` `extract_text` `describe_screen` `wait` `clipboard_get` `clipboard_set` `shell_run` `list_windows` `focus_window` `get_active_window` `open_app` `notify` `type_to_window`

### Browser Use (17)
`browser_launch` `browser_navigate` `browser_click` `browser_type` `browser_screenshot` `browser_exec_js` `browser_get_html` `browser_get_text` `browser_wait_for` `browser_tabs` `browser_new_tab` `browser_close_tab` `browser_switch_tab` `browser_download` `browser_upload` `browser_cookies` `browser_console`

### Discovery
`discover` `server_status`

## Security

- **`ALLOW_SHELL` gate** — shell execution is disabled by default. Set `ALLOW_SHELL=1` env var to enable.
- **No network exposure** — MCP runs over stdio, never opens ports
- **Local-only** — designed for desktop/headless server use, not multi-tenant

## How It Detects Your Setup

At startup (<5ms), DeskMCP probes:

| Signal | Desktop (KDE) | Headless |
|--------|--------------|----------|
| `WAYLAND_DISPLAY` | `wayland-0` | unset |
| `XDG_CURRENT_DESKTOP` | `KDE` | unset |
| `XDG_SESSION_TYPE` | `wayland` | unset |
| Browser CDP ports | Scanned from `/proc` | Launched fresh |
| **Provider** | `wayland_kde` | `headless` |

## OCR

Uses **Tesseract 5.5 LSTM** with TSV output for per-word bounding boxes and confidence scores. Powers `click_on_text`, `wait_for_text`, `extract_text`, and `describe_screen`.

## License

MIT — do whatever you want with it.
