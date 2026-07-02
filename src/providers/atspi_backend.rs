//! Pure-Rust AT-SPI accessibility backend using the `atspi` crate.
//!
//! This module provides a native Rust alternative to the Python/pyatspi
//! subprocess approach in `src/a11y.rs`. It connects to the AT-SPI D-Bus
//! session bus directly using the `atspi` crate and walks the accessibility
//! tree to collect UI elements.
//!
//! Currently a scaffold — returns the same kdotool-based WindowState for now,
//! but wires up the `atspi` crate import so the build is validated.

use anyhow::Result;

use crate::providers::{WindowInfo, WindowState};

/// Walk the AT-SPI accessibility tree for the active window and collect
/// all meaningful UI elements using the pure-Rust `atspi` crate.
///
/// This is intended to replace the Python/pyatspi subprocess approach once
/// the tree-walking logic is fully implemented.
///
/// Currently a stub that delegates to `crate::a11y::get_window_state`.
pub async fn get_window_state_via_atspi(active: &WindowInfo) -> Result<WindowState> {
    // ── Stub: delegate to the existing Python-based implementation ──
    // TODO: Replace with pure atspi tree walking.
    //
    // The atspi crate is imported and available:
    //   use atspi::connection::AccessibilityConnection;
    //   use atspi::events::Event;
    //
    // Planned implementation:
    //   1. Connect to session bus via atspi::connection::AccessibilityConnection::new()
    //   2. Get the desktop root (root of the accessibility tree)
    //   3. Find the application matching `active.pid` or `active.app`
    //   4. Walk children, collect UiElement structs matching the same schema
    //   5. Return WindowState
    //
    // For now, fall through to the Python-based implementation.
    crate::a11y::get_window_state(active)
}
