#!/usr/bin/env python3
"""Minimal desk-mcp test - just init + env_get without browser."""
import subprocess, json, os, sys, select

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
        while line in (b'\r\n', b'\n', b''): line = p.stdout.readline()
        while True:
            d = line.decode().strip()
            if d.startswith('Content-Length:'):
                cl = int(d.split(':',1)[1].strip())
                break
            line = p.stdout.readline()
        p.stdout.readline()
        return json.loads(p.stdout.read(cl))
    return None

print('Init...', flush=True)
send({'jsonrpc':'2.0','id':'i','method':'initialize',
      'params':{'protocolVersion':'2024-11-05','clientInfo':{'name':'t','version':'1'}}})
r = recv()
print(f'Init: {"OK" if r else "FAIL"}', flush=True)
if r:
    print(f'  Server: {r["result"]["serverInfo"]["name"]}', flush=True)

send({'jsonrpc':'2.0','method':'notifications/initialized'})

# Test several lightweight tools
for tool, args in [
    ('server_status', {}),
    ('env_get', {'name': 'HOME'}),
    ('clipboard_read', {}),
    ('window_list', {}),
]:
    print(f'Calling {tool}...', flush=True)
    send({'jsonrpc':'2.0','id':tool,'method':'tools/call',
          'params':{'name':tool,'arguments':args}})
    r = recv()
    if r and 'result' in r:
        ct = json.loads(r['result']['content'][0]['text'])
        status = 'OK' if ct.get('ok') else f'ERR: {ct.get("error","?")[:80]}'
        print(f'  {tool}: {status}', flush=True)
    else:
        print(f'  {tool}: FAIL (no response)', flush=True)

err = p.stderr.read().decode()
if err:
    print(f'\nStderr:\n{err[-500:]}', flush=True)

p.terminate()
p.wait()
print('Done', flush=True)
