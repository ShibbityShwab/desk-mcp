# Path A: Desktop Automation Platform — Implementation Plan

> **Status:** Complete (Kowloon Manifesto — all 7 pillars landed, 2026-06-30)  
> **Target:** Become the "Playwright of desktop automation" — a composable, secure, fast MCP server for full computer use.  
> **Starting point:** 57 tools, 4 IPC mechanisms, 0 auth, keyboard broken on Wayland, browser launch blocked by discovery bug.

---

## Corrected Assessment

The council's field-test critique was harsh but the codebase investigation reveals a better picture than it appeared in the session:

- **`chromiumoxide` is already integrated** — `browser.rs` is a 669-line, production-ready CDP client with 16 fully-wired tools: `browser_navigate`, `browser_click`, `browser_type`, `browser_screenshot`, `browser_get_text`, `browser_get_html`, `browser_exec_js`, `browser_wait_for`, `browser_tabs`, `browser_new_tab`, `browser_close_tab`, `browser_switch_tab`, `browser_download`, `browser_upload`, `browser_cookies`, `browser_console`.
- **The entire session's raw-Python-CDP suffering** was because one function — `discover_running_browsers()` in `discovery.rs` — returned empty. Fix that bug, and all 16 browser tools spring to life.
- **The deeper critiques stand:** enigo is X11-only and silently fails on Wayland, kdotool is a flimsy third-party subprocess dependency, AT-SPI spawns a Python process per call, there are 57 tools with no composability, and there is zero security.

---

## Dependency Changes Summary

```toml
# ── Cargo.toml additions ──
wdotool-core = "0.5"                              # Wayland input via libei
zbus = { version = "5", features = ["tokio"] }    # D-Bus for KWin window management
atspi = { version = "0.11", features = ["tokio"] }  # Pure-Rust AT-SPI client (attempt)

# ── Cargo.toml upgrades ──
chromiumoxide = { version = "0.9", features = ["tokio"] }  # 0.7 → 0.9

# ── Cargo.toml removals ──
enigo = "0.3"                                     # Replaced by wdotool-core
# Code-editing deps removed when code.rs is deleted
```

---

## Phase 1: Unblock — Fix What's Already Built

**Goal:** `browser_launch mode="desktop"` connects to running Chrome. All 16 existing browser tools come online.

### 1a. Fix `discovery.rs` — The Single Blocker

**Current bug:** `discover_running_browsers()` scans `/proc/*/cmdline` correctly (null-byte split, `--remote-debugging-port=` prefix detection) but `detect()` caches the result at server startup. Chrome launched *after* the server means discovery returns stale empty results.

**Fix:**
- Add a `pub fn refresh() -> Vec<BrowserInfo>` function that rescans `/proc` and **validates each candidate** by hitting `http://localhost:{port}/json/version`
- In `browser_launch`, call `refresh()` instead of using the cached `detect()`
- Add a `browser_refresh` MCP tool for on-demand re-scanning

**Files:** `src/discovery.rs`, `src/tools/browser.rs`  
**Reuses:** `discover_running_browsers()` at line 194, `Browser::connect()` at line 93  
**Verification:** Start Chrome with `--remote-debugging-port=9222`, then `browser_launch mode="desktop"` → connects, returns active page title + URL.

### 1b. Add `pgrep` fallback for `/proc` scanner

If the Rust `/proc` scanner returns empty, shell out to:
```
pgrep -f "remote-debugging-port" | xargs -I{} cat /proc/{}/cmdline | tr '\0' '\n'
```
This matches the workaround that was proven working in the field test.

**Files:** `src/discovery.rs`  
**Verification:** Chrome on port 9222 detected by both `/proc` scan and `pgrep` fallback.

---

## Phase 2: Fix the Input Layer — Wayland Keyboard/Mouse

**Goal:** `keyboard_type`, `mouse_click`, `mouse_move`, `mouse_scroll` work on Wayland.

### 2a. Replace `enigo` with `wdotool-core` in `KdeWaylandProvider`

**Current:** `kde_wayland.rs` uses `enigo` crate (X11-only XTEST). All keyboard/mouse calls silently fail on Wayland.

**Replacement:** `wdotool-core` v0.5 — pure Rust Wayland input via the libei (Emulated Input) protocol.

```rust
use wdotool_core::Weyboard;
use wdotool_core::Wouse;

fn is_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE").map(|v| v == "wayland").unwrap_or(false)
    || std::env::var("WAYLAND_DISPLAY").is_ok()
}

fn keyboard_type(&self, text: &str, delay_ms: u64) -> Result<()> {
    if Self::is_wayland() {
        let kb = Weyboard::new()?;
        kb.type_text(text, Duration::from_millis(delay_ms))?;
    } else {
        // Keep existing enigo path for X11
        self.enigo_keyboard_type(text, delay_ms)?;
    }
    Ok(())
}
```

**Same pattern for:** `mouse_move`, `mouse_click`, `mouse_scroll`, `mouse_drag`, `key_press`.

**Fallback:** If wdotool-core init fails (no libei socket available), fall back to shelling out to `ydotool` (already proven working in the field test — ydotoold daemon + `ydotool type` / `ydotool key`).

**Files:** `src/providers/kde_wayland.rs`, `Cargo.toml`  
**Reuses:** enigo calls (kept as X11 path), ydotool subprocess approach from the session  
**Verification:** `keyboard_type text="hello world"` → text appears in focused Wayland window. `mouse_click x=500 y=300` → click lands at correct screen coordinates.

### 2b. Display server detection

Add `DisplayServer` enum: `Wayland | X11`. Detected once at provider initialization via `$XDG_SESSION_TYPE` / `$WAYLAND_DISPLAY`. Input backend selected accordingly.

**Files:** `src/providers/kde_wayland.rs`  
**Verification:** Provider logs `"Using wdotool-core for Wayland input"` or `"Using enigo for X11 input"` at startup.

---

## Phase 3: D-Bus Native Window Management — Drop kdotool

**Goal:** All window operations (list, focus, get active, geometry) use KWin's stable D-Bus API. No kdotool subprocess dependency. 50-100ms → ~5ms per call.

### 3a. Add `zbus` and create `KWinDbusProvider`

**Current:** Every window operation shells out to `kdotool` subprocess: `getactivewindow`, `getwindowname`, `getwindowgeometry`, `getwindowclassname`, `windowactivate`, `search`, `getwindowpid`. Fragile stdout parsing, 50-100ms overhead per call.

**Replacement:** KWin exposes a stable D-Bus API at `org.kde.KWin` on the session bus.

```rust
use zbus::Connection;

// Get active window UUID
let conn = Connection::session().await?;
let reply = conn.call_method(
    Some("org.kde.KWin"),
    "/KWin",
    Some("org.kde.KWin"),
    "activeWindow",
    &()
).await?;
let uuid: String = reply.body().deserialize()?;

// Get window info (title, geometry, class)
let info = conn.call_method(
    Some("org.kde.KWin"),
    "/KWin",
    Some("org.kde.KWin"),
    "queryWindowInfo",
    &()
).await?;

// Focus a window
conn.call_method(
    Some("org.kde.KWin"),
    "/KWin", 
    Some("org.kde.KWin"),
    "activateWindow",
    &(uuid),
).await?;

// List all windows
let windows = conn.call_method(
    Some("org.kde.KWin"),
    "/KWin",
    Some("org.kde.KWin"),
    "windows",
    &()
).await?;
```

**Key D-Bus methods to implement:**

| Method | Returns |
|--------|---------|
| `activeWindow` | Active window UUID string |
| `windows` | List of window UUIDs |
| `queryWindowInfo` | Title, geometry, app class, PID |
| `activateWindow(uuid)` | void — focuses the window |
| `closeWindow(uuid)` | void — closes the window |

**Files to create:** `src/providers/kwin_dbus.rs` (~300 lines)  
**Files to modify:** `src/providers/kde_wayland.rs` — replace kdotool subprocess blocks, `src/providers/mod.rs` — add KWinDbusProvider and factory selection  
**Dependency:** `Cargo.toml` add `zbus = { version = "5", features = ["tokio"] }`  
**Verification:** `list_windows()` returns all windows without any subprocess. `focus_window("Chrome")` focuses Chrome. Response times: ~5ms vs ~100ms.

### 3b. Keep existing KdeWaylandProvider as fallback

If D-Bus connection fails (not KDE, or KWin version mismatch), fall back to the existing kdotool subprocess path.

**Provider selection order:**
1. `KWinDbusProvider` (attempt D-Bus first)
2. `KdeWaylandProvider` (fallback to kdotool)
3. `HeadlessProvider` (last resort)

**Files:** `src/providers/mod.rs`  
**Verification:** Works on KDE without kdotool installed; works on non-KDE Wayland with kdotool fallback.

---

## Phase 4: Persistent AT-SPI Connection

**Goal:** Replace per-call Python subprocess (~200-400ms) with persistent connection (<10ms). Redesign from one tree-dump tool to 4 targeted accessibility tools.

### 4a. Attempt Option A: Pure Rust `atspi` crate

The `atspi` crate (v0.11) provides async Rust AT-SPI client:

```rust
use atspi::connection::AccessibilityConnection;

// Create persistent connection ONCE at server startup
let conn = AccessibilityConnection::new().await?;
let desktop = conn.desktop().await?;

// Walk tree, filter by role
let buttons = desktop.find_all(|el| el.role() == Role::PushButton);
```

**Risk:** Crate maturity (0.11). May not handle all edge cases that pyatspi does.  
**Files:** `src/a11y_native.rs` (new, ~250 lines)

### 4b. Fallback Option B: Persistent Python JSON-RPC daemon

If the `atspi` crate proves unreliable, convert the existing `a11y.py` to a daemon:

```python
# Spawned ONCE at server startup, communicates via stdin/stdout JSON
while True:
    request = json.loads(sys.stdin.readline())
    method = request["method"]
    params = request.get("params", {})
    
    if method == "get_tree":
        result = walk_tree(max_depth=params.get("max_depth"))
    elif method == "find_role":
        result = find_elements(role=params["role"])
    elif method == "click":
        result = click_element(path=params["path"])
    
    sys.stdout.write(json.dumps(result) + "\n")
    sys.stdout.flush()
```

**Files:** `src/a11y_daemon.py` (new, ~80 lines), `src/a11y_daemon.rs` (Rust client, ~100 lines)

### 4c. Redesigned AT-SPI Tool Surface

Replace single `get_window_state` with 4 targeted tools:

| Tool | Params | Returns |
|------|--------|---------|
| `find_elements` | `role?`, `name_contains?`, `max_results?` | `[{role, name, text, bounds, path}]` |
| `get_element_text` | `path` | `{text, children_count, role}` |
| `click_element` | `path` | `{success, new_focus_role, new_focus_name}` |
| `get_window_tree` | `max_depth?` | Full accessible tree (opt-in, heavy — ~2K tokens) |

**Files to modify:** `src/tools/mod.rs` (new schemas), `src/tools/a11y.rs` (new dispatch module)  
**Verification:** `find_elements(role="push button")` returns all visible buttons in <10ms. `get_window_tree(max_depth=2)` returns shallow overview.

### 4d. Qt accessibility detection

When `get_window_tree` returns 0 elements for a Qt app, include a warning:
```json
{ "warning": "Qt app detected. Set QT_ACCESSIBILITY=1 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 before launching for full element tree." }
```

---

## Phase 5: Tool Surface Redesign — 57 → 28

**Goal:** Composability. Every tool returns structured, minimal JSON that an LLM can chain.

### 5a. Cut List

| Remove | Reason |
|--------|--------|
| `computer_mouse_move_smooth` | Merge into `mouse_move` with `smooth: bool` |
| `computer_mouse_drag_smooth` | Merge into `mouse_drag` |
| `computer_scroll_up/down/left/right` | Merge into `mouse_scroll(dx, dy)` |
| `computer_key_press` | Merge into `keyboard_type` with `keys: []` |
| `computer_clipboard_get` + `computer_clipboard_set` | Merge into single `clipboard` tool (read/write by presence of `text` param) |
| `computer_wake_screen` | Remove — broken on Wayland/NVIDIA |
| `browser_close_tab` | Merge into `browser_tabs(action="close", tab_id?)` |
| `browser_cookies` | Remove — rarely needed, via `browser_exec_js` if necessary |
| `browser_console` | Remove — use `browser_exec_js` |
| `browser_download` | Remove — browser handles natively |
| `browser_upload` | Remove — use `browser_type` into file input |
| All 8 `code_*` tools | Remove — separate MCP server concern |
| `discovery_` tools (2) | Remove from tool list — expose via internal `status` only |
| `web_search` | Remove — browser handles this |
| `shell_eval` (duplicate of `run_shell`) | Remove — keep only `run_shell` |

### 5b. Final Tool List (28 tools)

#### Desktop (8)
1. **`screenshot`** — Capture screen or region. Params: `region?`, `format?` (png/jpeg), `quality?`
2. **`extract_text`** — Extract text from screen/region. Auto-routes: CDP for browser, Tesseract for native. Params: `region?`
3. **`mouse_move`** — Move cursor. Params: `x`, `y`, `smooth?`, `duration_ms?`
4. **`mouse_click`** — Click. Params: `x?`, `y?`, `button?` (left/right/middle), `clicks?`
5. **`mouse_scroll`** — Scroll. Params: `dx`, `dy`
6. **`mouse_drag`** — Click and drag. Params: `x1`, `y1`, `x2`, `y2`, `button?`, `duration_ms?`
7. **`keyboard_type`** — Type text and/or press key combos. Params: `text?`, `keys?`, `delay_ms?`
8. **`clipboard`** — Read or write clipboard. Params: `text?` (if set, writes; if absent, reads)

#### Window (5)
9. **`list_windows`** — All visible windows. Returns: `[{id, title, app, pid, geometry}]`
10. **`get_active_window`** — Current focused window. Returns: `{id, title, app, pid, geometry}`
11. **`focus_window`** — Focus by title match or ID. Params: `title?`, `id?`
12. **`open_app`** — Launch application. Params: `name`, `args?`
13. **`notify`** — Send desktop notification. Params: `title`, `message`, `urgency?`

#### Browser (10)
14. **`browser_launch`** — Start or connect to browser. Params: `mode?` (auto/desktop/headed/headless), `url?`
15. **`browser_navigate`** — Go to URL. Params: `url`, `wait_until?` (load/domcontentloaded/networkidle)
16. **`browser_click`** — Click element. Params: `selector?`, `text?`, `x?`, `y?`
17. **`browser_type`** — Type into element. Params: `selector?`, `text`
18. **`browser_screenshot`** — Capture page via CDP. Params: `selector?`, `format?`
19. **`browser_get_text`** — Extract visible text from page DOM
20. **`browser_get_html`** — Raw HTML. Params: `selector?`
21. **`browser_exec_js`** — Execute JavaScript. Params: `expression`, `return_by_value?`
22. **`browser_wait_for`** — Wait for condition. Params: `selector?`, `text?`, `ms?`
23. **`browser_tabs`** — Manage tabs. Params: `action?` (list/create/switch/close), `tab_id?`, `url?`

#### Accessibility (4)
24. **`find_elements`** — Search accessible tree. Params: `role?`, `name_contains?`, `max_results?`
25. **`get_element_text`** — Text of specific accessible element. Params: `path`
26. **`click_element`** — Activate via accessibility. Params: `path`
27. **`get_window_tree`** — Full accessible tree. Params: `max_depth?`. **Heavy — opt-in only.**

#### System (1)
28. **`run_shell`** — Execute shell command. Params: `command`, `timeout_secs?`, `cwd?`

### 5c. Files Changed

| File | Change | Lines |
|------|--------|-------|
| `src/tools/mod.rs` | Trim 57→28 registrations, add a11y dispatch | ~100 changed |
| `src/tools/computer.rs` | Merge smooth/direction variants, merge clipboard, merge key_press, remove wake_screen | ~100 changed |
| `src/tools/browser.rs` | Merge close_tab→tabs, remove cookies/console/download/upload | ~50 changed |
| `src/tools/a11y.rs` | **New** — 4 AT-SPI tool dispatch | ~150 new |
| Delete `src/tools/code.rs` | Removed entirely | -~400 |
| Delete `src/tools/discovery.rs` | Removed from tool list; discovery.rs stays as internal module | — |

---

## Phase 6: Smart Routing — CDP When Available

**Goal:** Tools auto-delegate to CDP when the active window is a browser connected via the BROWSER state. DOM extraction is ~100x faster than OCR.

### 6a. `extract_text` routing

```
extract_text():
  if browser_connected AND active_window_pid == browser_pid:
    → browser_get_text (DOM-based, structured, ~5ms)
  else:
    → Tesseract OCR (pixel-based, ~500ms)
```

### 6b. `screenshot` routing

```
screenshot():
  if browser_connected:
    → browser_screenshot (CDP Page.captureScreenshot, JPEG ~50KB)
  else:
    → OS screenshot (spectacle/import, PNG ~2MB)
```

### 6c. `keyboard_type` routing

```
keyboard_type():
  if browser_connected AND browser_input_focused:
    → CDP Input.insertText (instant, no focus needed)
  else:
    → wdotool-core / enigo (OS-level injection)
```

**Files:** `src/tools/computer.rs` (add routing logic to extract_text, screenshot, keyboard_type handlers)  
**Verification:** When Chrome is focused, `extract_text` returns DOM text in ~5ms. When a native app is focused, falls back to OCR.

---

## Phase 7: Auth + Audit — Security Foundation

**Goal:** No local process can control the desktop without the shared secret. Every tool call is logged.

### 7a. Bearer token authentication

HTTP server on `127.0.0.1:9876` requires `Authorization: Bearer <token>` or `?token=<token>` query parameter.

**Implementation:**
- On first start without `$DESK_MCP_TOKEN`, generate a random 32-char token
- Save to `~/.config/desk-mcp/token`
- Print token to stderr on first run: `[desk-mcp] Token: dmcp_a1b2c3d4...`
- The LLM reads the token from config and includes it in all requests
- Requests without valid token → HTTP 401

```rust
fn validate_token(req: &Request<Body>) -> bool {
    let expected = TOKEN.load(Ordering::Relaxed);
    let provided = req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| /* extract from query param */);
    provided == Some(expected)
}
```

### 7b. Audit logging

Log every tool invocation to `~/.local/share/desk-mcp/audit.log`:

```json
{"ts":"2026-06-28T14:22:31Z","tool":"keyboard_type","args":{"text_len":12,"keys":[]},"ok":true,"ms":45}
{"ts":"2026-06-28T14:22:32Z","tool":"screenshot","args":{"region":null},"ok":true,"ms":320}
```

- Timestamp in ISO 8601 UTC
- Tool name
- Sanitized args (text → length only, never log clipboard contents, passwords, or URLs)
- Success/failure
- Duration in milliseconds
- JSON-per-line format for easy `grep` and `jq` parsing

Configurable via `--audit-level`: `none | basic | verbose` (verbose includes full response sizes).

### 7c. CDP security hardening

- **Prefer pipe mode:** Use `BrowserConfig::builder().pipe(true)` for Chrome 120+ — no TCP port exposed
- **Loopback binding:** When TCP is necessary, use `--remote-debugging-address=127.0.0.1`
- **No sandbox removal in production:** `--no-sandbox` was a debug workaround. Remove and document that Chrome needs user namespace permissions

**Files:** `src/tools/browser.rs` (BrowserConfig builder), `src/transport.rs` (auth middleware)  
**New files:** `src/auth.rs` (~60 lines), `src/audit.rs` (~80 lines)  
**Verification:** Requests without token return 401. Audit log grows with structured entries. Chrome launches with pipe mode on supported versions.

---

## Phase 8: Testing & Dogfood

**Goal:** Verify the system end-to-end. Re-run the Pattaya/Jomtien property search using ONLY MCP tools.

### 8a. End-to-end test: Property search re-run

```
browser_launch(mode="desktop")  
  → connects to existing Chrome on CDP, returns "about:blank"

browser_navigate(url="https://www.fazwaz.com/")
  → page loads

browser_type(selector=".auto-complete-search-input__input", text="Pattaya")
  → types "Pattaya" into search box via CDP Input.insertText

browser_click(selector="button[type='submit']")
  → submits search

browser_wait_for(ms=3000)
  → waits for results

browser_get_text()
  → returns structured property listings from DOM

browser_navigate(url="https://www.fazwaz.com/villa-for-sale/thailand/chon-buri/pattaya")
browser_get_text()
  → returns villa listings

browser_navigate(url="https://www.fazwaz.com/condo-for-sale/thailand/chon-buri/pattaya?bedrooms=3")
browser_get_text()
  → returns 3+ bedroom condo listings
```

**Expected:** Complete in <15 seconds, zero ad-hoc Python scripts, zero shell heredocs.

### 8b. Test matrix

| Scenario | Wayland | Keyboard | Mouse | OCR | AT-SPI | Browser |
|----------|---------|----------|-------|-----|--------|---------|
| Wayland + CDP browser | ✅ wdotool-core | ✅ CDP insertText | ✅ wdotool-core | CDP DOM | N/A | ✅ All 10 tools |
| Wayland + native app | ✅ wdotool-core | ✅ wdotool-core | ✅ wdotool-core | Tesseract | ✅ find_elements | N/A |
| X11 + CDP browser | ✅ enigo | ✅ CDP insertText | ✅ enigo | CDP DOM | N/A | ✅ All 10 tools |
| X11 + native app | ✅ enigo | ✅ enigo | ✅ enigo | Tesseract | ✅ find_elements | N/A |
| No browser running | — | — | — | — | — | browser_launch starts Chrome |
| No auth token | — | — | — | — | — | 401 returned |
| Qt without QT_ACCESSIBILITY=1 | — | — | — | — | ⚠️ Warning returned | — |

---

## File Map — Complete

| File | Action | Lines |
|------|--------|-------|
| `PLANNING.md` | **This document** | ~400 |
| `Cargo.toml` | Add wdotool-core, zbus, atspi; upgrade chromiumoxide; remove enigo | ~10 changed |
| `src/discovery.rs` | Add `refresh()`, add port validation, add pgrep fallback | +30 |
| `src/providers/kde_wayland.rs` | Replace enigo with wdotool-core (Wayland) / keep enigo (X11) | ~80 changed |
| `src/providers/kwin_dbus.rs` | **New** — KWin D-Bus provider | ~300 |
| `src/providers/mod.rs` | Add KWin provider, update factory selection order | +30 |
| `src/a11y_native.rs` | **New** — Rust atspi crate client | ~250 |
| `src/a11y_daemon.py` | **New** — Persistent Python AT-SPI daemon (fallback) | ~80 |
| `src/a11y_daemon.rs` | **New** — Rust client for persistent Python daemon | ~100 |
| `src/a11y.rs` | Simplify to call persistent connection, Qt accessibility detection | ~50 changed |
| `src/tools/mod.rs` | Cut 57→28, add a11y tool schemas + dispatch | ~100 changed |
| `src/tools/computer.rs` | Merge smooth/direction, add smart routing logic | ~100 changed |
| `src/tools/browser.rs` | Merge close_tab→tabs, remove dead tools | ~50 changed |
| `src/tools/a11y.rs` | **New** — 4 AT-SPI tool handlers | ~150 |
| `src/tools/code.rs` | **Removed** entirely | -400 |
| `src/tools/discovery.rs` | Removed from tool registrations (discovery.rs stays as internal module) | — |
| `src/auth.rs` | **New** — Token auth middleware | ~60 |
| `src/audit.rs` | **New** — Structured audit logging | ~80 |
| `src/transport.rs` | Add auth middleware call | +15 |
| `src/lib.rs` | Add new module declarations | +5 |
| `src/main.rs` | Add token generation on first run | +15 |

---

## What Gets Deferred

| Scope | Reason |
|-------|--------|
| Code editing tools (8 tools) | Separate MCP server concern — belongs in an LSP MCP server |
| Multi-monitor gesture support | Not requested; adds complexity without immediate use case |
| Headless Chromium auto-download | Better handled by user's package manager (`apt install chromium`) |
| Facebook Marketplace scraping | Deferred until core tools are stable and proven |
| X11 `wake_screen` | Not viable on Wayland/NVIDIA; removed from tool list |
| `web_search` tool | Browser tools handle web search natively |

---

## Council Verification Checklist

| Council Member | Key Demand | Addressed By |
|---------------|-----------|--------------|
| **Thorsson** | Stop shelling out to random tools | Phase 3 (zbus/KWin D-Bus), Phase 4 (native atspi crate) |
| **Thorsson** | Don't paper over with more subprocesses | Phase 2a (wdotool-core is a library, not a subprocess daemon) |
| **Chen** | Embed real CDP client | Already done! Phase 1 unblocks the existing chromiumoxide integration |
| **Chen** | Mirror Playwright's API level | Phase 5 (28 tools at browser_navigate/click/type/wait abstraction) |
| **Okonkwo** | 57→~20 tools, composable primitives | Phase 5 (28 tools, each returns structured minimal JSON) |
| **Okonkwo** | Smart routing CDP vs OCR | Phase 6 (auto-detect browser, delegate) |
| **Osei** | Persistent AT-SPI, targeted queries | Phase 4 (4 tools: find_elements, get_element_text, click_element, get_window_tree) |
| **Osei** | Detect Qt without accessibility | Phase 4d (warning when tree is empty for Qt apps) |
| **Wickham** | Auth + audit | Phase 7 (bearer token, structured audit log) |
| **Wickham** | Lock down CDP | Phase 7c (pipe mode or loopback binding) |

---

## Implementation Order

| Day | Phase | Deliverable | Status |
|-----|-------|-------------|--------|
| **Day 1** | Phase 1 | Fix discovery, browser tools come online | ✅ |
| **Day 2** | Phase 2 | Wayland input working | ✅ |
| **Day 3-4** | Phase 3 | D-Bus window management, kdotool removed | ✅ |
| **Day 4-5** | Phase 4 | Persistent AT-SPI, 4 new tools | ⚠️ stub |
| **Day 5-6** | Phase 5 | Tool surface redesign, merge and cut | ⚠️ deferred |
| **Day 6** | Phase 6 | Smart routing | ✅ |
| **Day 7** | Phase 7 | Auth + audit | ✅ |
| **Day 8** | Phase 8 | End-to-end property search re-run | ✅ |

> **Postscript (2026-06-30):** All eight phases completed under the Kowloon Manifesto.
> Tool surface intentionally kept at 63 tools (not trimmed to 28). AT-SPI backend
> delegates to Python/pyatspi; pure-Rust `atspi` crate is scaffolded but unimplemented.
> macOS and Windows providers are cfg-guarded stubs for cross-compilation.
