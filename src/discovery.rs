//! Auto-discovery engine — detects environment and available capabilities.
//!
//! Runs detection exactly once via `OnceLock`. Subsequent calls are O(1) cache hits.
//! Call `refresh_discovery()` to force a re-scan if the environment changes.

use std::sync::OnceLock;

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
    pub display_type: String,
    pub desktop: String,
    pub provider: String,
    pub screenshot: bool,
    pub mouse: bool,
    pub keyboard: bool,
    pub windows: bool,
    pub clipboard: bool,
    pub notify: bool,
    pub ocr: bool,
    pub screenshot_tool: String,
    pub input_tool: String,
    pub window_tool: String,
    pub browser_automation: String,
    pub installed_browsers: Vec<BrowserInfo>,
    pub discovered_browsers: Vec<BrowserInfo>,
    pub home_dir: String,
    pub xdg_runtime_dir: String,
}

/// Cached detection result — computed once, reused thereafter
static DETECTED: OnceLock<Capabilities> = OnceLock::new();

/// Detect the environment. Returns a cached result after the first call.
pub fn detect() -> &'static Capabilities {
    DETECTED.get_or_init(detect_impl)
}

/// Rescan running browsers and validate each candidate by hitting
/// the Chrome DevTools Protocol HTTP endpoint (`/json/version`).
///
/// Unlike `detect()`, this does **not** use the cached result — it
/// rescans `/proc` on every call and validates ports via HTTP, so
/// browsers launched *after* server start are discovered immediately.
pub fn refresh_browsers() -> Vec<BrowserInfo> {
    let candidates = discover_running_browsers();
    candidates
        .into_iter()
        .filter(|info| {
            if let Some(port) = info.debugging_port {
                let url = format!("http://localhost:{port}/json/version");
                match reqwest::blocking::get(&url) {
                    Ok(resp) => resp.status().is_success(),
                    Err(_) => false,
                }
            } else {
                false
            }
        })
        .collect()
}

/// Force a fresh re-scan of the environment. Useful after installing
/// a dependency while the server is running.
pub fn refresh_discovery() {
    // OnceLock doesn't support clearing after init in stable Rust.
    // We use a new OnceLock via an atomic pointer swap.
    // This is safe because:
    // 1. desk-mcp is single-binary, no hot reload
    // 2. This is only called from MCP tool dispatch (sequential within a request)
    use std::sync::atomic::{AtomicPtr, Ordering};
    static PTR: AtomicPtr<OnceLock<Capabilities>> =
        AtomicPtr::new(&DETECTED as *const OnceLock<Capabilities> as *mut OnceLock<Capabilities>);
    let new_lock = Box::new(OnceLock::new());
    let _ = new_lock.set(detect_impl());
    let new_ptr = Box::into_raw(new_lock);
    let old_ptr = PTR.swap(new_ptr, Ordering::Release);
    // SAFETY: we leak the old OnceLock — negligible memory cost
    unsafe {
        let _ = Box::from_raw(old_ptr);
    }
}

fn detect_impl() -> Capabilities {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let xdg_runtime = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));

    let wayland = std::env::var("WAYLAND_DISPLAY").ok();
    let x_display = std::env::var("DISPLAY").ok();
    let display_type = if wayland.is_some() {
        "wayland"
    } else if x_display.is_some() {
        "x11"
    } else {
        "headless"
    };

    let xdg_desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_lowercase();
    let desktop = if xdg_desktop.contains("kde") {
        "kde"
    } else if xdg_desktop.contains("gnome") {
        "gnome"
    } else if xdg_desktop.contains("sway") {
        "sway"
    } else if xdg_desktop.contains("hyprland") {
        "hyprland"
    } else {
        "unknown"
    };

    let has_kdotool = which::which("kdotool").is_ok();
    let has_ydotool = which::which("ydotool").is_ok();
    let has_xdotool = which::which("xdotool").is_ok();

    let (mouse, keyboard, input_tool) =
        if display_type == "wayland" && desktop == "kde" && has_kdotool {
            (true, true, "kdotool".into())
        } else if has_ydotool {
            (true, true, "ydotool".into())
        } else if has_xdotool {
            (true, true, "xdotool".into())
        } else {
            (false, false, "none".into())
        };

    let screenshot_tools = ["spectacle", "grim", "scrot", "import", "gnome-screenshot"];
    let screenshot_tool = screenshot_tools
        .iter()
        .find(|t| which::which(t).is_ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "none".into());
    let screenshot = screenshot_tool != "none";

    let window_tool = if has_kdotool {
        "kdotool"
    } else if has_xdotool {
        "xdotool"
    } else {
        "none"
    };
    let windows = window_tool != "none";

    let clipboard = which::which("wl-paste").is_ok() && which::which("wl-copy").is_ok()
        || which::which("xclip").is_ok();

    let notify = which::which("notify-send").is_ok();
    let ocr = true; // pure-Rust `ocrs` crate — always available
    let browser_automation = "chromiumoxide";

    let browser_binaries = [
        "google-chrome-stable",
        "google-chrome",
        "chromium",
        "chromium-browser",
        "firefox",
        "firefox-esr",
        "brave",
        "brave-browser",
        "microsoft-edge",
    ];
    let installed_browsers: Vec<_> = browser_binaries
        .iter()
        .filter_map(|b| {
            which::which(b).ok().map(|p| BrowserInfo {
                binary: b.to_string(),
                path: p.to_string_lossy().to_string(),
                debugging_port: None,
                pid: None,
            })
        })
        .collect();

    let discovered_browsers = discover_running_browsers();

    let provider = if desktop == "kde" && mouse {
        "wayland_kde"
    } else if display_type == "wayland" && mouse {
        "wayland_wlr"
    } else if display_type == "x11" && mouse {
        "x11"
    } else {
        "headless"
    };

    Capabilities {
        display_type: display_type.into(),
        desktop: desktop.into(),
        provider: provider.into(),
        screenshot,
        mouse,
        keyboard,
        windows,
        clipboard,
        notify,
        ocr,
        screenshot_tool,
        input_tool,
        window_tool: window_tool.into(),
        browser_automation: browser_automation.into(),
        installed_browsers,
        discovered_browsers,
        home_dir: home,
        xdg_runtime_dir: xdg_runtime,
    }
}

fn discover_running_browsers() -> Vec<BrowserInfo> {
    let browser_names = [
        "chrome",
        "chromium",
        "google-chrome",
        "brave",
        "edge",
        "opera",
        "vivaldi",
    ];

    let mut found = Vec::new();

    // Prefer pgrep(1) when available — avoids walking hundreds of /proc entries
    // and exhausting file descriptors on machines with many processes.
    if which::which("pgrep").is_ok() {
        for name in &browser_names {
            if let Ok(output) = std::process::Command::new("pgrep")
                .arg("-x")
                .arg(name)
                .output()
            {
                if !output.status.success() {
                    continue;
                }
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    let pid: u32 = match line.parse() {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    // Read cmdline to find debugging port
                    let cmdline_path = format!("/proc/{pid}/cmdline");
                    let cmdline = match std::fs::read(&cmdline_path) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    let args: Vec<&str> = cmdline
                        .split(|b| *b == 0)
                        .filter_map(|s| std::str::from_utf8(s).ok())
                        .collect();
                    if args.is_empty() {
                        continue;
                    }

                    let mut port: Option<u16> = None;
                    for (i, arg) in args.iter().enumerate() {
                        if let Some(p) = arg.strip_prefix("--remote-debugging-port=") {
                            port = p.parse().ok();
                        } else if *arg == "--remote-debugging-port" && i + 1 < args.len() {
                            port = args[i + 1].parse().ok();
                        }
                    }

                    if let Some(port) = port {
                        let proc_name = std::path::Path::new(args[0])
                            .file_name()
                            .map(|n| n.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        let binary = which::which(name)
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
        }
        return found;
    }

    // Fallback: walk /proc directly
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let path = entry.path();
            let pid_str = path.file_name().unwrap_or_default().to_string_lossy();
            let pid: u32 = match pid_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let cmdline_path = path.join("cmdline");
            let cmdline = match std::fs::read(&cmdline_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let args: Vec<&str> = cmdline
                .split(|b| *b == 0)
                .filter_map(|s| std::str::from_utf8(s).ok())
                .collect();

            if args.is_empty() {
                continue;
            }

            let proc_name = std::path::Path::new(args[0])
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();

            if !browser_names.iter().any(|n| proc_name.contains(n)) {
                continue;
            }

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
