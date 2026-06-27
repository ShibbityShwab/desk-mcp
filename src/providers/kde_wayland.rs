//! KDE Wayland provider — uses spectacle, ydotool, kdotool, wl-clipboard.
//!
//! This is the primary provider for personal desktop use on KDE Plasma 6 Wayland.
//! All display-dependent operations shell out to battle-tested CLI tools.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use image::GenericImageView;

use super::*;

// ── Linux input key codes ─────────────────────────────────────
const KEYS: &[(&str, u16)] = &[
    ("esc", 1), ("escape", 1),
    ("1", 2), ("2", 3), ("3", 4), ("4", 5), ("5", 6), ("6", 7), ("7", 8), ("8", 9), ("9", 10), ("0", 11),
    ("-", 12), ("=", 13), ("backspace", 14),
    ("tab", 15),
    ("q", 16), ("w", 17), ("e", 18), ("r", 19), ("t", 20), ("y", 21), ("u", 22), ("i", 23), ("o", 24), ("p", 25),
    ("[", 26), ("]", 27), ("return", 28), ("enter", 28),
    ("a", 30), ("s", 31), ("d", 32), ("f", 33), ("g", 34), ("h", 35), ("j", 36), ("k", 37), ("l", 38),
    (";", 39), ("'", 40), ("`", 41), ("\\", 43),
    ("z", 44), ("x", 45), ("c", 46), ("v", 47), ("b", 48), ("n", 49), ("m", 50),
    (",", 51), (".", 52), ("/", 53),
    ("space", 57), (" ", 57),
    ("capslock", 58),
    ("f1", 59), ("f2", 60), ("f3", 61), ("f4", 62), ("f5", 63), ("f6", 64),
    ("f7", 65), ("f8", 66), ("f9", 67), ("f10", 68), ("f11", 87), ("f12", 88),
    ("home", 102), ("up", 103), ("pageup", 104), ("page_up", 104),
    ("left", 105), ("right", 106), ("end", 107), ("down", 108),
    ("pagedown", 109), ("page_down", 109), ("insert", 110), ("delete", 111),
    ("leftctrl", 29), ("leftshift", 42), ("leftalt", 56),
    ("rightctrl", 97), ("rightshift", 54), ("rightalt", 100),
    ("ctrl", 29), ("shift", 42), ("alt", 56), ("super", 125), ("meta", 125), ("win", 125),
    ("print", 99), ("pause", 119), ("menu", 139),
];

fn key_code(name: &str) -> Option<u16> {
    KEYS.iter().find(|(k, _)| *k == name).map(|(_, c)| *c)
}

#[derive(Default)]
pub struct KdeWaylandProvider;

impl ComputerProvider for KdeWaylandProvider {
    fn name(&self) -> &str { "wayland_kde" }

    // ── Screenshot ───────────────────────────────────────
    fn screenshot(&self, region: Option<(i32, i32, u32, u32)>) -> Result<Vec<u8>> {
        let tmp = tempfile::NamedTempFile::new()?;
        let path = tmp.path().to_string_lossy().to_string();

        let output = Command::new("spectacle")
            .args(["-b", "-n", "-f", "-o", &path])
            .output()
            .context("spectacle not found — install spectacle for KDE screenshots")?;

        if !output.status.success() {
            bail!("spectacle failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let img = image::open(&path)?;

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

    fn get_screen_size(&self) -> Result<ScreenSize> {
        let bytes = self.screenshot(None)?;
        let img = image::load_from_memory(&bytes)?;
        Ok(ScreenSize { width: img.width(), height: img.height() })
    }

    // ── Mouse ────────────────────────────────────────────
    fn mouse_move(&self, x: i32, y: i32, _smooth: bool, _duration_ms: u64) -> Result<()> {
        run("ydotool", &["mousemove", "-a", "--", &x.to_string(), &y.to_string()])
    }

    fn mouse_click(&self, button: &str, x: Option<i32>, y: Option<i32>, clicks: u32) -> Result<()> {
        let btn = match button {
            "left" => "0xC0", "right" => "0xC1", "middle" => "0xC2",
            _ => bail!("unknown button: {button}"),
        };

        if let (Some(x), Some(y)) = (x, y) {
            self.mouse_move(x, y, false, 0)?;
        }

        for _ in 0..clicks {
            run("ydotool", &["click", btn])?;
        }
        Ok(())
    }

    fn mouse_scroll(&self, _dx: i32, dy: i32, x: Option<i32>, y: Option<i32>) -> Result<()> {
        if let (Some(x), Some(y)) = (x, y) {
            self.mouse_move(x, y, false, 0)?;
        }
        if dy != 0 {
            let dir = if dy < 0 { "" } else { "-" };
            run("ydotool", &["bakers", "--wheel", &format!("{dir}{}", dy.abs())])?;
        }
        Ok(())
    }

    fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, _button: &str, _duration_ms: u64) -> Result<()> {
        self.mouse_move(x1, y1, false, 0)?;
        run("ydotool", &["click", "0xC0"])?; // press
        self.mouse_move(x2, y2, false, 0)?;
        run("ydotool", &["click", "0xC0"])?; // release
        Ok(())
    }

    // ── Keyboard ─────────────────────────────────────────
    fn keyboard_type(&self, text: &str, _delay_ms: u64) -> Result<()> {
        let mut child = Command::new("ydotool")
            .args(["type", "--"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        let output = child.wait_with_output()?;
        if !output.status.success() {
            bail!("ydotool type failed: {}", String::from_utf8_lossy(&output.stderr));
        }
        Ok(())
    }

    fn key_press(&self, key: &str) -> Result<()> {
        let parts: Vec<&str> = key.split('+').map(|s| s.trim()).collect();
        let (mods, main) = parts.split_at(parts.len().saturating_sub(1));
        let main = main.first().copied().unwrap_or("");

        let mut presses = Vec::new();
        let mut releases = Vec::new();

        for m in mods {
            let code = key_code(&m.to_lowercase())
                .ok_or_else(|| anyhow::anyhow!("unknown modifier: {m}"))?;
            presses.push(code);
            releases.push(code);
        }

        let main_code = key_code(&main.to_lowercase())
            .or_else(|| {
                let c = if main.len() == 1 {
                    main.chars().next().unwrap().to_uppercase().to_string()
                } else {
                    main.to_string()
                };
                key_code(&c.to_lowercase())
            })
            .ok_or_else(|| anyhow::anyhow!("unknown key: {main}"))?;

        presses.push(main_code);
        releases.push(main_code);

        let mut args: Vec<String> = Vec::new();
        args.push("key".into());
        for c in &presses { args.push(format!("{c}:1")); }
        for c in releases.iter().rev() { args.push(format!("{c}:0")); }

        run("ydotool", &args.iter().map(|s| s.as_str()).collect::<Vec<_>>())
    }

    // ── Clipboard ────────────────────────────────────────
    fn clipboard_get(&self) -> Result<String> {
        if which::which("wl-paste").is_ok() {
            Ok(run_stdout("wl-paste", &["-n"])?)
        } else if which::which("xclip").is_ok() {
            Ok(run_stdout("xclip", &["-selection", "clipboard", "-o"])?)
        } else {
            bail!("No clipboard tool (install wl-clipboard or xclip)")
        }
    }

    fn clipboard_set(&self, text: &str) -> Result<()> {
        if which::which("wl-copy").is_ok() {
            let mut child = Command::new("wl-copy")
                .stdin(Stdio::piped())
                .spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()?;
        } else if which::which("xclip").is_ok() {
            let mut child = Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()?;
        } else {
            bail!("No clipboard tool");
        }
        Ok(())
    }

    // ── Shell ────────────────────────────────────────────
    fn shell_run(&self, command: &str, _timeout_secs: u64) -> Result<ShellResult> {
        let output = Command::new("sh")
            .args(["-c", command])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn shell")?
            .wait_with_output()
            .context("shell command failed")?;

        let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if stdout.len() > 8000 { stdout.truncate(8000); stdout.push_str("\n... (truncated)"); }
        if stderr.len() > 4000 { stderr.truncate(4000); stderr.push_str("\n... (truncated)"); }

        Ok(ShellResult {
            returncode: output.status.code().unwrap_or(-1),
            stdout,
            stderr,
        })
    }

    // ── Windows (kdotool) ────────────────────────────────
    fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        let wids = run_stdout("kdotool", &["search", "--limit", "0", ".*"])?;
        let mut windows = Vec::new();

        for wid in wids.lines() {
            let wid = wid.trim();
            if wid.is_empty() { continue; }

            let title = run_kdotool("getwindowname", wid);
            let app = run_kdotool("getwindowclassname", wid);
            let pid_str = run_kdotool("getwindowpid", wid);
            let geom_str = run_kdotool("getwindowgeometry", wid);

            let pid = pid_str.parse::<u32>().ok();
            let geometry = parse_kdotool_geometry(&geom_str);

            windows.push(WindowInfo {
                id: wid.to_string(),
                title,
                app,
                pid,
                geometry,
            });
        }

        Ok(windows)
    }

    fn focus_window(&self, title_match: &str) -> Result<WindowMatch> {
        let needle = title_match.to_lowercase();
        let all = self.list_windows().unwrap_or_default();

        for w in &all {
            if w.title.to_lowercase().contains(&needle) || w.app.to_lowercase().contains(&needle) {
                let _ = run_kdotool("windowactivate", &w.id);
                return Ok(WindowMatch {
                    matched: true,
                    id: Some(w.id.clone()),
                    title: Some(w.title.clone()),
                    app: Some(w.app.clone()),
                    candidates: None,
                });
            }
        }

        Ok(WindowMatch {
            matched: false,
            id: None,
            title: None,
            app: None,
            candidates: Some(all.iter().map(|w| w.title.clone()).collect()),
        })
    }

    fn get_active_window(&self) -> Result<Option<WindowInfo>> {
        let wid = run_kdotool("getactivewindow", "");
        if wid.is_empty() { return Ok(None); }

        let title = run_kdotool("getwindowname", &wid);
        let app = run_kdotool("getwindowclassname", &wid);
        let pid_str = run_kdotool("getwindowpid", &wid);
        let geom_str = run_kdotool("getwindowgeometry", &wid);

        Ok(Some(WindowInfo {
            id: wid,
            title,
            app,
            pid: pid_str.parse().ok(),
            geometry: parse_kdotool_geometry(&geom_str),
        }))
    }

    // ── Apps / Notifications ─────────────────────────────
    fn open_app(&self, app_name: &str) -> Result<()> {
        // Try kdotool search first
        let found = run_kdotool("search", &format!("--class {app_name}"));
        if !found.is_empty() {
            let wid = found.lines().next().unwrap_or("");
            if !wid.is_empty() {
                let _ = run_kdotool("windowactivate", wid);
                return Ok(());
            }
        }
        // Fallback: launch
        Command::new(app_name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    }

    fn notify(&self, title: &str, message: &str, urgency: &str) -> Result<()> {
        run("notify-send", &["-u", urgency, title, message])
    }
}

// ── Helpers ──────────────────────────────────────────────────
fn run(cmd: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(cmd)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run {cmd}"))?;

    if !output.status.success() {
        bail!("{cmd} failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

fn run_stdout(cmd: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .with_context(|| format!("failed to run {cmd}"))?;

    if !output.status.success() {
        bail!("{cmd} failed");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_kdotool(cmd: &str, arg: &str) -> String {
    let args: Vec<&str> = if arg.is_empty() {
        vec![cmd]
    } else {
        vec![cmd, arg]
    };
    Command::new("kdotool")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn parse_kdotool_geometry(text: &str) -> WindowGeometry {
    let mut geom = WindowGeometry { x: 0, y: 0, width: 0, height: 0 };

    for line in text.lines() {
        let line = line.trim();
        if let Some(pos) = line.strip_prefix("Position: ") {
            let parts: Vec<&str> = pos.split(',').collect();
            if parts.len() == 2 {
                geom.x = parts[0].trim().parse().unwrap_or(0);
                geom.y = parts[1].trim().parse().unwrap_or(0);
            }
        } else if let Some(size) = line.strip_prefix("Geometry: ") {
            let parts: Vec<&str> = size.split('x').collect();
            if parts.len() == 2 {
                geom.width = parts[0].trim().parse().unwrap_or(0);
                geom.height = parts[1].trim().parse().unwrap_or(0);
            }
        }
    }

    geom
}
