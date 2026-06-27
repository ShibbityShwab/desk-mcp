"""Provider base class — defines the interface for computer use backends.

Each provider (KDE Wayland, wlroots, X11, headless) implements this
interface using the appropriate CLI tools for the environment.
"""

from abc import ABC, abstractmethod
from typing import Optional


class ComputerProvider(ABC):
    """Abstract base for all computer use providers."""

    name: str = "base"

    # ── Screenshot ───────────────────────────────────────────
    @abstractmethod
    def screenshot(self, region: Optional[tuple] = None) -> bytes:
        """Capture screen. Returns PNG bytes."""
        ...

    @abstractmethod
    def get_screen_size(self) -> dict:
        """Return {"width": int, "height": int}."""
        ...

    # ── Mouse ────────────────────────────────────────────────
    @abstractmethod
    def mouse_move(self, x: int, y: int, smooth: bool = False, duration_ms: int = 200) -> None:
        ...

    @abstractmethod
    def mouse_click(self, button: str = "left", x: Optional[int] = None, y: Optional[int] = None, clicks: int = 1) -> None:
        ...

    @abstractmethod
    def mouse_scroll(self, dx: int = 0, dy: int = 0, x: Optional[int] = None, y: Optional[int] = None) -> None:
        ...

    @abstractmethod
    def mouse_drag(self, x1: int, y1: int, x2: int, y2: int, button: str = "left", duration_ms: int = 500) -> None:
        ...

    # ── Keyboard ─────────────────────────────────────────────
    @abstractmethod
    def keyboard_type(self, text: str, delay_ms: int = 10) -> None:
        ...

    @abstractmethod
    def key_press(self, key: str) -> None:
        """Press a key or key combo: 'Return', 'ctrl+c', 'alt+Tab', etc."""
        ...

    # ── Clipboard ────────────────────────────────────────────
    @abstractmethod
    def clipboard_get(self) -> str:
        ...

    @abstractmethod
    def clipboard_set(self, text: str) -> None:
        ...

    # ── Shell ────────────────────────────────────────────────
    @abstractmethod
    def shell_run(self, command: str, timeout: int = 30) -> dict:
        """Returns {"returncode": int, "stdout": str, "stderr": str}."""
        ...

    # ── Windows ──────────────────────────────────────────────
    @abstractmethod
    def list_windows(self) -> list[dict]:
        ...

    @abstractmethod
    def focus_window(self, title_match: str) -> dict:
        ...

    @abstractmethod
    def get_active_window(self) -> Optional[dict]:
        ...

    # ── Apps / Notifications ─────────────────────────────────
    @abstractmethod
    def open_app(self, app_name: str) -> None:
        ...

    @abstractmethod
    def notify(self, title: str, message: str, urgency: str = "normal") -> None:
        ...
