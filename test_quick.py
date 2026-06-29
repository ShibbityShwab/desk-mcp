import subprocess, json, os, sys, time
env = os.environ.copy()
env.update({'ALLOW_CODE':'1','ALLOW_SHELL':'1','RUST_LOG':'info','DISPLAY':':0','XAUTHORITY':'/run/user/1000/xauth_XLYcRo'})
p = subprocess.Popen(['./target/release/desk-mcp'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)

s=lambda m:(p.stdin.write(f"Content-Length: {len(json.dumps(m))}\r\n\r\n{json.dumps(m)}".encode()), p.stdin.flush())
def r(timeout=5):
    import select
    if select.select([p.stdout], [], [], timeout)[0]:
        l = p.stdout.readline()
        while l in(b'\r\n',b'\n',b''): l = p.stdout.readline()
        while True:
            d = l.decode().strip()
            if d.startswith('Content-Length:'):
                cl = int(d.split(':',1)[1].strip())
                break
            l = p.stdout.readline()
        p.stdout.readline()
        return json.loads(p.stdout.read(cl))
    return None

def c(name, args=None):
    s({"jsonrpc":"2.0","id":name,"method":"tools/call","params":{"name":name,"arguments":args or {}}})
    return r(10)

print("Init...", flush=True)
s({"jsonrpc":"2.0","id":"i","method":"initialize","params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"t","version":"1"}}})
resp = r()
print(f"Server: {resp['result']['serverInfo']['name']}" if resp else "NO RESPONSE", flush=True)
s({"jsonrpc":"2.0","method":"notifications/initialized"})

print("Status...", flush=True)
st = c("server_status")
if st:
    ct = json.loads(st["result"]["content"][0]["text"])
    print(f"Tool: {ct['result']['screenshot_tool']}, provider: {ct['result']['provider']}", flush=True)
else:
    print("NO STATUS RESPONSE", flush=True)

print("Screenshot...", flush=True)
ss = c("screenshot")
if ss:
    ct = json.loads(ss["result"]["content"][0]["text"])
    if ct.get("ok"):
        print(f"CAPTURED! Size: {len(ct['result'].get('data',''))} chars", flush=True)
    else:
        print(f"Error: {json.dumps(ct.get('error','?'))[:200]}", flush=True)
else:
    print("TIMEOUT - screenshot hung", flush=True)

err = p.stderr.read().decode()
if err:
    print(f"\nStderr: {err[-400:]}", flush=True)
p.terminate()
