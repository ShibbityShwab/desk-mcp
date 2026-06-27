"""KDE Wayland provider — uses kdotool, spectacle, ydotool, wl-clipboard.

This is the primary provider for personal desktop use on KDE Plasma 6 Wayland.
"""

import os
import re
import shutil
import subprocess
import tempfile
import time
from io import BytesIO
from typing import Optional

from PIL import Image
from ._base import ComputerProvider

# ydotool socket — required for keyboard/mouse on Wayland
_YDOTOOL_SOCKET = os.environ.get(
    "YDOTOOL_SOCKET", f"/run/user/{os.getuid()}/ydotoold.sock"
)
os.environ.setdefault("YDOTOOL_SOCKET", _YDOTOOL_SOCKET)


# ── Linux input key codes ─────────────────────────────────────
KEYS = {
    "esc": 1, "escape": 1,
    "1": 2, "2": 3, "3": 4, "4": 5, "5": 6, "6": 7, "7": 8, "8": 9, "9": 10, "0": 11,
    "-": 12, "=": 13, "backspace": 14,
    "tab": 15,
    "q": 16, "w": 17, "e": 18, "r": 19, "t": 20, "y": 21, "u": 22, "i": 23, "o": 24, "p": 25,
    "[": 26, "]": 27, "return": 28, "enter": 28,
    "a": 30, "s": 31, "d": 32, "f": 33, "g": 34, "h": 35, "j": 36, "k": 37, "l": 38,
    ";": 39, "'": 40, "`": 41, "\\": 43,
    "z": 44, "x": 45, "c": 46, "v": 47, "b": 48, "n": 49, "m": 50,
    ",": 51, ".": 52, "/": 53,
    "space": 57, " ": 57,
    "capslock": 58,
    "f1": 59, "f2": 60, "f3": 61, "f4": 62, "f5": 63, "f6": 64,
    "f7": 65, "f8": 66, "f9": 67, "f10": 68, "f11": 87, "f12": 88,
    "home": 102, "up": 103, "pageup": 104, "page_up": 104,
    "left": 105, "right": 106, "end": 107, "down": 108,
    "pagedown": 109, "page_down": 109, "insert": 110, "delete": 111,
    "leftctrl": 29, "leftshift": 42, "leftalt": 56,
    "rightctrl": 97, "rightshift": 54, "rightalt": 100,
    "ctrl": 29, "shift": 42, "alt": 56, "super": 125, "meta": 125, "win": 125,
    "print": 99, "pause": 119, "menu": 139,
}

_GEOM_RE = re.compile(r"Position:\s*(-?\d+),(-?\d+)\s*Geometry:\s*(\d+)x(\d+)", re.S)


def _run(args: list[str], timeout: int = 10, check: bool = False) -> subprocess.CompletedProcess:
    return subprocess.run(args, capture_output=True, text=True, timeout=timeout, check=check)


class KDEWaylandProvider(ComputerProvider):
    name = "wayland_kde"

    # ── Screenshot ───────────────────────────────────────────
    def screenshot(self, region: Optional[tuple] = None) -> bytes:
        """Capture via spectacle. Returns PNG bytes."""
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            tmp_path = f.name
        try:
            proc = _run(
                ["spectacle", "-b", "-n", "-f", "-o", tmp_path],
                timeout=10,
            )
            if proc.returncode != 0 or not os.path.exists(tmp_path):
                raise RuntimeError(f"spectacle failed: {proc.stderr or proc.returncode}")

            img = Image.open(tmp_path).convert("RGB")

            if region and len(region) == 4:
                x, y, w, h = map(int, region)
                x = max(0, min(x, img.width - 1))
                y = max(0, min(y, img.height - 1))
                w = max(1, min(w, img.width - x))
                h = max(1, min(h, img.height - y))
                img = img.crop((x, y, x + w, y + h))

            buf = BytesIO()
            img.save(buf, format="PNG")
            return buf.getvalue()
        finally:
            try:
                os.unlink(tmp_path)
            except FileNotFoundError:
                pass

    def get_screen_size(self) -> dict:
        img_bytes = self.screenshot()
        img = Image.open(BytesIO(img_bytes))
        return {"width": img.width, "height": img.height}

    # ── Mouse ────────────────────────────────────────────────
    def mouse_move(self, x: int, y: int, smooth: bool = False, duration_ms: int = 200) -> None:
        if smooth:
            # Interpolate movement
            steps = max(1, duration_ms // 16)  # ~60fps
            # Get current position — we skip this optimization and just go direct
        _run(["ydotool", "mousemove", "-a", "--", str(x), str(y)])

    def mouse_click(self, button: str = "left", x: Optional[int] = None,
                    y: Optional[int] = None, clicks: int = 1) -> None:
        btn_map = {"left": "0xC0", "right": "0xC1", "middle": "0xC2"}
        btn = btn_map.get(button, "0xC0")

        if x is not None and y is not None:
            _run(["ydotool", "mousemove", "-a", "--", str(x), str(y)])

        for _ in range(clicks):
            _run(["ydotool", "click", btn])

    def mouse_scroll(self, dx: int = 0, dy: int = 0,
                     x: Optional[int] = None, y: Optional[int] = None) -> None:
        if x is not None and y is not None:
            _run(["ydotool", "mousemove", "-a", "--", str(x), str(y)])

        # ydotool bakers --wheel handles scrolling
        if dy != 0:
            sign = "" if dy < 0 else "-"
            _run(["ydotool", "bakers", "--wheel", f"{sign}{abs(dy)}"])

    def mouse_drag(self, x1: int, y1: int, x2: int, y2: int,
                   button: str = "left", duration_ms: int = 500) -> None:
        _run(["ydotool", "mousemove", "-a", "--", str(x1), str(y1)])
        _run(["ydotool", "click", "0xC0"])  # press
        steps = max(1, duration_ms // 16)
        for i in range(1, steps + 1):
            ix = x1 + (x2 - x1) * i // steps
            iy = y1 + (y2 - y1) * i // steps
            _run(["ydotool", "mousemove", "-a", "--", str(ix), str(iy)])
        _run(["ydotool", "click", "0xC0"])  # release

    # ── Keyboard ─────────────────────────────────────────────
    def keyboard_type(self, text: str, delay_ms: int = 10) -> None:
        proc = subprocess.run(
            ["ydotool", "type", "--"],
            input=text, capture_output=True, text=True, timeout=30,
        )
        if proc.returncode != 0:
            raise RuntimeError(f"ydotool type failed: {proc.stderr}")

    def _key_combo(self, key: str) -> None:
        """Parse 'ctrl+shift+t' → press modifiers + key → release reverse."""
        parts = [p.strip() for p in key.split("+")]
        mods = parts[:-1]
        main = parts[-1]
        presses, releases = [], []

        for m in mods:
            ml = m.lower()
            if ml not in KEYS:
                raise ValueError(f"unknown modifier: {m}")
            presses.append(KEYS[ml])
            releases.append(KEYS[ml])

        # Try to resolve the main key
        ml = main.lower()
        if ml in KEYS:
            presses.append(KEYS[ml])
            releases.append(KEYS[ml])
        elif len(main) == 1:
            # Single char — try uppercase, then lowercase
            c = main.capitalize() if not main.isupper() else main
            if c in KEYS:
                presses.append(KEYS[c])
                releases.append(KEYS[c])
            elif main.lower() in KEYS:
                presses.append(KEYS[main.lower()])
                releases.append(KEYS[main.lower()])
            else:
                raise ValueError(f"unknown key: {main}")
        else:
            c = main.capitalize()
            if c in KEYS:
                presses.append(KEYS[c])
                releases.append(KEYS[c])
            else:
                raise ValueError(f"unknown key: {main}")

        args = [f"{c}:1" for c in presses] + [f"{c}:0" for c in reversed(releases)]
        _run(["ydotool", "key"] + args)

    def key_press(self, key: str) -> None:
        self._key_combo(key)

    # ── Clipboard ────────────────────────────────────────────
    def clipboard_get(self) -> str:
        if shutil.which("wl-paste"):
            proc = _run(["wl-paste", "-n"], check=True)
            return proc.stdout
        elif shutil.which("xclip"):
            proc = _run(["xclip", "-selection", "clipboard", "-o"], check=True)
            return proc.stdout
        raise RuntimeError("No clipboard tool available (install wl-clipboard or xclip)")

    def clipboard_set(self, text: str) -> None:
        if shutil.which("wl-copy"):
            subprocess.run(["wl-copy"], input=text.encode(), check=True)
        elif shutil.which("xclip"):
            p = subprocess.Popen(["xclip", "-selection", "clipboard"], stdin=subprocess.PIPE)
            p.communicate(input=text.encode())
        else:
            raise RuntimeError("No clipboard tool available")

    # ── Shell ────────────────────────────────────────────────
    def shell_run(self, command: str, timeout: int = 30) -> dict:
        try:
            proc = subprocess.run(
                command, shell=True, capture_output=True, text=True,
                timeout=timeout,
            )
            out, err = proc.stdout, proc.stderr
            out_limit, err_limit = 8000, 4000
            if len(out) > out_limit:
                out = out[:out_limit] + f"\n... (truncated {len(proc.stdout) - out_limit} bytes)"
            if len(err) > err_limit:
                err = err[:err_limit] + f"\n... (truncated {len(proc.stderr) - err_limit} bytes)"
            return {"returncode": proc.returncode, "stdout": out, "stderr": err}
        except subprocess.TimeoutExpired:
            raise RuntimeError(f"Command timed out after {timeout}s")

    # ── Windows (kdotool) ────────────────────────────────────
    def _kdotool(self, cmd: str, *args: str) -> str:
        try:
            p = _run(["kdotool", cmd, *args], timeout=5)
            return p.stdout.strip() if p.returncode == 0 else ""
        except Exception:
            return ""

    def _parse_geometry(self, text: str) -> dict:
        m = _GEOM_RE.search(text)
        if not m:
            return {"x": 0, "y": 0, "width": 0, "height": 0}
        return {
            "x": int(m.group(1)), "y": int(m.group(2)),
            "width": int(m.group(3)), "height": int(m.group(4)),
        }

    def list_windows(self) -> list[dict]:
        wids = self._kdotool("search", "--limit", "0", ".*").splitlines()
        windows = []
        for wid in wids:
            wid = wid.strip()
            if not wid:
                continue
            windows.append({
                "id": wid,
                "title": self._kdotool("getwindowname", wid),
                "app": self._kdotool("getwindowclassname", wid),
                "pid": int(self._kdotool("getwindowpid", wid) or 0) or None,
                "geometry": self._parse_geometry(self._kdotool("getwindowgeometry", wid)),
            })
        return windows

    def focus_window(self, title_match: str) -> dict:
        needle = title_match.lower()
        for w in self.list_windows():
            title = (w["title"] or "").lower()
            app = (w["app"] or "").lower()
            if needle in title or needle in app:
                self._kdotool("windowactivate", w["id"])
                return {"matched": True, **w}
        return {"matched": False, "query": title_match, "candidates": [w["title"] for w in self.list_windows()]}

    def get_active_window(self) -> Optional[dict]:
        wid = self._kdotool("getactivewindow")
        if not wid:
            return None
        return {
            "id": wid,
            "title": self._kdotool("getwindowname", wid),
            "app": self._kdotool("getwindowclassname", wid),
            "pid": int(self._kdotool("getwindowpid", wid) or 0) or None,
            "geometry": self._parse_geometry(self._kdotool("getwindowgeometry", wid)),
        }

    # ── Apps / Notifications ─────────────────────────────────
    def open_app(self, app_name: str) -> None:
        # Try kdotool first, fallback to CLI
        out = self._kdotool("search", "--class", app_name)
        if out:
            self._kdotool("windowactivate", out.splitlines()[0].strip())
            return
        # Fallback: launch via shell
        subprocess.Popen([app_name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    def notify(self, title: str, message: str, urgency: str = "normal") -> None:
        if shutil.which("notify-send"):
            _run(["notify-send", "-u", urgency, title, message])
