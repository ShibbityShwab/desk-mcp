#!/usr/bin/env python3
"""Phase 8 quick verification."""
import subprocess, json, os, select

env = os.environ.copy()
env.update({'ALLOW_CODE':'1','ALLOW_SHELL':'1','RUST_LOG':'warn'})
p = subprocess.Popen(['./target/release/desk-mcp'],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)

def send(msg):
    body = json.dumps(msg)
    p.stdin.write(f'Content-Length: {len(body)}\r\n\r\n{body}'.encode())
    p.stdin.flush()

def recv(timeout=10):
    if select.select([p.stdout], [], [], timeout)[0]:
        line = p.stdout.readline()
        while line in (b'\r\n',b'\n',b''): line = p.stdout.readline()
        while True:
            d = line.decode().strip()
            if d.startswith('Content-Length:'):
                cl = int(d.split(':',1)[1].strip())
                break
            line = p.stdout.readline()
        p.stdout.readline()
        return json.loads(p.stdout.read(cl))
    return None

send({'jsonrpc':'2.0','id':'i','method':'initialize',
      'params':{'protocolVersion':'2024-11-05','clientInfo':{'name':'e2e','version':'1'}}})
recv()
send({'jsonrpc':'2.0','method':'notifications/initialized'})

passed = 0
total = 0

def check(desc, ok):
    global passed, total
    total += 1
    print(f'  {"PASS" if ok else "FAIL"}: {desc}', flush=True)
    if ok: passed += 1

# Tool list checks
send({'jsonrpc':'2.0','id':'l','method':'tools/list'})
r = recv()
tools = [t['name'] for t in r['result']['tools']]
check('discover removed', 'discover' not in tools)
check('server_status exists', 'server_status' in tools)
check('find_elements exists', 'find_elements' in tools)
check(f'63 tools', len(tools) == 63)

# Quick tools
for tool, args in [
    ('screenshot', {}),
    ('env_get', {'name': 'HOME'}),
    ('window_list', {}),
]:
    send({'jsonrpc':'2.0','id':tool,'method':'tools/call',
          'params':{'name':tool,'arguments':args}})
    r = recv(15)
    ct = json.loads(r['result']['content'][0]['text']) if r else {}
    check(f'{tool} ok', r is not None and (ct.get('ok') or 'error' in ct))

p.terminate()
p.wait()
print(f'\n{passed}/{total} passed', flush=True)
