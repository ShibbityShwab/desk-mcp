//! Browser use tool handlers — 17 tools for web automation via CDP.
//!
//! Uses chromiumoxide (pure Rust Chrome DevTools Protocol client)
//! to control Chromium/Chrome browsers headless or with a visible window.

use crate::response::{self, ToolResponse};
use anyhow::Result;
use chromiumoxide::{
    browser::Browser, cdp::browser_protocol::target::CreateTargetParams, js::EvaluationResult,
    layout::Point, Page,
};
use futures::StreamExt;
use serde_json::Value;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

/// Global browser handle — RwLock enables concurrent read access.
/// Read-only ops (tabs, cookies, console) share the lock.
/// Mutating ops (launch, new_tab, close_tab, switch_tab) take exclusive access.
static BROWSER: std::sync::LazyLock<RwLock<Option<BrowserState>>> =
    std::sync::LazyLock::new(|| RwLock::new(None));

pub(crate) struct BrowserState {
    _browser: Browser,
    current_page: Page,
    #[allow(dead_code)]
    is_headless: bool,
    /// Chrome child process — killed on drop if headless.
    #[allow(dead_code)]
    chrome_child: Option<std::process::Child>,
}

impl Drop for BrowserState {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.chrome_child {
            let _ = child.kill();
            let _ = child.wait();
            tracing::debug!("chrome child process cleaned up");
        }
    }
}

/// Find a free TCP port for Chrome DevTools.
fn find_free_port() -> Result<u16, String> {
    use std::net::TcpListener;
    TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("Cannot find free port: {e}"))
        .and_then(|l| l.local_addr().map(|a| a.port()).map_err(|e| format!("{e}")))
}

/// Handle all browser tool calls
pub async fn handle(name: &str, args: Value, session_id: Option<&str>) -> ToolResponse {
    let result = handle_inner(name, args, session_id).await;
    match result {
        Ok(value) => response::ok(value),
        Err(message) => response::err("BROWSER_ERROR", &message),
    }
}

async fn handle_inner(name: &str, mut args: Value, session_id: Option<&str>) -> Result<Value, String> {
    match name {
        "browser_launch" => browser_launch(&mut args, session_id).await,
        _ => {
            let page = get_page_for_session(session_id).await?;
            match name {
                "browser_navigate" => browser_navigate(&page, &args).await,
                "browser_click" => browser_click(&page, &args).await,
                "browser_type" => browser_type(&page, &args).await,
                "browser_screenshot" => browser_screenshot(&page, &args).await,
                "browser_exec_js" => browser_exec_js(&page, &args).await,
                "browser_get_html" => browser_get_html(&page, &args).await,
                "browser_get_text" => browser_get_text(&page, &args).await,
                "browser_wait_for" => browser_wait_for(&page, &args).await,
                "browser_tabs" => browser_tabs().await,
                "browser_new_tab" => browser_new_tab(&args).await,
                "browser_close_tab" => browser_close_tab(&page, &args).await,
                "browser_switch_tab" => browser_switch_tab(&args).await,
                "browser_download" => browser_download(&page, &args).await,
                "browser_upload" => browser_upload(&page, &args).await,
                "browser_cookies" => browser_cookies(&page).await,
                "browser_console" => browser_console(&page, &args).await,
                _ => Err(format!("unknown browser tool: {name}")),
            }
        }
    }
}

/// Check whether a browser is connected (useful for Tier-2 resolution gating).
pub async fn is_connected() -> bool {
    BROWSER.read().await.is_some()
}

/// Get the current page
pub async fn get_page() -> Result<Page, String> {
    let guard = BROWSER.read().await;
    match guard.as_ref() {
        Some(state) => Ok(state.current_page.clone()),
        None => Err("Browser not launched. Call browser_launch first.".into()),
    }
}

/// Get the current page, preferring session browser state over global.
pub async fn get_page_for_session(session_id: Option<&str>) -> Result<Page, String> {
    if let Some(id) = session_id {
        if let Some(session) = crate::session::SESSIONS.get_session(&id.to_string()) {
            let guard = session.browser_page.read().await;
            if let Some(ref page) = *guard {
                return Ok(page.clone());
            }
        }
    }
    // Fallback to global
    get_page().await
}

// ═══════════════════ HELPERS ═══════════════════

/// Connect to a running Chrome DevTools endpoint on localhost.
///
/// Uses a custom reqwest client with timeouts to fetch `/json/version`,
/// then passes the WebSocket URL directly to `Browser::connect()` wrapped
/// in a 10s timeout. This avoids chromiumoxide's internal HTTP client
/// (reqwest 0.13) which can hang indefinitely on some systems.
async fn connect_to_cdp_port(
    port: u16,
) -> Result<(Browser, chromiumoxide::handler::Handler), String>
{
    let version_url = format!("http://localhost:{port}/json/version");

    // Custom reqwest client with timeouts
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let version_body = client
        .get(&version_url)
        .send()
        .await
        .map_err(|e| format!("Failed to reach Chrome DevTools on port {port}: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Failed to read /json/version on port {port}: {e}"))?;

    let version_json: serde_json::Value = serde_json::from_str(&version_body)
        .map_err(|e| format!("Failed to parse /json/version on port {port}: {e}"))?;

    let ws_url_raw = version_json["webSocketDebuggerUrl"]
        .as_str()
        .ok_or_else(|| format!("No webSocketDebuggerUrl in /json/version response on port {port}"))?;

    // Replace localhost with 127.0.0.1 — some async-tungstenite builds
    // hang on localhost DNS resolution under tokio.
    let ws_url = ws_url_raw.replace("localhost", "127.0.0.1");

    tracing::info!(port = port, ws_url = %ws_url, "connecting to Chrome DevTools WebSocket");

    let connect_result =
        tokio::time::timeout(Duration::from_secs(10), Browser::connect(ws_url)).await;

    match connect_result {
        Ok(Ok(pair)) => Ok(pair),
        Ok(Err(e)) => Err(format!("CDP handshake failed on port {port}: {e}")),
        Err(_elapsed) => Err(format!(
            "CDP handshake timed out after 10s on port {port}. Chrome may be hung."
        )),
    }
}

// ═══════════════════ TOOL HANDLERS ═══════════════════

async fn browser_launch(args: &mut Value, session_id: Option<&str>) -> Result<Value, String> {
    let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("auto");

    // Try connecting to an already-running browser first.
    // Use refresh_browsers() (not the cached detect()) so browsers
    // launched *after* server start are discovered immediately.
    if mode == "auto" || mode == "desktop" {
        let discovered = crate::discovery::refresh_browsers();
        for info in &discovered {
            if let Some(port) = info.debugging_port {
                // Use the same robust connect pattern as the headless path:
                // custom reqwest client with timeouts + /json/version → WS URL,
                // to avoid chromiumoxide's internal HTTP client hanging.
                match connect_to_cdp_port(port).await {
                    Ok((browser, mut handler)) => {
                        // Get first page, or create one
                        let page = match browser.pages().await {
                            Ok(pages) => match pages.into_iter().next() {
                                Some(p) => p,
                                None => {
                                    return Err("Connected but no pages available".into());
                                }
                            },
                            Err(e) => return Err(format!("Failed to list pages: {e}")),
                        };

                        // Spawn handler to consume events
                        tokio::spawn(async move { while handler.next().await.is_some() {} });

                        let title = page.get_title().await.ok().flatten().unwrap_or_default();
                        let url = page.url().await.ok().flatten().unwrap_or_default();

                        // Clone page before moving it into the global BROWSER so we can
                        // attach a copy to the session.
                        let page_for_session = page.clone();

                        let mut guard = BROWSER.write().await;
                        *guard = Some(BrowserState {
                            _browser: browser,
                            current_page: page,
                            is_headless: false,
                            chrome_child: None,
                        });

                        if let Some(sid) = session_id {
                            if let Some(session) = crate::session::SESSIONS.get_session(&sid.to_string()) {
                                session.attach_page(page_for_session).await;
                            }
                        }

                        return Ok(serde_json::json!({
                            "connected": true,
                            "mode": "desktop",
                            "port": port,
                            "title": title,
                            "url": url,
                        }));
                    }
                    Err(e) => {
                        tracing::debug!(port = port, error = %e, "desktop browser connect failed, trying next");
                    }
                }
            }
        }
    }

    // Launch headless
    if mode == "auto" || mode == "headless" {
        // Pick the first installed browser from discovery. chromiumoxide's
        // own default-detection looks for `chromium`, `chromium-browser`,
        // `google-chrome` — but distros commonly install only
        // `google-chrome-stable` (no `google-chrome` symlink), so we must
        // pass the path explicitly.
        let caps = crate::discovery::detect();
        let chrome_path = caps.installed_browsers.first().map(|b| b.path.clone());

        // Pre-flight: verify Chrome binary can start before attempting full launch.
        // A simple --version check catches missing libraries, bad binaries, or
        // permission issues in <1 second instead of a 25-second timeout.
        if let Some(ref path) = chrome_path {
            let test = std::process::Command::new(path).arg("--version").output();
            match test {
                Ok(out) if out.status.success() => {
                    let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    tracing::info!(path = %path, version = %ver, "Chrome pre-flight OK");
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    return Err(format!(
                        "Chrome at `{path}` failed pre-flight check (exit {}). \
                         Stderr: {stderr}. Is the browser installed correctly?",
                        out.status.code().unwrap_or(-1)
                    ));
                }
                Err(e) => {
                    return Err(format!(
                        "Cannot execute Chrome at `{path}`: {e}. \
                         Check permissions or reinstall the browser."
                    ));
                }
            }
        }

        // Use a per-launch user-data-dir so stale `SingletonLock` files
        // from a previous crash don't block subsequent launches.
        let user_data_dir = tempfile::Builder::new()
            .prefix("desk-mcp-chrome-")
            .tempdir()
            .map_err(|e| format!("Failed to create user-data-dir: {e}"))?;

        // ── Direct Chrome launch (bypasses chromiumoxide's Browser::launch) ──
        // We spawn Chrome ourselves so every step has its own timeout and
        // we get meaningful diagnostics at each phase. chromiumoxide's
        // Browser::launch() can hang indefinitely on some systems.
        let chrome_bin = chrome_path.as_deref().ok_or(
            "No Chrome/Chromium installation found. Install: sudo apt install chromium-browser",
        )?;

        // Find a free TCP port for Chrome's DevTools protocol
        let port = find_free_port()?;
        tracing::info!(port = port, chrome = %chrome_bin, "launching Chrome headless");

        eprintln!("[desk-mcp] Launching headless Chrome (this may take a few seconds)...");

        // Spawn Chrome
        let mut chrome_child = std::process::Command::new(chrome_bin)
            .arg(format!("--remote-debugging-port={port}"))
            .arg("--headless=new")
            .arg("--no-sandbox")
            .arg("--disable-gpu")
            .arg("--disable-dev-shm-usage")
            .arg("--disable-setuid-sandbox")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--hide-scrollbars")
            .arg("--mute-audio")
            .arg(format!(
                "--user-data-dir={}",
                user_data_dir.path().display()
            ))
            .arg("about:blank")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| {
                format!(
                    "Cannot start Chrome at `{chrome_bin}`: {e}. \
                 Verify the browser is installed and executable."
                )
            })?;

        // Wait for Chrome's DevTools endpoint to become ready.
        // We poll /json/version because stderr parsing is fragile across versions.
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

        let version_url = format!("http://localhost:{port}/json/version");
        let start = std::time::Instant::now();
        let deadline = start + Duration::from_secs(20);
        let mut connected = false;

        while std::time::Instant::now() < deadline {
            match client.get(&version_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    connected = true;
                    break;
                }
                _ => {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
            }
        }

        if !connected {
            // Kill the Chrome child
            let _ = chrome_child.kill();
            let _ = chrome_child.wait();
            // Read any stderr for diagnostics
            let stderr_out = chrome_child
                .stderr
                .take()
                .and_then(|mut p| {
                    use std::io::Read;
                    let mut buf = String::new();
                    p.read_to_string(&mut buf).ok().map(|_| buf)
                })
                .unwrap_or_default();
            return Err(format!(
                "Chrome started but DevTools did not respond within 20s. \
                 Port: {port}. Chrome stderr: {stderr_out:.200} \
                 Try: chrome --headless=new --no-sandbox --remote-debugging-port={port} about:blank"
            ));
        }

        tracing::info!(
            port = port,
            elapsed_ms = start.elapsed().as_millis(),
            "Chrome DevTools ready"
        );

        // Connect to the CDP endpoint via our robust helper — same pattern
        // used for desktop mode. Uses custom reqwest with timeouts and
        // wraps the WebSocket handshake in a 10s deadline.
        let (browser, mut handler) = match connect_to_cdp_port(port).await {
            Ok(pair) => pair,
            Err(e) => {
                let _ = chrome_child.kill();
                let _ = chrome_child.wait();
                return Err(format!(
                    "Chrome DevTools is listening on port {port} but CDP handshake failed: {e}. \
                     Try: chrome --headless=new --no-sandbox --remote-debugging-port={port} about:blank"
                ));
            }
        };

        // Wait briefly for browser to initialize
        tokio::time::sleep(Duration::from_millis(500)).await;

        let page = browser
            .new_page(CreateTargetParams::default())
            .await
            .map_err(|e| format!("Failed to create page: {e}"))?;

        // Spawn handler to consume events
        tokio::spawn(async move { while handler.next().await.is_some() {} });

        // Clone page before moving it into the global BROWSER so we can
        // attach a copy to the session.
        let page_for_session = page.clone();

        let mut guard = BROWSER.write().await;
        *guard = Some(BrowserState {
            _browser: browser,
            current_page: page,
            is_headless: true,
            chrome_child: Some(chrome_child),
        });

        if let Some(sid) = session_id {
            if let Some(session) = crate::session::SESSIONS.get_session(&sid.to_string()) {
                session.attach_page(page_for_session).await;
            }
        }

        return Ok(serde_json::json!({
            "connected": true,
            "mode": "headless",
            "title": "about:blank",
            "url": "about:blank",
        }));
    }

    Err("No browser found. Install chromium (pacman -S chromium) and ensure --remote-debugging-port is set, or allow headless mode.".into())
}

async fn browser_navigate(page: &Page, args: &Value) -> Result<Value, String> {
    let url = args["url"].as_str().ok_or("url is required")?;

    let nav: chromiumoxide::cdp::browser_protocol::page::NavigateParams = url.into();

    page.goto(nav)
        .await
        .map_err(|e| format!("Navigation failed: {e}"))?;

    let current_url = page.url().await.ok().flatten().unwrap_or_default();
    let title = page.get_title().await.ok().flatten().unwrap_or_default();

    Ok(serde_json::json!({
        "url": current_url,
        "title": title,
    }))
}

async fn browser_click(page: &Page, args: &Value) -> Result<Value, String> {
    if let Some(selector) = args.get("selector").and_then(|v| v.as_str()) {
        let element = page
            .find_element(selector)
            .await
            .map_err(|e| format!("Element not found ({selector}): {e}"))?;

        element
            .click()
            .await
            .map_err(|e| format!("Click failed: {e}"))?;

        return Ok(serde_json::json!({"clicked": selector}));
    }

    if let (Some(x), Some(y)) = (
        args.get("x").and_then(|v| v.as_f64()),
        args.get("y").and_then(|v| v.as_f64()),
    ) {
        page.click(Point::new(x, y))
            .await
            .map_err(|e| format!("Click at ({x},{y}) failed: {e}"))?;

        return Ok(serde_json::json!({"clicked": {"x": x, "y": y}}));
    }

    Err("Either 'selector' or 'x'+'y' coords required".into())
}

async fn browser_type(page: &Page, args: &Value) -> Result<Value, String> {
    let selector = args["selector"].as_str().ok_or("selector is required")?;
    let text = args["text"].as_str().ok_or("text is required")?;

    let element = page
        .find_element(selector)
        .await
        .map_err(|e| format!("Element not found ({selector}): {e}"))?;

    element
        .type_str(text)
        .await
        .map_err(|e| format!("Type failed: {e}"))?;

    Ok(serde_json::json!({"selector": selector, "typed": text}))
}

async fn browser_screenshot(page: &Page, args: &Value) -> Result<Value, String> {
    use base64::Engine;

    if let Some(selector) = args.get("selector").and_then(|v| v.as_str()) {
        let element = page
            .find_element(selector)
            .await
            .map_err(|e| format!("Element not found ({selector}): {e}"))?;

        let format = chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat::Png;
        let bytes = element
            .screenshot(format)
            .await
            .map_err(|e| format!("Element screenshot failed: {e}"))?;

        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        return Ok(serde_json::json!({
            "image_base64": b64,
            "format": "png",
            "size_bytes": bytes.len(),
            "selector": selector,
        }));
    }

    let params = chromiumoxide::page::ScreenshotParams::default();
    let bytes = page
        .screenshot(params)
        .await
        .map_err(|e| format!("Screenshot failed: {e}"))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    Ok(serde_json::json!({
        "image_base64": b64,
        "format": "png",
        "size_bytes": bytes.len(),
    }))
}

async fn browser_exec_js(page: &Page, args: &Value) -> Result<Value, String> {
    let code = args["code"].as_str().ok_or("code is required")?;

    let result: EvaluationResult = page
        .evaluate(code)
        .await
        .map_err(|e| format!("JS execution failed: {e}"))?;

    let value: Value = result
        .into_value()
        .map_err(|e| format!("Failed to deserialize JS result: {e}"))?;

    Ok(serde_json::json!({"result": value}))
}

async fn browser_get_html(page: &Page, args: &Value) -> Result<Value, String> {
    let html = if let Some(selector) = args.get("selector").and_then(|v| v.as_str()) {
        // Use JS to get outerHTML of the element
        let script = format!(
            "(() => {{ const el = document.querySelector({sel}); return el ? el.outerHTML : ''; }})()",
            sel = serde_json::to_string(selector).unwrap_or_else(|_| format!("\"{selector}\""))
        );
        let result: EvaluationResult = page
            .evaluate(script.as_str())
            .await
            .map_err(|e| format!("Failed to get element HTML: {e}"))?;
        result.into_value::<String>().unwrap_or_default()
    } else {
        page.content().await.map_err(|e| format!("{e}"))?
    };

    Ok(serde_json::json!({"html": html}))
}

async fn browser_get_text(page: &Page, args: &Value) -> Result<Value, String> {
    let text = if let Some(selector) = args.get("selector").and_then(|v| v.as_str()) {
        let sel = selector.replace('\\', "\\\\").replace('\'', "\\'");
        let script = format!("document.querySelector('{sel}')?.innerText || ''");
        let result: EvaluationResult = page
            .evaluate(script.as_str())
            .await
            .map_err(|e| format!("Text extraction failed: {e}"))?;
        result.into_value::<String>().unwrap_or_default()
    } else {
        let result: EvaluationResult = page
            .evaluate("document.body?.innerText || ''")
            .await
            .map_err(|e| format!("Text extraction failed: {e}"))?;
        result.into_value::<String>().unwrap_or_default()
    };

    Ok(serde_json::json!({"text": text}))
}

async fn browser_wait_for(page: &Page, args: &Value) -> Result<Value, String> {
    let timeout = args.get("timeout").and_then(|v| v.as_f64()).unwrap_or(30.0);

    if let Some(selector) = args.get("selector").and_then(|v| v.as_str()) {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs_f64() < timeout {
            if let Ok(_elm) = page.find_element(selector).await {
                // Found it! Get the tag name
                let sel_escaped = selector.replace('\\', "\\\\").replace('\'', "\\'");
                let script = format!(
                    "document.querySelector('{sel_escaped}')?.tagName?.toLowerCase() || ''"
                );
                let tag: String = page
                    .evaluate(script.as_str())
                    .await
                    .ok()
                    .and_then(|r| r.into_value().ok())
                    .unwrap_or_default();

                return Ok(serde_json::json!({
                    "found": true,
                    "tag": tag,
                    "selector": selector,
                    "waited_ms": start.elapsed().as_millis(),
                }));
            }
            sleep(Duration::from_millis(300)).await;
        }
        return Err(format!("Selector '{selector}' not found within {timeout}s"));
    }

    if let Some(text) = args.get("text").and_then(|v| v.as_str()) {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs_f64() < timeout {
            let script = "document.body?.innerText || ''";
            let body_text: String = page
                .evaluate(script)
                .await
                .ok()
                .and_then(|r| r.into_value().ok())
                .unwrap_or_default();

            if body_text.contains(text) {
                return Ok(serde_json::json!({
                    "found": true,
                    "text": text,
                    "waited_ms": start.elapsed().as_millis(),
                }));
            }
            sleep(Duration::from_millis(300)).await;
        }
        return Err(format!("Text '{text}' not found within {timeout}s"));
    }

    Err("Either 'selector' or 'text' required".into())
}

async fn browser_tabs() -> Result<Value, String> {
    let guard = BROWSER.read().await;
    let state = guard.as_ref().ok_or("Browser not launched")?;

    let pages = state
        ._browser
        .pages()
        .await
        .map_err(|e| format!("Failed to list pages: {e}"))?;

    let mut tabs = Vec::new();
    for page in &pages {
        let title = page.get_title().await.ok().flatten().unwrap_or_default();
        let url = page.url().await.ok().flatten().unwrap_or_default();
        let tid = page.target_id().inner().clone();

        tabs.push(serde_json::json!({
            "target_id": tid,
            "title": title,
            "url": url,
        }));
    }

    Ok(serde_json::json!({
        "tabs": tabs,
        "count": tabs.len(),
    }))
}

async fn browser_new_tab(args: &Value) -> Result<Value, String> {
    let guard = BROWSER.read().await;
    let state = guard.as_ref().ok_or("Browser not launched")?;

    let page = state
        ._browser
        .new_page(CreateTargetParams::default())
        .await
        .map_err(|e| format!("Failed to create new page: {e}"))?;

    if let Some(url) = args.get("url").and_then(|v| v.as_str()) {
        let nav: chromiumoxide::cdp::browser_protocol::page::NavigateParams = url.into();
        page.goto(nav)
            .await
            .map_err(|e| format!("Navigate failed: {e}"))?;
    }

    let tid = page.target_id().inner().clone();
    let title = page.get_title().await.ok().flatten().unwrap_or_default();
    let url = page.url().await.ok().flatten().unwrap_or_default();

    Ok(serde_json::json!({
        "target_id": tid,
        "title": title,
        "url": url,
    }))
}

async fn browser_close_tab(page: &Page, args: &Value) -> Result<Value, String> {
    // Check if a specific tab index is requested
    if let Some(idx) = args.get("index").and_then(|v| v.as_u64()) {
        let guard = BROWSER.read().await;
        let state = guard.as_ref().ok_or("Browser not launched")?;

        let pages = state
            ._browser
            .pages()
            .await
            .map_err(|e| format!("Failed to list pages: {e}"))?;

        if let Some(target) = pages.get(idx as usize) {
            if pages.len() <= 1 {
                return Err("Cannot close the last tab".into());
            }
            let target = target.clone();
            target.close().await.map_err(|e| format!("{e}"))?;
            return Ok(serde_json::json!({"closed": idx}));
        }
        return Err(format!("Tab index {idx} out of range"));
    }

    // Close the current page
    let guard = BROWSER.read().await;
    let state = guard.as_ref().ok_or("Browser not launched")?;

    let pages = state
        ._browser
        .pages()
        .await
        .map_err(|e| format!("Failed to list pages: {e}"))?;

    if pages.len() <= 1 {
        return Err("Cannot close the last tab".into());
    }

    let current = page.clone();
    current.close().await.map_err(|e| format!("{e}"))?;

    Ok(serde_json::json!({"closed": true}))
}

async fn browser_switch_tab(args: &Value) -> Result<Value, String> {
    let guard = BROWSER.read().await;
    let state = guard.as_ref().ok_or("Browser not launched")?;

    let pages = state
        ._browser
        .pages()
        .await
        .map_err(|e| format!("Failed to list pages: {e}"))?;

    let target: Option<Page> = if let Some(idx) = args.get("index").and_then(|v| v.as_u64()) {
        pages.get(idx as usize).cloned()
    } else if let Some(title_match) = args.get("title").and_then(|v| v.as_str()) {
        let title_lower = title_match.to_lowercase();
        let mut found = None;
        for page in &pages {
            let t = page.get_title().await.ok().flatten().unwrap_or_default();
            if t.to_lowercase().contains(&title_lower) {
                found = Some(page.clone());
                break;
            }
        }
        found
    } else {
        return Err("Either 'index' or 'title' required".into());
    };

    let target = target.ok_or("Tab not found")?;
    target
        .activate()
        .await
        .map_err(|e| format!("Failed to switch tab: {e}"))?;

    let title = target.get_title().await.ok().flatten().unwrap_or_default();
    let url = target.url().await.ok().flatten().unwrap_or_default();

    Ok(serde_json::json!({
        "switched_to": {
            "target_id": target.target_id().inner().clone(),
            "title": title,
            "url": url,
        }
    }))
}

async fn browser_download(page: &Page, args: &Value) -> Result<Value, String> {
    let selector = args["selector"].as_str().ok_or("selector is required")?;

    page.find_element(selector)
        .await
        .map_err(|e| format!("Download element not found ({selector}): {e}"))?
        .click()
        .await
        .map_err(|e| format!("Download click failed: {e}"))?;

    Ok(serde_json::json!({
        "clicked": selector,
        "note": "Download initiated. Check browser downloads directory.",
    }))
}

async fn browser_upload(page: &Page, args: &Value) -> Result<Value, String> {
    let selector = args["selector"].as_str().ok_or("selector is required")?;
    let _files: Vec<&str> = args["files"]
        .as_array()
        .ok_or("files array is required")?
        .iter()
        .filter_map(|v| v.as_str())
        .collect();

    // Chromiumoxide doesn't expose upload_files directly.
    // We use the page keyboard to interact with a file input instead.
    let _element = page
        .find_element(selector)
        .await
        .map_err(|e| format!("Upload element not found ({selector}): {e}"))?;

    // NOTE: Direct file upload via Element is limited in chromiumoxide.
    // For now, report what would be uploaded.
    Ok(serde_json::json!({
        "uploaded": false,
        "selector": selector,
        "note": "Direct file upload not supported via chromiumoxide. Use keyboard simulation or shell_run with xdotool to interact with file dialog.",
    }))
}

async fn browser_cookies(_page: &Page) -> Result<Value, String> {
    let guard = BROWSER.read().await;
    let state = guard.as_ref().ok_or("Browser not launched")?;

    let cookies = state
        ._browser
        .get_cookies()
        .await
        .map_err(|e| format!("Failed to get cookies: {e}"))?;

    Ok(serde_json::json!({
        "cookies": cookies,
    }))
}

async fn browser_console(page: &Page, args: &Value) -> Result<Value, String> {
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50);

    let script = format!(
        "(() => {{ const msgs = window.__console_log || []; return msgs.slice(-{}); }})()",
        limit
    );

    let result: EvaluationResult = page
        .evaluate(script.as_str())
        .await
        .map_err(|e| format!("Failed to get console: {e}"))?;

    let messages: Value = result.into_value().unwrap_or(serde_json::json!([]));

    Ok(serde_json::json!({
        "messages": messages,
        "note": "Console messages captured via window.__console_log. Set up CDP Runtime.consoleAPICalled listener for real-time capture.",
    }))
}
