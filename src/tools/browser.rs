//! Browser use tool handlers — 17 tools for web automation via CDP.
//!
//! Uses chromiumoxide (pure Rust Chrome DevTools Protocol client)
//! to control Chromium/Chrome browsers headless or with a visible window.

use crate::response::{self, ToolResponse};
use anyhow::Result;
use chromiumoxide::{
    browser::{Browser, BrowserConfig},
    cdp::browser_protocol::target::CreateTargetParams,
    js::EvaluationResult,
    layout::Point,
    Page,
};
use futures::StreamExt;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

/// Global browser handle — lazily initialized, shared across calls
static BROWSER: std::sync::LazyLock<Mutex<Option<BrowserState>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));

struct BrowserState {
    _browser: Browser,
    current_page: Page,
    #[allow(dead_code)]
    is_headless: bool,
}

/// Handle all browser tool calls
pub async fn handle(name: &str, args: Value) -> ToolResponse {
    let result = handle_inner(name, args).await;
    match result {
        Ok(value) => response::ok(value),
        Err(message) => response::err("BROWSER_ERROR", &message),
    }
}

async fn handle_inner(name: &str, mut args: Value) -> Result<Value, String> {
    match name {
        "browser_launch" => browser_launch(&mut args).await,
        _ => {
            let page = get_page().await?;
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

/// Get the current page
async fn get_page() -> Result<Page, String> {
    let guard = BROWSER.lock().await;
    match guard.as_ref() {
        Some(state) => Ok(state.current_page.clone()),
        None => Err("Browser not launched. Call browser_launch first.".into()),
    }
}

// ═══════════════════ TOOL HANDLERS ═══════════════════

async fn browser_launch(args: &mut Value) -> Result<Value, String> {
    let mode = args
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("auto");

    // Try connecting to an already-running browser first
    if mode == "auto" || mode == "desktop" {
        let caps = crate::discovery::detect();
        for info in &caps.discovered_browsers {
            if let Some(port) = info.debugging_port {
                let url = format!("http://localhost:{port}");
                if let Ok((browser, mut handler)) = Browser::connect(&url).await {
                    // Get first page, or create one
                    let page = match browser.pages().await {
                        Ok(pages) => match pages.into_iter().next() {
                            Some(p) => p,
                            None => {
                                // Need a page — create one, but we just connected
                                // Can't create without handler's session
                                return Err("Connected but no pages available".into());
                            }
                        },
                        Err(e) => return Err(format!("Failed to list pages: {e}")),
                    };

                    // Spawn handler to consume events
                    tokio::spawn(async move {
                        while handler.next().await.is_some() {}
                    });

                    let title = page
                        .get_title()
                        .await
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    let url = page.url().await.ok().flatten().unwrap_or_default();

                    let mut guard = BROWSER.lock().await;
                    *guard = Some(BrowserState {
                        _browser: browser,
                        current_page: page,
                        is_headless: false,
                    });

                    return Ok(serde_json::json!({
                        "connected": true,
                        "mode": "desktop",
                        "port": port,
                        "title": title,
                        "url": url,
                    }));
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

        // Use a per-launch user-data-dir so stale `SingletonLock` files
        // from a previous crash don't block subsequent launches.
        let user_data_dir = tempfile::Builder::new()
            .prefix("desk-mcp-chrome-")
            .tempdir()
            .map_err(|e| format!("Failed to create user-data-dir: {e}"))?;

        let extra_args = [
            // Common headless-on-Linux stability flags. Without these,
            // Chrome 149 can hang on launch in some environments (notably
            // when /dev/shm is small or when GPU is unavailable).
            "--disable-dev-shm-usage",
            "--disable-gpu",
            "--disable-setuid-sandbox",
            "--no-first-run",
            "--no-default-browser-check",
        ];

        let mut builder = BrowserConfig::builder();
        builder = builder
            .no_sandbox()
            .new_headless_mode()
            .window_size(1280, 1024)
            .user_data_dir(user_data_dir.path())
            .launch_timeout(Duration::from_secs(20))
            .request_timeout(Duration::from_secs(15))
            .args(extra_args);
        if let Some(path) = chrome_path.as_deref() {
            builder = builder.chrome_executable(path);
        }

        let config = builder
            .build()
            .map_err(|e| format!("Invalid browser config: {e}"))?;

        // Wrap the entire launch future in a hard timeout. chromiumoxide's
        // own `launch_timeout` only bounds the wait for the DevTools websocket
        // URL on chrome's stderr — it does NOT bound the rest of the launch
        // path (e.g. CDP handshake). Without this outer wrapper, a chrome
        // child that starts but then hangs leaves the entire MCP call
        // stuck forever. On timeout, the future is dropped, which drops
        // the inner `Browser` instance, which (because `kill_on_drop`
        // defaults to true in chromiumoxide) kills the chrome child.
        const LAUNCH_TIMEOUT_SECS: u64 = 25;
        let launch_result = tokio::time::timeout(
            Duration::from_secs(LAUNCH_TIMEOUT_SECS),
            Browser::launch(config),
        )
        .await;

        let (browser, mut handler) = match launch_result {
            Ok(Ok(pair)) => pair,
            Ok(Err(e)) => {
                return Err(format!(
                    "Failed to launch browser: {e}. \
                     Detected browser path: `{}`. \
                     User-data-dir: `{}`. \
                     Try running the browser manually to diagnose.",
                    chrome_path
                        .clone()
                        .unwrap_or_else(|| "<no browser detected>".into()),
                    user_data_dir.path().display()
                ));
            }
            Err(_elapsed) => {
                return Err(format!(
                    "Browser launch timed out after {LAUNCH_TIMEOUT_SECS}s — \
                     Chrome did not become responsive. Detected browser path: \
                     `{}`. User-data-dir: `{}`.",
                    chrome_path
                        .clone()
                        .unwrap_or_else(|| "<no browser detected>".into()),
                    user_data_dir.path().display()
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
        tokio::spawn(async move {
            while handler.next().await.is_some() {}
        });

        let mut guard = BROWSER.lock().await;
        *guard = Some(BrowserState {
            _browser: browser,
            current_page: page,
            is_headless: true,
        });

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

    let nav: chromiumoxide::cdp::browser_protocol::page::NavigateParams =
        url.into();

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
    let timeout = args
        .get("timeout")
        .and_then(|v| v.as_f64())
        .unwrap_or(30.0);

    if let Some(selector) = args.get("selector").and_then(|v| v.as_str()) {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs_f64() < timeout {
            if let Ok(_elm) = page.find_element(selector).await {
                // Found it! Get the tag name
                let sel_escaped = selector.replace('\\', "\\\\").replace('\'', "\\'");
                let script = format!("document.querySelector('{sel_escaped}')?.tagName?.toLowerCase() || ''");
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
    let guard = BROWSER.lock().await;
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
    let guard = BROWSER.lock().await;
    let state = guard.as_ref().ok_or("Browser not launched")?;

    let page = state
        ._browser
        .new_page(CreateTargetParams::default())
        .await
        .map_err(|e| format!("Failed to create new page: {e}"))?;

    if let Some(url) = args.get("url").and_then(|v| v.as_str()) {
        let nav: chromiumoxide::cdp::browser_protocol::page::NavigateParams =
            url.into();
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
        let guard = BROWSER.lock().await;
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
    let guard = BROWSER.lock().await;
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
    let guard = BROWSER.lock().await;
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
    let guard = BROWSER.lock().await;
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
