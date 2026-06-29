#!/usr/bin/env python3
"""Isolated browser_launch test - 25s timeout + stderr capture."""
import subprocess, json, os, sys, select, time

env = os.environ.copy()
env.update({'ALLOW_CODE':'1','ALLOW_SHELL':'1','RUST_LOG':'info'})

print('Starting desk-mcp...', flush=True)
p = subprocess.Popen(['./target/release/desk-mcp'],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)

def send(msg):
    body = json.dumps(msg)
    p.stdin.write(f'Content-Length: {len(body)}\r\n\r\n{body}'.encode())
    p.stdin.flush()

def recv(timeout=10):
    if select.select([p.stdout], [], [], timeout)[0]:
        line = p.stdout.readline()
        while line in (b'\r\n', b'\n', b''):
            if not line: return None
            line = p.stdout.readline()
        if not line: return None
        while True:
            d = line.decode().strip()
            if d.startswith('Content-Length:'):
                cl = int(d.split(':',1)[1].strip())
                break
            line = p.stdout.readline()
            if not line: return None
        p.stdout.readline()
        return json.loads(p.stdout.read(cl))
    return None

send({'jsonrpc':'2.0','id':'i','method':'initialize',
      'params':{'protocolVersion':'2024-11-05','clientInfo':{'name':'t','version':'1'}}})
r = recv()
print(f'Init: {"OK" if r else "FAIL"}', flush=True)
send({'jsonrpc':'2.0','method':'notifications/initialized'})

print('Calling browser_launch...', flush=True)
send({'jsonrpc':'2.0','id':'b','method':'tools/call',
      'params':{'name':'browser_launch','arguments':{'mode':'headless'}}})

r = recv(25)
if r:
    ct = json.loads(r['result']['content'][0]['text'])
    print(f'Result ok={ct.get("ok")}', flush=True)
    if not ct.get('ok'):
        err = ct.get('error', '?')
        print(f'Error: {str(err)[:300]}', flush=True)
    else:
        res = ct.get('result', {})
        print(f'Connected! url={res.get("url","?")}', flush=True)
else:
    print('TIMEOUT after 25s', flush=True)

p.terminate()
try:
    p.wait(timeout=3)
except:
    p.kill()

import fcntl
fd = p.stderr.fileno()
fl = fcntl.fcntl(fd, fcntl.F_GETFL)
fcntl.fcntl(fd, fcntl.F_SETFL, fl | os.O_NONBLOCK)
try:
    err = p.stderr.read().decode()
    if err:
        print(f'\nStderr:\n{err[-2000:]}\n===', flush=True)
except:
    pass
print('Done', flush=True)
