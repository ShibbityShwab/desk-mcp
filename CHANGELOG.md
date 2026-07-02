# Changelog

All notable changes to desk-mcp are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] - 2026-06-30

### Added
- Pipe transport for browser automation (`transport: "pipe"`) — no open TCP ports
- Auto-launch Chrome desktop mode with headless fallback
- Platform-specific Chromium install instructions when browser not found
- `resolve_chrome_binary()` with $CHROME env var support

### Changed
- OCR: replaced leptess C library binding with tesseract CLI subprocess (fixes Tesseract 5.5 incompatibility)
- Clipboard: arboard init is non-fatal, wl-paste/wl-copy fallback works on Wayland
- Notifications: replaced notify-rust with notify-send CLI (no more crashes on Wayland)
- Tool dispatch timeout increased from 60s to 300s
- Policy engine: fully configurable via policy.yaml

### Removed
- leptess dependency (replaced by tesseract CLI)
- notify-rust dependency (replaced by notify-send CLI)

### Fixed
- Clipboard crash on Wayland (arboard now fails silently)
- Browser launch hanging on missing Chromium (proper pre-flight check)
- OCR failing with Tesseract 5.5 (CLI-based approach works across versions)
