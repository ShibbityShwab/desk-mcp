//! Auto-discovery engine — detects environment and available capabilities.
//!
//! Runs in <1ms — only checks env vars, filesystem paths, and process lists.
//! No heavy imports, no network calls.

use std::env;

/// Information about a discovered browser
#[derive(Debug, Clone, serde::Serialize)]
pub struct BrowserInfo {
    pub binary: String,
    pub path: String,
    pub debugging_port: Option<u16>,
    pub pid: Option<u32>,
}

/// System capabilities detected at startup
#[derive(Debug, Clone, serde::Serialize)]
pub struct Capabilities {
    pub display_type: String,    // "wayland", "x11", "headless"
    pub desktop: String,         // "kde", "gnome", "sway", "unknown"
    pub provider: String,        // which provider to use

    // Tool availability
    pub screenshot: bool,
    pub mouse: bool,
    pub keyboard: bool,
    pub windows: bool,
    pub clipboard: bool,
    pub notify: bool,
    pub ocr: bool,

    // Tools
    pub screenshot_tool: String,
    pub input_tool: String,
    pub window_tool: String,

    // Browser automation
    pub browser_automation: String, // "chromiumoxide", "none"
    pub installed_browsers: Vec<BrowserInfo>,
    pub discovered_browsers: Vec<BrowserInfo>,

    // Environment
    pub home_dir: String,
    pub xdg_runtime_dir: String,
}

/// Detect the environment. Called once at startup.
pub fn detect() -> Capabilities {
    let home = env::var("HOME").unwrap_or_else(|_| "/root".into());
    let _xdg_config = env::var("XDG_CONFIG_HOME")
        .unwrap_or_else(|_| format!("{home}/.config"));
    let xdg_runtime = env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));

    // ── Display type ──────────────────────────────────────
    let wayland = env::var("WAYLAND_DISPLAY").ok();
    let x_display = env::var("DISPLAY").ok();
    let display_type = if wayland.is_some() { "wayland" } else if x_display.is_some() { "x11" } else { "headless" };

    // ── Desktop environment ────────────────────────────────
    let xdg_desktop = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_lowercase();
    let desktop = if xdg_desktop.contains("kde") { "kde" }
        else if xdg_desktop.contains("gnome") { "gnome" }
        else if xdg_desktop.contains("sway") { "sway" }
        else if xdg_desktop.contains("hyprland") { "hyprland" }
        else { "unknown" };

    // ── Input tool detection ──────────────────────────────
    let has_kdotool = which::which("kdotool").is_ok();
    let has_ydotool = which::which("ydotool").is_ok();
    let has_xdotool = which::which("xdotool").is_ok();

    let (mouse, keyboard, input_tool) = if display_type == "wayland" && desktop == "kde" && has_kdotool {
        (true, true, "kdotool".into())
    } else if has_ydotool {
        (true, true, "ydotool".into())
    } else if has_xdotool {
        (true, true, "xdotool".into())
    } else {
        (false, false, "none".into())
    };

    // ── Screenshot tool ───────────────────────────────────
    let screenshot_tools = ["spectacle", "grim", "scrot", "import", "gnome-screenshot"];
    let screenshot_tool = screenshot_tools.iter()
        .find(|t| which::which(t).is_ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "none".into());
    let screenshot = screenshot_tool != "none";

    // ── Window tool ───────────────────────────────────────
    let window_tool = if has_kdotool { "kdotool" }
        else if has_xdotool { "xdotool" }
        else { "none" };
    let windows = window_tool != "none";

    // ── Clipboard ─────────────────────────────────────────
    let clipboard = which::which("wl-paste").is_ok() && which::which("wl-copy").is_ok()
        || which::which("xclip").is_ok();

    // ── Notifications ─────────────────────────────────────
    let notify = which::which("notify-send").is_ok();

    // ── OCR ───────────────────────────────────────────────
    let ocr = which::which("tesseract").is_ok();

    // ── Browser automation ────────────────────────────────
    let browser_automation = "chromiumoxide"; // Always available since it's compiled in

    // ── Installed browsers ────────────────────────────────
    let browser_binaries = [
        "google-chrome-stable", "google-chrome", "chromium", "chromium-browser",
        "firefox", "firefox-esr", "brave", "brave-browser", "microsoft-edge",
    ];
    let installed_browsers: Vec<_> = browser_binaries.iter()
        .filter_map(|b| which::which(b).ok().map(|p| BrowserInfo {
            binary: b.to_string(),
            path: p.to_string_lossy().to_string(),
            debugging_port: None,
            pid: None,
        }))
        .collect();

    // ── Discover running browsers with debugging ports ─────
    let discovered_browsers = discover_running_browsers();

    // ── Determine provider ────────────────────────────────
    let provider = if desktop == "kde" && mouse { "wayland_kde" }
        else if display_type == "wayland" && mouse { "wayland_wlr" }
        else if display_type == "x11" && mouse { "x11" }
        else { "headless" };

    Capabilities {
        display_type: display_type.into(),
        desktop: desktop.into(),
        provider: provider.into(),
        screenshot, mouse, keyboard, windows, clipboard, notify, ocr,
        screenshot_tool, input_tool, window_tool: window_tool.into(),
        browser_automation: browser_automation.into(),
        installed_browsers,
        discovered_browsers,
        home_dir: home,
        xdg_runtime_dir: xdg_runtime,
    }
}

/// Scan /proc for browser processes with --remote-debugging-port
fn discover_running_browsers() -> Vec<BrowserInfo> {
    let browser_names = [
        "chrome", "chromium", "google-chrome", "brave", "edge", "opera", "vivaldi",
    ];

    let mut found = Vec::new();

    // Read /proc/*/cmdline to find browser processes with debugging ports
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let path = entry.path();
            let pid_str = path.file_name().unwrap_or_default().to_string_lossy();
            let pid: u32 = match pid_str.parse() { Ok(p) => p, Err(_) => continue };

            let cmdline_path = path.join("cmdline");
            let cmdline = match std::fs::read(&cmdline_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Parse null-delimited cmdline
            let args: Vec<&str> = cmdline.split(|b| *b == 0)
                .filter_map(|s| std::str::from_utf8(s).ok())
                .collect();

            if args.is_empty() { continue; }

            let proc_name = std::path::Path::new(args[0])
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();

            if !browser_names.iter().any(|n| proc_name.contains(n)) {
                continue;
            }

            // Look for --remote-debugging-port
            let mut port: Option<u16> = None;
            for (i, arg) in args.iter().enumerate() {
                if let Some(p) = arg.strip_prefix("--remote-debugging-port=") {
                    port = p.parse().ok();
                } else if *arg == "--remote-debugging-port" && i + 1 < args.len() {
                    port = args[i + 1].parse().ok();
                }
            }

            if let Some(port) = port {
                let binary = which::which(&proc_name)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| args[0].to_string());

                found.push(BrowserInfo {
                    binary: proc_name,
                    path: binary,
                    debugging_port: Some(port),
                    pid: Some(pid),
                });
            }
        }
    }

    found
}
