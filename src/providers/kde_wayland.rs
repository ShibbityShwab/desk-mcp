//! KDE Wayland provider — spectacle (screenshot), enigo (X11 input),
//! wdotool-core (Wayland input via libei), ydotool (Wayland fallback),
//! kdotool (windows), arboard (clipboard), notify-rust (notifications).
//!
//! This is the primary provider for personal desktop use on KDE Plasma 6 Wayland.
//!
//! Input routing:
//!   Wayland → wdotool-core (libei), fallback to ydotool CLI
//!   X11     → enigo (XTEST)

use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::*;
use anyhow::{bail, Context, Result};
use enigo::{Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};

// ── Display server detection ──────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum DisplayServer {
    Wayland,
    X11,
    Headless,
}

fn detect_display_server() -> DisplayServer {
    if std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v == "wayland")
            .unwrap_or(false)
    {
        DisplayServer::Wayland
    } else if std::env::var("DISPLAY").is_ok() {
        DisplayServer::X11
    } else {
        DisplayServer::Headless
    }
}

// ── wdotool-core backend (Wayland input via libei) ─────────────────────

static WDO_BACKEND: OnceLock<Option<wdotool_core::backend::DynBackend>> = OnceLock::new();

fn get_wdo_backend() -> Option<&'static wdotool_core::backend::DynBackend> {
    WDO_BACKEND
        .get_or_init(|| {
            let env = wdotool_core::backend::detector::Environment::detect();
            let result = block_on_wdo(wdotool_core::backend::detector::build(&env, None));
            match result {
                Ok(backend) => {
                    tracing::info!("wdotool-core backend initialized for Wayland input");
                    Some(backend)
                }
                Err(e) => {
                    tracing::warn!(
                        "wdotool-core backend unavailable: {e}; falling back to enigo/ydotool"
                    );
                    None
                }
            }
        })
        .as_ref()
}

/// Run an async wdotool-core operation from a synchronous context.
/// Uses `block_in_place` to temporarily step out of the tokio runtime,
/// then `block_on` to drive the future to completion.
fn block_on_wdo<F: std::future::Future>(f: F) -> F::Output {
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(f))
}

/// Shell-escape a string for safe use in `sh -c` commands
fn shell_escape(s: &str) -> String {
    let escaped = s.replace('\'', "'\\''");
    format!("'{}'", escaped)
}

// ── Key mapping ────────────────────────────────────────────────

fn map_key(name: &str) -> Option<Key> {
    Some(match name {
        "esc" | "escape" => Key::Escape,
        "return" | "enter" => Key::Return,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "space" | " " => Key::Space,
        "capslock" => Key::CapsLock,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "page_up" => Key::PageUp,
        "pagedown" | "page_down" => Key::PageDown,
        "insert" => Key::Insert,
        "delete" => Key::Delete,
        "ctrl" | "leftctrl" | "rightctrl" | "control" => Key::Control,
        "shift" | "leftshift" | "rightshift" => Key::Shift,
        "alt" | "leftalt" | "rightalt" => Key::Alt,
        "super" | "meta" | "win" => Key::Meta,
        "print" => Key::PrintScr,
        "pause" => Key::Pause,
        "menu" => Key::LMenu,
        "numlock" => Key::Numlock,
        "scrolllock" => Key::ScrollLock,
        s if s.len() == 1 => Key::Unicode(s.chars().next().unwrap()),
        _ => return None,
    })
}

fn to_enigo() -> Result<Enigo> {
    Enigo::new(&Settings::default()).context("enigo: failed to connect to compositor")
}

fn map_button(name: &str) -> Result<enigo::Button> {
    Ok(match name {
        "left" => enigo::Button::Left,
        "right" => enigo::Button::Right,
        "middle" => enigo::Button::Middle,
        other => bail!("unknown button: {other}"),
    })
}

// ── DBus auto-detection ───────────────────────────────────────────

/// Ensure DBUS_SESSION_BUS_ADDRESS is available. Tries:
/// 1. Existing env var
/// 2. /run/user/<uid>/bus (systemd user session)
/// 3. dbus-launch (last resort — creates new bus, limited functionality)
fn ensure_dbus() -> Option<String> {
    if let Ok(addr) = std::env::var("DBUS_SESSION_BUS_ADDRESS") {
        if !addr.is_empty() {
            return Some(addr);
        }
    }

    // Probe systemd user bus (most common on modern Linux)
    let uid = unsafe { libc::getuid() };
    let systemd_bus = format!("unix:path=/run/user/{uid}/bus");
    if std::path::Path::new(&format!("/run/user/{uid}/bus")).exists() {
        tracing::info!("auto-detected DBus at {systemd_bus}");
        return Some(systemd_bus);
    }

    // Probe common socket paths
    for candidate in &[
        format!("/run/user/{uid}/dbus-session"),
        format!("/tmp/dbus-{uid}"),
    ] {
        if std::path::Path::new(candidate).exists() {
            let addr = format!("unix:path={candidate}");
            tracing::info!("auto-detected DBus at {addr}");
            return Some(addr);
        }
    }

    // Last resort: launch a private bus (limited — can't talk to desktop session)
    if let Ok(output) = std::process::Command::new("dbus-launch")
        .arg("--sh-syntax")
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(addr) = line.strip_prefix("DBUS_SESSION_BUS_ADDRESS='") {
                let addr = addr.trim_end_matches("';");
                tracing::info!("launched private DBus: {addr}");
                return Some(addr.to_string());
            }
        }
    }

    tracing::warn!("no DBus session bus found — spectacle may not work");
    None
}

/// Load a PNG screenshot file and optionally crop to a region.
fn load_and_crop_screenshot(path: &str, region: Option<(i32, i32, u32, u32)>) -> Result<Vec<u8>> {
    let bytes = std::fs::read(path).with_context(|| format!("read screenshot file {path}"))?;
    let img = image::load_from_memory(&bytes).context("decode screenshot PNG")?;

    let result = if let Some((x, y, w, h)) = region {
        let cropped = img.crop_imm(
            x.max(0) as u32,
            y.max(0) as u32,
            w.min(img.width()),
            h.min(img.height()),
        );
        let mut buf = std::io::Cursor::new(Vec::new());
        cropped.write_to(&mut buf, image::ImageFormat::Png)?;
        buf.into_inner()
    } else {
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png)?;
        buf.into_inner()
    };

    Ok(result)
}

// ── Provider ───────────────────────────────────────────────────

#[derive(Default)]
pub struct KdeWaylandProvider;

static CLIPBOARD: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();

fn get_clipboard_lock() -> &'static Mutex<Option<arboard::Clipboard>> {
    CLIPBOARD.get_or_init(|| {
        Mutex::new(arboard::Clipboard::new().ok())
    })
}

impl ComputerProvider for KdeWaylandProvider {
    fn name(&self) -> &str {
        "wayland_kde"
    }

    // ── Screenshot ───────────────────────────────────────────
    fn screenshot(&self, region: Option<(i32, i32, u32, u32)>) -> Result<Vec<u8>> {
        // Try spectacle first (KDE native), with automatic DBus detection.
        // spectacle needs the user's DBus session bus to communicate with
        // the KDE compositor. If DBUS_SESSION_BUS_ADDRESS isn't set, probe
        // common locations (/run/user/<uid>/bus) and running processes.
        let dbus_addr = ensure_dbus();
        // Use a fixed temp path — NamedTempFile keeps the file open,
        // and tempdir paths can confuse spectacle's output handling.
        let path = format!("/tmp/deskmcp_screenshot_{}.png", std::process::id());

        let mut cmd = Command::new("spectacle");
        cmd.args(["-b", "-n", "-o", &path]);
        if let Some(ref addr) = dbus_addr {
            cmd.env("DBUS_SESSION_BUS_ADDRESS", addr);
        }

        let spectacle_result = cmd.output();

        match spectacle_result {
            Ok(output) if output.status.success() => {
                // spectacle -b runs in background; wait for the file to be written.
                // The file might appear as 0 bytes first, then grow — so we
                // wait until it's at least 1KB (screenshots are typically MBs).
                for attempt in 0..50 {
                    if let Ok(meta) = std::path::Path::new(&path).metadata() {
                        if meta.len() > 1024 {
                            tracing::info!(
                                "screenshot file ready ({attempt} attempts, {} bytes)",
                                meta.len()
                            );
                            return load_and_crop_screenshot(&path, region);
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                tracing::warn!("spectacle succeeded but file never stabilized — trying fallback");
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                tracing::warn!(
                    "spectacle failed (stderr: {stderr:.200}), trying ImageMagick fallback"
                );
            }
            Err(e) => {
                tracing::warn!("spectacle not found: {e}, trying ImageMagick fallback");
            }
        }

        // Fallback: try ImageMagick 7 (magick import) then ImageMagick 6 (import)
        for tool in &["magick", "import"] {
            let fallback_result = Command::new(tool)
                .args(["import", "-window", "root", &path])
                .output();

            match fallback_result {
                Ok(output) if output.status.success() => {
                    tracing::info!("screenshot captured via {tool}");
                    return load_and_crop_screenshot(&path, region);
                }
                Ok(_) => continue,
                Err(_) => continue,
            }
        }

        // Nothing worked — give a helpful error
        let uid = unsafe { libc::getuid() };
        bail!(
            "Cannot capture screenshot. Tried:\n\
             - spectacle (KDE): needs DBus session. Try: export DBUS_SESSION_BUS_ADDRESS=\"unix:path=/run/user/{uid}/bus\"\n\
             - magick import / import (ImageMagick): X11-only, won't work on Wayland\n\
             \n\
             Fix: ensure DBUS_SESSION_BUS_ADDRESS is set to your desktop session bus,\n\
             or install grim (for wlroots) / gnome-screenshot."
        );
    }

    // ── Mouse ────────────────────────────────────────────────────
    fn mouse_move(&self, x: i32, y: i32, smooth: bool, dur_ms: u64) -> Result<()> {
        // Try wdotool-core on Wayland
        if detect_display_server() == DisplayServer::Wayland {
            if let Some(backend) = get_wdo_backend() {
                return block_on_wdo(backend.mouse_move(x, y, true))
                    .context("wdotool mouse_move failed");
            }
            // Fallback to ydotool
            let mut cmd = Command::new("ydotool");
            cmd.args(["mousemove", "--absolute", &x.to_string(), &y.to_string()]);
            let output = cmd
                .output()
                .context("ydotool not found — install ydotool for Wayland input")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!("ydotool mousemove failed: {stderr}");
            }
            if smooth && dur_ms > 0 {
                std::thread::sleep(Duration::from_millis(dur_ms));
            }
            return Ok(());
        }

        // X11: enigo
        let mut enigo = to_enigo()?;
        if smooth && dur_ms > 0 {
            let steps = 20;
            let dt = Duration::from_millis(dur_ms / steps);
            let start = Instant::now();
            enigo.move_mouse(x, y, Coordinate::Abs)?;
            std::thread::sleep(dt);
            let elapsed = start.elapsed();
            if elapsed < Duration::from_millis(dur_ms) {
                std::thread::sleep(Duration::from_millis(dur_ms) - elapsed);
            }
        } else {
            enigo.move_mouse(x, y, Coordinate::Abs)?;
        }
        Ok(())
    }

    fn mouse_click(&self, button: &str, x: Option<i32>, y: Option<i32>, clicks: u32) -> Result<()> {
        if detect_display_server() == DisplayServer::Wayland {
            if let Some(backend) = get_wdo_backend() {
                if let (Some(x), Some(y)) = (x, y) {
                    block_on_wdo(backend.mouse_move(x, y, true))
                        .context("wdotool mouse_move before click")?;
                }
                let btn = match button {
                    "left" => wdotool_core::types::MouseButton::Left,
                    "right" => wdotool_core::types::MouseButton::Right,
                    "middle" => wdotool_core::types::MouseButton::Middle,
                    other => bail!("unknown button: {other}"),
                };
                for _ in 0..clicks {
                    block_on_wdo(
                        backend.mouse_button(btn, wdotool_core::types::KeyDirection::PressRelease),
                    )
                    .context("wdotool mouse_button")?;
                    if clicks > 1 {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
                return Ok(());
            }
            // Fallback to ydotool
            if let (Some(x), Some(y)) = (x, y) {
                let _ = Command::new("ydotool")
                    .args(["mousemove", "--absolute", &x.to_string(), &y.to_string()])
                    .output();
            }
            let btn_code = match button {
                "left" => "0xC0",
                "right" => "0xC1",
                "middle" => "0xC2",
                _ => bail!("unknown button: {button}"),
            };
            for _ in 0..clicks {
                let output = Command::new("ydotool")
                    .args(["click", btn_code])
                    .output()
                    .context("ydotool not found")?;
                if !output.status.success() {
                    bail!(
                        "ydotool click failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
            return Ok(());
        }

        // X11: enigo
        let mut enigo = to_enigo()?;
        let btn = map_button(button)?;
        if let (Some(x), Some(y)) = (x, y) {
            enigo.move_mouse(x, y, Coordinate::Abs)?;
        }
        for _ in 0..clicks {
            enigo.button(btn, Direction::Click)?;
            if clicks > 1 {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        Ok(())
    }

    fn mouse_scroll(&self, dx: i32, dy: i32, x: Option<i32>, y: Option<i32>) -> Result<()> {
        if detect_display_server() == DisplayServer::Wayland {
            if let Some(backend) = get_wdo_backend() {
                if let (Some(x), Some(y)) = (x, y) {
                    block_on_wdo(backend.mouse_move(x, y, true))
                        .context("wdotool mouse_move before scroll")?;
                }
                block_on_wdo(backend.scroll(dx as f64, dy as f64))
                    .context("wdotool scroll failed")?;
                return Ok(());
            }
            // ydotool fallback: ydotoold handles scroll via relative mouse moves
            // Simulate scroll with keyboard (PageUp/PageDown) or just skip
            tracing::warn!(
                "ydotool scroll not directly supported; consider installing wdotool-core"
            );
            return Ok(());
        }

        // X11: enigo
        let mut enigo = to_enigo()?;
        if let (Some(x), Some(y)) = (x, y) {
            enigo.move_mouse(x, y, Coordinate::Abs)?;
        }
        if dy != 0 {
            enigo.scroll(dy, enigo::Axis::Vertical)?;
        }
        if dx != 0 {
            enigo.scroll(dx, enigo::Axis::Horizontal)?;
        }
        Ok(())
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
        if detect_display_server() == DisplayServer::Wayland {
            if let Some(backend) = get_wdo_backend() {
                let btn = match button {
                    "left" => wdotool_core::types::MouseButton::Left,
                    "right" => wdotool_core::types::MouseButton::Right,
                    "middle" => wdotool_core::types::MouseButton::Middle,
                    other => bail!("unknown button: {other}"),
                };
                block_on_wdo(backend.mouse_move(x1, y1, true)).context("wdotool drag start")?;
                block_on_wdo(backend.mouse_button(btn, wdotool_core::types::KeyDirection::Press))
                    .context("wdotool drag press")?;
                let steps = 30_usize;
                let step_dur = Duration::from_millis(dur_ms.max(30) / steps as u64);
                let dx = (x2 - x1) as f64 / steps as f64;
                let dy = (y2 - y1) as f64 / steps as f64;
                for i in 1..=steps {
                    let cx = x1 + (dx * i as f64) as i32;
                    let cy = y1 + (dy * i as f64) as i32;
                    block_on_wdo(backend.mouse_move(cx, cy, true))?;
                    std::thread::sleep(step_dur);
                }
                block_on_wdo(backend.mouse_button(btn, wdotool_core::types::KeyDirection::Release))
                    .context("wdotool drag release")?;
                return Ok(());
            }
            // ydotool fallback: mousedown, mousemove steps, mouseup
            let btn_code = match button {
                "left" => "0xC0",
                "right" => "0xC1",
                "middle" => "0xC2",
                _ => bail!("unknown button: {button}"),
            };
            let _ = Command::new("ydotool")
                .args(["mousemove", "--absolute", &x1.to_string(), &y1.to_string()])
                .output();
            let _ = Command::new("ydotool")
                .args(["mousedown", btn_code])
                .output();
            std::thread::sleep(Duration::from_millis(20));
            let steps = 30_usize;
            let step_dur = Duration::from_millis(dur_ms.max(30) / steps as u64);
            let dx = (x2 - x1) as f64 / steps as f64;
            let dy = (y2 - y1) as f64 / steps as f64;
            for i in 1..=steps {
                let cx = x1 + (dx * i as f64) as i32;
                let cy = y1 + (dy * i as f64) as i32;
                let _ = Command::new("ydotool")
                    .args(["mousemove", "--absolute", &cx.to_string(), &cy.to_string()])
                    .output();
                std::thread::sleep(step_dur);
            }
            let _ = Command::new("ydotool")
                .args(["mousemove", "--absolute", &x2.to_string(), &y2.to_string()])
                .output();
            let _ = Command::new("ydotool").args(["mouseup", btn_code]).output();
            return Ok(());
        }

        // X11: enigo
        let mut enigo = to_enigo()?;
        let btn = map_button(button)?;

        enigo.move_mouse(x1, y1, Coordinate::Abs)?;
        std::thread::sleep(Duration::from_millis(20));
        enigo.button(btn, Direction::Press)?;

        let steps = 30;
        let step_dur = Duration::from_millis(dur_ms.max(30) / steps);
        let dx = (x2 - x1) as f64 / steps as f64;
        let dy = (y2 - y1) as f64 / steps as f64;

        for i in 1..=steps {
            let cx = x1 + (dx * i as f64) as i32;
            let cy = y1 + (dy * i as f64) as i32;
            enigo.move_mouse(cx, cy, Coordinate::Abs)?;
            std::thread::sleep(step_dur);
        }

        enigo.move_mouse(x2, y2, Coordinate::Abs)?;
        std::thread::sleep(Duration::from_millis(20));
        enigo.button(btn, Direction::Release)?;

        Ok(())
    }

    // ── Keyboard ─────────────────────────────────────────────────
    fn keyboard_type(&self, text: &str, delay_ms: u64) -> Result<()> {
        if detect_display_server() == DisplayServer::Wayland {
            if let Some(backend) = get_wdo_backend() {
                block_on_wdo(backend.type_text(text, Duration::from_millis(delay_ms.max(1))))
                    .context("wdotool type_text failed")?;
                return Ok(());
            }
            // Fallback to ydotool
            let output = Command::new("ydotool")
                .args(["type", "--key-delay", &delay_ms.to_string(), text])
                .output()
                .context("ydotool not found — install ydotool for Wayland input")?;
            if !output.status.success() {
                bail!(
                    "ydotool type failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            return Ok(());
        }

        // X11: enigo
        let mut enigo = to_enigo()?;
        let delay = Duration::from_millis(delay_ms.max(1));
        for c in text.chars() {
            enigo.key(Key::Unicode(c), Direction::Click)?;
            std::thread::sleep(delay);
        }
        Ok(())
    }

    fn key_press(&self, combo: &str) -> Result<()> {
        let parts: Vec<&str> = combo.split('+').map(|s| s.trim()).collect();
        if parts.is_empty() {
            bail!("empty key combo");
        }

        if detect_display_server() == DisplayServer::Wayland {
            if let Some(backend) = get_wdo_backend() {
                block_on_wdo(backend.key(combo, wdotool_core::types::KeyDirection::PressRelease))
                    .context("wdotool key press failed")?;
                return Ok(());
            }
            // Fallback to ydotool
            // ydotool syntax: ydotool key <code>:<state> [<code>:<state> ...]
            // State 1 = press, 0 = release
            let modifier_codes: Vec<&str> = parts[..parts.len() - 1]
                .iter()
                .filter_map(|p| {
                    Some(match *p {
                        "ctrl" | "control" => "29",
                        "shift" => "42",
                        "alt" => "56",
                        "super" | "meta" | "win" => "125",
                        _ => return None,
                    })
                })
                .collect();
            let main_code = match *parts.last().unwrap() {
                "a" => "30",
                "b" => "48",
                "c" => "46",
                "d" => "32",
                "e" => "18",
                "f" => "33",
                "g" => "34",
                "h" => "35",
                "i" => "23",
                "j" => "36",
                "k" => "37",
                "l" => "38",
                "m" => "50",
                "n" => "49",
                "o" => "24",
                "p" => "25",
                "q" => "16",
                "r" => "19",
                "s" => "31",
                "t" => "20",
                "u" => "22",
                "v" => "47",
                "w" => "17",
                "x" => "45",
                "y" => "21",
                "z" => "44",
                "tab" => "15",
                "enter" | "return" => "28",
                "space" => "57",
                "escape" | "esc" => "1",
                "backspace" => "14",
                "delete" => "111",
                "up" | "UP" => "103",
                "down" | "DOWN" => "108",
                "left" | "LEFT" => "105",
                "right" | "RIGHT" => "106",
                "home" => "102",
                "end" => "107",
                "pageup" => "104",
                "pagedown" => "109",
                other => bail!("unknown key: {other}"),
            };

            // Build press sequence: modifiers down, main key down+up, modifiers up
            let mut key_args = Vec::new();
            for code in &modifier_codes {
                key_args.push(format!("{code}:1"));
            }
            key_args.push(format!("{main_code}:1"));
            key_args.push(format!("{main_code}:0"));
            for code in modifier_codes.iter().rev() {
                key_args.push(format!("{code}:0"));
            }

            let output = Command::new("ydotool")
                .arg("key")
                .args(&key_args)
                .output()
                .context("ydotool not found — install ydotool for Wayland input")?;
            if !output.status.success() {
                bail!(
                    "ydotool key failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            return Ok(());
        }

        // X11: enigo
        let mut enigo = to_enigo()?;
        for part in &parts[..parts.len() - 1] {
            let key = map_key(part).ok_or_else(|| anyhow::anyhow!("unknown key: {part}"))?;
            enigo.key(key, Direction::Press)?;
        }
        let main = parts.last().unwrap();
        let main_key = map_key(main).ok_or_else(|| anyhow::anyhow!("unknown key: {main}"))?;
        enigo.key(main_key, Direction::Click)?;
        for part in parts[..parts.len() - 1].iter().rev() {
            let key = map_key(part).unwrap();
            enigo.key(key, Direction::Release)?;
        }
        Ok(())
    }

    // ── Clipboard (arboard with wl-copy/wl-paste fallback) ─────
    fn clipboard_get(&self) -> Result<String> {
        // Try arboard first (fails gracefully on Wayland)
        let mut lock = get_clipboard_lock()
            .lock()
            .map_err(|e| anyhow::anyhow!("clipboard lock poisoned: {}", e))?;
        if let Some(ref mut clipboard) = *lock {
            if let Ok(text) = clipboard.get_text() {
                if !text.is_empty() {
                    return Ok(text);
                }
            }
        }

        // Fallback: wl-paste (Wayland) or xclip (X11)
        if let Ok(out) = Command::new("wl-paste").arg("-n").output() {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !text.is_empty() {
                    return Ok(text);
                }
            }
        }

        // Second fallback: xclip (X11)
        if let Ok(out) = Command::new("xclip")
            .args(["-selection", "clipboard", "-o"])
            .output()
        {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !text.is_empty() {
                    return Ok(text);
                }
            }
        }

        bail!("clipboard empty or contains non-text")
    }

    fn clipboard_set(&self, text: &str) -> Result<()> {
        // Try arboard first (if available)
        if let Ok(mut lock) = get_clipboard_lock().lock() {
            if let Some(ref mut clipboard) = *lock {
                let _ = clipboard.set_text(text);
            }
        }

        // wl-copy (Wayland) — spawn detached, don't wait
        let _ = std::process::Command::new("sh")
            .args(["-c", &format!("echo -n {} | wl-copy", shell_escape(text))])
            .spawn();

        // xclip (X11) — also detach
        let _ = std::process::Command::new("sh")
            .args([
                "-c",
                &format!(
                    "echo -n {} | xclip -selection clipboard -in 2>/dev/null",
                    shell_escape(text)
                ),
            ])
            .spawn();

        Ok(())
    }

    // ── Windows (kdotool — KDE-specific) ─────────────────────
    fn get_active_window(&self) -> Result<Option<WindowInfo>> {
        // Step 1: get window UUID
        let out_id = Command::new("kdotool")
            .args(["getactivewindow"])
            .output()
            .context("kdotool not found")?;
        if !out_id.status.success() {
            return Ok(None);
        }
        let id = String::from_utf8_lossy(&out_id.stdout).trim().to_string();
        if id.is_empty() {
            return Ok(None);
        }

        // Step 2: get window title (separate call, more reliable)
        let title = Command::new("kdotool")
            .args(["getwindowname", &id])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                } else {
                    None
                }
            })
            .unwrap_or_else(|| format!("Window {{{id}}}"));

        // Step 3: get geometry
        let mut geometry = WindowGeometry {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        if let Ok(out) = Command::new("kdotool")
            .args(["getwindowgeometry", &id])
            .output()
        {
            if out.status.success() {
                let raw = String::from_utf8_lossy(&out.stdout);
                for line in raw.lines() {
                    let line = line.trim();
                    if let Some(val) = line.strip_prefix("X: ") {
                        geometry.x = val.trim().parse().unwrap_or(0);
                    } else if let Some(val) = line.strip_prefix("Y: ") {
                        geometry.y = val.trim().parse().unwrap_or(0);
                    } else if let Some(val) = line.strip_prefix("Width: ") {
                        geometry.width = val.trim().parse().unwrap_or(0);
                    } else if let Some(val) = line.strip_prefix("Height: ") {
                        geometry.height = val.trim().parse().unwrap_or(0);
                    }
                }
            }
        }

        // Step 4: get app class name
        let app = Command::new("kdotool")
            .args(["getwindowclassname", &id])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                } else {
                    None
                }
            })
            .unwrap_or_default();

        // Step 5: detect PID — multiple fallbacks
        let mut pid: Option<u32> = None;

        // 5a: kdotool getwindowpid (may or may not be available depending on version)
        if let Ok(out) = Command::new("kdotool").args(["getwindowpid", &id]).output() {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().parse().ok();
                pid = p;
            }
        }

        // 5b: Strip org.kde./org.gnome./com./net. prefix and pgrep -x the short name
        if pid.is_none() && !app.is_empty() {
            let short_name = app
                .strip_prefix("org.kde.")
                .or_else(|| app.strip_prefix("org.gnome."))
                .or_else(|| app.strip_prefix("com."))
                .or_else(|| app.strip_prefix("net."))
                .unwrap_or(&app);
            if let Ok(out) = Command::new("pgrep").args(["-x", short_name]).output() {
                if out.status.success() {
                    pid = String::from_utf8_lossy(&out.stdout)
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim()
                        .parse()
                        .ok();
                }
            }
        }

        // 5c: pgrep -f with the short name (looser match)
        if pid.is_none() && !app.is_empty() {
            let short_name = app
                .strip_prefix("org.kde.")
                .or_else(|| app.strip_prefix("org.gnome."))
                .or_else(|| app.strip_prefix("com."))
                .or_else(|| app.strip_prefix("net."))
                .unwrap_or(&app);
            if let Ok(out) = Command::new("pgrep").args(["-f", short_name]).output() {
                if out.status.success() {
                    let stdout_str = String::from_utf8_lossy(&out.stdout);
                    let candidates: Vec<&str> = stdout_str.lines().collect();
                    // Take the first one (usually the main process)
                    pid = candidates.first().and_then(|s| s.trim().parse().ok());
                }
            }
        }

        // 5d: scan /proc for a process whose cmdline contains the short binary name
        if pid.is_none() && !app.is_empty() {
            let short_name = app
                .strip_prefix("org.kde.")
                .or_else(|| app.strip_prefix("org.gnome."))
                .or_else(|| app.strip_prefix("com."))
                .or_else(|| app.strip_prefix("net."))
                .unwrap_or(&app);
            if let Ok(entries) = std::fs::read_dir("/proc") {
                for entry in entries.flatten() {
                    let fname = entry.file_name();
                    let fname_str = fname.to_string_lossy();
                    if let Ok(_p) = fname_str.parse::<u32>() {
                        let cmdline_path = format!("/proc/{fname_str}/cmdline");
                        if let Ok(cmdline) = std::fs::read_to_string(&cmdline_path) {
                            // cmdline is null-separated; check if short_name appears
                            if cmdline.split('\0').any(|seg| seg.contains(short_name)) {
                                pid = fname_str.parse().ok();
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(Some(WindowInfo {
            id,
            title,
            app,
            pid,
            geometry,
        }))
    }

    fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        let output = Command::new("kdotool")
            .args(["search", "--name", "."])
            .output()
            .context("kdotool not found")?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let raw = String::from_utf8_lossy(&output.stdout);
        let ids: Vec<&str> = raw.lines().filter(|l| !l.is_empty()).collect();

        let mut windows = Vec::with_capacity(ids.len());
        for id in &ids {
            let id = id.trim();
            if id.is_empty() {
                continue;
            }

            // Get title
            let title = Command::new("kdotool")
                .args(["getwindowname", id])
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            let mut info = WindowInfo {
                id: id.to_string(),
                title,
                app: String::new(),
                pid: None,
                geometry: WindowGeometry {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                },
            };

            // Get geometry
            if let Ok(out) = Command::new("kdotool")
                .args(["getwindowgeometry", id])
                .output()
            {
                if out.status.success() {
                    let geo_raw = String::from_utf8_lossy(&out.stdout);
                    for line in geo_raw.lines() {
                        let line = line.trim();
                        if let Some(val) = line.strip_prefix("X: ") {
                            info.geometry.x = val.trim().parse().unwrap_or(0);
                        } else if let Some(val) = line.strip_prefix("Y: ") {
                            info.geometry.y = val.trim().parse().unwrap_or(0);
                        } else if let Some(val) = line.strip_prefix("Width: ") {
                            info.geometry.width = val.trim().parse().unwrap_or(0);
                        } else if let Some(val) = line.strip_prefix("Height: ") {
                            info.geometry.height = val.trim().parse().unwrap_or(0);
                        }
                    }
                }
            }

            // Get app class
            if let Ok(out) = Command::new("kdotool")
                .args(["getwindowclassname", id])
                .output()
            {
                if out.status.success() {
                    info.app = String::from_utf8_lossy(&out.stdout).trim().to_string();
                }
            }

            // Detect PID via kdotool getwindowpid first, then pgrep fallback
            // 1: kdotool getwindowpid
            if let Ok(out) = Command::new("kdotool").args(["getwindowpid", id]).output() {
                if out.status.success() {
                    if let Ok(p) = String::from_utf8_lossy(&out.stdout).trim().parse() {
                        info.pid = Some(p);
                    }
                }
            }
            // 2: Strip org.kde./org.gnome./com./net. prefix and pgrep -x
            if info.pid.is_none() && !info.app.is_empty() {
                let short_name = info
                    .app
                    .strip_prefix("org.kde.")
                    .or_else(|| info.app.strip_prefix("org.gnome."))
                    .or_else(|| info.app.strip_prefix("com."))
                    .or_else(|| info.app.strip_prefix("net."))
                    .unwrap_or(&info.app);
                if let Ok(out) = Command::new("pgrep").args(["-x", short_name]).output() {
                    if out.status.success() {
                        info.pid = String::from_utf8_lossy(&out.stdout)
                            .lines()
                            .next()
                            .unwrap_or("")
                            .trim()
                            .parse()
                            .ok();
                    }
                }
            }
            // 3: pgrep -f with short name
            if info.pid.is_none() && !info.app.is_empty() {
                let short_name = info
                    .app
                    .strip_prefix("org.kde.")
                    .or_else(|| info.app.strip_prefix("org.gnome."))
                    .or_else(|| info.app.strip_prefix("com."))
                    .or_else(|| info.app.strip_prefix("net."))
                    .unwrap_or(&info.app);
                if let Ok(out) = Command::new("pgrep").args(["-f", short_name]).output() {
                    if out.status.success() {
                        info.pid = String::from_utf8_lossy(&out.stdout)
                            .lines()
                            .next()
                            .unwrap_or("")
                            .trim()
                            .parse()
                            .ok();
                    }
                }
            }
            // 4: scan /proc
            if info.pid.is_none() && !info.app.is_empty() {
                let short_name = info
                    .app
                    .strip_prefix("org.kde.")
                    .or_else(|| info.app.strip_prefix("org.gnome."))
                    .or_else(|| info.app.strip_prefix("com."))
                    .or_else(|| info.app.strip_prefix("net."))
                    .unwrap_or(&info.app);
                if let Ok(entries) = std::fs::read_dir("/proc") {
                    for entry in entries.flatten() {
                        let fname = entry.file_name();
                        let fname_str = fname.to_string_lossy();
                        if fname_str.parse::<u32>().is_ok() {
                            let cmdline_path = format!("/proc/{fname_str}/cmdline");
                            if let Ok(cmdline) = std::fs::read_to_string(&cmdline_path) {
                                if cmdline.split('\0').any(|seg| seg.contains(short_name)) {
                                    info.pid = fname_str.parse().ok();
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            windows.push(info);
        }

        Ok(windows)
    }

    fn focus_window(&self, title_match: &str) -> Result<WindowMatch> {
        // Attempt 1: kdotool search
        let output = Command::new("kdotool")
            .args(["search", "--name", title_match])
            .output()
            .context("kdotool not found")?;

        let raw = String::from_utf8_lossy(&output.stdout);
        let candidates: Vec<String> = raw.lines().map(|l| l.trim().to_string()).collect();

        // Attempt 2: if no match or ambiguous, regex-filter by window title
        let fid = match candidates.first() {
            Some(id) => {
                // Verify this candidate actually matches by checking its title
                let info = self.get_active_window().ok().flatten();
                if let Some(ref wi) = info {
                    let combined = format!("{} {}", wi.title, wi.app).to_lowercase();
                    if combined.contains(&title_match.to_lowercase()) {
                        id.clone()
                    } else {
                        // Try to find a better match from list_windows
                        match self.list_windows() {
                            Ok(windows) => {
                                let lower = title_match.to_lowercase();
                                let better_id = windows
                                    .iter()
                                    .find(|w| {
                                        w.title.to_lowercase().contains(&lower)
                                            || w.app.to_lowercase().contains(&lower)
                                    })
                                    .map(|w| w.id.clone());
                                match better_id {
                                    Some(bid) => bid,
                                    None => id.clone(), // fallback to kdotool pick
                                }
                            }
                            Err(_) => id.clone(),
                        }
                    }
                } else {
                    id.clone()
                }
            }
            None => {
                return Ok(WindowMatch {
                    matched: false,
                    id: None,
                    title: None,
                    app: None,
                    candidates: Some(candidates),
                })
            }
        };

        let status = Command::new("kdotool")
            .args(["windowactivate", &fid])
            .output()
            .context("kdotool windowactivate failed")?;

        if !status.status.success() {
            bail!("windowactivate failed for {}", fid);
        }

        let active = self.get_active_window().ok().flatten();

        Ok(WindowMatch {
            matched: true,
            id: Some(fid),
            title: active.as_ref().map(|w| w.title.clone()),
            app: active.as_ref().and_then(|w| {
                if w.app.is_empty() {
                    None
                } else {
                    Some(w.app.clone())
                }
            }),
            candidates: Some(candidates.into_iter().take(10).collect()),
        })
    }

    fn open_app(&self, name: &str) -> Result<()> {
        let status = Command::new("kstart5")
            .arg(name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        if status.is_ok() && status.as_ref().unwrap().success() {
            return Ok(());
        }

        // Fallback: try direct binary
        Command::new(name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("open_app failed — neither kstart5 nor direct exec worked")?;

        Ok(())
    }

    // ── Screen size ────────────────────────────────────────
    fn get_screen_size(&self) -> Result<ScreenSize> {
        // Try kdotool first
        if let Ok(out) = Command::new("kdotool")
            .args(["getactivewindow", "getwindowgeometry"])
            .output()
        {
            if out.status.success() {
                let raw = String::from_utf8_lossy(&out.stdout);
                let mut w: u32 = 1920;
                let mut h: u32 = 1080;
                for line in raw.lines() {
                    let line = line.trim();
                    if let Some(val) = line.strip_prefix("Width: ") {
                        w = val.trim().parse().unwrap_or(1920);
                    } else if let Some(val) = line.strip_prefix("Height: ") {
                        h = val.trim().parse().unwrap_or(1080);
                    }
                }
                return Ok(ScreenSize {
                    width: w,
                    height: h,
                });
            }
        }
        // Fallback: try xrandr or just assume 1920x1080
        if let Ok(out) = Command::new("xrandr").output() {
            if out.status.success() {
                let raw = String::from_utf8_lossy(&out.stdout);
                for line in raw.lines() {
                    if line.contains(" connected") {
                        if let Some(res) = line.split_whitespace().find(|s| {
                            s.contains('x') && s.chars().filter(|c| *c == 'x').count() == 1
                        }) {
                            let parts: Vec<&str> = res.split('x').collect();
                            if parts.len() == 2 {
                                return Ok(ScreenSize {
                                    width: parts[0].parse().unwrap_or(1920),
                                    height: parts[1]
                                        .split('+')
                                        .next()
                                        .unwrap_or("1080")
                                        .parse()
                                        .unwrap_or(1080),
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(ScreenSize {
            width: 1920,
            height: 1080,
        })
    }

    // ── Shell ──────────────────────────────────────────────
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

    // ── Notify (uses notify-send CLI — reliable, never crashes) ──
    fn notify(&self, title: &str, message: &str, urgency: &str) -> Result<()> {
        let uval = match urgency {
            "critical" => "critical",
            "low" => "low",
            _ => "normal",
        };
        let _ = std::process::Command::new("notify-send")
            .arg("-u")
            .arg(uval)
            .arg(title)
            .arg(message)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        Ok(())
    }

    fn get_window_state(&self) -> Result<WindowState> {
        let active = self.get_active_window()?.context("no active window")?;
        crate::a11y::get_window_state(&active)
    }
}
