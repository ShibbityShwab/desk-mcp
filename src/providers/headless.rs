//! Headless provider — graceful degradation when no display server is available.
//!
//! Display-dependent tools return errors. Clipboard (arboard), notifications
//! (notify-rust), and shell may still work if their backends are available.

use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use super::*;

#[derive(Default)]
pub struct HeadlessProvider;

impl ComputerProvider for HeadlessProvider {
    fn name(&self) -> &str {
        "headless"
    }

    fn screenshot(&self, _region: Option<(i32, i32, u32, u32)>) -> Result<Vec<u8>> {
        bail!("screenshot not available in headless mode (no display server)")
    }

    fn get_screen_size(&self) -> Result<ScreenSize> {
        Ok(ScreenSize {
            width: 1920,
            height: 1080,
        }) // default
    }

    fn mouse_move(&self, _x: i32, _y: i32, _smooth: bool, _duration_ms: u64) -> Result<()> {
        bail!("mouse not available in headless mode")
    }

    fn mouse_click(
        &self,
        _button: &str,
        _x: Option<i32>,
        _y: Option<i32>,
        _clicks: u32,
    ) -> Result<()> {
        bail!("mouse not available in headless mode")
    }

    fn mouse_scroll(&self, _dx: i32, _dy: i32, _x: Option<i32>, _y: Option<i32>) -> Result<()> {
        bail!("mouse not available in headless mode")
    }

    fn mouse_drag(
        &self,
        _x1: i32,
        _y1: i32,
        _x2: i32,
        _y2: i32,
        _button: &str,
        _duration_ms: u64,
    ) -> Result<()> {
        bail!("mouse not available in headless mode")
    }

    fn keyboard_type(&self, _text: &str, _delay_ms: u64) -> Result<()> {
        bail!("keyboard not available in headless mode")
    }

    fn key_press(&self, _combo: &str) -> Result<()> {
        bail!("keyboard not available in headless mode")
    }

    fn clipboard_get(&self) -> Result<String> {
        let mut clipboard =
            arboard::Clipboard::new().context("arboard: clipboard not available")?;
        clipboard
            .get_text()
            .context("clipboard empty or contains non-text")
    }

    fn clipboard_set(&self, text: &str) -> Result<()> {
        let mut clipboard =
            arboard::Clipboard::new().context("arboard: clipboard not available")?;
        clipboard
            .set_text(text)
            .context("arboard: failed to set clipboard")
    }

    fn shell_run(&self, command: &str, _timeout_secs: u64) -> Result<ShellResult> {
        let output = Command::new("sh").args(["-c", command]).output()?;

        let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stdout.len() > 8000 {
            stdout.truncate(8000);
        }
        if stderr.len() > 4000 {
            stderr.truncate(4000);
        }

        Ok(ShellResult {
            returncode: output.status.code().unwrap_or(-1),
            stdout,
            stderr,
        })
    }

    fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        bail!("window management not available in headless mode")
    }

    fn focus_window(&self, _title: &str) -> Result<WindowMatch> {
        bail!("window management not available in headless mode")
    }

    fn get_active_window(&self) -> Result<Option<WindowInfo>> {
        bail!("window management not available in headless mode")
    }

    fn open_app(&self, app_name: &str) -> Result<()> {
        Command::new(app_name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    }

    fn notify(&self, title: &str, message: &str, urgency: &str) -> Result<()> {
        let urgency_level = match urgency {
            "critical" => notify_rust::Urgency::Critical,
            "low" => notify_rust::Urgency::Low,
            _ => notify_rust::Urgency::Normal,
        };
        notify_rust::Notification::new()
            .summary(title)
            .body(message)
            .urgency(urgency_level)
            .show()
            .context("notify-rust: failed to show notification")?;
        Ok(())
    }

    fn get_window_state(&self) -> Result<WindowState> {
        bail!("get_window_state not supported in headless mode (no AT-SPI bus)")
    }
}
