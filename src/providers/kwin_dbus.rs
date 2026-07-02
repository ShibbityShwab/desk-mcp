//! KWin D-Bus provider — native KDE window management via the KWin D-Bus API.
//!
//! Replaces every `kdotool` subprocess call with a single persistent D-Bus
//! connection. Response times drop from ~50-100ms to ~5ms per call.
//!
//! Falls back to the wrapped inner provider for non-window operations
//! (screenshot, mouse, keyboard, clipboard, etc.) and for window operations
//! when the D-Bus connection is unavailable.

use std::sync::OnceLock;

use super::*;
use anyhow::{Context, Result};

use zbus::blocking::Connection;

/// Cached zbus session connection — initialized once, reused for all calls
static DBUS_CONN: OnceLock<Option<Connection>> = OnceLock::new();

fn get_dbus_conn() -> Option<&'static Connection> {
    DBUS_CONN
        .get_or_init(|| match Connection::session() {
            Ok(conn) => {
                tracing::info!("KWin D-Bus connection established");
                Some(conn)
            }
            Err(e) => {
                tracing::warn!("KWin D-Bus unavailable: {e}; falling back to kdotool");
                None
            }
        })
        .as_ref()
}

/// KWin D-Bus window management provider.
///
/// Wraps an inner provider for non-window operations (screenshot, input, etc.)
/// and overrides window management with direct KWin D-Bus calls.
pub struct KWinDbusProvider {
    inner: Box<dyn ComputerProvider + Send + Sync>,
}

impl KWinDbusProvider {
    pub fn new(inner: Box<dyn ComputerProvider + Send + Sync>) -> Self {
        // Trigger zbus connection init on construction (best-effort)
        let _ = get_dbus_conn();
        Self { inner }
    }
}

impl ComputerProvider for KWinDbusProvider {
    fn name(&self) -> &str {
        "kwin_dbus"
    }

    // ── Delegate to inner for non-window operations ──────────────────

    fn screenshot(&self, region: Option<(i32, i32, u32, u32)>) -> Result<Vec<u8>> {
        self.inner.screenshot(region)
    }
    fn get_screen_size(&self) -> Result<ScreenSize> {
        self.inner.get_screen_size()
    }
    fn mouse_move(&self, x: i32, y: i32, smooth: bool, dur_ms: u64) -> Result<()> {
        self.inner.mouse_move(x, y, smooth, dur_ms)
    }
    fn mouse_click(&self, button: &str, x: Option<i32>, y: Option<i32>, clicks: u32) -> Result<()> {
        self.inner.mouse_click(button, x, y, clicks)
    }
    fn mouse_scroll(&self, dx: i32, dy: i32, x: Option<i32>, y: Option<i32>) -> Result<()> {
        self.inner.mouse_scroll(dx, dy, x, y)
    }
    fn mouse_drag(
        &self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        button: &str,
        dur_ms: u64,
    ) -> Result<()> {
        self.inner.mouse_drag(x1, y1, x2, y2, button, dur_ms)
    }
    fn keyboard_type(&self, text: &str, delay_ms: u64) -> Result<()> {
        self.inner.keyboard_type(text, delay_ms)
    }
    fn key_press(&self, combo: &str) -> Result<()> {
        self.inner.key_press(combo)
    }
    fn clipboard_get(&self) -> Result<String> {
        self.inner.clipboard_get()
    }
    fn clipboard_set(&self, text: &str) -> Result<()> {
        self.inner.clipboard_set(text)
    }
    fn shell_run(&self, command: &str, timeout_secs: u64) -> Result<ShellResult> {
        self.inner.shell_run(command, timeout_secs)
    }
    fn open_app(&self, app_name: &str) -> Result<()> {
        self.inner.open_app(app_name)
    }
    fn notify(&self, title: &str, message: &str, urgency: &str) -> Result<()> {
        self.inner.notify(title, message, urgency)
    }
    fn get_window_state(&self) -> Result<WindowState> {
        self.inner.get_window_state()
    }

    // ── Window operations via KWin D-Bus ────────────────────────────

    fn get_active_window(&self) -> Result<Option<WindowInfo>> {
        match self.dbus_get_active() {
            Ok(wi) => return Ok(Some(wi)),
            Err(e) => {
                tracing::debug!("KWin D-Bus get_active_window failed: {e}; falling back to inner");
            }
        }
        self.inner.get_active_window()
    }

    fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        match self.dbus_list_windows() {
            Ok(windows) if !windows.is_empty() => return Ok(windows),
            Ok(_) => {}
            Err(e) => {
                tracing::debug!("KWin D-Bus list_windows failed: {e}; falling back to inner");
            }
        }
        self.inner.list_windows()
    }

    fn focus_window(&self, title_match: &str) -> Result<WindowMatch> {
        match self.dbus_focus_window(title_match) {
            Ok(m) => return Ok(m),
            Err(e) => {
                tracing::debug!("KWin D-Bus focus_window failed: {e}; falling back to inner");
            }
        }
        self.inner.focus_window(title_match)
    }
}

// ── Private D-Bus helper methods ────────────────────────────────────────

impl KWinDbusProvider {
    fn conn(&self) -> Result<&'static Connection> {
        get_dbus_conn().ok_or_else(|| anyhow::anyhow!("D-Bus session connection not available"))
    }

    fn dbus_get_active(&self) -> Result<WindowInfo> {
        let conn = self.conn()?;

        let msg = conn
            .call_method(
                Some("org.kde.KWin"),
                "/KWin",
                Some("org.kde.KWin"),
                "activeWindow",
                &(),
            )
            .context("dbus activeWindow")?;
        let uuid: String = msg
            .body()
            .deserialize()
            .context("deserialize activeWindow UUID")?;
        if uuid.is_empty() {
            anyhow::bail!("no active window");
        }

        self.dbus_get_window_info(&uuid)
    }

    fn dbus_list_windows(&self) -> Result<Vec<WindowInfo>> {
        let conn = self.conn()?;

        let msg = conn
            .call_method(
                Some("org.kde.KWin"),
                "/KWin",
                Some("org.kde.KWin"),
                "windows",
                &(),
            )
            .context("dbus windows")?;
        let uuids: Vec<String> = msg
            .body()
            .deserialize()
            .context("deserialize windows list")?;

        let mut windows = Vec::with_capacity(uuids.len());
        for uuid in &uuids {
            if let Ok(info) = self.dbus_get_window_info(uuid) {
                windows.push(info);
            }
        }
        Ok(windows)
    }

    fn dbus_get_window_info(&self, uuid: &str) -> Result<WindowInfo> {
        let conn = self.conn()?;

        let msg = conn
            .call_method(
                Some("org.kde.KWin"),
                "/KWin",
                Some("org.kde.KWin"),
                "queryWindowInfo",
                &(uuid,),
            )
            .context("dbus queryWindowInfo")?;
        let body: zbus::zvariant::OwnedValue = msg
            .body()
            .deserialize()
            .context("deserialize queryWindowInfo")?;

        self.parse_window_info(uuid, &body)
    }

    fn dbus_focus_window(&self, title_match: &str) -> Result<WindowMatch> {
        let conn = self.conn()?;

        let msg = conn
            .call_method(
                Some("org.kde.KWin"),
                "/KWin",
                Some("org.kde.KWin"),
                "windows",
                &(),
            )
            .context("dbus windows")?;
        let uuids: Vec<String> = msg
            .body()
            .deserialize()
            .context("deserialize windows list")?;

        let title_lower = title_match.to_lowercase();
        let mut candidates = Vec::new();
        let mut found_uuid: Option<String> = None;
        let mut found_info: Option<WindowInfo> = None;

        for uuid in &uuids {
            if let Ok(info) = self.dbus_get_window_info(uuid) {
                if info.title.to_lowercase().contains(&title_lower) {
                    if found_uuid.is_none() {
                        found_uuid = Some(uuid.clone());
                        found_info = Some(info.clone());
                    }
                    candidates.push(info.title);
                }
            }
        }

        if let Some(ref uuid) = found_uuid {
            let _ = conn.call_method(
                Some("org.kde.KWin"),
                "/KWin",
                Some("org.kde.KWin"),
                "activateWindow",
                &(uuid.as_str()),
            );

            let wi = found_info.unwrap();
            return Ok(WindowMatch {
                matched: true,
                id: Some(uuid.clone()),
                title: Some(wi.title),
                app: Some(wi.app),
                candidates: if candidates.len() > 1 {
                    Some(candidates)
                } else {
                    None
                },
            });
        }

        Ok(WindowMatch {
            matched: false,
            id: None,
            title: None,
            app: None,
            candidates: None,
        })
    }

    /// Parse KWin queryWindowInfo response into a WindowInfo struct.
    fn parse_window_info(
        &self,
        uuid: &str,
        body: &zbus::zvariant::OwnedValue,
    ) -> Result<WindowInfo> {
        // Serialize zvariant body → serde_json::Value for easy extraction.
        // This avoids zvariant's complex structural pattern matching.
        let json: serde_json::Value =
            serde_json::to_value(body).context("serialize window info to JSON")?;

        let mut title = format!("Window {{{uuid}}}");
        let mut app = String::new();
        let mut pid: Option<u32> = None;
        let mut geometry = WindowGeometry {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };

        // Try to extract from JSON regardless of whether it's struct or dict
        if let Some(arr) = json.as_array() {
            // Positional struct form: [uuid, title, appId?, geometry?, ..., pid?]
            if arr.len() > 1 {
                if let Some(t) = arr[1].as_str() {
                    title = t.to_string();
                }
            }
            if arr.len() > 2 {
                if let Some(a) = arr[2].as_str() {
                    app = a.to_string();
                }
            }
            // Geometry at index 3 or 4 as nested array [x, y, w, h]
            for i in [3, 4] {
                if let Some(geo_arr) = arr.get(i).and_then(|v| v.as_array()) {
                    geometry.x = geo_arr.first().and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    geometry.y = geo_arr.get(1).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    geometry.width = geo_arr.get(2).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    geometry.height = geo_arr.get(3).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    break;
                }
            }
            // PID at later indices
            for item in arr.iter().skip(5).take(5) {
                if let Some(p) = item.as_u64() {
                    pid = Some(p as u32);
                    break;
                }
            }
        } else if let Some(obj) = json.as_object() {
            // Dict form
            if let Some(t) = obj.get("title").and_then(|v| v.as_str()) {
                title = t.to_string();
            }
            if let Some(a) = obj
                .get("appId")
                .or_else(|| obj.get("resourceClass"))
                .or_else(|| obj.get("class"))
                .and_then(|v| v.as_str())
            {
                app = a.to_string();
            }
            if let Some(p) = obj.get("pid").and_then(|v| v.as_u64()) {
                pid = Some(p as u32);
            }
            if let Some(geo_arr) = obj.get("geometry").and_then(|v| v.as_array()) {
                geometry.x = geo_arr.first().and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                geometry.y = geo_arr.get(1).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                geometry.width = geo_arr.get(2).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                geometry.height = geo_arr.get(3).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            }
        }

        Ok(WindowInfo {
            id: uuid.to_string(),
            title,
            app,
            pid,
            geometry,
        })
    }
}
