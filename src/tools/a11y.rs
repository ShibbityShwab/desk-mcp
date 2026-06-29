//! AT-SPI accessibility tool handlers — 4 targeted tools.
//!
//! Replaces the single `get_window_state` heavy tree-dump with:
//!   find_elements  — search by role/name (lightweight, <10ms with native backend)
//!   get_element_text — text of specific element by path
//!   click_element — activate element via accessibility API
//!   get_window_tree — full tree (heavy, opt-in only, ~2K tokens)

use crate::response::{self, ToolResponse};
use crate::PROVIDER;
use serde_json::Value;

pub async fn handle(name: &str, args: Value) -> ToolResponse {
    let result = handle_inner(name, args).await;
    match result {
        Ok(value) => response::ok(value),
        Err(message) => response::err("A11Y_ERROR", &message),
    }
}

async fn handle_inner(name: &str, args: Value) -> Result<Value, String> {
    match name {
        "find_elements" => find_elements(args).await,
        "get_element_text" => get_element_text(args).await,
        "click_element" => click_element(args).await,
        "get_window_tree" => get_window_tree(args).await,
        _ => Err(format!("unknown a11y tool: {name}")),
    }
}

/// Search the accessibility tree for elements matching role and/or name.
async fn find_elements(args: Value) -> Result<Value, String> {
    let role_filter = args
        .get("role")
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase());
    let name_contains = args
        .get("name_contains")
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase());
    let max_results = args
        .get("max_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as usize;

    // Get the active window state from the provider
    let state = PROVIDER
        .get_window_state()
        .map_err(|e| format!("failed to get window state: {e}"))?;

    // Detect Qt without accessibility
    let qt_warning = if (state.window.app.is_empty() || state.window.app.contains("Qt"))
        && state.element_count == 0
    {
        Some("Qt app detected with no accessible elements. Set QT_ACCESSIBILITY=1 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 before launching for full element tree.")
    } else {
        None
    };

    let mut matches: Vec<Value> = Vec::new();
    for el in &state.elements {
        if matches.len() >= max_results {
            break;
        }

        let role_match = role_filter
            .as_ref()
            .is_none_or(|f| el.role.to_lowercase().contains(f));
        let name_match = name_contains
            .as_ref()
            .is_none_or(|f| el.name.to_lowercase().contains(f));

        if role_match && name_match {
            matches.push(serde_json::json!({
                "index": el.index,
                "role": el.role,
                "name": el.name,
                "text": el.value,
                "description": el.description,
                "bounds": el.bounds,
                "enabled": el.enabled,
                "focused": el.focused,
                "actions": el.actions,
            }));
        }
    }

    let mut result = serde_json::json!({
        "elements": matches,
        "count": matches.len(),
        "total": state.element_count,
        "window": {
            "title": state.window.title,
            "app": state.window.app,
        },
    });

    if let Some(ref warn) = qt_warning {
        result.as_object_mut().unwrap().insert(
            "warning".into(),
            serde_json::Value::String(warn.to_string()),
        );
    }

    Ok(result)
}

/// Get text and metadata for a specific accessibility element by its index/path.
async fn get_element_text(args: Value) -> Result<Value, String> {
    let index = args
        .get("path")
        .and_then(|v| v.as_u64())
        .or_else(|| args.get("index").and_then(|v| v.as_u64()))
        .ok_or("'path' (element index) is required")? as u32;

    let state = PROVIDER
        .get_window_state()
        .map_err(|e| format!("failed to get window state: {e}"))?;

    let el = state
        .elements
        .iter()
        .find(|e| e.index == index)
        .ok_or_else(|| format!("no element with index {index}"))?;

    Ok(serde_json::json!({
        "index": el.index,
        "role": el.role,
        "name": el.name,
        "text": el.value,
        "description": el.description,
        "children_count": el.children.len(),
        "children": el.children,
    }))
}

/// Activate (click/press) an accessibility element by its index/path.
async fn click_element(args: Value) -> Result<Value, String> {
    let index = args
        .get("path")
        .and_then(|v| v.as_u64())
        .or_else(|| args.get("index").and_then(|v| v.as_u64()))
        .ok_or("'path' (element index) is required")? as u32;

    let state = PROVIDER
        .get_window_state()
        .map_err(|e| format!("failed to get window state: {e}"))?;

    let el = state
        .elements
        .iter()
        .find(|e| e.index == index)
        .ok_or_else(|| format!("no element with index {index}"))?;

    // Try to click via the provider's accessibility support
    // For now, use the bounds to do a mouse click if coordinates are available
    if let Some(ref bounds) = el.bounds {
        PROVIDER
            .mouse_click(
                "left",
                Some(bounds.x + bounds.width / 2),
                Some(bounds.y + bounds.height / 2),
                1,
            )
            .map_err(|e| format!("click failed: {e}"))?;

        return Ok(serde_json::json!({
            "clicked": true,
            "index": el.index,
            "role": el.role,
            "name": el.name,
            "position": {"x": bounds.x + bounds.width / 2, "y": bounds.y + bounds.height / 2},
        }));
    }

    Err(format!(
        "element {} ({}) has no bounds — cannot click by coordinates",
        el.index, el.role
    ))
}

/// Get the full accessibility tree for the active window.
/// **Heavy** — opt-in only. Use `max_depth` to limit the response size.
async fn get_window_tree(args: Value) -> Result<Value, String> {
    let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(3) as usize;

    let state = PROVIDER
        .get_window_state()
        .map_err(|e| format!("failed to get window state: {e}"))?;

    // Build a tree structure from the flat element list
    let mut indexed: std::collections::HashMap<u32, &crate::providers::UiElement> =
        std::collections::HashMap::new();
    for el in &state.elements {
        indexed.insert(el.index, el);
    }

    fn build_tree(
        idx: u32,
        indexed: &std::collections::HashMap<u32, &crate::providers::UiElement>,
        depth: usize,
        max_depth: usize,
    ) -> Option<Value> {
        if depth > max_depth {
            return None;
        }
        let el = indexed.get(&idx)?;
        let children: Vec<Value> = el
            .children
            .iter()
            .filter_map(|&c| build_tree(c, indexed, depth + 1, max_depth))
            .collect();

        Some(serde_json::json!({
            "index": el.index,
            "role": el.role,
            "name": el.name,
            "text": el.value,
            "children": children,
        }))
    }

    // Find root elements (those not appeared as children of others)
    let all_children: std::collections::HashSet<u32> = state
        .elements
        .iter()
        .flat_map(|e| e.children.iter().copied())
        .collect();
    let roots: Vec<Value> = state
        .elements
        .iter()
        .filter(|e| !all_children.contains(&e.index))
        .filter_map(|e| build_tree(e.index, &indexed, 0, max_depth))
        .collect();

    let mut result = serde_json::json!({
        "window": {
            "title": state.window.title,
            "app": state.window.app,
        },
        "element_count": state.element_count,
        "max_depth": max_depth,
        "roots": roots,
    });

    // Qt accessibility detection
    if state.element_count == 0 {
        result.as_object_mut().unwrap().insert("warning".into(), serde_json::Value::String(
            "No accessible elements found. If this is a Qt app, set QT_ACCESSIBILITY=1 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 before launching.".to_string()
        ));
    }

    Ok(result)
}
