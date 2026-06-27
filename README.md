# AionUI Unified — State of the Art Computer Use + Browser Use MCP

Single MCP server providing full agentic control of a Linux system. CPU-only.
Dual-mode: personal desktop (with real auth/apps) or headless server (with virtualized display).

## Architecture

```
┌──────────────────────────────────────────────────┐
│              AionUI Unified MCP Server            │
│                                                   │
│  Computer Use (24 tools)    Browser Use (17)      │
│  mouse, keyboard, OCR,      Playwright CDP,       │
│  clipboard, windows,        cookies, console,     │
│  shell, notifications        downloads, uploads   │
│                                                   │
│              Discovery + Status (2)               │
│              auto-detect environment             │
└──────────────────────┬───────────────────────────┘
                       │
┌──────────────────────┴───────────────────────────┐
│           Provider Layer (Strategy Pattern)        │
│  wayland_kde  │  x11  │  headless                 │
└──────────────────────────────────────────────────┘
```

## Quick Start

```bash
# Install dependencies
sudo pacman -S ydotool spectacle wl-clipboard kdotool python-pip
pip install mcp pillow rapidocr-onnxruntime playwright
playwright install chromium

# Enable ydotool daemon
systemctl --user enable --now ydotoold

# Run
python3 server.py
```

## Dual Mode

| Mode | Display | Input | Browser | Use Case |
|------|---------|-------|---------|----------|
| **Personal Desktop** | Real display (KDE Wayland) | ydotool + kdotool | Connect to user's Chrome via CDP | Real auth, real apps |
| **Headless Server** | Xvfb virtual display | ydotool | Headless Chromium via Playwright | Server automation |

## Requirements

- Linux (KDE Wayland, GNOME, X11, or headless)
- Python 3.12+
- CPU-only (no GPU required)
- ydotool daemon for input
