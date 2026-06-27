"""Auto-discovery engine — detects environment and available capabilities.

Runs in <20ms at import time. Only uses shutil.which(), os.environ, and
importlib.util.find_spec() — no heavy imports, no subprocess calls.
"""

import os
import shutil
import importlib.util
from dataclasses import dataclass, field
from typing import Optional


@dataclass
class BrowserInfo:
    binary: str
    path: str
    debugging_port: Optional[int] = None
    pid: Optional[int] = None
    profile_dir: Optional[str] = None


@dataclass
class Capabilities:
    display_type: str = "headless"  # "wayland", "x11", "headless"
    desktop: str = "unknown"  # "kde", "gnome", "sway", "hyprland", "unknown"
    provider: str = "headless"  # which provider to use

    # Tool availability
    screenshot: bool = False
    mouse: bool = False
    keyboard: bool = False
    windows: bool = False
    clipboard: bool = False
    notify: bool = False
    ocr: bool = False

    # Tools
    screenshot_tool: str = ""
    input_tool: str = ""
    window_tool: str = ""

    # Browsers
    browser_automation: str = "none"  # "playwright", "none"
    browsers: list = field(default_factory=list)
    discovered_browsers: list = field(default_factory=list)  # list of BrowserInfo

    # Other
    shell: bool = True
    xdg_runtime_dir: str = ""
    xdg_config_home: str = ""
    home_dir: str = ""


_caps: Optional[Capabilities] = None


def detect() -> Capabilities:
    """Detect environment. Returns cached result after first call."""
    global _caps
    if _caps is not None:
        return _caps

    caps = Capabilities()
    caps.home_dir = os.path.expanduser("~")
    caps.xdg_config_home = os.environ.get("XDG_CONFIG_HOME", os.path.join(caps.home_dir, ".config"))
    caps.xdg_runtime_dir = os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")

    # ── Display type ──────────────────────────────────────────
    if os.environ.get("WAYLAND_DISPLAY"):
        caps.display_type = "wayland"
    elif os.environ.get("DISPLAY"):
        caps.display_type = "x11"

    # ── Desktop environment ────────────────────────────────────
    xdg = os.environ.get("XDG_CURRENT_DESKTOP", "").lower()
    if "kde" in xdg:
        caps.desktop = "kde"
    elif "gnome" in xdg:
        caps.desktop = "gnome"
    elif "sway" in xdg:
        caps.desktop = "sway"
    elif "hyprland" in xdg:
        caps.desktop = "hyprland"
    elif "xfce" in xdg:
        caps.desktop = "xfce"
    elif "cinnamon" in xdg:
        caps.desktop = "cinnamon"

    # ── Input tools ────────────────────────────────────────────
    if caps.display_type == "wayland" and caps.desktop == "kde":
        caps.input_tool = "kdotool"
        caps.window_tool = "kdotool"
        caps.mouse = shutil.which("kdotool") is not None
        caps.keyboard = caps.mouse
        caps.windows = caps.mouse
    elif caps.display_type == "wayland":
        caps.input_tool = "ydotool"
        caps.window_tool = "atspi"  # AT-SPI for non-KDE Wayland
        caps.mouse = shutil.which("ydotool") is not None
        caps.keyboard = caps.mouse
        caps.windows = importlib.util.find_spec("pyatspi") is not None
    elif caps.display_type == "x11":
        caps.input_tool = "xdotool"
        caps.window_tool = "xdotool"
        caps.mouse = shutil.which("xdotool") is not None
        caps.keyboard = caps.mouse
        caps.windows = caps.mouse
    else:
        # Headless — check what's available
        if shutil.which("ydotool"):
            caps.input_tool = "ydotool"
            caps.mouse = True
            caps.keyboard = True

    # ── Screenshot ─────────────────────────────────────────────
    for cmd in ("spectacle", "grim", "scrot", "import", "gnome-screenshot"):
        if shutil.which(cmd):
            caps.screenshot = True
            caps.screenshot_tool = cmd
            break
    if not caps.screenshot and caps.display_type == "x11":
        if importlib.util.find_spec("mss"):
            caps.screenshot = True
            caps.screenshot_tool = "mss"

    # ── Clipboard ──────────────────────────────────────────────
    caps.clipboard = (
        (shutil.which("wl-copy") is not None and shutil.which("wl-paste") is not None)
        or (shutil.which("xclip") is not None)
    )

    # ── Notifications ──────────────────────────────────────────
    caps.notify = shutil.which("notify-send") is not None

    # ── OCR ────────────────────────────────────────────────────
    caps.ocr = importlib.util.find_spec("rapidocr_onnxruntime") is not None

    # ── Browser automation ─────────────────────────────────────
    if importlib.util.find_spec("playwright"):
        caps.browser_automation = "playwright"

    # ── Installed browsers ─────────────────────────────────────
    for binary in (
        "google-chrome-stable", "google-chrome", "chromium", "chromium-browser",
        "firefox", "firefox-esr", "brave", "brave-browser",
        "microsoft-edge", "opera", "vivaldi",
    ):
        path = shutil.which(binary)
        if path:
            caps.browsers.append(BrowserInfo(binary=binary, path=path))

    # ── Discover running browsers with debugging ports ─────────
    _discover_running_browsers(caps)

    # ── Determine provider ─────────────────────────────────────
    if caps.desktop == "kde" and caps.mouse:
        caps.provider = "wayland_kde"
    elif caps.display_type == "wayland" and caps.mouse:
        caps.provider = "wayland_wlr"
    elif caps.display_type == "x11" and caps.mouse:
        caps.provider = "x11"
    else:
        caps.provider = "headless"

    _caps = caps
    return caps


def _discover_running_browsers(caps: Capabilities) -> None:
    """Scan running processes for browsers with --remote-debugging-port."""
    try:
        import psutil
    except ImportError:
        return

    browser_names = {
        "chrome", "chromium", "chromium-browser",
        "google-chrome", "google-chrome-stable",
        "brave", "brave-browser", "edge", "opera", "vivaldi",
    }

    for proc in psutil.process_iter(["pid", "name", "cmdline"]):
        try:
            info = proc.info
            name = (info["name"] or "").lower()
            cmdline = info["cmdline"] or []

            # Match browser process
            if not any(bn in name for bn in browser_names):
                continue

            # Look for --remote-debugging-port
            for i, arg in enumerate(cmdline):
                if arg.startswith("--remote-debugging-port="):
                    port = int(arg.split("=", 1)[1])
                elif arg == "--remote-debugging-port" and i + 1 < len(cmdline):
                    try:
                        port = int(cmdline[i + 1])
                    except ValueError:
                        continue
                else:
                    continue

                bi = BrowserInfo(
                    binary=name,
                    path=shutil.which(name) or "",
                    debugging_port=port,
                    pid=info["pid"],
                )
                caps.discovered_browsers.append(bi)
                break
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            continue


def is_kde_wayland() -> bool:
    c = detect()
    return c.provider == "wayland_kde"


def is_headless() -> bool:
    return detect().provider == "headless"
