---
layout: default
title: Security — desk-mcp
---

# Security Model

desk-mcp gives AI agents significant power — it can move your mouse, type on your
keyboard, read your screen, and run code. Security is a first-class concern.

##  Guardrails

### 1. Dangerous Operations are Off By Default

| Operation | Default | How to Enable |
|-----------|---------|---------------|
| `shell_run` | **OFF** | Set `ALLOW_SHELL=1` env var |
| `code_run` | **OFF** | Set `ALLOW_CODE=1` env var |
| File access | Sandboxed | Set `DESKMCP_WORKSPACE` env var |

Without these flags, the AI agent can only:
- Take screenshots
- Use OCR to read the screen
- Move the mouse and type
- Use the browser (in sandboxed headless mode)

### 2. Workspace Sandbox

All file operations (`file_read`, `file_write`, `file_edit`, `grep`, `glob`)
are confined to the `DESKMCP_WORKSPACE` directory.

Default: `$HOME/Projects`. If that doesn't exist, falls back to `$HOME`.

**How it works:**
1. Resolve the candidate path to its canonical (absolute) form
2. Resolve the workspace root to its canonical form
3. Reject the operation if the candidate path doesn't start with the workspace root

Example of what gets blocked:
```
DESKMCP_WORKSPACE=/home/user/projects

file_read("/home/user/projects/src/main.rs")       ✅ Allowed
file_read("src/main.rs")                            ✅ Allowed (relative)
file_read("/etc/passwd")                            ❌ Blocked
file_read("../../etc/passwd")                       ❌ Blocked (resolves outside workspace)
```

### 3. Execution Timeouts

- `shell_run`: 30 second default, 300 second maximum
- `code_run`: 30 second default, 300 second maximum
- `browser_launch`: 25 second timeout
- `browser_wait_for`: 30 second default

### 4. Browser Isolation

The default browser mode is **headless** with a fresh temporary user data directory.
No cookies, history, or credentials from your real browser are shared.

To connect to your real desktop Chrome, you must explicitly set `mode: "desktop"`
and Chrome must have been launched with `--remote-debugging-port=9222`.

### 5. No Network Listening

desk-mcp communicates over stdin/stdout only. It does not bind to any TCP port
or listen on the network.

### 6. Provider Isolation

Each provider is a self-contained module. The headless provider has no access to
desktop input tools. Providers never share state.

##  Best Practices

1. **Start minimal.** Run without `ALLOW_SHELL` and `ALLOW_CODE` first.
   Only enable them when you trust the AI's behavior.

2. **Use a dedicated workspace.** Set `DESKMCP_WORKSPACE` to a project-specific
   directory, never to `/` or `$HOME`.

3. **Monitor the logs.** Set `RUST_LOG=info` and watch for unexpected tool calls.

4. **Use a separate user account** for maximum isolation (Linux).

5. **Keep desk-mcp updated.** Run `cargo install --git ...` or download the
   latest release.

##  Limitations

- desk-mcp cannot prevent the AI from typing malicious commands if
  `ALLOW_SHELL=1` and the terminal is focused.
- OCR reads everything on your screen — sensitive information may be
  visible to the AI.
- Browser automation can access any website the headless Chrome can reach.

**Treat the AI agent like a remote desktop user — only give it the access you'd
give a human contractor.**
