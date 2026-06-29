#!/usr/bin/env python3
"""Test only browser_launch in isolation."""
import subprocess, json, os, sys, select

env = os.environ.copy()
env.update({'ALLOW_CODE':'1','ALLOW_SHELL':'1','RUST_LOG':'info'})
p = subprocess.Popen(['./target/release/desk-mcp'],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)

def send(msg):
    body = json.dumps(msg)
    p.stdin.write(f'Content-Length: {len(body)}\r\n\r\n{body}'.encode())
    p.stdin.flush()

def recv(timeout=45):
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
assert r, "No init response"
print(f'Server: {r["result"]["serverInfo"]["name"]}', flush=True)
send({'jsonrpc':'2.0','method':'notifications/initialized'})

# First check if chrome is detected
print('Checking discovery...', flush=True)
send({'jsonrpc':'2.0','id':'d','method':'tools/call',
      'params':{'name':'server_status','arguments':{}}})
r = recv()
if r:
    ct = json.loads(r['result']['content'][0]['text'])
    res = ct.get('result', {})
    print(f'  Display: {res.get("display","?")}', flush=True)
    print(f'  Browsers installed: {res.get("installed_browsers",[])}', flush=True)
    print(f'  Running browsers: {res.get("running_browsers",0)}', flush=True)

# Try browser launch
print('Launching headless browser...', flush=True)
send({'jsonrpc':'2.0','id':'b','method':'tools/call',
      'params':{'name':'browser_launch','arguments':{'mode':'headless'}}})
r = recv(45)
if r:
    ct = json.loads(r['result']['content'][0]['text'])
    print(f'  ok: {ct.get("ok")}', flush=True)
    if ct.get('ok'):
        res = ct.get('result', {})
        print(f'  connected: {res.get("connected")}', flush=True)
        print(f'  url: {res.get("url","?")}', flush=True)
    else:
        err = ct.get('error', '?')
        if isinstance(err, list):
            err = err[0] if err else '?'
        print(f'  error: {str(err)[:300]}', flush=True)
else:
    print('  FAIL: No response (timeout)', flush=True)

err_out = p.stderr.read().decode()
if err_out:
    print(f'\nStderr:\n{err_out[-2000:]}', flush=True)

p.terminate()
p.wait()
print('Done', flush=True)
