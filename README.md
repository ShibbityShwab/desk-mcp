# desk-mcp — Full Desktop Control for AI Agents

[![CI](https://github.com/ShibbityShwab/desk-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/ShibbityShwab/desk-mcp/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Give AI agents (Claude, GPT, etc.) **full desktop control** — screenshots, mouse, keyboard,
OCR, browser automation, and code tools — all through a single MCP (Model Context Protocol) server.

50 tools. Pure Rust. One binary. Zero config for KDE + Wayland.

##  One-Line Install

```bash
curl -fsSL https://raw.githubusercontent.com/ShibbityShwab/desk-mcp/main/install.sh | bash
```

##  Quick Start

### 1. Install desk-mcp

```bash
# One-liner
curl -fsSL https://raw.githubusercontent.com/ShibbityShwab/desk-mcp/main/install.sh | bash

# Or from source
cargo install --git https://github.com/ShibbityShwab/desk-mcp
```

### 2. Configure your MCP client

Add this to your Claude Desktop config (`~/.config/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "desk-mcp": {
      "command": "desk-mcp"
    }
  }
}
```

### 3. Start using it

Ask your AI: *"Take a screenshot of my desktop and describe what you see"*

##  What It Can Do

###  Computer Use (24 tools)
| Tool | Description |
|------|-------------|
| `screenshot` | Capture screen as base64 PNG |
| `describe_screen` | Screenshot + OCR = text description |
| `find_text` | Locate text on screen, return position |
| `mouse_move` | Move mouse to X,Y |
| `click` | Click at position (or current location) |
| `double_click` | Double-click |
| `right_click` | Right-click |
| `mouse_drag` | Click, drag, release |
| `type_text` | Type text at current focus |
| `key_down` / `key_up` | Press/release individual keys |
| `press_key` | Press and release a key |
| `key_combo` | Hold modifiers + press key |
| `shell_run` | Run shell commands (guarded) |
| `env_get` | Read environment variables |
| `window_list` | List open windows |
| `window_focus` | Focus a window by title |
| `window_resize` | Resize a window |
| `window_close` | Close a window |
| `clipboard_read` / `clipboard_write` | Read/write clipboard |
| `notify` | Send desktop notification |
| `get_active_window_title` | Get focused window title |
| `discover` | Environment detection info |
| `server_status` | Server health + capabilities |

###  Browser Use (17 tools)
| Tool | Description |
|------|-------------|
| `browser_launch` | Launch or connect to Chromium |
| `browser_navigate` | Navigate to URL |
| `browser_click` | Click element (by selector or X,Y) |
| `browser_type` | Type into input field |
| `browser_screenshot` | Screenshot page or element |
| `browser_exec_js` | Execute JavaScript |
| `browser_get_html` | Get full HTML or element HTML |
| `browser_get_text` | Extract visible text |
| `browser_wait_for` | Wait for selector or text to appear |
| `browser_tabs` | List all tabs |
| `browser_new_tab` | Open new tab |
| `browser_close_tab` | Close a tab |
| `browser_switch_tab` | Switch to a tab |
| `browser_download` | Click download link |
| `browser_upload` | Click file upload input |
| `browser_cookies` | Get all cookies |
| `browser_console` | Get console messages |

###  Code Mode (8 tools)
| Tool | Description |
|------|-------------|
| `file_read` | Read file with line numbers |
| `file_write` | Write file |
| `file_edit` | Exact string replacement (replace one or all) |
| `grep` | Regex search across files |
| `glob` | Find files by pattern |
| `code_run` | Execute Python, Bash, Node, Ruby, Perl, PHP |
| `code_lint` | Lint code (Rust, Python, JS, Shell, Go) |
| `code_build` | Build project (auto-detect build system) |

##  Security

desk-mcp takes security seriously. All dangerous operations are **off by default**.

| Guard | Default | What it controls |
|-------|---------|-----------------|
| `ALLOW_SHELL=1` | OFF | Enables `shell_run` |
| `ALLOW_CODE=1` | OFF | Enables `code_run` |
| `DESKMCP_WORKSPACE` | `$HOME/Projects` | Sandboxes all file operations |

Set via MCP config:
```json
{
  "mcpServers": {
    "desk-mcp": {
      "command": "desk-mcp",
      "env": {
        "ALLOW_SHELL": "1",
        "ALLOW_CODE": "1",
        "DESKMCP_WORKSPACE": "/home/user/code"
      }
    }
  }
}
```

##  Environment Support

| OS | Desktop | Provider | Status |
|----|---------|----------|--------|
| Linux | KDE (Wayland) | `kde_wayland` | Full |
| Linux | KDE (X11) | `x11` | Partial |
| Linux | GNOME (Wayland) | `wayland_wlr` | Partial |
| Linux | Sway/Hyprland (wlr) | `wayland_wlr` | Partial |
| Linux | Headless/VNC | `headless` | Screenshots only |
| macOS | — | Not yet | Planned |
| Windows | — | Not yet | Planned |

### Dependencies

```bash
# Arch
sudo pacman -S spectacle ydotool tesseract tesseract-data-eng chromium

# Ubuntu/Debian
sudo apt install spectacle ydotool tesseract-ocr chromium-browser

# Fedora
sudo dnf install spectacle ydotool tesseract chromium
```

##  Architecture

```
MCP Client (Claude, GPT, etc.)
        │
        ▼
    desk-mcp (Rust binary)
        │
   ┌────┼─────────────┐
   │    │              │
   ▼    ▼              ▼
computer  browser    code
(24 tools) (17 tools) (8 tools)
   │
   ▼
providers (pluggable backends)
├── kde_wayland.rs  (KDE + Wayland)
├── headless.rs     (no display)
└── wlr.rs          (wlroots-based)
```

For detailed architecture: [ARCHITECTURE.md](ARCHITECTURE.md)

##  Performance

- **Browser**: `RwLock` for concurrent read access (no serialization on read-only ops)
- **Discovery**: `OnceLock` caching — detection runs once, O(1) thereafter
- **Glob**: Native Rust `glob` crate (no `find` subprocess)
- **Release**: LTO + single codegen unit + strip = minimal binary

Built with  in Rust. MIT licensed.
