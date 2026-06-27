"""Headless provider — graceful degradation when no display is available.

Returns NOT_IMPLEMENTED errors for display-dependent tools.
Some tools (shell_run, clipboard, notify) may still work if their deps are available.
"""

import os
import shutil
import subprocess
from typing import Optional

from ._base import ComputerProvider


class HeadlessProvider(ComputerProvider):
    name = "headless"

    def _not_available(self, tool: str) -> None:
        raise RuntimeError(f"{tool} not available in headless mode (no display server)")

    def screenshot(self, region: Optional[tuple] = None) -> bytes:
        # Try Xvfb screenshot if DISPLAY is set
        display = os.environ.get("DISPLAY", "")
        if display and shutil.which("import"):
            import tempfile
            with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
                tmp = f.name
            try:
                subprocess.run(["import", "-window", "root", tmp], check=True, timeout=5)
                with open(tmp, "rb") as f:
                    return f.read()
            finally:
                try:
                    os.unlink(tmp)
                except FileNotFoundError:
                    pass
        self._not_available("screenshot")

    def get_screen_size(self) -> dict:
        display = os.environ.get("DISPLAY", "")
        if display and shutil.which("xdpyinfo"):
            try:
                out = subprocess.run(["xdpyinfo"], capture_output=True, text=True, timeout=3)
                for line in out.stdout.splitlines():
                    if "dimensions:" in line:
                        parts = line.strip().split()
                        dims = parts[1].split("x")
                        return {"width": int(dims[0]), "height": int(dims[1])}
            except Exception:
                pass
        return {"width": 1920, "height": 1080}  # default

    def mouse_move(self, x: int, y: int, smooth: bool = False, duration_ms: int = 200) -> None:
        self._not_available("mouse_move")

    def mouse_click(self, button: str = "left", x: Optional[int] = None,
                    y: Optional[int] = None, clicks: int = 1) -> None:
        self._not_available("mouse_click")

    def mouse_scroll(self, dx: int = 0, dy: int = 0,
                     x: Optional[int] = None, y: Optional[int] = None) -> None:
        self._not_available("mouse_scroll")

    def mouse_drag(self, x1: int, y1: int, x2: int, y2: int,
                   button: str = "left", duration_ms: int = 500) -> None:
        self._not_available("mouse_drag")

    def keyboard_type(self, text: str, delay_ms: int = 10) -> None:
        self._not_available("keyboard_type")

    def key_press(self, key: str) -> None:
        self._not_available("key_press")

    def clipboard_get(self) -> str:
        if shutil.which("xclip"):
            out = subprocess.run(["xclip", "-selection", "clipboard", "-o"],
                                 capture_output=True, text=True, timeout=3)
            return out.stdout
        self._not_available("clipboard_get")

    def clipboard_set(self, text: str) -> None:
        if shutil.which("xclip"):
            p = subprocess.Popen(["xclip", "-selection", "clipboard"], stdin=subprocess.PIPE)
            p.communicate(input=text.encode())
            return
        self._not_available("clipboard_set")

    def shell_run(self, command: str, timeout: int = 30) -> dict:
        try:
            proc = subprocess.run(command, shell=True, capture_output=True, text=True, timeout=timeout)
            out, err = proc.stdout, proc.stderr
            if len(out) > 8000:
                out = out[:8000] + f"\n... (truncated {len(proc.stdout) - 8000} bytes)"
            if len(err) > 4000:
                err = err[:4000] + f"\n... (truncated {len(proc.stderr) - 4000} bytes)"
            return {"returncode": proc.returncode, "stdout": out, "stderr": err}
        except subprocess.TimeoutExpired:
            raise RuntimeError(f"Command timed out after {timeout}s")

    def list_windows(self) -> list[dict]:
        self._not_available("list_windows")

    def focus_window(self, title_match: str) -> dict:
        self._not_available("focus_window")

    def get_active_window(self) -> Optional[dict]:
        self._not_available("get_active_window")

    def open_app(self, app_name: str) -> None:
        # Try to launch app anyway
        subprocess.Popen([app_name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    def notify(self, title: str, message: str, urgency: str = "normal") -> None:
        if shutil.which("notify-send"):
            subprocess.run(["notify-send", "-u", urgency, title, message])
