---
layout: default
title: Configuration — desk-mcp
---

# Configuration

desk-mcp is configured entirely through environment variables set in your MCP client config.

## MCP Client Config

### Claude Desktop

Edit `~/.config/Claude/claude_desktop_config.json`:

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

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ALLOW_SHELL` | (unset) | Set to `"1"` or `"true"` to enable `shell_run` |
| `ALLOW_CODE` | (unset) | Set to `"1"` or `"true"` to enable `code_run` |
| `DESKMCP_WORKSPACE` | `$HOME/Projects` | Root directory for file operations |
| `RUST_LOG` | `info` | Log level for tracing (`debug`, `info`, `warn`, `error`) |

## Security Configuration

### Minimal (Default)
No shell, no code execution. Safe for untrusted AI agents.

```json
{
  "mcpServers": {
    "desk-mcp": {
      "command": "desk-mcp"
    }
  }
}
```

### Development
Allow shell + code with a workspace sandbox.

```json
{
  "mcpServers": {
    "desk-mcp": {
      "command": "desk-mcp",
      "env": {
        "ALLOW_SHELL": "1",
        "ALLOW_CODE": "1",
        "DESKMCP_WORKSPACE": "/home/user/dev"
      }
    }
  }
}
```

### Full Access (use with caution)
```json
{
  "mcpServers": {
    "desk-mcp": {
      "command": "desk-mcp",
      "env": {
        "ALLOW_SHELL": "1",
        "ALLOW_CODE": "1",
        "DESKMCP_WORKSPACE": "/"
      }
    }
  }
}
```

## Command Line Options

```
desk-mcp [OPTIONS]

Options:
  --version          Print version and exit
  --help             Print help and exit
```

The server runs on stdin/stdout (standard MCP transport). No network port is needed.

## Logging

Set `RUST_LOG` for different verbosity levels:

```bash
# Full debug output
RUST_LOG=debug desk-mcp

# Only warnings and errors
RUST_LOG=warn desk-mcp

# Specific module
RUST_LOG=desk_mcp::tools::browser=debug desk-mcp
```

## Provider Selection

desk-mcp auto-detects your provider at startup:

1. Checks `XDG_CURRENT_DESKTOP` for desktop environment
2. Checks `WAYLAND_DISPLAY` / `DISPLAY` for display type
3. Scans for installed tools (spectacle, grim, ydotool, xdotool)
4. Selects the best available provider

The `discover` tool shows which provider was selected:

```json
{
  "provider": "wayland_kde",
  "screenshot": true,
  "mouse": true,
  "keyboard": true,
  "ocr": true
}
```

## Refresh Discovery

If you install a new tool while the server is running, call `discover` again.
The discovery cache is automatically refreshed on new tool calls.
