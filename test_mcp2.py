#!/usr/bin/env python3
"""Final comprehensive desk-mcp test — all improvements."""
import subprocess, json, os, sys

env = os.environ.copy()
env.update({"ALLOW_CODE": "1", "ALLOW_SHELL": "1", "RUST_LOG": "warn"})
proc = subprocess.Popen(["./target/release/desk-mcp"],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)

def send(msg):
    body = json.dumps(msg)
    proc.stdin.write(f"Content-Length: {len(body)}\r\n\r\n{body}".encode())
    proc.stdin.flush()

def recv():
    line = proc.stdout.readline()
    while line in (b"\r\n", b"\n", b""):
        line = proc.stdout.readline()
        if not line: return None
    cl = 0
    while True:
        d = line.decode().strip()
        if d.startswith("Content-Length:"):
            cl = int(d.split(":",1)[1].strip())
            break
        line = proc.stdout.readline()
        if not line: return None
    proc.stdout.readline()
    return json.loads(proc.stdout.read(cl))

def call(name, args=None):
    send({"jsonrpc":"2.0","id":name,"method":"tools/call",
          "params":{"name":name,"arguments":args or {}}})
    r = recv()
    if r and "result" in r and "content" in r["result"]:
        return json.loads(r["result"]["content"][0]["text"])
    return r

def check(desc, condition, detail=""):
    if condition:
        print(f"  \033[32mPASS\033[0m {desc}")
        return True
    else:
        print(f"  \033[31mFAIL\033[0m {desc} {detail}")
        return False

send({"jsonrpc":"2.0","id":"i","method":"initialize",
      "params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"test","version":"1"}}})
recv()
send({"jsonrpc":"2.0","method":"notifications/initialized"})

passed = 0; total = 0

# 1. Tool count
print("\n=== New Tools ===")
send({"jsonrpc":"2.0","id":"l","method":"tools/list"})
r = recv()
tools = r["result"]["tools"]
names = [t["name"] for t in tools]
total += 1; passed += check(f"{len(tools)} tools (was 62, now 64)", len(tools) == 64)
total += 1; passed += check("env_get registered", "env_get" in names)
total += 1; passed += check("web_fetch registered", "web_fetch" in names)

# 2. env_get
print("\n=== env_get ===")
r = call("env_get", {"name": "HOME"})
total += 1; passed += check("Reads HOME", r.get("ok") and r.get("result",{}).get("value","") != "")
r = call("env_get", {"name": "NONEXISTENT_XYZ"})
total += 1; passed += check("Returns empty for missing", r.get("ok") and r.get("result",{}).get("value","") == "")

# 3. File tools with CWD workspace
print("\n=== CWD Workspace ===")
r = call("file_read", {"path": "Cargo.toml", "lines": 1})
total += 1; passed += check("file_read works without DESKMCP_WORKSPACE",
    r.get("ok") and r.get("result",{}).get("content",[{}])[0].get("text","") == "[package]")

# 4. web_fetch
print("\n=== web_fetch ===")
r = call("web_fetch", {"url": "https://example.com", "format": "text", "max_bytes": 3000})
total += 1; passed += check("Fetches example.com (200)", r.get("ok") and r.get("result",{}).get("status") == 200)
content = r.get("result",{}).get("content","")
total += 1; passed += check("Strips style/script tags", r.get("ok") and "body{" not in content and "Example Domain" in content)
r = call("web_fetch", {"url": "not-a-url"})
total += 1; passed += check("Rejects invalid URL", not r.get("ok"))

# 5. Unknown tool suggestions
print("\n=== Error Messages ===")
r = call("env_gett", {})  # typo
total += 1; passed += check("Suggests env_get for typo",
    not r.get("ok") and "env_get" in str(r.get("error","")))

# 6. Browser launch (headless)
print("\n=== Browser Launch ===")
print("  (launching headless Chrome — this may take 5-15s)...")
r = call("browser_launch", {"mode": "headless"})
total += 1; passed += check("Browser launches in headless mode",
    r.get("ok") and r.get("result",{}).get("connected") == True)
if r.get("ok"):
    # Verify it actually works
    r2 = call("browser_navigate", {"url": "https://example.com"})
    total += 1; passed += check("Can navigate to example.com",
        r2.get("ok") and "Example" in str(r2.get("result",{}).get("title","")))
    r3 = call("browser_exec_js", {"code": "document.title"})
    total += 1; passed += check("JS execution works", r3.get("ok"))

print(f"\n{'='*60}")
print(f"Results: {passed}/{total} passed")
print("ALL PASSED!" if passed == total else f"{total-passed} failures")
proc.terminate()
proc.wait()
