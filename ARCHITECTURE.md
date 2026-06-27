# AionUI Unified MCP Server — Architecture v1.0

## Council of Experts — Synthesized Design
**Date:** 2026-06-27
**Target:** Single state-of-the-art MCP for full computer use + browser use, CPU-only, dual-mode (headless server + personal desktop)

---

## 1. System Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    AionUI Unified MCP Server                 │
│                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │  Computer Use │  │  Browser Use │  │  Discovery    │      │
│  │  24 tools     │  │  17 tools    │  │  4 tools      │      │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘      │
│         │                 │                 │               │
│  ┌──────┴─────────────────┴─────────────────┴───────┐      │
│  │              Provider Layer (Strategy)             │      │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────┐ │      │
│  │  │ KDE      │ │ wlroots  │ │ X11      │ │Head- │ │      │
│  │  │ Wayland  │ │ Wayland  │ │ (xdotool)│ │less  │ │      │
│  │  │(kdotool) │ │ (ydotool)│ │          │ │      │ │      │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────┘ │      │
│  └──────────────────────┬───────────────────────────┘      │
│                         │                                   │
│  ┌──────────────────────┴───────────────────────────┐      │
│  │              Auto-Discovery Engine                 │      │
│  │  Detects: display type, desktop env, available    │      │
│  │  tools, browsers, auth sources, installed apps    │      │
│  └──────────────────────────────────────────────────┘      │
└─────────────────────────────────────────────────────────────┘
```

## 2. Mode Detection

At startup (<20ms), detect environment:

| Signal | Desktop (KDE) | Desktop (Other) | Headless |
|--------|--------------|-----------------|----------|
| WAYLAND_DISPLAY | wayland-0 | wayland-1 | (unset) |
| XDG_CURRENT_DESKTOP | KDE | GNOME/Sway/... | (unset) |
| Physical GPU (DRM) | ✅ | ✅ | ❌ |
| DISPLAY | :0 | :0 | :99 (Xvfb) |
| **Provider** | `wayland_kde` | `wayland_wlr` or `x11` | `headless` |

## 3. Display/Screenshot Stack

### Personal Desktop (KDE Wayland)
- **Primary:** `spectacle -b -n -f -o /tmp/file.png` (~220ms, 2560×1440)
- **Advanced:** Persistent PipeWire screencast helper (sub-50ms, requires C helper linking libKPipeWireRecord)
- **Fallback:** Spectacle DBus API (~140ms, requires spectacle in background mode)

### Headless Server
- **Browser:** Playwright's built-in `headless=True` (no display needed)
- **Full desktop:** Xvfb virtual display (`Xvfb :99 -screen 0 1920x1080x24`)
- **Screenshot:** `xdotool` or `import` (ImageMagick) on Xvfb

## 4. OCR Stack (Hybrid Two-Tier)

```
Screenshot → Tier 1: Tesseract 5.5 LSTM (~50ms, fast)
                ↓ low confidence?
             Tier 2: RapidOCR ONNX (~110ms, precise)
                ↓ still low?
             Tier 3: Crop region → re-OCR with RapidOCR
```

| Engine | Speed (1440p) | Accuracy (UI) | CPU Load |
|--------|--------------|---------------|----------|
| Tesseract 5.5 LSTM | ~50ms | ★★★☆☆ | Very Low |
| RapidOCR ONNX | ~110ms | ★★★★☆ | Low |
| Tesseract + preprocess | ~80ms | ★★★★☆ | Low |

Preprocessing: CLAHE contrast enhancement → 1.5x upscale → Tesseract

## 5. Input Stack

| Tool | X11 | Wayland | Headless | Notes |
|------|-----|---------|----------|-------|
| **ydotool** | ✅ | ✅ | ✅ | Primary universal backend via /dev/uinput |
| **kdotool** | ❌ | ✅ (KDE) | ❌ | KDE-native, used when KDE detected |
| **xdotool** | ✅ | ❌ | ✅ | X11/Xvfb fallback |

Keyboard: ydotool `key` and `type` commands with full Linux input key code mapping
Mouse: ydotool `mousemove`, `click`, `bakers --wheel`

## 6. Browser Stack

### Playwright (primary)
- `playwright.async_api` — async Python API
- **Desktop mode:** `chromium.connect_over_cdp("http://127.0.0.1:9222")` — connects to user's running Chrome
- **Headless mode:** `chromium.launch(headless=True)` — fresh Chromium
- **Firefox:** `firefox.launch_persistent_context()` — launch-only, can't connect to existing
- CPU-only flags: `--disable-gpu`, `--disable-software-rasterizer`

### Self-Discovery
- Scan processes for `--remote-debugging-port` via `psutil`
- Check `~/.config/google-chrome/DevToolsActivePort`
- Check common browser binary paths

## 7. Window Management

### Personal Desktop (KDE)
- `kdotool search`, `getwindowname`, `getwindowgeometry`, `windowactivate` via KWin D-Bus

### Cross-Environment Fallback
- AT-SPI (`pyatspi`) — accessibility tree, works on any DE with AT-SPI enabled
- X11 fallback: `xdotool search`, `getwindowname`, `windowactivate`

## 8. Unified Response Contract

Every tool returns:
```json
{"ok": true, "result": {...}, "error": null}
// or
{"ok": false, "result": null, "error": {"code": "...", "message": "...", "detail": "..."}}
```

Error codes: `DEPENDENCY_MISSING`, `TIMEOUT`, `PROVIDER_ERROR`, `INVALID_ARGS`, `PERMISSION_DENIED`, `NOT_IMPLEMENTED`

## 9. Complete Tool List (42 tools)

### Computer Use (24 tools)
1. `screenshot` — Capture screen/region as base64 PNG/JPEG
2. `get_screen_size` — Display resolution
3. `mouse_move` — Move cursor (teleport or smooth)
4. `mouse_click` — Click at position
5. `mouse_double_click` — Double-click
6. `mouse_scroll` — Scroll wheel
7. `mouse_drag` — Click and drag
8. `keyboard_type` — Type text string
9. `key_press` — Press key/combo (ctrl+c, alt+Tab, etc.)
10. `press_hotkey` — Multiple key combo
11. `click_on_text` — OCR → find text → click
12. `wait_for_text` — Poll for text appearance
13. `extract_text` — OCR full screen/region → all text with coords
14. `describe_screen` — AI summary of screen content
15. `wait` — Sleep N seconds
16. `clipboard_get` — Read clipboard
17. `clipboard_set` — Write clipboard
18. `shell_run` — Execute shell command (gated by ALLOW_SHELL env var)
19. `list_windows` — Enumerate windows
20. `focus_window` — Activate window by title/app match
21. `get_active_window` — Currently focused window info
22. `open_app` — Launch application by name
23. `notify` — Send desktop notification
24. `type_to_window` — Focus window → type text

### Browser Use (17 tools)
25. `browser_launch` — Launch/connect to browser
26. `browser_navigate` — Navigate to URL
27. `browser_click` — Click element by selector/text/coordinates
28. `browser_type` — Type into input field
29. `browser_screenshot` — Screenshot page/element
30. `browser_exec_js` — Execute JavaScript
31. `browser_get_html` — Get page HTML
32. `browser_get_text` — Get visible text content
33. `browser_wait_for` — Wait for selector/text
34. `browser_tabs` — List open tabs
35. `browser_new_tab` — Open new tab
36. `browser_close_tab` — Close tab
37. `browser_switch_tab` — Switch to tab by index/title
38. `browser_download` — Trigger download
39. `browser_upload` — Upload file(s)
40. `browser_cookies` — Get/set cookies
41. `browser_dialog` — Handle alert/confirm/prompt
42. `browser_console` — Get console messages

### Discovery & Status (2 tools)
43. `discover` — Report all detected capabilities, browsers, apps
44. `server_status` — Health check: uptime, memory, tool availability
