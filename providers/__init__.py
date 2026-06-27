"""Provider factory — selects the right backend based on auto-discovery."""

from ._base import ComputerProvider


def get_provider() -> ComputerProvider:
    """Select and return the best provider for the current environment."""
    from ..discovery import detect

    caps = detect()

    if caps.provider == "wayland_kde":
        from .wayland_kde import KDEWaylandProvider
        return KDEWaylandProvider()

    elif caps.provider == "wayland_wlr":
        # For wlroots-based compositors (Sway, Hyprland), try grim + ydotool
        from .wayland_kde import KDEWaylandProvider  # reuse KDE provider but some tools won't work
        return KDEWaylandProvider()

    elif caps.provider == "x11":
        # X11 fallback — reuse KDE provider skeleton but tools adapt via shutil.which
        from .wayland_kde import KDEWaylandProvider
        return KDEWaylandProvider()

    else:
        from .headless import HeadlessProvider
        return HeadlessProvider()
