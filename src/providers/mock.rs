//! Mock provider — deterministic in-memory provider for testing.
//!
//! Every `ComputerProvider` method records an action and returns pre-configured
//! data. No OS calls are made. Use the builder pattern to set up test scenarios.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use chrono::Utc;
use serde::Serialize;
use serde_json::json;

use super::*;

// ── Action log entry ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct MockAction {
    pub timestamp: String,
    pub action: String,
    pub params: serde_json::Value,
}

// ── MockProvider ─────────────────────────────────────────────────────

pub struct MockProvider {
    pub screen: Mutex<Vec<u8>>,
    pub cursor: Mutex<(i32, i32)>,
    pub clipboard: Mutex<String>,
    pub windows: Mutex<Vec<WindowInfo>>,
    pub active_window: Mutex<Option<String>>,
    pub element_tree: Mutex<WindowState>,
    pub action_log: Mutex<Vec<MockAction>>,
    pub shell_responses: Mutex<HashMap<String, ShellResult>>,
    pub screen_size: ScreenSize,
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockProvider {
    // ── Constructor ───────────────────────────────────────────────

    pub fn new() -> Self {
        MockProvider {
            screen: Mutex::new(vec![]),
            cursor: Mutex::new((0, 0)),
            clipboard: Mutex::new(String::new()),
            windows: Mutex::new(vec![]),
            active_window: Mutex::new(None),
            element_tree: Mutex::new(WindowState {
                window: WindowInfo {
                    id: "mock-1".into(),
                    title: "Mock Window".into(),
                    app: "mock".into(),
                    pid: None,
                    geometry: WindowGeometry {
                        x: 0,
                        y: 0,
                        width: 1920,
                        height: 1080,
                    },
                },
                elements: vec![],
                element_count: 0,
            }),
            action_log: Mutex::new(vec![]),
            shell_responses: Mutex::new(HashMap::new()),
            screen_size: ScreenSize {
                width: 1920,
                height: 1080,
            },
        }
    }

    // ── Builder helpers ───────────────────────────────────────────

    /// Load a PNG screenshot to return from `screenshot()`.
    pub fn with_screenshot(mut self, png_path: &str) -> Self {
        let data = std::fs::read(png_path)
            .unwrap_or_else(|e| panic!("mock: failed to read screenshot {}: {e}", png_path));
        self.screen = Mutex::new(data);
        self
    }

    /// Set raw PNG bytes directly (no filesystem access in WASM / no_std-ish contexts).
    pub fn with_screenshot_bytes(mut self, bytes: Vec<u8>) -> Self {
        self.screen = Mutex::new(bytes);
        self
    }

    /// Add a window to the managed window list.
    pub fn with_window(self, title: &str, app: &str, geometry: WindowGeometry) -> Self {
        let id = format!("mock-win-{}", self.windows.lock().unwrap().len());
        let win = WindowInfo {
            id,
            title: title.to_string(),
            app: app.to_string(),
            pid: None,
            geometry,
        };
        self.windows.lock().unwrap().push(win);
        self
    }

    /// Replace the element tree returned by `get_window_state()`.
    pub fn with_element_tree(self, elements: Vec<UiElement>) -> Self {
        let mut tree = self.element_tree.lock().unwrap();
        tree.elements = elements;
        tree.element_count = tree.elements.len();
        drop(tree);
        self
    }

    /// Provide a pre-canned shell response for a command.
    pub fn with_shell_response(self, command: &str, result: ShellResult) -> Self {
        self.shell_responses
            .lock()
            .unwrap()
            .insert(command.to_string(), result);
        self
    }

    /// Set a specific screen size (default: 1920×1080).
    pub fn with_screen_size(mut self, width: u32, height: u32) -> Self {
        self.screen_size = ScreenSize { width, height };
        self
    }

    /// Set the clipboard content.
    pub fn with_clipboard(self, text: &str) -> Self {
        *self.clipboard.lock().unwrap() = text.to_string();
        self
    }

    /// Set initial cursor position.
    pub fn with_cursor(self, x: i32, y: i32) -> Self {
        *self.cursor.lock().unwrap() = (x, y);
        self
    }

    // ── Inspection helpers ────────────────────────────────────────

    /// Return the most recent action, if any.
    pub fn last_action(&self) -> Option<MockAction> {
        self.action_log.lock().unwrap().last().cloned()
    }

    /// Return all recorded actions.
    pub fn actions(&self) -> Vec<MockAction> {
        self.action_log.lock().unwrap().clone()
    }

    /// Current cursor position.
    pub fn cursor_position(&self) -> (i32, i32) {
        *self.cursor.lock().unwrap()
    }

    /// Current clipboard content.
    pub fn clipboard_content(&self) -> String {
        self.clipboard.lock().unwrap().clone()
    }

    /// Current active window ID.
    pub fn active_window_id(&self) -> Option<String> {
        self.active_window.lock().unwrap().clone()
    }

    /// Number of recorded actions.
    pub fn action_count(&self) -> usize {
        self.action_log.lock().unwrap().len()
    }

    // ── Internal helpers ──────────────────────────────────────────

    fn record(&self, action: &str, params: serde_json::Value) {
        let entry = MockAction {
            timestamp: Utc::now().to_rfc3339(),
            action: action.to_string(),
            params,
        };
        self.action_log.lock().unwrap().push(entry);
    }

    fn snapshot_windows(&self) -> Vec<WindowInfo> {
        self.windows.lock().unwrap().clone()
    }
}

// ── ComputerProvider impl ────────────────────────────────────────────

impl ComputerProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }

    // ── Screenshot ────────────────────────────────────────────────

    fn screenshot(&self, region: Option<(i32, i32, u32, u32)>) -> Result<Vec<u8>> {
        self.record("screenshot", json!({ "region": region }));

        let full = self.screen.lock().unwrap().clone();
        if full.is_empty() {
            // Return a 1×1 black PNG when no screenshot is loaded.
            // Minimal valid PNG: 8-byte signature + IHDR + IDAT + IEND.
            return Ok(MINIMAL_PNG.to_vec());
        }

        match region {
            None => Ok(full),
            Some((x, y, w, h)) => {
                // Simple crop: delegate to the image crate if available, otherwise
                // return the full image and let callers handle it.
                // We attempt to crop with the `image` crate.
                let img = image::load_from_memory(&full).unwrap_or_else(|_| {
                    image::DynamicImage::new_rgba8(self.screen_size.width, self.screen_size.height)
                });
                let cropped = img.crop_imm(
                    x.max(0) as u32,
                    y.max(0) as u32,
                    w.min(self.screen_size.width - x.max(0) as u32),
                    h.min(self.screen_size.height - y.max(0) as u32),
                );
                let mut buf = std::io::Cursor::new(Vec::new());
                cropped
                    .write_to(&mut buf, image::ImageFormat::Png)
                    .unwrap_or(());
                Ok(buf.into_inner())
            }
        }
    }

    fn get_screen_size(&self) -> Result<ScreenSize> {
        self.record("get_screen_size", json!({}));
        Ok(self.screen_size.clone())
    }

    // ── Mouse ────────────────────────────────────────────────────

    fn mouse_move(&self, x: i32, y: i32, smooth: bool, duration_ms: u64) -> Result<()> {
        self.record(
            "mouse_move",
            json!({ "x": x, "y": y, "smooth": smooth, "duration_ms": duration_ms }),
        );
        *self.cursor.lock().unwrap() = (x, y);
        Ok(())
    }

    fn mouse_click(&self, button: &str, x: Option<i32>, y: Option<i32>, clicks: u32) -> Result<()> {
        let pos = match (x, y) {
            (Some(cx), Some(cy)) => {
                *self.cursor.lock().unwrap() = (cx, cy);
                (cx, cy)
            }
            _ => *self.cursor.lock().unwrap(),
        };
        self.record(
            "mouse_click",
            json!({ "button": button, "x": pos.0, "y": pos.1, "clicks": clicks }),
        );
        Ok(())
    }

    fn mouse_scroll(&self, dx: i32, dy: i32, x: Option<i32>, y: Option<i32>) -> Result<()> {
        let pos = match (x, y) {
            (Some(cx), Some(cy)) => {
                *self.cursor.lock().unwrap() = (cx, cy);
                (cx, cy)
            }
            _ => *self.cursor.lock().unwrap(),
        };
        self.record(
            "mouse_scroll",
            json!({ "dx": dx, "dy": dy, "x": pos.0, "y": pos.1 }),
        );
        Ok(())
    }

    fn mouse_drag(
        &self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        button: &str,
        duration_ms: u64,
    ) -> Result<()> {
        self.record(
            "mouse_drag",
            json!({ "x1": x1, "y1": y1, "x2": x2, "y2": y2, "button": button, "duration_ms": duration_ms }),
        );
        // End position becomes current cursor.
        *self.cursor.lock().unwrap() = (x2, y2);
        Ok(())
    }

    // ── Keyboard ─────────────────────────────────────────────────

    fn keyboard_type(&self, text: &str, delay_ms: u64) -> Result<()> {
        self.record(
            "keyboard_type",
            json!({ "text": text, "delay_ms": delay_ms }),
        );
        Ok(())
    }

    fn key_press(&self, key: &str) -> Result<()> {
        self.record("key_press", json!({ "key": key }));
        Ok(())
    }

    // ── Clipboard ────────────────────────────────────────────────

    fn clipboard_get(&self) -> Result<String> {
        let content = self.clipboard.lock().unwrap().clone();
        self.record("clipboard_get", json!({ "result": content }));
        Ok(content)
    }

    fn clipboard_set(&self, text: &str) -> Result<()> {
        *self.clipboard.lock().unwrap() = text.to_string();
        self.record("clipboard_set", json!({ "text": text }));
        Ok(())
    }

    // ── Shell ────────────────────────────────────────────────────

    fn shell_run(&self, command: &str, _timeout_secs: u64) -> Result<ShellResult> {
        self.record("shell_run", json!({ "command": command }));

        // Check pre-canned responses.
        let responses = self.shell_responses.lock().unwrap();
        if let Some(result) = responses.get(command) {
            return Ok(result.clone());
        }
        drop(responses);

        // Default success.
        Ok(ShellResult {
            returncode: 0,
            stdout: format!("mock: ran '{}'", command),
            stderr: String::new(),
        })
    }

    // ── Windows ──────────────────────────────────────────────────

    fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        let wins = self.snapshot_windows();
        self.record("list_windows", json!({ "count": wins.len() }));
        Ok(wins)
    }

    fn focus_window(&self, title_match: &str) -> Result<WindowMatch> {
        self.record("focus_window", json!({ "title_match": title_match }));

        let wins = self.snapshot_windows();
        let candidates: Vec<String> = wins
            .iter()
            .filter(|w| w.title.to_lowercase().contains(&title_match.to_lowercase()))
            .map(|w| w.title.clone())
            .collect();

        if let Some(first) = wins
            .iter()
            .find(|w| w.title.to_lowercase().contains(&title_match.to_lowercase()))
        {
            *self.active_window.lock().unwrap() = Some(first.id.clone());
            Ok(WindowMatch {
                matched: true,
                id: Some(first.id.clone()),
                title: Some(first.title.clone()),
                app: Some(first.app.clone()),
                candidates: if candidates.len() > 1 {
                    Some(candidates)
                } else {
                    None
                },
            })
        } else {
            Ok(WindowMatch {
                matched: false,
                id: None,
                title: None,
                app: None,
                candidates: if candidates.is_empty() {
                    None
                } else {
                    Some(candidates)
                },
            })
        }
    }

    fn get_active_window(&self) -> Result<Option<WindowInfo>> {
        let active_id = self.active_window.lock().unwrap().clone();
        self.record("get_active_window", json!({ "active_id": active_id }));

        match active_id {
            Some(ref id) => {
                let wins = self.snapshot_windows();
                Ok(wins.into_iter().find(|w| &w.id == id))
            }
            None => Ok(None),
        }
    }

    // ── Apps / Notifications ─────────────────────────────────────

    fn open_app(&self, app_name: &str) -> Result<()> {
        self.record("open_app", json!({ "app_name": app_name }));
        Ok(())
    }

    fn notify(&self, title: &str, message: &str, urgency: &str) -> Result<()> {
        self.record(
            "notify",
            json!({ "title": title, "message": message, "urgency": urgency }),
        );
        Ok(())
    }

    // ── Accessibility / Element Trees ────────────────────────────

    fn get_window_state(&self) -> Result<WindowState> {
        let state = self.element_tree.lock().unwrap().clone();
        self.record(
            "get_window_state",
            json!({ "element_count": state.element_count }),
        );
        Ok(state)
    }
}

// ── Minimal valid 1×1 black PNG (for empty-screen fallback) ────────

const MINIMAL_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // signature
    0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR length
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1×1
    0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, // RGB, 8-bit
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, // IHDR CRC + IDAT length
    0x54, 0x08, 0xD7, 0x63, 0x60, 0x60, 0x60, 0x00, // IDAT (zlib-compressed
    0x00, 0x00, 0x04, 0x00, 0x01, 0xD6, 0x25, 0x02, //  black pixel)
    0x6E, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, // IDAT CRC + IEND
    0x44, 0xAE, 0x42, 0x60, 0x82, // IEND CRC
];

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_has_defaults() {
        let p = MockProvider::new();
        assert_eq!(p.name(), "mock");
        assert_eq!(p.screen_size.width, 1920);
        assert_eq!(p.screen_size.height, 1080);
        assert_eq!(p.cursor_position(), (0, 0));
        assert_eq!(p.clipboard_content(), "");
        assert_eq!(p.action_count(), 0);
    }

    #[test]
    fn mouse_click_updates_cursor() {
        let p = MockProvider::new();
        p.mouse_click("left", Some(100), Some(200), 1).unwrap();
        assert_eq!(p.cursor_position(), (100, 200));
        assert_eq!(p.action_count(), 1);

        let action = p.last_action().unwrap();
        assert_eq!(action.action, "mouse_click");
        assert_eq!(action.params["x"], 100);
        assert_eq!(action.params["y"], 200);
    }

    #[test]
    fn mouse_click_uses_current_cursor_when_no_coords() {
        let p = MockProvider::new();
        p.mouse_move(50, 60, false, 0).unwrap();
        p.mouse_click("right", None, None, 2).unwrap();
        assert_eq!(p.cursor_position(), (50, 60));
        let action = p.last_action().unwrap();
        assert_eq!(action.params["x"], 50);
        assert_eq!(action.params["y"], 60);
        assert_eq!(action.params["clicks"], 2);
    }

    #[test]
    fn clipboard_roundtrip() {
        let p = MockProvider::new();
        p.clipboard_set("hello mock").unwrap();
        assert_eq!(p.clipboard_content(), "hello mock");
        assert_eq!(p.clipboard_get().unwrap(), "hello mock");
    }

    #[test]
    fn shell_default_success() {
        let p = MockProvider::new();
        let res = p.shell_run("echo hi", 5).unwrap();
        assert_eq!(res.returncode, 0);
        assert!(res.stdout.contains("echo hi"));
        assert!(res.stderr.is_empty());
    }

    #[test]
    fn shell_precanned_response() {
        let p = MockProvider::new().with_shell_response(
            "ls /fake",
            ShellResult {
                returncode: 2,
                stdout: String::new(),
                stderr: "No such file".into(),
            },
        );
        let res = p.shell_run("ls /fake", 5).unwrap();
        assert_eq!(res.returncode, 2);
        assert_eq!(res.stderr, "No such file");
    }

    #[test]
    fn focus_window_finds_match() {
        let p = MockProvider::new()
            .with_window(
                "Calculator",
                "gnome-calculator",
                WindowGeometry {
                    x: 10,
                    y: 20,
                    width: 400,
                    height: 300,
                },
            )
            .with_window(
                "Terminal",
                "alacritty",
                WindowGeometry {
                    x: 0,
                    y: 0,
                    width: 800,
                    height: 600,
                },
            );

        let m = p.focus_window("calc").unwrap();
        assert!(m.matched);
        assert_eq!(m.title.as_deref(), Some("Calculator"));
        assert_eq!(p.active_window_id().unwrap(), "mock-win-0");
    }

    #[test]
    fn focus_window_no_match() {
        let p = MockProvider::new();
        let m = p.focus_window("nonexistent").unwrap();
        assert!(!m.matched);
        assert!(m.id.is_none());
    }

    #[test]
    fn get_active_window_returns_focused() {
        let p = MockProvider::new().with_window(
            "App",
            "test-app",
            WindowGeometry {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
        );
        p.focus_window("App").unwrap();
        let active = p.get_active_window().unwrap();
        assert!(active.is_some());
        assert_eq!(active.unwrap().title, "App");
    }

    #[test]
    fn keyboard_actions_recorded() {
        let p = MockProvider::new();
        p.keyboard_type("hello world", 50).unwrap();
        p.key_press("enter").unwrap();
        assert_eq!(p.action_count(), 2);

        let actions = p.actions();
        assert_eq!(actions[0].action, "keyboard_type");
        assert_eq!(actions[0].params["text"], "hello world");
        assert_eq!(actions[1].action, "key_press");
        assert_eq!(actions[1].params["key"], "enter");
    }

    #[test]
    fn screenshot_returns_loaded_bytes() {
        let p = MockProvider::new().with_screenshot_bytes(vec![1, 2, 3, 4]);
        let data = p.screenshot(None).unwrap();
        assert_eq!(data, vec![1, 2, 3, 4]);
    }

    #[test]
    fn screenshot_empty_fallback_to_minimal_png() {
        let p = MockProvider::new();
        let data = p.screenshot(None).unwrap();
        assert!(!data.is_empty());
        // Starts with PNG signature.
        assert_eq!(
            &data[0..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );
    }

    #[test]
    fn element_tree_roundtrip() {
        let p = MockProvider::new().with_element_tree(vec![UiElement {
            index: 0,
            role: "button".into(),
            name: "OK".into(),
            value: None,
            description: Some("Confirm".into()),
            actions: vec!["click".into()],
            bounds: Some(ElementBounds {
                x: 10,
                y: 20,
                width: 80,
                height: 30,
            }),
            enabled: true,
            focused: false,
            children: vec![],
        }]);

        let state = p.get_window_state().unwrap();
        assert_eq!(state.element_count, 1);
        assert_eq!(state.elements[0].role, "button");
        assert_eq!(state.elements[0].name, "OK");
    }

    #[test]
    fn mouse_drag_records_and_updates_cursor() {
        let p = MockProvider::new();
        p.mouse_drag(0, 0, 300, 400, "left", 200).unwrap();
        assert_eq!(p.cursor_position(), (300, 400));
        let action = p.last_action().unwrap();
        assert_eq!(action.action, "mouse_drag");
        assert_eq!(action.params["x1"], 0);
        assert_eq!(action.params["y2"], 400);
    }

    #[test]
    fn builder_chaining() {
        let p = MockProvider::new()
            .with_screen_size(2560, 1440)
            .with_cursor(42, 99)
            .with_clipboard("preloaded");

        assert_eq!(p.screen_size.width, 2560);
        assert_eq!(p.screen_size.height, 1440);
        assert_eq!(p.cursor_position(), (42, 99));
        assert_eq!(p.clipboard_content(), "preloaded");
    }
}
