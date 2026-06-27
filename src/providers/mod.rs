//! Computer use provider trait and factory.

pub mod headless;
pub mod kde_wayland;

use anyhow::Result;

/// Abstract interface for platform-specific computer control.
///
/// Each provider (KDE Wayland, X11, headless) implements this trait
/// using the appropriate CLI tools for the environment.
pub trait ComputerProvider: Send + Sync {
    /// Provider name for logging/discovery
    fn name(&self) -> &str;

    // ── Screenshot ───────────────────────────────────────
    fn screenshot(&self, region: Option<(i32, i32, u32, u32)>) -> Result<Vec<u8>>;
    fn get_screen_size(&self) -> Result<ScreenSize>;

    // ── Mouse ────────────────────────────────────────────
    fn mouse_move(&self, x: i32, y: i32, smooth: bool, duration_ms: u64) -> Result<()>;
    fn mouse_click(&self, button: &str, x: Option<i32>, y: Option<i32>, clicks: u32) -> Result<()>;
    fn mouse_scroll(&self, dx: i32, dy: i32, x: Option<i32>, y: Option<i32>) -> Result<()>;
    fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, button: &str, duration_ms: u64) -> Result<()>;

    // ── Keyboard ─────────────────────────────────────────
    fn keyboard_type(&self, text: &str, delay_ms: u64) -> Result<()>;
    fn key_press(&self, key: &str) -> Result<()>;

    // ── Clipboard ────────────────────────────────────────
    fn clipboard_get(&self) -> Result<String>;
    fn clipboard_set(&self, text: &str) -> Result<()>;

    // ── Shell ────────────────────────────────────────────
    fn shell_run(&self, command: &str, timeout_secs: u64) -> Result<ShellResult>;

    // ── Windows ──────────────────────────────────────────
    fn list_windows(&self) -> Result<Vec<WindowInfo>>;
    fn focus_window(&self, title_match: &str) -> Result<WindowMatch>;
    fn get_active_window(&self) -> Result<Option<WindowInfo>>;

    // ── Apps / Notifications ─────────────────────────────
    fn open_app(&self, app_name: &str) -> Result<()>;
    fn notify(&self, title: &str, message: &str, urgency: &str) -> Result<()>;
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScreenSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ShellResult {
    pub returncode: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WindowInfo {
    pub id: String,
    pub title: String,
    pub app: String,
    pub pid: Option<u32>,
    pub geometry: WindowGeometry,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WindowMatch {
    pub matched: bool,
    pub id: Option<String>,
    pub title: Option<String>,
    pub app: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<String>>,
}

/// Select the best provider for the current environment
pub fn get_provider() -> Box<dyn ComputerProvider + Send + Sync> {
    let caps = crate::discovery::detect();
    match caps.provider.as_str() {
        "wayland_kde" => Box::new(kde_wayland::KdeWaylandProvider::default()),
        "wayland_wlr" => Box::new(kde_wayland::KdeWaylandProvider::default()),
        "x11" => Box::new(kde_wayland::KdeWaylandProvider::default()),
        _ => Box::new(headless::HeadlessProvider::default()),
    }
}
