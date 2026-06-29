# desk-mcp — The Protocol Layer Between AI and the Graphical World

> Single binary. Pure Rust. Full desktop control for AI agents.

[![CI](https://github.com/ShibbityShwab/desk-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/ShibbityShwab/desk-mcp/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/desk-mcp)](https://crates.io/crates/desk-mcp)

## Philosophy

*AI agents deserve the same control over computers that humans have — not through
brittle pixel-guessing, but through structured, secure, composable primitives that
mirror how operating systems actually work. desk-mcp is the protocol layer between
language models and graphical environments.*

## Decision Tree

```
Should I use desk-mcp?

  ┌─ Do you need AI to control a graphical desktop? ─────┐
  │                                                       │
  No → Use standard MCP tools (file system, shell)        │
  │                                                       │
  Yes → Are you on Linux (Wayland/X11)? ────────────────┐│
  │     │                                                 ││
  │     No → Not yet — macOS/Windows coming Weeks 8–9    ││
  │     │                                                 ││
  │     Yes → Is your use case browser-only? ────────────┐││
  │           │                                           │││
  │           Yes → Consider Playwright for richer        │││
  │           │     browser testing APIs.                 │││
  │           │     Still need native apps? → use desk-mcp│││
  │           │                                           │││
  │           No → Do you need element-level precision?  │││
  │                 │                                     │││
  │                 No → desk-mcp with OCR mode          │││
  │                 │                                     │││
  │                 Yes → desk-mcp (AT-SPI + CDP) ───────┘│││
  └───────────────────────────────────────────────────────┘││
                                                           ││
  ┌────────────────────────────────────────────────────────┘│
  │  ┌──────────────────────────────────────────────────────┘
  │  │
  ▼  ▼
  desk-mcp is for you ✓
```

## Why desk-mcp?

desk-mcp is the fastest path from an AI agent to a working graphical desktop.
It runs as a single Rust binary with **sub-10ms cold start**, resolves screen
elements through a **three-tier precision system** (AT-SPI accessibility tree
→ browser CDP DOM → Tesseract OCR), and gates every dangerous operation behind
a **layered security model** with bearer token auth, confirmation gating, rate
limiting, and structured audit logging.

## One-Line Install

```bash
curl -fsSL https://raw.githubusercontent.com/ShibbityShwab/desk-mcp/main/install.sh | bash
```

Or from source:

```bash
cargo install --git https://github.com/ShibbityShwab/desk-mcp
```

## Quick Start

### 1. Install

```bash
curl -fsSL https://raw.githubusercontent.com/ShibbityShwab/desk-mcp/main/install.sh | bash
```

### 2. Configure your MCP client

Add to your Claude Desktop config (`~/.config/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "desk-mcp": {
      "command": "desk-mcp"
    }
  }
}
```

For HTTP/SSE transport (remote agents):

```bash
desk-mcp --http 0.0.0.0:9273
```

On first start, a bearer token is generated and saved to `~/.config/desk-mcp/token`.
All HTTP requests must include `Authorization: Bearer <token>` or `?token=<token>`.

### 3. Ask your AI

*"List my open windows and take a screenshot of the active one."*

*"Open Firefox, navigate to github.com, and search for 'desk-mcp'."*

*"Find the Submit button on screen and click it."*

## Architecture

```
                        MCP Clients
                 (Claude, GPT, Cline, etc.)
                            │
               stdio ───────┴────── HTTP/SSE (axum)
                            │
                      desk-mcp binary
                            │
                  ┌─────────┼──────────┐
                  │         │          │
              discovery   safety    transport
              (OnceLock)  (gating)  (JSON-RPC 2.0)
                  │         │          │
                  └─────────┼──────────┘
                            │
                    Resolution Router
                            │
         ┌──────────────────┼──────────────────┐
         │                  │                  │
    Tier 1: AT-SPI     Tier 2: CDP       Tier 3: OCR
    (accessibility      (chromiumoxide     (leptess/
     tree, <10ms)        DOM, ~12ms)       bounding boxes,
                                            ~500ms)
         │                  │                  │
         └──────────────────┼──────────────────┘
                            │
                    Provider Layer
              (ComputerProvider trait)
                            │
    ┌───────────┬───────────┼───────────┬───────────┐
    │           │           │           │           │
  KWin     kde_wayland   wayland_wlr    x11     headless
  (D-Bus)  (wdotool-core, enigo)      (enigo)  (no display)
    │           │           │           │           │
    └───────────┴───────────┼───────────┴───────────┘
                            │
                      Operating System
                   (Linux, macOS*, Windows*)
```

**Three-tier resolution** means the agent never guesses pixel coordinates:

1. **AT-SPI accessibility tree** — when the target app exposes accessibility (most native
   Linux apps do), desk-mcp reads the exact bounds, role, and label of every UI element.
   Precision: **~8ms**, exact element-level.

2. **Browser CDP DOM** — for web content, chromiumoxide provides the live DOM tree with
   CSS selectors, element bounds, and JavaScript execution. Precision: **~12ms**.

3. **OCR fallback** — when neither AT-SPI nor CDP is available (X11 legacy apps, Electron
   apps without accessibility), Tesseract OCR with Sobel edge detection finds clickable
   regions by analyzing the screen visually. Precision: **~500ms**, region-level.

[Full architecture document](ARCHITECTURE.md)

## Tools at a Glance

desk-mcp exposes **63 tools** across seven categories. For full schemas and
parameter reference, see [docs/tools.md](docs/tools.md). For machine-readable
schemas designed for AI agent consumption, see [docs/AGENTS.md](docs/AGENTS.md).

| Category | Count | Key Capabilities |
|----------|-------|-----------------|
| Computer Use | 26 | Screenshot, mouse (move/click/drag/scroll), keyboard (type/press/combo), text-on-screen detection, clipboard, shell (guarded), windows (list/focus/state), notifications |
| Browser Use | 18 | Full CDP automation via Chromium: launch, navigate, click, type, screenshot, JavaScript execution, HTML/text extraction, tab management, downloads, uploads |
| Code Mode | 8 | File I/O with workspace sandbox, regex grep, glob file search, multi-language execution (Python, Bash, Node, Ruby, Perl, PHP), linting, build |
| Accessibility | 4 | AT-SPI element tree queries: find by role/name, get element text, click element via accessibility API, full window tree dump |
| Web | 2 | DuckDuckGo search (no API key), URL fetch with HTML-to-text extraction |
| Safety | 4 | Request confirmation for gated operations, approve/deny, list pending |
| Status | 1 | Server health check with provider info and capability flags |

## Performance

All measurements taken on Linux KDE Wayland, AMD Ryzen 7, 32 GB RAM. CDP
benchmarks use headless Chromium with a warm browser instance. OCR benchmarks
use Tesseract 5 on a 1920×1080 screen.

| Operation | Method | Latency | vs Human |
|-----------|--------|---------|----------|
| Find button + click | AT-SPI element tree | ~8 ms | 125× faster |
| Find button + click | Browser CDP selector | ~12 ms | 83× faster |
| Find button + click | OCR fallback (Sobel + Tesseract) | ~500–800 ms | 2× faster |
| Extract visible text | CDP `document.body.innerText` | ~5 ms | — |
| Extract visible text | Tesseract OCR full screen | ~400–600 ms | — |
| Screenshot (1080p PNG) | Native screenshot tool | ~120 ms | — |
| Window list | KWin D-Bus (no subprocess) | ~5 ms | — |
| Cold start (binary launch) | Rust, no runtime | ~8 ms | — |

> **Honesty note:** AT-SPI and CDP latencies are measured from a running server;
> they exclude the one-time browser launch (~1.5 s) and AT-SPI daemon handshake
> (~200 ms). OCR fallback latency includes the full screenshot + Tesseract +
> bounding-box cycle.

## Security Model

```
Request → Auth Token → Policy Engine → Confirmation Gate → Rate Limiter → Audit Log → Execute
  │          │              │                 │                │             │          │
  │    dmcp_<32-char>  ALLOW_SHELL       request/         30/min/tool    JSONL at     Result
  │    random token,   ALLOW_CODE        approve/deny     token bucket   ~/.local/    returned
  │    Bearer header   DESKMCP_          flow             burst=5        share/       to agent
  │    or ?token=      WORKSPACE                                          desk-mcp/
  │                                                                       audit.log
```

**Layer by layer:**

- **Auth Token** — HTTP transport requires a bearer token auto-generated on first
  start and stored at `~/.config/desk-mcp/token`. stdio transport inherits the
  parent process identity.

- **Policy Engine** — Three boolean gates control dangerous capabilities.
  `ALLOW_SHELL` and `ALLOW_CODE` are **off by default**. `DESKMCP_WORKSPACE`
  sandboxes all file operations to a single directory tree using canonical path
  resolution — `../../etc/passwd` is blocked.

- **Confirmation Gate** — Nine tools (`shell_run`, `file_write`, `file_edit`,
  `code_run`, `code_build`, `browser_download`, `mouse_click`, `keyboard_type`,
  `open_app`) require explicit user approval via the `request_confirmation` →
  `approve`/`deny` flow. The agent cannot proceed until a human confirms.

- **Rate Limiter** — Token bucket algorithm: 30 actions per minute per tool,
  burst of 5. Prevents runaway agent loops before they cause damage.

- **Audit Log** — Every tool invocation writes a JSONL entry to
  `~/.local/share/desk-mcp/audit.log`. Sensitive arguments (clipboard contents,
  text payloads, secrets) are sanitized before logging. Each entry records
  timestamp, tool name, sanitized args, success/failure, and duration.

[Full security documentation](docs/security.md)

## Comparison

| Tool | Language | Native Desktop | Element Tree | Browser CDP | Auth | Audit | desk-mcp advantage |
|------|----------|---------------|--------------|-------------|------|-------|-------------------|
| **desk-mcp** | Rust | Linux (macOS/Win W8–9) | AT-SPI | chromiumoxide | Bearer token | JSONL | Speed, safety layers, 63 tools |
| pyautogui | Python | All | ❌ | ❌ | ❌ | ❌ | desk-mcp: 50× faster cold start, element precision, security model |
| playwright | Node/Python | ❌ browser-only | ❌ | ✅ | ❌ | ❌ | desk-mcp: controls native apps, terminals, IDEs — not just browser |
| selenium | Multi | ❌ browser-only | ❌ | WebDriver proxy | ❌ | ❌ | desk-mcp: native CDP (no driver server), desktop control |
| anthropic-cua | Python | Linux | ❌ | ❌ | ❌ | ❌ | desk-mcp: CDP + AT-SPI + 63 tools vs 3, bearer auth, audit |
| nut.js | Node | All | ❌ | ❌ | ❌ | ❌ | desk-mcp: native element tree, auth, audit, no npm install |
| robot.js | Node | All | ❌ | ❌ | ❌ | ❌ | desk-mcp: maintained, security model, structured resolution |

> **Where alternatives beat desk-mcp:** Playwright and Selenium have richer
> browser-testing APIs (network interception, trace viewer, assertion library).
> pyautogui and nut.js work on Windows and macOS today. desk-mcp currently
> requires Linux; cross-platform support is in active development.

## Why not X?

**Why not Playwright?** Playwright is browser-only. desk-mcp controls your
entire desktop: native apps, terminals, file managers, IDEs, system dialogs. If
your agent needs to interact with anything outside a browser tab, you need
desk-mcp.

**Why Rust instead of Python?** Single binary. No venv, no pip install, no
`ImportError`. Sub-10 ms cold start. LTO + single codegen unit = one small
stripped binary. Cannot panic from a native dependency segfault at runtime —
`panic = "abort"` at the release profile level.

**Why MCP instead of REST?** MCP is the agent protocol standard. Claude, GPT,
Cline, and every major AI platform speak MCP natively. REST is for dashboards
and humans. desk-mcp supports both: stdio MCP for local agents, HTTP/SSE for
remote agents — same JSON-RPC 2.0 dispatch underneath.

**Why not ship an AI model?** desk-mcp is infrastructure, not an AI platform.
It provides structured, verifiable data to models. Models do the reasoning on
top of that data. Shipping a model would mean shipping opinions about how to
interpret the desktop — which defeats the purpose of a protocol layer.

**Why not just use xdotool/ydotool directly?** desk-mcp wraps these tools
(and increasingly replaces them with native D-Bus calls). It adds structured
resolution (AT-SPI element trees instead of guess-and-click), security
(confirmation gating so the agent can't go rogue), and audit (so you can
review everything the agent did). Raw xdotool gives you none of this.

## Environment Support

| OS | Desktop | Provider | Status |
|----|---------|----------|--------|
| Linux | KDE (Wayland) | `kwin_dbus` → `kde_wayland` | **Full** |
| Linux | KDE (X11) | `x11` | Partial |
| Linux | GNOME (Wayland) | `wayland_wlr` | Partial |
| Linux | Sway/Hyprland (wlr) | `wayland_wlr` | Partial |
| Linux | Headless/VNC | `headless` | Screenshots only |
| macOS | — | — | Coming Week 8 |
| Windows | — | — | Coming Week 9 |

### Dependencies by Distribution

```bash
# Arch
sudo pacman -S spectacle ydotool tesseract tesseract-data-eng chromium

# Ubuntu/Debian
sudo apt install spectacle ydotool tesseract-ocr chromium-browser

# Fedora
sudo dnf install spectacle ydotool tesseract chromium
```

## The Roadmap

- **Now (v0.5):** Three-tier resolution router, policy engine with
  confirmation gating, mock provider, session manager, KWin D-Bus native
  window management
- **Week 6:** Record-replay testing harness, CI with Xvfb, fuzzing harness
  for tool inputs
- **Week 8:** macOS native provider (CoreGraphics + Accessibility APIs)
- **Week 9:** Windows native provider (UI Automation + Win32)
- **Week 10:** Plugin SDK for third-party providers, tool recipes system,
  dynamic provider loading
- **Week 12:** OpenTelemetry tracing, latency budgets per operation,
  observability dashboard

## Contributing

desk-mcp has a clear contributor ladder. Every level has a concrete path:

```
Level 0: Fix a typo → PR accepted in <1 hour
         → Look for files with "good first issue" labels or obvious
           spelling/grammar fixes in docs/

Level 1: Add a tool parameter → ~50 lines of Rust
         → Understand the tool definition in src/tools/mod.rs and the
           handler in src/tools/*.rs. Add the parameter to the JSON
           schema, read it in the handler, add one behavior branch.

Level 2: Write a tool recipe → JSON, no Rust required
         → Tool recipes are composable sequences of tool calls packaged
           as reusable workflows. Author them in JSON against the
           AGENTS.md schema.

Level 3: Write a provider → ~500 lines of Rust
         → Implement the ComputerProvider trait in src/providers/.
           See docs/PROVIDERS.md (coming Week 10) for the full guide.

Level 4: Core architecture → by invitation after sustained contribution
         → After several quality contributions, you'll be invited to
           work on the resolution router, policy engine, or transport
           layer.
```

## License

MIT — see [LICENSE](LICENSE) for full text.
