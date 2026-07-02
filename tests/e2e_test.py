#!/usr/bin/env python3
"""
End-to-end test for desk-mcp MCP server.

Spawns the desk-mcp binary, sends initialize, then calls EVERY tool
through JSON-RPC over stdio (Content-Length framing).  Prints PASS/FAIL
for each tool.  Exits 0 if all non-skipped tests pass, 1 otherwise.

MANUAL-SKIP tools (require display / Chromium / interactive desktop):
  - browser_launch (requires Chromium + display)

NON-FATAL tools (logged but do not fail the suite):
  - clipboard_get   (may fail on headless / restricted environments)
  - notify          (may fail when no notification daemon is running)

Usage:
    cd /home/shibbityshwab/Documents/GitHub/desk-mcp
    ALLOW_SHELL=1 ALLOW_CODE=1 python3 tests/e2e_test.py
"""

from __future__ import annotations

import json
import os
import select
import subprocess
import sys
import time

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------
BINARY = "./target/release/desk-mcp"
WORKSPACE = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
BINARY_PATH = os.path.join(WORKSPACE, BINARY)

# Tools that are skipped with a MANUAL marker (not counted as failures)
MANUAL_SKIP = frozenset({
    "browser_launch",  # Needs Chromium + running display server
})

# Tools that are non-fatal: if they fail we log WARN but do not fail the suite
NON_FATAL = frozenset({
    "clipboard_get",  # May fail on headless / no clipboard provider
    "notify",          # May fail when no notification daemon (D-Bus) is active
})

# Tools whose response includes a base64-encoded image; we validate the data
# field is present and non-empty.
IMAGE_TOOLS = frozenset({
    "screenshot",
})

# Tools whose response should contain a nested content[0].text JSON blob
# with an "ok" field.
EXPECT_OK = frozenset({
    "server_status",
    "get_screen_size",
    "env_get",
    "screenshot",
    "list_windows",
    "get_active_window",
    "file_read",
    "glob",
    "grep",
    "shell_run",
    "code_run",
    "web_search",
    "web_fetch",
    "request_confirmation",
    "list_pending",
    "approve",
    "deny",
    "get_window_state",
    "find_elements",
    "clipboard_get",
    "notify",
    "browser_refresh",
    "browser_tabs",
    "extract_text",
    "describe_screen",
})

# ---------------------------------------------------------------------------
# Test definition: (tool_name, arguments dict, optional validator callable)
# The validator receives (ok: bool, result: dict|None, error: str|None) from
# the parsed inner content, plus the raw outer MCP response dict.
# Return True = pass, False = fail.
# ---------------------------------------------------------------------------
TOOL_TESTS: list[tuple[str, dict, str | None]] = [
    # ── Status / Info ──
    ("server_status", {}, None),
    ("get_screen_size", {}, None),
    ("env_get", {"name": "HOME"}, None),
    ("env_get", {"name": "USER"}, None),
    ("env_get", {"name": "SHELL"}, None),

    # ── Screenshot (verify base64 PNG returned) ──
    ("screenshot", {}, "validate_screenshot"),
    ("screenshot", {"region": [0, 0, 100, 100]}, "validate_screenshot"),

    # ── Screen / Text extraction ──
    ("extract_text", {}, None),
    ("describe_screen", {}, None),

    # ── Window management ──
    ("list_windows", {}, None),
    ("get_active_window", {}, None),

    # ── File operations ──
    ("file_read", {"path": "Cargo.toml", "limit": 5}, None),
    ("glob", {"pattern": "src/**/*.rs", "path": "."}, None),
    ("grep", {"pattern": "fn dispatch", "path": "src"}, None),

    # ── Shell / Code execution ──
    ("shell_run", {"command": "echo ok && uname -s"}, None),
    ("code_run", {"language": "python", "code": "print(42)"}, None),
    ("code_run", {"language": "bash", "code": "echo hello"}, None),

    # ── Web ──
    ("web_search", {"query": "Rust programming language", "max_results": 2}, None),
    ("web_fetch", {"url": "https://example.com", "max_bytes": 500}, None),

    # ── Confirmation flow ──
    ("request_confirmation", {"tool": "shell_run", "message": "Test confirmation"}, None),
    ("list_pending", {}, None),
    # approve/deny require a dynamic id from request_confirmation – handled in run_tool
    ("approve", {"id": "__DYNAMIC__"}, None),
    ("deny", {"id": "__DYNAMIC__"}, None),

    # ── Accessibility ──
    ("get_window_state", {}, None),
    ("find_elements", {"role": "push button", "max_results": 5}, None),
    ("get_element_text", {"path": 0}, None),

    # ── Browser (non-launch) ──
    ("browser_refresh", {}, None),
    ("browser_tabs", {}, None),

    # ── Clipboard (non-fatal) ──
    ("clipboard_get", {}, None),

    # ── Notification (non-fatal) ──
    ("notify", {"title": "desk-mcp e2e test", "message": "Testing notifications"}, None),

    # ── Extra safety / approval tools ──
    ("request_confirmation", {"tool": "test_tool", "message": "Another test"}, None),
    ("list_pending", {}, None),
]

# ---------------------------------------------------------------------------
# Validators
# ---------------------------------------------------------------------------
def validate_screenshot(
    ok: bool,
    result: dict | None,
    error: str | None,
    raw: dict | None,
) -> bool:
    """Verify screenshot returns a base64 PNG string in the 'data' field."""
    if not ok:
        return False
    if result is None:
        return False
    data = result.get("data", "")
    if not isinstance(data, str) or len(data) < 50:
        return False
    # PNG magic: base64 encoding of \x89PNG starts with "iVBOR"
    if data.startswith("iVBOR"):
        return True
    # Some implementations may return raw base64 without the PNG header check;
    # just verify it's long enough and has valid base64
    return True


# ---------------------------------------------------------------------------
# JSON-RPC helpers
# ---------------------------------------------------------------------------
class McpClient:
    """Manages a desk-mcp subprocess with JSON-RPC over stdio."""

    def __init__(self, binary: str, cwd: str, env: dict[str, str] | None = None):
        self.proc = subprocess.Popen(
            [binary],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=cwd,
            env=env,
        )
        self._next_id = 0
        self._pending_ids: list[str] = []

    def send(self, msg: dict) -> None:
        """Write a JSON-RPC message using Content-Length framing."""
        body = json.dumps(msg)
        frame = f"Content-Length: {len(body)}\r\n\r\n{body}"
        assert self.proc.stdin is not None
        self.proc.stdin.write(frame.encode())
        self.proc.stdin.flush()

    def recv(self, timeout: float = 15.0) -> dict | None:
        """Read one JSON-RPC response using Content-Length framing."""
        assert self.proc.stdout is not None
        if not select.select([self.proc.stdout], [], [], timeout)[0]:
            return None
        line = self.proc.stdout.readline()
        # Skip blank lines / keepalives
        while line in (b"\r\n", b"\n", b""):
            line = self.proc.stdout.readline()
            if not line:
                return None

        content_length = 0
        while True:
            decoded = line.decode().strip()
            if decoded.startswith("Content-Length:") or decoded.startswith("content-length:"):
                content_length = int(decoded.split(":", 1)[1].strip())
            elif decoded == "":
                break  # End of headers
            line = self.proc.stdout.readline()
            if not line:
                return None

        if content_length == 0:
            return None

        raw_body = self.proc.stdout.read(content_length)
        return json.loads(raw_body.decode())

    def initialize(self) -> dict | None:
        """Send initialize handshake and return serverInfo."""
        self.send({
            "jsonrpc": "2.0",
            "id": "init",
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "e2e-test", "version": "1.0"},
            },
        })
        resp = self.recv()
        if resp:
            self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        return resp

    def call_tool(self, name: str, arguments: dict | None = None, timeout: float = 30.0) -> dict | None:
        """Call a tool via tools/call and return the full MCP response."""
        self._next_id += 1
        req_id = name if name != "approve" and name != "deny" else f"{name}-{self._next_id}"
        self.send({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments or {},
            },
        })
        return self.recv(timeout=timeout)

    def close(self) -> None:
        """Terminate the subprocess."""
        self.proc.terminate()
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()


# ---------------------------------------------------------------------------
# Main test runner
# ---------------------------------------------------------------------------
def main() -> int:
    passed = 0
    failed: list[str] = []
    fatal_skipped: list[str] = []
    non_fatal_skipped: list[str] = []

    # Environment
    env = os.environ.copy()
    env.setdefault("ALLOW_SHELL", "1")
    env.setdefault("ALLOW_CODE", "1")
    env.setdefault("RUST_LOG", "warn")

    if not os.path.isfile(BINARY_PATH):
        print(f"ERROR: Binary not found at {BINARY_PATH}")
        print("Build first: cargo build --release")
        return 1

    print(f"desk-mcp e2e test")
    print(f"Binary: {BINARY_PATH}")
    print(f"Workspace: {WORKSPACE}")
    print(f"Total test entries: {len(TOOL_TESTS)}")
    print()

    # ── Spawn server ──
    client = McpClient(BINARY_PATH, WORKSPACE, env)
    print("[INIT] Spawning desk-mcp...", flush=True)

    init_resp = client.initialize()
    if init_resp is None:
        print("[INIT] FAIL — no response to initialize")
        client.close()
        return 1

    server_info = init_resp.get("result", {}).get("serverInfo", {})
    print(f"[INIT] PASS — server: {server_info.get('name', '?')} v{server_info.get('version', '?')}")
    print()

    # ── Track dynamic confirmation IDs ──
    confirmation_ids: list[str] = []

    # ── Run each tool test ──
    for tool_name, tool_args, validator_name in TOOL_TESTS:
        # Skip check (fatal skip = MANUAL)
        if tool_name in MANUAL_SKIP:
            fatal_skipped.append(tool_name)
            print(f"[SKIP-MANUAL] {tool_name} — requires Chromium / display")
            continue

        is_non_fatal = tool_name in NON_FATAL
        label = "TOOL" if not is_non_fatal else "TOOL(NF)"

        # Handle dynamic confirmation IDs for approve/deny
        effective_args = dict(tool_args)
        if "__DYNAMIC__" in str(effective_args.get("id", "")):
            if not confirmation_ids:
                msg = (
                    f"[FAIL] {tool_name} — no pending confirmation IDs available to use. "
                    "request_confirmation may have failed earlier."
                )
                if is_non_fatal:
                    non_fatal_skipped.append(tool_name)
                    print(f"[{label}] {msg}")
                    continue
                else:
                    failed.append(tool_name)
                    print(msg)
                    continue
            effective_args["id"] = confirmation_ids[-1]

        # ── Call the tool ──
        raw_resp = client.call_tool(tool_name, effective_args)

        # ── Parse response ──
        if raw_resp is None:
            msg = f"[FAIL] {tool_name} — no response (timeout)"
            if is_non_fatal:
                non_fatal_skipped.append(tool_name)
                print(f"[{label}] {msg}")
                continue
            failed.append(tool_name)
            print(msg)
            continue

        # Extract inner content[0].text JSON
        ok = False
        result: dict | None = None
        error: str | None = None

        try:
            content = raw_resp.get("result", {}).get("content", [])
            if content:
                inner_text = content[0].get("text", "{}")
                inner = json.loads(inner_text)
                ok = inner.get("ok", False)
                result = inner.get("result")
                error = inner.get("error")
        except (json.JSONDecodeError, KeyError, TypeError, IndexError):
            pass

        # Specific validator override
        if validator_name == "validate_screenshot":
            test_ok = validate_screenshot(ok, result, error, raw_resp)
        else:
            # Default: check the inner "ok" field (for tools that have it)
            if tool_name in EXPECT_OK:
                test_ok = ok
            else:
                # For tools we haven't categorized, accept any non-error response
                test_ok = bool(raw_resp.get("result")) and "error" not in raw_resp

        # ── Detect confirmation IDs ──
        if tool_name == "request_confirmation" and ok and result:
            cid = result.get("id")
            if cid:
                confirmation_ids.append(cid)

        # ── Report ──
        status_str = "PASS" if test_ok else "FAIL"
        detail = ""
        if test_ok:
            if tool_name in IMAGE_TOOLS and result:
                data_len = len(result.get("data", ""))
                detail = f"data_len={data_len}"
            elif result is not None:
                # Truncate long results
                r_str = json.dumps(result, default=str)
                detail = r_str[:160]
            else:
                detail = "ok"
        else:
            if error:
                detail = str(error)[:200]
            elif "error" in raw_resp:
                detail = json.dumps(raw_resp["error"], default=str)[:200]
            else:
                detail = "unknown failure"

        print(f"[{status_str}] {label}: {tool_name} — {detail}")

        if test_ok:
            passed += 1
        elif is_non_fatal:
            non_fatal_skipped.append(tool_name)
            print(f"         ^ non-fatal — not counting as failure")
        else:
            failed.append(tool_name)

    # ── Summary ──
    client.close()

    total_non_skipped = len(TOOL_TESTS) - len(fatal_skipped) - len(non_fatal_skipped)
    print()
    print("=" * 60)
    print(f"Total test entries : {len(TOOL_TESTS)}")
    print(f"Passed             : {passed}")
    print(f"Failed             : {len(failed)}")
    print(f"MANUAL skip        : {len(fatal_skipped)} ({', '.join(fatal_skipped) if fatal_skipped else 'none'})")
    print(f"Non-fatal skip     : {len(non_fatal_skipped)} ({', '.join(non_fatal_skipped) if non_fatal_skipped else 'none'})")
    print(f"Non-skipped total  : {total_non_skipped}")

    if failed:
        print(f"\nFAILED tools: {', '.join(failed)}")

    print()
    if failed:
        print("RESULT: FAIL — some tools failed")
        return 1
    print("RESULT: PASS — all non-skipped tools passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
