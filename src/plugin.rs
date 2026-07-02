//! Dynamic provider loading from shared libraries.
//!
//! Plugins live in `~/.config/desk-mcp/plugins/*.so` (Linux / BSD)
//! or the platform-appropriate config directory.
//!
//! Each plugin exports a single `create_provider` symbol that returns
//! a heap-allocated [`ComputerProvider`] trait object.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::providers::ComputerProvider;

/// Signature of the `create_provider` entry-point that every plugin must export.
type ProviderFactory = fn() -> Box<dyn ComputerProvider + Send + Sync>;

// ── Plugin directory ────────────────────────────────────────────────────

/// Platform-appropriate plugin directory: `$XDG_CONFIG_HOME/desk-mcp/plugins`
/// or `~/.config/desk-mcp/plugins`.
fn plugin_dir() -> PathBuf {
    let mut p = dirs_next::config_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    p.push("desk-mcp");
    p.push("plugins");
    p
}

// ── Loading ─────────────────────────────────────────────────────────────

/// Load a single provider plugin from a shared library.
///
/// # Safety
///
/// The caller must ensure the shared library at `path` is trusted.
/// This function calls into foreign code through `libloading` and
/// invokes the `create_provider` factory symbol.
pub unsafe fn load_provider(
    path: &Path,
) -> Result<Box<dyn ComputerProvider + Send + Sync>, String> {
    let lib = libloading::Library::new(path)
        .map_err(|e| format!("Failed to load plugin {}: {e}", path.display()))?;

    // We must leak `lib` so the symbols remain live for the lifetime
    // of the provider.  The provider trait object holds no reference to
    // the library, and libloading does not support a "keep alive" RAII
    // guard inside the returned object without an extra allocation.
    // Pragmatic compromise: leak the library handle.
    let lib = Box::leak(Box::new(lib));

    let factory: libloading::Symbol<ProviderFactory> =
        lib.get(b"create_provider").map_err(|e| {
            format!(
                "Plugin {} missing 'create_provider' symbol: {e}",
                path.display()
            )
        })?;

    Ok(factory())
}

/// Discover and load all provider plugins from the plugin directory.
///
/// Returns `(file_stem, Provider)` pairs.  Files that cannot be loaded
/// are silently skipped after logging a warning.
pub fn load_all_plugins() -> Vec<(String, Box<dyn ComputerProvider + Send + Sync>)> {
    let dir = plugin_dir();
    if !dir.exists() {
        return vec![];
    }

    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(dir = %dir.display(), error = %e, "cannot read plugin directory");
            return vec![];
        }
    };

    let mut plugins = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();

        // Only consider shared-library extensions
        let ext = path.extension().and_then(OsStr::to_str).unwrap_or("");
        if ext != "so" && ext != "dylib" && ext != "dll" {
            continue;
        }

        // Derive a human-readable name from the file stem
        let name = path
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or("unknown")
            .to_string();

        // Safety: plugins are trusted shared objects on disk.
        match unsafe { load_provider(&path) } {
            Ok(provider) => {
                tracing::info!(
                    plugin = %name,
                    provider = %provider.name(),
                    "loaded plugin"
                );
                plugins.push((name, provider));
            }
            Err(e) => {
                tracing::warn!(plugin = %name, error = %e, "failed to load plugin");
            }
        }
    }

    plugins
}

/// List the names of all discovered plugin files (without loading them).
pub fn list_plugins() -> Vec<String> {
    let dir = plugin_dir();
    if !dir.exists() {
        return vec![];
    }

    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let ext = path.extension().and_then(OsStr::to_str).unwrap_or("");
            if ext == "so" || ext == "dylib" || ext == "dll" {
                path.file_stem().and_then(OsStr::to_str).map(String::from)
            } else {
                None
            }
        })
        .collect()
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_dir_is_absolute() {
        let d = plugin_dir();
        assert!(d.is_absolute(), "plugin dir must be absolute: {d:?}");
        assert!(d.ends_with("desk-mcp/plugins") || d.ends_with("desk-mcp\\plugins"));
    }

    #[test]
    fn list_plugins_empty_when_dir_missing() {
        // Plugin dir typically does not exist in test / CI environments
        let names = list_plugins();
        // Just assert we get a vec back (empty is the expected case)
        assert!(names.is_empty());
    }

    #[test]
    fn load_all_plugins_empty_when_dir_missing() {
        let plugins = load_all_plugins();
        assert!(plugins.is_empty());
    }
}
