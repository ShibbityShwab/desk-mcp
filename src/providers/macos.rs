//! macOS provider — CoreGraphics + Accessibility APIs.
//!
//! **Status: STUB** — compiles on all platforms via `cfg!(target_os)` runtime
//! checks that return `McpError::NotAvailable` with platform-specific guidance.
//!
//! ## Implementation roadmap
//!
//! 1. Add to Cargo.toml:
//!    ```toml
//!    [target.'cfg(target_os = "macos")'.dependencies]
//!    core-graphics = "0.23"
//!    core-foundation = "0.9"
//!    accessibility-sys = "0.2"
//!    ```
//! 2. Replace each `cfg!()` block in this file with the real implementation.
//!    The TODO comments document the exact API calls needed per method.
//! 3. Test on macOS 14+ (Sonoma) — Accessibility APIs require
//!    `NSAccessibility` permission in System Preferences.

use super::*;
use anyhow::Result;

// ── Stub provider (compiles on all platforms) ──────────────────────

pub struct MacOSProvider;

impl ComputerProvider for MacOSProvider {
    fn name(&self) -> &str {
        "macos"
    }

    // ── Screenshot ───────────────────────────────────────

    fn screenshot(&self, _region: Option<(i32, i32, u32, u32)>) -> Result<Vec<u8>> {
        if cfg!(target_os = "macos") {
            // TODO: CGDisplayCreateImage → CGImageDestination → PNG bytes
            //   let display = CGDisplay::main();
            //   let image = display.image()?;
            //   let (x, y, w, h) = if let Some(region) = region { ... } else { (0, 0, ...) };
            //   let cropped = image.cropped(CGRect::new(...))?;
            //   let png = image_to_png(cropped)?;
            Err(anyhow::anyhow!(
                "macOS native screenshot not yet implemented"
            ))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    fn get_screen_size(&self) -> Result<ScreenSize> {
        if cfg!(target_os = "macos") {
            // TODO: CGDisplayBounds → CGRect → width/height
            //   let main = CGDisplay::main();
            //   let bounds = main.bounds();
            //   Ok(ScreenSize { width: bounds.size.width as u32, height: bounds.size.height as u32 })
            Err(anyhow::anyhow!(
                "macOS native get_screen_size not yet implemented"
            ))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    // ── Mouse ────────────────────────────────────────────

    fn mouse_move(&self, _x: i32, _y: i32, _smooth: bool, _duration_ms: u64) -> Result<()> {
        if cfg!(target_os = "macos") {
            // TODO: CGEventCreateMouseEvent → CGEventPost(kCGHIDEventTap, …)
            //   let event = CGEvent::new_mouse_event(
            //       CGEventType::MouseMoved,
            //       CGPoint::new(x as f64, y as f64),
            //       kCGMouseButtonLeft,
            //   )?;
            //   event.post(kCGHIDEventTap)?;
            // For smooth: CGEventSetIntegerValueField(event, kCGMouseEventDeltaX, dx)
            Err(anyhow::anyhow!(
                "macOS native mouse_move not yet implemented"
            ))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
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
        if cfg!(target_os = "macos") {
            // TODO: CGEventCreateMouseEvent with kCGEventLeftMouseDown/Up
            //   let button = match button { "left" => kCGMouseButtonLeft, ... };
            //   let down = CGEvent::new_mouse_event(kCGEventLeftMouseDown, point, button)?;
            //   let up = CGEvent::new_mouse_event(kCGEventLeftMouseUp, point, button)?;
            //   for _ in 0..clicks { down.post(kCGHIDEventTap)?; up.post(kCGHIDEventTap)?; }
            Err(anyhow::anyhow!(
                "macOS native mouse_click not yet implemented"
            ))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    fn mouse_scroll(&self, _dx: i32, _dy: i32, _x: Option<i32>, _y: Option<i32>) -> Result<()> {
        if cfg!(target_os = "macos") {
            // TODO: CGEventCreateScrollWheelEvent
            //   let event = CGEvent::new_scroll_event(
            //       kCGScrollEventUnitPixel,
            //       2,   // wheel count
            //       dy,  // vertical
            //       dx,  // horizontal
            //   )?;
            //   event.post(kCGHIDEventTap)?;
            //   If x/y provided: move mouse first with CGEventCreateMouseEvent
            Err(anyhow::anyhow!(
                "macOS native mouse_scroll not yet implemented"
            ))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
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
        if cfg!(target_os = "macos") {
            // TODO: CGEventCreateMouseEvent for left_down → move → left_up
            //   Move to (x1, y1), press button, interpolate points, release at (x2, y2)
            //   Use CGEventPost for each step; thread::sleep for duration
            Err(anyhow::anyhow!(
                "macOS native mouse_drag not yet implemented"
            ))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    // ── Keyboard ─────────────────────────────────────────

    fn keyboard_type(&self, _text: &str, _delay_ms: u64) -> Result<()> {
        if cfg!(target_os = "macos") {
            // TODO: CGEventCreateKeyboardEvent → CGEventPost
            //   For each char: look up keycode via UCKeyTranslate / TISCopyCurrentKeyboardInputSource
            //   let event = CGEvent::new_keyboard_event(keycode, true)?;  // key down
            //   event.post(kCGHIDEventTap)?;
            //   thread::sleep(delay);
            //   let event = CGEvent::new_keyboard_event(keycode, false)?; // key up
            //   event.post(kCGHIDEventTap)?;
            Err(anyhow::anyhow!(
                "macOS native keyboard_type not yet implemented"
            ))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    fn key_press(&self, _key: &str) -> Result<()> {
        if cfg!(target_os = "macos") {
            // TODO: CGEventCreateKeyboardEvent → parse modifier + key
            //   Parse combo like "ctrl+c", "cmd+shift+t"
            //   CGEventSetFlags(event, kCGEventFlagMaskCommand | ...);
            //   Post key down + key up
            Err(anyhow::anyhow!(
                "macOS native key_press not yet implemented"
            ))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    // ── Clipboard ────────────────────────────────────────

    fn clipboard_get(&self) -> Result<String> {
        if cfg!(target_os = "macos") {
            // TODO: NSPasteboard.general.string(forType: .string)
            //   let pb = NSPasteboard::generalPasteboard();
            //   pb.stringForType(NSPasteboardTypeString)?
            //   Or use arboard (already in deps, works on macOS)
            Err(anyhow::anyhow!(
                "macOS native clipboard_get not yet implemented"
            ))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    fn clipboard_set(&self, _text: &str) -> Result<()> {
        if cfg!(target_os = "macos") {
            // TODO: NSPasteboard.general.clearContents() + setString(_:forType:)
            Err(anyhow::anyhow!(
                "macOS native clipboard_set not yet implemented"
            ))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    // ── Shell ────────────────────────────────────────────

    fn shell_run(&self, _command: &str, _timeout_secs: u64) -> Result<ShellResult> {
        if cfg!(target_os = "macos") {
            // TODO: std::process::Command (cross-platform, but gate behind macOS for consistency)
            Err(anyhow::anyhow!(
                "macOS native shell_run not yet implemented"
            ))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    // ── Windows ──────────────────────────────────────────

    fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        if cfg!(target_os = "macos") {
            // TODO: CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID)
            //   Parse the CFArray of dictionaries: kCGWindowName, kCGWindowOwnerName, ...
            //   Map to WindowInfo struct
            Err(anyhow::anyhow!(
                "macOS native list_windows not yet implemented"
            ))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    fn focus_window(&self, _title_match: &str) -> Result<WindowMatch> {
        if cfg!(target_os = "macos") {
            // TODO: list_windows() → filter by title → AXUIElementPerformAction(AXRaiseAction)
            //   Then set frontmost via NSRunningApplication
            Err(anyhow::anyhow!(
                "macOS native focus_window not yet implemented"
            ))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    fn get_active_window(&self) -> Result<Option<WindowInfo>> {
        if cfg!(target_os = "macos") {
            // TODO: NSWorkspace.shared.frontmostApplication
            //   + CGWindowListCopyWindowInfo → filter for active app's windows
            Err(anyhow::anyhow!(
                "macOS native get_active_window not yet implemented"
            ))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    // ── Apps / Notifications ─────────────────────────────

    fn open_app(&self, _app_name: &str) -> Result<()> {
        if cfg!(target_os = "macos") {
            // TODO: NSWorkspace.shared.openApplication(at:config:completionHandler:)
            //   Or use `/usr/bin/open -a "AppName"`
            Err(anyhow::anyhow!("macOS native open_app not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    fn notify(&self, _title: &str, _message: &str, _urgency: &str) -> Result<()> {
        if cfg!(target_os = "macos") {
            // TODO: NSUserNotificationCenter (deprecated) or UNUserNotificationCenter (10.14+)
            //   Create UNMutableNotificationContent, set title/body/sound,
            //   Deliver via UNUserNotificationCenter.current()
            Err(anyhow::anyhow!("macOS native notify not yet implemented"))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
                std::env::consts::OS
            ))
        }
    }

    // ── Accessibility / Element Trees ────────────────────

    fn get_window_state(&self) -> Result<WindowState> {
        if cfg!(target_os = "macos") {
            // TODO: AXUIElementCopyAttributeValue → walk accessibility tree
            //   Start from active window AXUIElement:
            //     AXUIElementCopyAttributeValue(el, kAXRoleAttribute) → role
            //     AXUIElementCopyAttributeValue(el, kAXTitleAttribute) → name
            //     AXUIElementCopyAttributeValue(el, kAXChildrenAttribute) → children
            //   Walk recursively; emit UiElement for each node
            Err(anyhow::anyhow!(
                "macOS native get_window_state not yet implemented"
            ))
        } else {
            Err(anyhow::anyhow!(
                "macOS provider requires macOS. Running on {}",
                std::env::consts::OS
            ))
        }
    }
}
