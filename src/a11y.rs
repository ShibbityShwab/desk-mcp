//! AT-SPI Accessibility Tree Integration
//!
//! Uses Python's pyatspi2 library to query the Linux accessibility bus.
//! This is the approach Cua uses for Linux — pyatspi handles bus discovery,
//! app registration, and tree walking correctly.
//!
//! Falls back gracefully when AT-SPI is unavailable or the app doesn't
//! expose accessibility (common with Electron apps).

use std::process::Command;

use anyhow::{bail, Context, Result};
use serde_json;

use crate::providers::{UiElement, WindowInfo, WindowState};

/// Enable AT-SPI screen reader mode so all apps expose their accessibility trees.
pub(crate) fn enable_accessibility_for_atspi() -> Result<()> {
    let output = Command::new("dbus-send")
        .args([
            "--session",
            "--dest=org.a11y.Bus",
            "--print-reply",
            "/org/a11y/bus",
            "org.freedesktop.DBus.Properties.Set",
            "string:org.a11y.Status",
            "string:ScreenReaderEnabled",
            "variant:boolean:true",
        ])
        .output()
        .context("AT-SPI: dbus-send for ScreenReaderEnabled failed")?;

    if !output.status.success() {
        eprintln!("AT-SPI: ScreenReaderEnabled set returned non-zero (may not be supported)");
    }
    Ok(())
}

/// Walk the AT-SPI tree for the active window and collect all meaningful UI elements.
/// Uses Python/pyatspi via subprocess for reliable bus discovery.
pub fn get_window_state(active: &WindowInfo) -> Result<WindowState> {
    let _ = enable_accessibility_for_atspi();

    let pid = active
        .pid
        .context("No PID for active window; AT-SPI requires a PID to find the accessible app")?;

    let app_name = &active.app;

    // Use Python with pyatspi to walk the tree
    let script = format!(
        r#"
import gi, json
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi

pid = {pid}
app_name = "{app_name_escaped}"

# Try to find the accessible for this PID
# If not found, try matching by name

found_elements = []
Atspi.init()
desktop = Atspi.get_desktop(0)

def simplify_role(role_name):
    m = {{
        'push button': 'button', 'toggle button': 'button', 'menu button': 'button',
        'text': 'text', 'entry': 'text', 'password text': 'text',
        'label': 'label', 'static': 'label',
        'check box': 'checkbox',
        'radio button': 'radio',
        'combo box': 'dropdown',
        'menu': 'menu', 'menu bar': 'menu', 'popup menu': 'menu',
        'menu item': 'menuitem', 'check menu item': 'menuitem', 'radio menu item': 'menuitem',
        'slider': 'slider', 'scroll bar': 'scrollbar',
        'page tab': 'tab', 'page tab list': 'tablist',
        'list': 'list', 'list box': 'list', 'list item': 'listitem',
        'tree': 'tree', 'tree table': 'tree', 'tree item': 'treeitem',
        'link': 'link', 'separator': 'separator',
        'status bar': 'statusbar', 'progress bar': 'progressbar',
        'dialog': 'dialog', 'window': 'window', 'frame': 'window',
        'application': 'application', 'table': 'table',
        'desktop frame': 'desktop', 'panel': 'container',
    }}
    return m.get(role_name.lower().strip(), role_name.lower().strip())

def is_collectable(role):
    return role in ['button', 'text', 'label', 'checkbox', 'radio', 'dropdown',
                    'menuitem', 'slider', 'tab', 'listitem', 'treeitem', 'link',
                    'window', 'dialog', 'entry', 'password_text', 'combo box',
                    'scrollbar', 'application']

def get_actions(obj):
    try:
        n = obj.get_n_actions()
        return [obj.get_action_name(i) for i in range(min(n, 10))]
    except:
        return []

def get_value(obj):
    try:
        if hasattr(obj, 'query_text'):
            text = obj.query_text()
            if text:
                n = text.character_count
                if 0 < n <= 2000:
                    return text.get_text(0, n)
    except:
        pass
    return None

def walk(obj, elements, depth, max_depth=50, max_elements=500):
    if depth > max_depth or len(elements) >= max_elements:
        return
    try:
        name = obj.get_name() or ''
        role_raw = obj.get_role_name() or ''
        role = simplify_role(role_raw)
    except:
        return

    if is_collectable(role):
        try:
            desc = obj.get_description() or ''
            actions = get_actions(obj)
            value = get_value(obj) if role == 'text' else None
            enabled = obj.get_state().contains(0)  # ATSPI_STATE_ENABLED
            focused = obj.get_state().contains(3)  # ATSPI_STATE_FOCUSED

            # Bounds
            bounds = None
            try:
                ext = obj.get_extents(0)  # 0 = screen coords
                if ext.width > 0 and ext.height > 0:
                    bounds = {{'x': ext.x, 'y': ext.y, 'width': ext.width, 'height': ext.height}}
            except:
                pass

            idx = len(elements)
            children = []
            child_start = len(elements)
            try:
                n_children = obj.get_child_count()
                for i in range(min(n_children, 50)):
                    try:
                        child = obj.get_child_at_index(i)
                        walk(child, elements, depth + 1, max_depth, max_elements)
                    except:
                        pass
            except:
                pass
            child_end = len(elements)
            children = list(range(child_start, child_end))

            elements.append({{
                'index': idx, 'role': role, 'name': name,
                'value': value, 'description': desc if desc else None,
                'actions': actions, 'bounds': bounds,
                'enabled': enabled, 'focused': focused, 'children': children
            }})
        except Exception as e:
            pass
    else:
        # Walk children of non-collectable containers
        try:
            n_children = obj.get_child_count()
            for i in range(min(n_children, 50)):
                try:
                    child = obj.get_child_at_index(i)
                    walk(child, elements, depth + 1, max_depth, max_elements)
                except:
                    pass
        except:
            pass

# Find the right application
target_app = None
for i in range(desktop.get_child_count()):
    try:
        app = desktop.get_child_at_index(i)
        app_pid = app.get_process_id()
        if app_pid == pid or (app_name and app_name.lower() in app.get_name().lower()):
            target_app = app
            break
    except:
        continue

if target_app is None:
    # Last resort: try matching by name substring
    for i in range(desktop.get_child_count()):
        try:
            app = desktop.get_child_at_index(i)
            if app_name and app_name.lower() in app.get_name().lower():
                target_app = app
                break
        except:
            continue

if target_app is None:
    print(json.dumps({{"error": "no_accessible_app", "elements": [], "element_count": 0}}))
else:
    elements = []
    try:
        for i in range(target_app.get_child_count()):
            try:
                child = target_app.get_child_at_index(i)
                walk(child, elements, 0)
            except:
                pass
    except:
        pass
    print(json.dumps({{"elements": elements, "element_count": len(elements)}}))
"#,
        pid = pid,
        app_name_escaped = app_name.replace('\'', "'\\''"),
    );

    let output = Command::new("python3")
        .args(["-c", &script])
        .output()
        .context(
            "AT-SPI: python3 with pyatspi subprocess failed — is python3 + at-spi2 installed?",
        )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("AT-SPI: python3 error: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: serde_json::Value =
        serde_json::from_str(&stdout).context("AT-SPI: failed to parse Python output as JSON")?;

    if data.get("error").is_some() {
        // App not found in AT-SPI tree — return empty state gracefully
        return Ok(WindowState {
            window: active.clone(),
            elements: vec![],
            element_count: 0,
        });
    }

    let elements: Vec<UiElement> = serde_json::from_value(data["elements"].clone())
        .context("AT-SPI: failed to parse elements from Python output")?;

    let count = data["element_count"].as_u64().unwrap_or(0) as usize;

    Ok(WindowState {
        window: active.clone(),
        elements,
        element_count: count,
    })
}
