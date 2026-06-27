---
layout: default
title: desk-mcp
---

# desk-mcp — Full Desktop Control for AI Agents

Give AI agents **full desktop control** — screenshots, mouse, keyboard,
OCR, browser automation, and code tools — through a single MCP server.

##  One-Line Install

```bash
curl -fsSL https://raw.githubusercontent.com/ShibbityShwab/desk-mcp/main/install.sh | bash
```

##  What It Does

- ** Screenshot & OCR** — Capture screens and read text on them
- ** Mouse & Keyboard** — Full input automation
- ** Browser Automation** — Chrome/Chromium via CDP (17 tools)
- ** Code Tools** — Read, write, edit, search, execute, lint, build (8 tools)
- ** Window Management** — List, focus, resize, close windows
- ** Clipboard & Notifications** — Read/write clipboard, send OS notifications

##  Quick Links

- [Installation Guide](install)
- [Tool Reference](tools)
- [Configuration](configuration)
- [Security Model](security)
- [GitHub Repository](https://github.com/ShibbityShwab/desk-mcp)

##  Supported Environments

| OS | Desktop | Status |
|----|---------|--------|
| Linux | KDE (Wayland) | ✅ Full |
| Linux | KDE (X11) | ⚠️ Partial |
| Linux | GNOME / Sway / Hyprland | ⚠️ Partial |
| Linux | Headless / VNC | ⚠️ Screenshots only |

##  License

MIT — see [LICENSE](https://github.com/ShibbityShwab/desk-mcp/blob/main/LICENSE)
