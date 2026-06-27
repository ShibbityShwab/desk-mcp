#!/usr/bin/env bash
# desk-mcp installer — one-line install for Linux and macOS
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/ShibbityShwab/desk-mcp/main/install.sh | bash
#
# Or:
#   wget -qO- https://raw.githubusercontent.com/ShibbityShwab/desk-mcp/main/install.sh | bash

set -euo pipefail

REPO="ShibbityShwab/desk-mcp"
INSTALL_DIR="${DESKMCP_INSTALL_DIR:-$HOME/.local/bin}"
BINARY="desk-mcp"

# ── Color output ───────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

info()    { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }
header()  { echo -e "${CYAN}$*${NC}"; }

# ── Detect OS and architecture ─────────────────────────
header "desk-mcp installer"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
    x86_64|amd64)  ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *)             error "Unsupported architecture: $ARCH" ;;
esac

case "$OS" in
    linux)  PLATFORM="linux-${ARCH}" ;;
    darwin) PLATFORM="macos-${ARCH}" ;;
    *)      error "Unsupported OS: $OS" ;;
esac

ASSET="desk-mcp-${PLATFORM}"
info "Detected: ${PLATFORM}"

# ── Get latest release ─────────────────────────────────
RELEASE_URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"
CHECKSUM_URL="https://github.com/${REPO}/releases/latest/download/${ASSET}.sha256"

# ── Create install directory ───────────────────────────
mkdir -p "$INSTALL_DIR"

# ── Download binary ────────────────────────────────────
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

info "Downloading ${ASSET}..."
if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$RELEASE_URL" -o "$TMP_DIR/${BINARY}" || error "Download failed. Is there a release yet? Run: gh release create"
    curl -fsSL "$CHECKSUM_URL" -o "$TMP_DIR/${BINARY}.sha256" 2>/dev/null || true
elif command -v wget >/dev/null 2>&1; then
    wget -q "$RELEASE_URL" -O "$TMP_DIR/${BINARY}" || error "Download failed."
    wget -q "$CHECKSUM_URL" -O "$TMP_DIR/${BINARY}.sha256" 2>/dev/null || true
else
    error "Neither curl nor wget found. Install one and retry."
fi

# ── Verify checksum (if available) ─────────────────────
if [ -f "$TMP_DIR/${BINARY}.sha256" ] && [ -s "$TMP_DIR/${BINARY}.sha256" ]; then
    info "Verifying checksum..."
    EXPECTED=$(awk '{print $1}' "$TMP_DIR/${BINARY}.sha256")
    if command -v sha256sum >/dev/null 2>&1; then
        ACTUAL=$(sha256sum "$TMP_DIR/${BINARY}" | awk '{print $1}')
    elif command -v shasum >/dev/null 2>&1; then
        ACTUAL=$(shasum -a 256 "$TMP_DIR/${BINARY}" | awk '{print $1}')
    else
        warn "No sha256sum/shasum found — skipping checksum verification"
        ACTUAL="$EXPECTED"
    fi
    if [ "$EXPECTED" != "$ACTUAL" ]; then
        error "Checksum verification failed!"
    fi
    info "Checksum OK"
else
    warn "No checksum file — skipping verification"
fi

# ── Install ────────────────────────────────────────────
chmod +x "$TMP_DIR/${BINARY}"
cp "$TMP_DIR/${BINARY}" "$INSTALL_DIR/${BINARY}"

info "Installed to ${INSTALL_DIR}/${BINARY}"

# ── Add to PATH if needed ──────────────────────────────
if ! echo "$PATH" | tr ':' '\n' | grep -qxF "$INSTALL_DIR"; then
    info "Adding ${INSTALL_DIR} to PATH..."

    SHELL_CONFIG=""
    case "$SHELL" in
        */bash) SHELL_CONFIG="$HOME/.bashrc" ;;
        */zsh)  SHELL_CONFIG="$HOME/.zshrc" ;;
        */fish) SHELL_CONFIG="$HOME/.config/fish/config.fish" ;;
    esac

    if [ -n "$SHELL_CONFIG" ]; then
        mkdir -p "$(dirname "$SHELL_CONFIG")"
        if ! grep -q "$INSTALL_DIR" "$SHELL_CONFIG" 2>/dev/null; then
            echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$SHELL_CONFIG"
            info "Added to ${SHELL_CONFIG}. Restart your shell or run: source ${SHELL_CONFIG}"
        fi
    else
        warn "Could not detect shell config. Add this to your shell profile:"
        echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    fi
fi

# ── Verify installation ────────────────────────────────
if command -v "$BINARY" >/dev/null 2>&1 || [ -x "$INSTALL_DIR/${BINARY}" ]; then
    INSTALLED_VERSION=$("$INSTALL_DIR/${BINARY}" --version 2>/dev/null || echo "unknown")
    info "desk-mcp ${INSTALLED_VERSION} installed successfully!"
else
    warn "Installation may not be on PATH yet. Run: export PATH=\"$INSTALL_DIR:\$PATH\""
fi

# ── Quickstart ─────────────────────────────────────────
echo ""
header "Quickstart"
echo "  Run the server:"
echo "    ${BINARY}"
echo ""
echo "  Or configure your MCP client (e.g., Claude Desktop):"
echo '  { "mcpServers": { "desk-mcp": { "command": "'"${INSTALL_DIR}/${BINARY}"'" } } }'
echo ""
echo "  For full documentation: https://github.com/${REPO}#readme"
