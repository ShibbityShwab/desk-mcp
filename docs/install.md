---
layout: default
title: Installation — desk-mcp
---

# Installation

##  One-Line Install (Recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/ShibbityShwab/desk-mcp/main/install.sh | bash
```

This script:
1. Detects your OS and architecture
2. Downloads the latest release binary
3. Verifies the SHA256 checksum
4. Installs to `~/.local/bin`
5. Adds the directory to your PATH

##  Manual Download

Download the binary for your platform from [GitHub Releases](https://github.com/ShibbityShwab/desk-mcp/releases/latest):

| Platform | Binary |
|----------|--------|
| Linux x86_64 | `desk-mcp-linux-x86_64` |
| Linux aarch64 | `desk-mcp-linux-aarch64` |
| macOS x86_64 | `desk-mcp-macos-x86_64` |
| macOS aarch64 | `desk-mcp-macos-aarch64` |

```bash
# After downloading
chmod +x desk-mcp-*
sudo mv desk-mcp-* /usr/local/bin/desk-mcp
```

##  Cargo Install

```bash
cargo install --git https://github.com/ShibbityShwab/desk-mcp
```

##  Build from Source

```bash
git clone https://github.com/ShibbityShwab/desk-mcp
cd desk-mcp
cargo build --release
# Binary at target/release/desk-mcp
```

### System Dependencies

**Arch Linux:**
```bash
sudo pacman -S spectacle ydotool tesseract tesseract-data-eng chromium
```

**Ubuntu/Debian:**
```bash
sudo apt install spectacle ydotool tesseract-ocr chromium-browser
```

**Fedora:**
```bash
sudo dnf install spectacle ydotool tesseract chromium
```

##  Configure Your MCP Client

### Claude Desktop

Add to `~/.config/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "desk-mcp": {
      "command": "desk-mcp"
    }
  }
}
```

With security enabled:
```json
{
  "mcpServers": {
    "desk-mcp": {
      "command": "desk-mcp",
      "env": {
        "ALLOW_SHELL": "1",
        "ALLOW_CODE": "1",
        "DESKMCP_WORKSPACE": "/home/user/projects"
      }
    }
  }
}
```

### Any MCP Client

desk-mcp speaks the standard MCP protocol over stdin/stdout.
Point any compliant client at the `desk-mcp` binary.
