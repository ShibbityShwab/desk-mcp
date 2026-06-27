//! Headless provider — graceful degradation when no display server is available.
//!
//! All display-dependent tools return errors. Shell, clipboard (via xclip),
//! and notifications may still work if their dependencies are available.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Result, bail};

use super::*;

#[derive(Default)]
pub struct HeadlessProvider;

impl ComputerProvider for HeadlessProvider {
    fn name(&self) -> &str { "headless" }

    fn screenshot(&self, _region: Option<(i32, i32, u32, u32)>) -> Result<Vec<u8>> {
        // Try Xvfb via import if DISPLAY is set
        if let Ok(display) = std::env::var("DISPLAY") {
            if !display.is_empty() && which::which("import").is_ok() {
                let tmp = tempfile::NamedTempFile::new()?;
                let path = tmp.path().to_string_lossy().to_string();
                let output = Command::new("import")
                    .args(["-window", "root", &path])
                    .output()?;
                if output.status.success() {
                    let data = std::fs::read(&path)?;
                    return Ok(data);
                }
            }
        }
        bail!("screenshot not available in headless mode (no display server)")
    }

    fn get_screen_size(&self) -> Result<ScreenSize> {
        Ok(ScreenSize { width: 1920, height: 1080 }) // default
    }

    fn mouse_move(&self, _x: i32, _y: i32, _smooth: bool, _duration_ms: u64) -> Result<()> {
        bail!("mouse not available in headless mode")
    }

    fn mouse_click(&self, _button: &str, _x: Option<i32>, _y: Option<i32>, _clicks: u32) -> Result<()> {
        bail!("mouse not available in headless mode")
    }

    fn mouse_scroll(&self, _dx: i32, _dy: i32, _x: Option<i32>, _y: Option<i32>) -> Result<()> {
        bail!("mouse not available in headless mode")
    }

    fn mouse_drag(&self, _x1: i32, _y1: i32, _x2: i32, _y2: i32, _button: &str, _duration_ms: u64) -> Result<()> {
        bail!("mouse not available in headless mode")
    }

    fn keyboard_type(&self, _text: &str, _delay_ms: u64) -> Result<()> {
        bail!("keyboard not available in headless mode")
    }

    fn key_press(&self, _key: &str) -> Result<()> {
        bail!("keyboard not available in headless mode")
    }

    fn clipboard_get(&self) -> Result<String> {
        if which::which("xclip").is_ok() {
            let output = Command::new("xclip")
                .args(["-selection", "clipboard", "-o"])
                .output()?;
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            bail!("clipboard not available (install xclip)")
        }
    }

    fn clipboard_set(&self, text: &str) -> Result<()> {
        if which::which("xclip").is_ok() {
            let mut child = Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()?;
            Ok(())
        } else {
            bail!("clipboard not available")
        }
    }

    fn shell_run(&self, command: &str, _timeout_secs: u64) -> Result<ShellResult> {
        let output = Command::new("sh")
            .args(["-c", command])
            .output()?;

        let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stdout.len() > 8000 { stdout.truncate(8000); }
        if stderr.len() > 4000 { stderr.truncate(4000); }

        Ok(ShellResult {
            returncode: output.status.code().unwrap_or(-1),
            stdout,
            stderr,
        })
    }

    fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        bail!("window management not available in headless mode")
    }

    fn focus_window(&self, _title_match: &str) -> Result<WindowMatch> {
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
        if which::which("notify-send").is_ok() {
            let _ = Command::new("notify-send")
                .args(["-u", urgency, title, message])
                .output();
        }
        Ok(())
    }
}
