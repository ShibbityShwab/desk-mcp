//! Three-tier resolution router — unifies how desk-mcp resolves agent interaction targets.
//!
//! ## Tiers
//! 1. **Accessibility** — AT-SPI element tree (fastest, most precise)
//! 2. **Browser CDP**   — Chrome DevTools Protocol (for web content in a browser)
//! 3. **Vision + OCR**   — Screenshot → OCR → click (universal fallback)
//!
//! Each tier is tried in order; the first successful resolution wins.

use crate::providers::WindowState;
use crate::PROVIDER;
use serde::Serialize;
use std::time::Instant;

// ── Public types ────────────────────────────────────────────────────────────

/// What action the agent wants to perform.
#[derive(Debug, Clone)]
pub enum Action {
    Click { button: String },
    DoubleClick,
    RightClick,
    TypeText { text: String },
    PressKey { key: String },
    Scroll { dx: i32, dy: i32 },
}

/// How the agent has described the target.
#[derive(Debug, Clone)]
pub enum Target {
    ByRole {
        role: String,
        name_contains: Option<String>,
    },
    ByName {
        name: String,
    },
    ByText {
        text: String,
    },
    BySelector {
        selector: String,
    },
    ByCoordinates {
        x: i32,
        y: i32,
    },
}

/// Which resolution tier was used.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionTier {
    Accessibility,
    BrowserCdp,
    VisionOcr,
}

/// A resolved element ready for action.
pub struct ResolvedElement {
    pub bounds: Option<Rect>,
    pub tier: ResolutionTier,
    pub element_index: Option<u32>,
    pub selector: Option<String>,
    pub text_match: Option<String>,
}

/// Simple rectangle.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Outcome of `resolve_and_act`.
#[derive(Debug, Clone, Serialize)]
pub struct ActionResult {
    pub success: bool,
    pub tier_used: String,
    pub element_found: bool,
    pub position_clicked: Option<(i32, i32)>,
    pub duration_ms: u64,
}

// ── Main entry point ────────────────────────────────────────────────────────

/// Resolve a target through the three-tier cascade and perform the action.
pub async fn resolve_and_act(action: Action, target: Target) -> Result<ActionResult, String> {
    let start = Instant::now();

    // ── Tier 1: AT-SPI element tree ──
    if let Ok(state) = PROVIDER.get_window_state() {
        if state.element_count > 0 {
            if let Some(el) = find_in_tree(&state, &target) {
                return act_on_element(action, el, ResolutionTier::Accessibility, start);
            }
        }
    }

    // ── Tier 2: Browser CDP ──
    if browser_connected().await && active_window_is_browser().unwrap_or(false) {
        match resolve_via_cdp(&target).await {
            Ok(el) => {
                return act_via_cdp(action, el, start).await;
            }
            Err(_) => { /* fall through */ }
        }
    }

    // ── Tier 3: Vision + OCR fallback ──
    let png = PROVIDER
        .screenshot(None)
        .map_err(|e| format!("screenshot failed: {e}"))?;
    let active_window = PROVIDER.get_active_window().ok().flatten().map(|w| {
        serde_json::json!({
            "title": w.title,
            "app": w.app,
            "geometry": {"x": w.geometry.x, "y": w.geometry.y, "width": w.geometry.width, "height": w.geometry.height}
        })
    });
    let state = crate::vision::screen_state(&png, active_window)
        .map_err(|e| format!("screen_state failed: {e}"))?;
    match resolve_via_vision(&state, &target) {
        Some(el) => act_via_vision(action, el, start).await,
        None => Err("Could not resolve target through any tier".to_string()),
    }
}

// ── Tier 1: AT-SPI tree search ──────────────────────────────────────────────

/// Search the accessibility element tree for the given target.
fn find_in_tree(state: &WindowState, target: &Target) -> Option<ResolvedElement> {
    match target {
        Target::ByRole { role, name_contains } => {
            let role_lower = role.to_lowercase();
            let name_filter = name_contains.as_ref().map(|n| n.to_lowercase());
            for el in &state.elements {
                if el.role.to_lowercase().contains(&role_lower) {
                    if let Some(ref name) = name_filter {
                        if !el.name.to_lowercase().contains(name) {
                            continue;
                        }
                    }
                    return Some(ResolvedElement {
                        bounds: el.bounds.as_ref().map(|b| Rect {
                            x: b.x,
                            y: b.y,
                            width: b.width,
                            height: b.height,
                        }),
                        tier: ResolutionTier::Accessibility,
                        element_index: Some(el.index),
                        selector: None,
                        text_match: None,
                    });
                }
            }
            None
        }
        Target::ByName { name } => {
            let name_lower = name.to_lowercase();
            for el in &state.elements {
                if el.name.to_lowercase() == name_lower {
                    return Some(ResolvedElement {
                        bounds: el.bounds.as_ref().map(|b| Rect {
                            x: b.x,
                            y: b.y,
                            width: b.width,
                            height: b.height,
                        }),
                        tier: ResolutionTier::Accessibility,
                        element_index: Some(el.index),
                        selector: None,
                        text_match: None,
                    });
                }
            }
            None
        }
        Target::ByText { text } => {
            let text_lower = text.to_lowercase();
            for el in &state.elements {
                let name_lower = el.name.to_lowercase();
                let value_lower = el.value.as_ref().map(|v| v.to_lowercase()).unwrap_or_default();
                if name_lower.contains(&text_lower) || value_lower.contains(&text_lower) {
                    return Some(ResolvedElement {
                        bounds: el.bounds.as_ref().map(|b| Rect {
                            x: b.x,
                            y: b.y,
                            width: b.width,
                            height: b.height,
                        }),
                        tier: ResolutionTier::Accessibility,
                        element_index: Some(el.index),
                        selector: None,
                        text_match: Some(el.name.clone()),
                    });
                }
            }
            None
        }
        Target::BySelector { .. } => {
            // AT-SPI doesn't understand CSS selectors
            None
        }
        Target::ByCoordinates { x, y } => {
            // Find the element at those screen coordinates
            for el in &state.elements {
                if let Some(ref b) = el.bounds {
                    if *x >= b.x && *x <= b.x + b.width && *y >= b.y && *y <= b.y + b.height {
                        return Some(ResolvedElement {
                            bounds: Some(Rect {
                                x: b.x,
                                y: b.y,
                                width: b.width,
                                height: b.height,
                            }),
                            tier: ResolutionTier::Accessibility,
                            element_index: Some(el.index),
                            selector: None,
                            text_match: None,
                        });
                    }
                }
            }
            // No element found at coordinates — still resolvable as raw coords
            Some(ResolvedElement {
                bounds: Some(Rect {
                    x: *x,
                    y: *y,
                    width: 1,
                    height: 1,
                }),
                tier: ResolutionTier::Accessibility,
                element_index: None,
                selector: None,
                text_match: None,
            })
        }
    }
}

// ── Tier 1 action execution ─────────────────────────────────────────────────

fn act_on_element(
    action: Action,
    el: ResolvedElement,
    tier: ResolutionTier,
    start: Instant,
) -> Result<ActionResult, String> {
    let center = el.bounds.as_ref().map(|b| (b.x + b.width / 2, b.y + b.height / 2));
    let (cx, cy) = center.unwrap_or((0, 0));

    match action {
        Action::Click { button } => {
            PROVIDER
                .mouse_click(&button, Some(cx), Some(cy), 1)
                .map_err(|e| format!("click failed: {e}"))?;
        }
        Action::DoubleClick => {
            PROVIDER
                .mouse_click("left", Some(cx), Some(cy), 2)
                .map_err(|e| format!("double-click failed: {e}"))?;
        }
        Action::RightClick => {
            PROVIDER
                .mouse_click("right", Some(cx), Some(cy), 1)
                .map_err(|e| format!("right-click failed: {e}"))?;
        }
        Action::TypeText { text } => {
            PROVIDER
                .keyboard_type(&text, 10)
                .map_err(|e| format!("type failed: {e}"))?;
        }
        Action::PressKey { key } => {
            PROVIDER
                .key_press(&key)
                .map_err(|e| format!("key_press failed: {e}"))?;
        }
        Action::Scroll { dx, dy } => {
            PROVIDER
                .mouse_scroll(dx, dy, Some(cx), Some(cy))
                .map_err(|e| format!("scroll failed: {e}"))?;
        }
    }

    Ok(ActionResult {
        success: true,
        tier_used: tier_to_str(tier),
        element_found: true,
        position_clicked: center,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

// ── Tier 2: Browser CDP ─────────────────────────────────────────────────────

/// Check whether a browser is connected via CDP.
pub async fn browser_connected() -> bool {
    crate::tools::browser::is_connected().await
}

/// Check whether the active window belongs to a known browser.
fn active_window_is_browser() -> Result<bool, String> {
    let window = PROVIDER
        .get_active_window()
        .map_err(|e| format!("get_active_window failed: {e}"))?;
    let Some(win) = window else {
        return Ok(false);
    };

    let caps = crate::discovery::detect();
    let active_pid = win.pid;

    // Check if active window PID matches a discovered browser PID
    if let Some(pid) = active_pid {
        for info in &caps.discovered_browsers {
            if info.pid == Some(pid) {
                return Ok(true);
            }
        }
    }

    // Also check by app name (e.g. "chrome", "firefox", "chromium")
    let app_lower = win.app.to_lowercase();
    let browser_apps = [
        "chrome", "chromium", "firefox", "brave", "edge", "opera", "vivaldi",
        "google-chrome",
    ];
    for name in &browser_apps {
        if app_lower.contains(name) {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Resolve a target via CDP (Tier 2).
async fn resolve_via_cdp(target: &Target) -> Result<ResolvedElement, String> {
    let page = get_browser_page().await?;

    match target {
        Target::BySelector { selector } => {
            // Verify the selector exists
            page.find_element(selector.as_str())
                .await
                .map_err(|e| format!("CDP selector '{selector}' not found: {e}"))?;
            Ok(ResolvedElement {
                bounds: None,
                tier: ResolutionTier::BrowserCdp,
                element_index: None,
                selector: Some(selector.clone()),
                text_match: None,
            })
        }
        Target::ByText { text } => {
            // Search DOM for elements containing the text
            let escaped = text.replace('\\', "\\\\").replace('\'', "\\'");
            let script = format!(
                "(() => {{ \
                 const all = document.querySelectorAll('*'); \
                 for (const el of all) {{ \
                   if (el.innerText && el.innerText.trim() === '{escaped}') {{ \
                     return {{ tag: el.tagName, id: el.id, className: el.className, text: el.innerText.trim() }}; \
                   }} \
                 }} \
                 for (const el of all) {{ \
                   if (el.innerText && el.innerText.includes('{escaped}')) {{ \
                     return {{ tag: el.tagName, id: el.id, className: el.className, text: el.innerText.trim().substring(0, 200) }}; \
                   }} \
                 }} \
                 return null; \
                 }})()"
            );
            let result: chromiumoxide::js::EvaluationResult = page
                .evaluate(script.as_str())
                .await
                .map_err(|e| format!("CDP text search failed: {e}"))?;

            let found: Option<serde_json::Value> = result
                .into_value()
                .map_err(|e| format!("CDP result deserialize: {e}"))?;

            match found {
                Some(ref v) if !v.is_null() => {
                    // Build a selector from the found element info
                    let tag = v.get("tag").and_then(|t| t.as_str()).unwrap_or("*");
                    let id = v.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    let selector = if !id.is_empty() {
                        format!("#{id}")
                    } else {
                        // Use a text-based selector fallback
                        format!("{tag}")
                    };
                    Ok(ResolvedElement {
                        bounds: None,
                        tier: ResolutionTier::BrowserCdp,
                        element_index: None,
                        selector: Some(selector),
                        text_match: Some(text.clone()),
                    })
                }
                _ => Err(format!("CDP: text '{text}' not found in DOM")),
            }
        }
        Target::ByRole { role, name_contains } => {
            let role_sel = format!("[role=\"{role}\"]");
            // Try selecting by role first
            if let Ok(_el) = page.find_element(&role_sel).await {
                let selector = if let Some(ref name) = name_contains {
                    format!("[role=\"{role}\"][aria-label*=\"{name}\" i],[role=\"{role}\"][name*=\"{name}\" i]")
                } else {
                    role_sel
                };
                // Verify at least one match
                page.find_element(&selector)
                    .await
                    .map_err(|e| format!("CDP role+name selector not found: {e}"))?;
                return Ok(ResolvedElement {
                    bounds: None,
                    tier: ResolutionTier::BrowserCdp,
                    element_index: None,
                    selector: Some(selector),
                    text_match: None,
                });
            }
            Err(format!("CDP: no element with role '{role}'"))
        }
        Target::ByName { name } => {
            let selector = format!("[name=\"{name}\"],[aria-label=\"{name}\"]");
            page.find_element(&selector)
                .await
                .map_err(|e| format!("CDP name selector not found: {e}"))?;
            Ok(ResolvedElement {
                bounds: None,
                tier: ResolutionTier::BrowserCdp,
                element_index: None,
                selector: Some(selector),
                text_match: None,
            })
        }
        Target::ByCoordinates { .. } => {
            Err("CDP: coordinate-based targeting not supported in browser tier".into())
        }
    }
}

/// Execute an action via CDP (Tier 2).
async fn act_via_cdp(
    action: Action,
    el: ResolvedElement,
    start: Instant,
) -> Result<ActionResult, String> {
    let page = get_browser_page().await?;
    let selector = el.selector.as_deref().unwrap_or("body");

    match action {
        Action::Click { .. } | Action::DoubleClick | Action::RightClick => {
            let element = page
                .find_element(selector)
                .await
                .map_err(|e| format!("CDP click: element not found ({selector}): {e}"))?;

            if matches!(action, Action::RightClick) {
                // Right-click via dispatching contextmenu event
                let script = format!(
                    "document.querySelector('{sel}')?.dispatchEvent(new MouseEvent('contextmenu', {{bubbles: true}}))",
                    sel = selector.replace('\'', "\\'")
                );
                page.evaluate(script.as_str())
                    .await
                    .map_err(|e| format!("CDP right-click failed: {e}"))?;
            } else {
                let clicks: u32 = if matches!(action, Action::DoubleClick) {
                    2
                } else {
                    1
                };
                for _ in 0..clicks {
                    element
                        .click()
                        .await
                        .map_err(|e| format!("CDP click failed: {e}"))?;
                }
            }
        }
        Action::TypeText { text } => {
            let element = page
                .find_element(selector)
                .await
                .map_err(|e| format!("CDP type: element not found ({selector}): {e}"))?;
            element
                .type_str(&text)
                .await
                .map_err(|e| format!("CDP type failed: {e}"))?;
        }
        Action::PressKey { key } => {
            // Dispatch keyboard events via CDP Runtime.evaluate with full key properties.
            // Maps common keys to their keyCode/code values for robust browser handling.
            let (code, key_code, which) = map_key_props(&key);
            let escaped_key = key.replace('\\', "\\\\").replace('\'', "\\'");
            let script = format!(
                "['keydown','keypress','keyup'].forEach(t=>document.activeElement\
                 .dispatchEvent(new KeyboardEvent(t,{{key:'{escaped_key}',code:'{code}',\
                 keyCode:{key_code},which:{which},bubbles:true,cancelable:true}})));",
            );
            page.evaluate(script.as_str())
                .await
                .map_err(|e| format!("CDP key_press failed: {e}"))?;
        }
        Action::Scroll { dx, dy } => {
            let script = format!(
                "window.scrollBy({{left: {dx}, top: {dy}, behavior: 'smooth'}})",
            );
            page.evaluate(script.as_str())
                .await
                .map_err(|e| format!("CDP scroll failed: {e}"))?;
        }
    }

    Ok(ActionResult {
        success: true,
        tier_used: tier_to_str(ResolutionTier::BrowserCdp),
        element_found: true,
        position_clicked: None,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// Get the current browser page handle.
async fn get_browser_page() -> Result<chromiumoxide::Page, String> {
    crate::tools::browser::get_page().await
}

/// Map a key name to (code, keyCode, which) tuples for robust JS KeyboardEvent dispatch.
/// Covers common special keys; unknown keys default to (key, 0, 0).
fn map_key_props(key: &str) -> (String, u32, u32) {
    let (code, kc, w) = match key.to_lowercase().as_str() {
        "enter" | "return" => ("Enter", 13, 13),
        "tab" => ("Tab", 9, 9),
        "escape" | "esc" => ("Escape", 27, 27),
        "backspace" => ("Backspace", 8, 8),
        "delete" | "del" => ("Delete", 46, 46),
        "arrowup" | "up" => ("ArrowUp", 38, 38),
        "arrowdown" | "down" => ("ArrowDown", 40, 40),
        "arrowleft" | "left" => ("ArrowLeft", 37, 37),
        "arrowright" | "right" => ("ArrowRight", 39, 39),
        "space" | " " => ("Space", 32, 32),
        "home" => ("Home", 36, 36),
        "end" => ("End", 35, 35),
        "pageup" => ("PageUp", 33, 33),
        "pagedown" => ("PageDown", 34, 34),
        "insert" => ("Insert", 45, 45),
        "f1" => ("F1", 112, 112), "f2" => ("F2", 113, 113),
        "f3" => ("F3", 114, 114), "f4" => ("F4", 115, 115),
        "f5" => ("F5", 116, 116), "f6" => ("F6", 117, 117),
        "f7" => ("F7", 118, 118), "f8" => ("F8", 119, 119),
        "f9" => ("F9", 120, 120), "f10" => ("F10", 121, 121),
        "f11" => ("F11", 122, 122), "f12" => ("F12", 123, 123),
        _ if key.len() == 1 => {
            let ch = key.chars().next().unwrap();
            let code = ch.to_uppercase().next().unwrap() as u32;
            return (key.to_string(), code, code);
        }
        _ => return (String::new(), 0, 0),
    };
    (code.to_string(), kc, w)
}

// ── Tier 3: Vision + OCR ────────────────────────────────────────────────────

/// Resolve a target via screen capture + OCR (Tier 3).
fn resolve_via_vision(
    state: &crate::vision::ScreenState,
    target: &Target,
) -> Option<ResolvedElement> {
    match target {
        Target::ByText { text } => {
            let found = crate::ocr::find_text(&state.text_elements, text, true)?;
            let center = found.bounds.as_ref().map(|b| Rect {
                x: b.x + b.width / 2,
                y: b.y + b.height / 2,
                width: b.width,
                height: b.height,
            });
            Some(ResolvedElement {
                bounds: center,
                tier: ResolutionTier::VisionOcr,
                element_index: None,
                selector: None,
                text_match: Some(found.text.clone()),
            })
        }
        Target::ByCoordinates { x, y } => {
            // Direct coordinate-based resolution
            Some(ResolvedElement {
                bounds: Some(Rect {
                    x: *x,
                    y: *y,
                    width: 1,
                    height: 1,
                }),
                tier: ResolutionTier::VisionOcr,
                element_index: None,
                selector: None,
                text_match: None,
            })
        }
        // For role/name/selector, look in clickable regions
        Target::ByRole { role, name_contains } => {
            let role_lower = role.to_lowercase();
            for region in &state.clickable_regions {
                let region_text = region.text.to_lowercase();
                // Heuristic: match role to element type
                let type_str = format!("{:?}", region.element_type).to_lowercase();
                if type_str.contains(&role_lower) || region_text.contains(&role_lower) {
                    if let Some(ref name) = name_contains {
                        if !region_text.contains(&name.to_lowercase()) {
                            continue;
                        }
                    }
                    return Some(ResolvedElement {
                        bounds: Some(Rect {
                            x: region.x + (region.width as i32) / 2,
                            y: region.y + (region.height as i32) / 2,
                            width: region.width as i32,
                            height: region.height as i32,
                        }),
                        tier: ResolutionTier::VisionOcr,
                        element_index: None,
                        selector: None,
                        text_match: Some(region.text.clone()),
                    });
                }
            }
            None
        }
        Target::ByName { name } => {
            let name_lower = name.to_lowercase();
            for region in &state.clickable_regions {
                if region.text.to_lowercase().contains(&name_lower) {
                    return Some(ResolvedElement {
                        bounds: Some(Rect {
                            x: region.x + (region.width as i32) / 2,
                            y: region.y + (region.height as i32) / 2,
                            width: region.width as i32,
                            height: region.height as i32,
                        }),
                        tier: ResolutionTier::VisionOcr,
                        element_index: None,
                        selector: None,
                        text_match: Some(region.text.clone()),
                    });
                }
            }
            None
        }
        Target::BySelector { .. } => {
            // Vision tier doesn't understand CSS selectors
            None
        }
    }
}

/// Execute an action via screen coordinates (Tier 3 — Vision/OCR fallback).
async fn act_via_vision(
    action: Action,
    el: ResolvedElement,
    start: Instant,
) -> Result<ActionResult, String> {
    let center = el.bounds.as_ref().map(|b| (b.x, b.y));
    let (cx, cy) = center.unwrap_or((0, 0));

    match action {
        Action::Click { button } => {
            PROVIDER
                .mouse_click(&button, Some(cx), Some(cy), 1)
                .map_err(|e| format!("vision click failed: {e}"))?;
        }
        Action::DoubleClick => {
            PROVIDER
                .mouse_click("left", Some(cx), Some(cy), 2)
                .map_err(|e| format!("vision double-click failed: {e}"))?;
        }
        Action::RightClick => {
            PROVIDER
                .mouse_click("right", Some(cx), Some(cy), 1)
                .map_err(|e| format!("vision right-click failed: {e}"))?;
        }
        Action::TypeText { text } => {
            // Move to target, click to focus, then type
            PROVIDER
                .mouse_click("left", Some(cx), Some(cy), 1)
                .map_err(|e| format!("vision focus-click failed: {e}"))?;
            PROVIDER
                .keyboard_type(&text, 10)
                .map_err(|e| format!("vision type failed: {e}"))?;
        }
        Action::PressKey { key } => {
            PROVIDER
                .key_press(&key)
                .map_err(|e| format!("vision key_press failed: {e}"))?;
        }
        Action::Scroll { dx, dy } => {
            PROVIDER
                .mouse_scroll(dx, dy, Some(cx), Some(cy))
                .map_err(|e| format!("vision scroll failed: {e}"))?;
        }
    }

    Ok(ActionResult {
        success: true,
        tier_used: tier_to_str(ResolutionTier::VisionOcr),
        element_found: true,
        position_clicked: center,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn tier_to_str(tier: ResolutionTier) -> String {
    match tier {
        ResolutionTier::Accessibility => "accessibility".to_string(),
        ResolutionTier::BrowserCdp => "browser_cdp".to_string(),
        ResolutionTier::VisionOcr => "vision_ocr".to_string(),
    }
}

// ── Public dispatch helper (called from tools/mod.rs) ───────────────────────

/// Parse tool args into Action + Target, then resolve and act.
/// Returns a ToolResponse-ready result.
pub async fn dispatch_resolve(
    name: &str,
    args: &serde_json::Value,
) -> Result<crate::response::ToolResponse, String> {
    let (action, target) = parse_tool_args(name, args)?;
    let result = resolve_and_act(action, target).await?;
    Ok(crate::response::ok(result))
}

/// Parse tool name and args into Action + Target for the resolver.
fn parse_tool_args(name: &str, args: &serde_json::Value) -> Result<(Action, Target), String> {
    // Check for explicit `target` block first
    if let Some(target_obj) = args.get("target") {
        let target = parse_target(target_obj)?;
        let action = match name {
            "mouse_click" => Action::Click {
                button: args
                    .get("button")
                    .and_then(|v| v.as_str())
                    .unwrap_or("left")
                    .to_string(),
            },
            "mouse_double_click" => Action::DoubleClick,
            "keyboard_type" => Action::TypeText {
                text: args["text"].as_str().unwrap_or("").to_string(),
            },
            "click_on_text" => Action::Click {
                button: args
                    .get("button")
                    .and_then(|v| v.as_str())
                    .unwrap_or("left")
                    .to_string(),
            },
            "browser_click" => Action::Click {
                button: "left".to_string(),
            },
            "browser_type" => Action::TypeText {
                text: args["text"].as_str().unwrap_or("").to_string(),
            },
            _ => return Err(format!("resolution: unsupported tool '{name}'")),
        };
        return Ok((action, target));
    }

    // No explicit target object — infer from legacy fields
    match name {
        "mouse_click" => {
            // If x/y present, that's coordinates; otherwise, check for text/role/name/selector
            if args.get("x").is_some() && args.get("y").is_some() {
                let x = args["x"].as_i64().unwrap_or(0) as i32;
                let y = args["y"].as_i64().unwrap_or(0) as i32;
                return Ok((
                    Action::Click {
                        button: args
                            .get("button")
                            .and_then(|v| v.as_str())
                            .unwrap_or("left")
                            .to_string(),
                    },
                    Target::ByCoordinates { x, y },
                ));
            }
            // Fall through to check for text/role/name
            if let Some(target) = infer_target_from_args(args) {
                return Ok((
                    Action::Click {
                        button: args
                            .get("button")
                            .and_then(|v| v.as_str())
                            .unwrap_or("left")
                            .to_string(),
                    },
                    target,
                ));
            }
            Err("mouse_click: provide (x,y), a target block, or text/role/name".into())
        }
        "mouse_double_click" => {
            if args.get("x").is_some() && args.get("y").is_some() {
                let x = args["x"].as_i64().unwrap_or(0) as i32;
                let y = args["y"].as_i64().unwrap_or(0) as i32;
                return Ok((Action::DoubleClick, Target::ByCoordinates { x, y }));
            }
            if let Some(target) = infer_target_from_args(args) {
                return Ok((Action::DoubleClick, target));
            }
            Err("mouse_double_click: provide (x,y), a target block, or text/role/name".into())
        }
        "keyboard_type" => {
            let text = args["text"].as_str().unwrap_or("").to_string();
            // If there's a target (e.g. input field), resolve it
            if let Some(target) = infer_target_from_args(args) {
                return Ok((Action::TypeText { text }, target));
            }
            // Otherwise, just type at current focus — no resolution needed
            Err("keyboard_type: text-only (no target)".into())
        }
        "click_on_text" => {
            let text = args["text"].as_str().unwrap_or("").to_string();
            return Ok((
                Action::Click {
                    button: args
                        .get("button")
                        .and_then(|v| v.as_str())
                        .unwrap_or("left")
                        .to_string(),
                },
                Target::ByText { text },
            ));
        }
        "browser_click" => {
            // Check for explicit fields
            if let Some(sel) = args.get("selector").and_then(|v| v.as_str()) {
                return Ok((
                    Action::Click {
                        button: "left".to_string(),
                    },
                    Target::BySelector {
                        selector: sel.to_string(),
                    },
                ));
            }
            if let Some(text) = args.get("text").and_then(|v| v.as_str()) {
                return Ok((
                    Action::Click {
                        button: "left".to_string(),
                    },
                    Target::ByText {
                        text: text.to_string(),
                    },
                ));
            }
            if args.get("x").is_some() && args.get("y").is_some() {
                let x = args["x"].as_i64().unwrap_or(0) as i32;
                let y = args["y"].as_i64().unwrap_or(0) as i32;
                return Ok((
                    Action::Click {
                        button: "left".to_string(),
                    },
                    Target::ByCoordinates { x, y },
                ));
            }
            if let Some(target) = infer_target_from_args(args) {
                return Ok((Action::Click { button: "left".to_string() }, target));
            }
            Err("browser_click: provide selector, text, coords, or target block".into())
        }
        "browser_type" => {
            let text = args["text"].as_str().unwrap_or("").to_string();
            if let Some(sel) = args.get("selector").and_then(|v| v.as_str()) {
                return Ok((
                    Action::TypeText { text },
                    Target::BySelector {
                        selector: sel.to_string(),
                    },
                ));
            }
            if let Some(target) = infer_target_from_args(args) {
                return Ok((Action::TypeText { text }, target));
            }
            Err("browser_type: provide selector or target block".into())
        }
        _ => Err(format!("resolution: unsupported tool '{name}'")),
    }
}

/// Parse a target object from JSON.
fn parse_target(obj: &serde_json::Value) -> Result<Target, String> {
    if let Some(role) = obj.get("role").and_then(|v| v.as_str()) {
        let name_contains = obj.get("name_contains").and_then(|v| v.as_str()).map(|s| s.to_string());
        return Ok(Target::ByRole {
            role: role.to_string(),
            name_contains,
        });
    }
    if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
        return Ok(Target::ByName {
            name: name.to_string(),
        });
    }
    if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
        return Ok(Target::ByText {
            text: text.to_string(),
        });
    }
    if let Some(selector) = obj.get("selector").and_then(|v| v.as_str()) {
        return Ok(Target::BySelector {
            selector: selector.to_string(),
        });
    }
    if let (Some(x), Some(y)) = (
        obj.get("x").and_then(|v| v.as_i64()),
        obj.get("y").and_then(|v| v.as_i64()),
    ) {
        return Ok(Target::ByCoordinates {
            x: x as i32,
            y: y as i32,
        });
    }
    Err("target must include one of: role, name, text, selector, x+y".into())
}

/// Infer a target from top-level args fields (role, name, text, selector).
fn infer_target_from_args(args: &serde_json::Value) -> Option<Target> {
    if let Some(role) = args.get("role").and_then(|v| v.as_str()) {
        let name_contains = args
            .get("name_contains")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        return Some(Target::ByRole {
            role: role.to_string(),
            name_contains,
        });
    }
    if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
        return Some(Target::ByName {
            name: name.to_string(),
        });
    }
    if let Some(text) = args.get("text").and_then(|v| v.as_str()) {
        return Some(Target::ByText {
            text: text.to_string(),
        });
    }
    if let Some(selector) = args.get("selector").and_then(|v| v.as_str()) {
        return Some(Target::BySelector {
            selector: selector.to_string(),
        });
    }
    None
}
