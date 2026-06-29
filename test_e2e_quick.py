#!/usr/bin/env python3
"""Quick e2e smoke test for desk-mcp Phase 8 verification."""
import subprocess, json, os, sys, time, select

env = os.environ.copy()
env.update({'ALLOW_CODE':'1','ALLOW_SHELL':'1','RUST_LOG':'warn'})
p = subprocess.Popen(['./target/release/desk-mcp'],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)

def send(msg):
    body = json.dumps(msg)
    p.stdin.write(f'Content-Length: {len(body)}\r\n\r\n{body}'.encode())
    p.stdin.flush()

def recv(timeout=12):
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

passed = 0
total = 0

def check(desc, condition):
    global passed, total
    total += 1
    if condition:
        print(f'  PASS: {desc}', flush=True)
        passed += 1
    else:
        print(f'  FAIL: {desc}', flush=True)

# 1. Initialize
print('1. Initialize...', flush=True)
send({'jsonrpc':'2.0','id':'i','method':'initialize',
      'params':{'protocolVersion':'2024-11-05','clientInfo':{'name':'t','version':'1'}}})
r = recv()
check('Init handshake succeeds', r is not None and 'result' in r)
if r is None:
    print('FATAL: no init response', flush=True)
    sys.exit(1)
print(f'   Server: {r["result"]["serverInfo"]["name"]} v{r["result"]["serverInfo"]["version"]}', flush=True)
send({'jsonrpc':'2.0','method':'notifications/initialized'})

# 2. server_status
print('2. server_status...', flush=True)
send({'jsonrpc':'2.0','id':'s','method':'tools/call',
      'params':{'name':'server_status','arguments':{}}})
r = recv()
check('server_status responds', r is not None)
if r:
    ct = json.loads(r['result']['content'][0]['text'])
    check('server_status returns ok', ct.get('ok'))

# 3. Tools list
print('3. tools/list...', flush=True)
send({'jsonrpc':'2.0','id':'l','method':'tools/list'})
r = recv()
check('tools/list responds', r is not None)
if r:
    names = [t['name'] for t in r['result']['tools']]
    check('Tool count > 20', len(names) > 20)
    check('browser_launch exists', 'browser_launch' in names)
    check('screenshot exists', 'screenshot' in names)
    check('find_elements exists (a11y)', 'find_elements' in names)
    check('no discovery tools in list', 'discover' not in names)
    print(f'   Tools: {len(names)}', flush=True)

# 4. screenshot
print('4. screenshot...', flush=True)
send({'jsonrpc':'2.0','id':'ss','method':'tools/call',
      'params':{'name':'screenshot','arguments':{}}})
r = recv(20)
check('screenshot responds', r is not None)
if r:
    ct = json.loads(r['result']['content'][0]['text'])
    check('screenshot returns ok or graceful error', 
          ct.get('ok') or 'error' in ct)

# 5. env_get
print('5. env_get...', flush=True)
send({'jsonrpc':'2.0','id':'e','method':'tools/call',
      'params':{'name':'env_get','arguments':{'name':'HOME'}}})
r = recv()
check('env_get responds', r is not None)
if r:
    ct = json.loads(r['result']['content'][0]['text'])
    check('env_get HOME returns value', 
          ct.get('ok') and ct.get('result',{}).get('value','') != '')

# 6. Browser launch (headless)
print('6. browser_launch (headless)...', flush=True)
print('   This may take 10-20s...', flush=True)
send({'jsonrpc':'2.0','id':'b','method':'tools/call',
      'params':{'name':'browser_launch','arguments':{'mode':'headless'}}})
r = recv(30)
check('browser_launch responds', r is not None)
if r:
    ct = json.loads(r['result']['content'][0]['text'])
    check('browser launches successfully',
          ct.get('ok') and ct.get('result',{}).get('connected') == True)
    if ct.get('ok'):
        # 6a. Navigate
        print('6a. browser_navigate...', flush=True)
        send({'jsonrpc':'2.0','id':'bn','method':'tools/call',
              'params':{'name':'browser_navigate',
                        'arguments':{'url':'https://example.com'}}})
        r2 = recv(20)
        check('browser_navigate works',
              r2 is not None and json.loads(r2['result']['content'][0]['text']).get('ok'))
        # 6b. Exec JS
        print('6b. browser_exec_js...', flush=True)
        send({'jsonrpc':'2.0','id':'be','method':'tools/call',
              'params':{'name':'browser_exec_js',
                        'arguments':{'code':'document.title'}}})
        r3 = recv(10)
        check('browser_exec_js works', r3 is not None)

# Done
print(f'\n{"="*60}')
print(f'Results: {passed}/{total} passed')
if passed == total:
    print('ALL PASSED!')
else:
    print(f'{total-passed} FAILURES')
    # Dump stderr
    err = p.stderr.read().decode()
    if err:
        print(f'\nServer stderr (last 1000 chars):\n{err[-1000:]}')

p.terminate()
p.wait()
sys.exit(0 if passed == total else 1)
