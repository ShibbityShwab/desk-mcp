//! Windows provider — Win32 + UI Automation APIs.
//!
//! This is a STUB. Full implementation requires Windows-native compilation.
//! Compiles on Linux via cfg-gated empty impls that return friendly errors.
//!
//! Planned Windows-native dependencies (not yet added to Cargo.toml):
//!   - `windows` crate (official Microsoft bindings)
//!   - `winapi` or `windows-sys` (direct FFI alternative)
//!
//! Usage when Windows support lands:
//!   ```ignore
//!   // In Cargo.toml:
//!   [target.'cfg(target_os = "windows")'.dependencies]
//!   windows = { version = "0.58", features = [
//!       "Win32_Graphics_Gdi",
//!       "Win32_UI_Input_KeyboardAndMouse",
//!       "Win32_UI_WindowsAndMessaging",
//!       "Win32_UI_Accessibility",
//!       "Win32_System_Com",
//!   ]}
//!   ```

use super::*;
use anyhow::Result;

// ── Stub provider (compiles on all platforms) ──────────────────────

pub struct WindowsProvider;

impl ComputerProvider for WindowsProvider {
    fn name(&self) -> &str {
        "windows"
    }

    // ── Screenshot ───────────────────────────────────────

    fn screenshot(&self, _region: Option<(i32, i32, u32, u32)>) -> Result<Vec<u8>> {
        if cfg!(target_os = "windows") {
            // TODO: BitBlt from GetDC(NULL) → create compatible DC → capture → GDI+ PNG encode
            //   let hdc_screen = GetDC(NULL);
            //   let hdc_mem = CreateCompatibleDC(hdc_screen);
            //   let hbitmap = CreateCompatibleBitmap(hdc_screen, w, h);
            //   SelectObject(hdc_mem, hbitmap);
            //   BitBlt(hdc_mem, 0, 0, w, h, hdc_screen, x, y, SRCCOPY);
            //   // Convert HBITMAP → PNG bytes via GDI+ or image crate
            //   ReleaseDC(NULL, hdc_screen);
            Err(anyhow::anyhow!("Windows native screenshot not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    fn get_screen_size(&self) -> Result<ScreenSize> {
        if cfg!(target_os = "windows") {
            // TODO: GetSystemMetrics(SM_CXSCREEN) / GetSystemMetrics(SM_CYSCREEN)
            //   Or EnumDisplaySettings → DEVMODEW.dmPelsWidth / dmPelsHeight
            Err(anyhow::anyhow!("Windows native get_screen_size not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    // ── Mouse ────────────────────────────────────────────

    fn mouse_move(&self, _x: i32, _y: i32, _smooth: bool, _duration_ms: u64) -> Result<()> {
        if cfg!(target_os = "windows") {
            // TODO: SendInput with MOUSEINPUT (MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE)
            //   let mut input = INPUT { type: INPUT_MOUSE, ... };
            //   input.u.mi.dx = x * 65535 / screen_w;
            //   input.u.mi.dy = y * 65535 / screen_h;
            //   input.u.mi.dwFlags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE;
            //   SendInput(1, &input, size_of::<INPUT>());
            // For smooth: interpolate points and SendInput for each step with small sleep
            Err(anyhow::anyhow!("Windows native mouse_move not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    fn mouse_click(
        &self,
        _button: &str,
        _x: Option<i32>,
        _y: Option<i32>,
        _clicks: u32,
    ) -> Result<()> {
        if cfg!(target_os = "windows") {
            // TODO: SendInput with MOUSEINPUT for left down/up
            //   If x/y: set cursor pos first with SetCursorPos(x, y)
            //   let flags = match button { "left" => MOUSEEVENTF_LEFTDOWN, "right" => MOUSEEVENTF_RIGHTDOWN, ... };
            //   for _ in 0..clicks {
            //       SendInput down; SendInput up;
            //   }
            Err(anyhow::anyhow!("Windows native mouse_click not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    fn mouse_scroll(&self, _dx: i32, _dy: i32, _x: Option<i32>, _y: Option<i32>) -> Result<()> {
        if cfg!(target_os = "windows") {
            // TODO: SendInput with MOUSEINPUT → MOUSEEVENTF_WHEEL
            //   let mut input = INPUT { type: INPUT_MOUSE, ... };
            //   input.u.mi.mouseData = dy * WHEEL_DELTA;
            //   input.u.mi.dwFlags = MOUSEEVENTF_WHEEL;
            //   SendInput(...);
            // For horizontal scroll: MOUSEEVENTF_HWHEEL
            // If x/y provided: SetCursorPos first
            Err(anyhow::anyhow!("Windows native mouse_scroll not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
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
        if cfg!(target_os = "windows") {
            // TODO: SetCursorPos(x1, y1) → SendInput(MOUSEEVENTF_LEFTDOWN) →
            //   interpolate with SetCursorPos over duration → SendInput(MOUSEEVENTF_LEFTUP)
            //   let steps = duration_ms / 16; // ~60 fps
            //   for i in 0..steps { SetCursorPos(lerp(x1, x2, i/steps), lerp(y1, y2, i/steps)); Sleep(16); }
            Err(anyhow::anyhow!("Windows native mouse_drag not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    // ── Keyboard ─────────────────────────────────────────

    fn keyboard_type(&self, _text: &str, _delay_ms: u64) -> Result<()> {
        if cfg!(target_os = "windows") {
            // TODO: SendInput with KEYBDINPUT
            //   For each char: VkKeyScanEx to get virtual key + shift state
            //   If shift: SendInput for VK_SHIFT down
            //   SendInput for char key down → up
            //   If shift: SendInput for VK_SHIFT up
            //   Sleep(delay_ms) between keys
            Err(anyhow::anyhow!("Windows native keyboard_type not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    fn key_press(&self, _key: &str) -> Result<()> {
        if cfg!(target_os = "windows") {
            // TODO: SendInput with KEYBDINPUT → parse combo "ctrl+c", "win+shift+s"
            //   Parse modifiers: ctrl=VK_CONTROL, alt=VK_MENU, shift=VK_SHIFT, win=VK_LWIN
            //   Press modifiers down → press key down → release key → release modifiers
            Err(anyhow::anyhow!("Windows native key_press not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    // ── Clipboard ────────────────────────────────────────

    fn clipboard_get(&self) -> Result<String> {
        if cfg!(target_os = "windows") {
            // TODO: OpenClipboard(NULL) → GetClipboardData(CF_TEXT/CF_UNICODETEXT) → read
            //   Or use arboard (already in deps, cross-platform)
            Err(anyhow::anyhow!("Windows native clipboard_get not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    fn clipboard_set(&self, _text: &str) -> Result<()> {
        if cfg!(target_os = "windows") {
            // TODO: OpenClipboard → EmptyClipboard → SetClipboardData(CF_UNICODETEXT, ...) → CloseClipboard
            Err(anyhow::anyhow!("Windows native clipboard_set not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    // ── Shell ────────────────────────────────────────────

    fn shell_run(&self, _command: &str, _timeout_secs: u64) -> Result<ShellResult> {
        if cfg!(target_os = "windows") {
            // TODO: CreateProcessW with pipes for stdout/stderr capture
            //   Or use std::process::Command (cross-platform)
            Err(anyhow::anyhow!("Windows native shell_run not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    // ── Windows ──────────────────────────────────────────

    fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        if cfg!(target_os = "windows") {
            // TODO: EnumWindows callback → GetWindowTextW → GetWindowThreadProcessId →
            //   GetWindowRect → build WindowInfo list
            //   WindowInfo.from(enum_windows_callback(hwnd, lparam))
            Err(anyhow::anyhow!("Windows native list_windows not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    fn focus_window(&self, _title_match: &str) -> Result<WindowMatch> {
        if cfg!(target_os = "windows") {
            // TODO: EnumWindows → find by title → SetForegroundWindow(hwnd)
            //   Or FindWindowW(NULL, title) for exact match
            //   Return WindowMatch with matched: true/false
            Err(anyhow::anyhow!("Windows native focus_window not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    fn get_active_window(&self) -> Result<Option<WindowInfo>> {
        if cfg!(target_os = "windows") {
            // TODO: GetForegroundWindow() → GetWindowTextW → GetWindowThreadProcessId →
            //   GetWindowRect → build WindowInfo
            Err(anyhow::anyhow!("Windows native get_active_window not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    // ── Apps / Notifications ─────────────────────────────

    fn open_app(&self, _app_name: &str) -> Result<()> {
        if cfg!(target_os = "windows") {
            // TODO: ShellExecuteW(NULL, "open", app_name, ...) or CreateProcessW
            Err(anyhow::anyhow!("Windows native open_app not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    fn notify(&self, _title: &str, _message: &str, _urgency: &str) -> Result<()> {
        if cfg!(target_os = "windows") {
            // TODO: Windows toast notifications via IToastNotificationManager
            //   Or use notify-rust (already in deps, cross-platform)
            //   Or Shell_NotifyIcon for system tray bubble
            Err(anyhow::anyhow!("Windows native notify not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    // ── Accessibility / Element Trees ────────────────────

    fn get_window_state(&self) -> Result<WindowState> {
        if cfg!(target_os = "windows") {
            // TODO: IUIAutomation::ElementFromHandle(foreground_hwnd) →
            //   walk tree via IUIAutomationTreeWalker → collect role/name/value/bounds
            //   Map to UiElement struct
            //   CoInitializeEx(NULL, COINIT_APARTMENTTHREADED);
            //   let automation: IUIAutomation = CoCreateInstance(CLSID_CUIAutomation)?;
            //   let element = automation.ElementFromHandle(foreground_hwnd)?;
            //   walk_uia_tree(element, &mut elements, &mut Vec::new())?;
            Err(anyhow::anyhow!("Windows native get_window_state not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "Windows provider requires Windows. Running on {}",
                std::env::consts::OS
            ))
        }
    }
}
