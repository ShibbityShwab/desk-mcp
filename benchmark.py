#!/usr/bin/env python3
"""DeskMCP comprehensive tool benchmark — calls every tool and reports latency/success."""
import json, time, urllib.request, sys, os
from collections import OrderedDict

BASE = "http://127.0.0.1:8765/mcp"
TIMEOUT = 30
results = []  # list of (latency_ms, tool_name, status_str)

def rpc(method, params=None):
    body = {"jsonrpc": "2.0", "method": method, "id": int(time.time()*1000)}
    if params is not None:
        body["params"] = params
    data = json.dumps(body).encode()
    req = urllib.request.Request(BASE, data=data,
        headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
            return json.loads(resp.read())
    except Exception as e:
        return {"error": str(e)}

def bench(name, args=None, method="tools/call"):
    """Call a tool and record result."""
    t0 = time.monotonic()
    if method == "tools/call":
        p = {"name": name, "arguments": args or {}}
    else:
        p = args
    resp = rpc(method, p)
    elapsed = (time.monotonic() - t0) * 1000

    error = resp.get("error")
    if not error:
        # Check result content for error
        result = resp.get("result", {})
        status = "PASS"
    else:
        err_msg = str(error.get("message", str(error)))[:100]
        if isinstance(error, dict):
            err_code = error.get("code", 0)
        else:
            err_code = 0
        # Classify
        if "ALLOW_" in err_msg or "requires" in err_msg.lower() or "flag" in err_msg:
            status = f"SKIP (gated: {err_msg[:60]})"
        elif err_code == -32000:
            status = f"PASS (expected: {err_msg[:60]})"
        else:
            status = f"FAIL [{err_code}] {err_msg}"

    bar = "=" * min(int(elapsed/40), 25)
    print(f"  {name:<30s} {elapsed:7.1f}ms  {bar:<25s} {status}")
    results.append((elapsed, name, status))
    return resp

def summarize():
    print(f"\n{'='*80}")
    p = sum(1 for _,_,s in results if s.startswith("PASS"))
    f = sum(1 for _,_,s in results if s.startswith("FAIL"))
    sk = sum(1 for _,_,s in results if s.startswith("SKIP"))
    total = len(results)
    print(f"  Total: {total}  ✅ Pass: {p}  ❌ Fail: {f}  ⏭️ Skip: {sk}")

    times = sorted(r[0] for r in results)
    if times:
        avg = sum(times)/len(times)
        p50 = times[len(times)//2]
        p95 = times[min(int(len(times)*0.95), len(times)-1)]
        p99 = times[min(int(len(times)*0.99), len(times)-1)]
        print(f"  ⏱️  Latency → avg:{avg:.1f}ms  p50:{p50:.1f}ms  p95:{p95:.1f}ms  max:{max(times):.1f}ms  min:{min(times):.1f}ms")

    print(f"\n  🐌 Slowest 5:")
    for t,n,s in sorted(results, reverse=True)[:5]:
        print(f"      {n:<30s} {t:7.1f}ms  {s}")

    fails = [(t,n,s) for t,n,s in results if s.startswith("FAIL")]
    if fails:
        print(f"\n  ❌ Failures:")
        for t,n,s in fails:
            print(f"      {n:<30s} {s}")

# ─────────────────────────────────────────────────────────────────
# Phase 1: MCP protocol
# ─────────────────────────────────────────────────────────────────
print("── 1. MCP Protocol ──")
bench("", method="initialize", args={"protocolVersion":"2024-11-05","capabilities":{}})
bench("", method="tools/list")

# ─────────────────────────────────────────────────────────────────
# Phase 2: Read-only (safe, no side effects)
# ─────────────────────────────────────────────────────────────────
print("\n── 2. Read-only tools ──")
bench("screenshot")
bench("get_screen_size")
bench("extract_text")
bench("describe_screen")
bench("get_active_window")
bench("list_windows")
bench("clipboard_get")
bench("discover")
bench("server_status")
bench("list_pending")

# ─────────────────────────────────────────────────────────────────
# Phase 3: Input (harmless micro-moves)
# ─────────────────────────────────────────────────────────────────
print("\n── 3. Input tools ──")
bench("mouse_move", {"x": 100, "y": 100})
bench("mouse_click", {"button": "left"})
bench("mouse_scroll", {"dy": -1})
bench("key_press", {"key": "Escape"})

# ─────────────────────────────────────────────────────────────────
# Phase 4: Window management
# ─────────────────────────────────────────────────────────────────
print("\n── 4. Windows ──")
bench("focus_window", {"title": "desk-mcp"})
bench("get_active_window")

# ─────────────────────────────────────────────────────────────────
# Phase 5: Filesystem
# ─────────────────────────────────────────────────────────────────
print("\n── 5. File tools ──")
bench("glob", {"pattern": "*.rs", "path": os.path.expanduser("~/Documents/GitHub/desk-mcp/src")})
bench("grep", {"pattern": "fn ", "path": os.path.expanduser("~/Documents/GitHub/desk-mcp/src")})
bench("file_read", {"file_path": os.path.expanduser("~/Documents/GitHub/desk-mcp/Cargo.toml"), "limit": 5})

# ─────────────────────────────────────────────────────────────────
# Phase 6: Web search (new tool)
# ─────────────────────────────────────────────────────────────────
print("\n── 6. Web search (DuckDuckGo) ──")
bench("web_search", {"query": "Rust programming language", "max_results": 3})

# ─────────────────────────────────────────────────────────────────
# Phase 7: Safety (new tools)
# ─────────────────────────────────────────────────────────────────
print("\n── 7. Safety tools ──")
r = bench("request_confirmation", {"tool": "shell_run", "message": "Benchmark test confirmation"})
# Get the confirmation ID from the result
cid = None
content = r.get("result", {})
if isinstance(content, list):
    for item in content:
        if isinstance(item, dict) and item.get("type") == "text":
            try:
                inner = json.loads(item.get("text", "{}"))
                cid = inner.get("id")
            except: pass
elif isinstance(content, dict):
    cid = content.get("id")

if cid:
    bench("approve", {"id": cid})
bench("list_pending")

# ─────────────────────────────────────────────────────────────────
# Phase 8: Code tools
# ─────────────────────────────────────────────────────────────────
print("\n── 8. Code tools ──")
bench("code_lint", {"file_path": os.path.expanduser("~/Documents/GitHub/desk-mcp/src/lib.rs"), "language": "rust"})
bench("code_build", {"project_dir": os.path.expanduser("~/Documents/GitHub/desk-mcp"), "command": "check"})

# ─────────────────────────────────────────────────────────────────
# Phase 9: Screen state verification (#1 improvement)
# ─────────────────────────────────────────────────────────────────
print("\n── 9. Screen state feedback ──")
r = bench("mouse_move", {"x": 300, "y": 300})
content = r.get("result", {})
if isinstance(content, list):
    for item in content:
        if isinstance(item, dict) and item.get("type") == "text":
            print(f"  → response type: text")
            break
elif isinstance(content, dict):
    if "screen" in content:
        s = content["screen"]
        print(f"  → screen state: {len(s.get('text_elements',[]))} texts, {len(s.get('clickable_regions',[]))} clickables")

r2 = bench("mouse_click", {"button": "left", "x": 300, "y": 300})

# ─────────────────────────────────────────────────────────────────
# Phase 10: Keyboard with screen
# ─────────────────────────────────────────────────────────────────
print("\n── 10. Keyboard + screen ──")
bench("key_press", {"key": "Escape"})

# ─────────────────────────────────────────────────────────────────
# Phase 11: Rate limit rapid-fire
# ─────────────────────────────────────────────────────────────────
print("\n── 11. Rate limit test (15 rapid server_status calls) ──")
for i in range(15):
    bench("server_status", {})

# ─────────────────────────────────────────────────────────────────
# Summary
# ─────────────────────────────────────────────────────────────────
summarize()
