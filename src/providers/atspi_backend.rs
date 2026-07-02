//! Pure-Rust AT-SPI accessibility backend using the `atspi` crate.
//!
//! This module provides a native Rust alternative to the Python/pyatspi
//! subprocess approach in `src/a11y.rs`.
//!
//! ## Current state: hybrid (Python fallback)
//!
//! The `atspi` crate v0.11 is imported and available, but its public API
//! (generated zbus proxies) is designed for event-driven usage, not for
//! the recursive tree-walking pattern we need.  The `Accessible` trait
//! and `AccessibleId` type are not publicly re-exported in a way that
//! permits building child proxies from `ObjectPair` tuples.
//!
//! ## Path to full native implementation
//!
//! 1. Upgrade to a newer `atspi` release when one exposes tree-walking
//!    helpers (e.g. a `children()` iterator or public `ObjectPair`→proxy
//!    constructors).
//! 2. Alternatively: implement tree-walking directly on `zbus` v5 (which
//!    we already depend on for KWin D-Bus), bypassing the `atspi` crate
//!    entirely for the tree-walking layer while keeping it for types.
//!
//! Until then, all `get_window_state` calls go through `a11y::get_window_state`
//! (the battle-tested Python/pyatspi subprocess), and this module serves as
//! a validated compile-time placeholder for the `atspi` dependency.

use anyhow::Result;

use crate::providers::{WindowInfo, WindowState};

/// Walk the AT-SPI accessibility tree for the active window and collect
/// all meaningful UI elements.
///
/// Currently delegates to the Python/pyatspi subprocess (see module docs
/// for the native-implementation roadmap).
pub fn get_window_state_via_atspi(active: &WindowInfo) -> Result<WindowState> {
    // Delegate to the Python/pyatspi subprocess.
    // The pure-Rust path is blocked on atspi crate API limitations — see
    // the module-level documentation for the planned upgrade path.
    crate::a11y::get_window_state(active)
}
