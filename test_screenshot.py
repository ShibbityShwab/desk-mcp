#!/usr/bin/env python3
"""Take a screenshot and describe it using desk-mcp."""
import subprocess, json, os, select

env = os.environ.copy()
env.update({'ALLOW_CODE':'1','ALLOW_SHELL':'1','RUST_LOG':'warn'})
p = subprocess.Popen(['./target/release/desk-mcp'],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)

def send(msg):
    body = json.dumps(msg)
    p.stdin.write(f'Content-Length: {len(body)}\r\n\r\n{body}'.encode())
    p.stdin.flush()

def recv(timeout=30):
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

def call(name, args=None):
    send({'jsonrpc':'2.0','id':name,'method':'tools/call',
          'params':{'name':name,'arguments':args or {}}})
    r = recv(30)
    if r and 'result' in r:
        return json.loads(r['result']['content'][0]['text'])
    return None

# Init
print('Initializing...', flush=True)
send({'jsonrpc':'2.0','id':'i','method':'initialize',
      'params':{'protocolVersion':'2024-11-05','clientInfo':{'name':'test','version':'1'}}})
recv()
send({'jsonrpc':'2.0','method':'notifications/initialized'})

# Check server status first
print('\n--- Server Status ---', flush=True)
st = call('server_status')
if st and st.get('ok'):
    r = st.get('result', {})
    print(f"  Provider: {r.get('provider', '?')}", flush=True)
    print(f"  Display:  {r.get('display_type', '?')}", flush=True)
    print(f"  Desktop:  {r.get('desktop', '?')}", flush=True)
    print(f"  Screenshot tool: {r.get('screenshot_tool', '?')}", flush=True)
    print(f"  Screenshot available: {r.get('available', {}).get('screenshot', False)}", flush=True)

# Take screenshot
print('\n--- Screenshot ---', flush=True)
ss = call('screenshot')
if ss and ss.get('ok'):
    data = ss.get('result', {}).get('data', '')
    data_len = len(data) if data else 0
    print(f'  Screenshot captured: {data_len} chars base64 (~{data_len*3//4:,} bytes PNG)', flush=True)
    
    # Save screenshot
    import base64
    if data:
        img_data = base64.b64decode(data)
        with open('/tmp/desk-mcp-screenshot.png', 'wb') as f:
            f.write(img_data)
        print(f'  Saved to /tmp/desk-mcp-screenshot.png ({len(img_data):,} bytes)', flush=True)
else:
    err = ss.get('error', '?') if ss else 'no response'
    print(f'  Screenshot FAILED: {str(err)[:200]}', flush=True)

# Describe screen (screenshot + OCR)
print('\n--- Describe Screen (OCR) ---', flush=True)
ds = call('describe_screen')
if ds and ds.get('ok'):
    text = ds.get('result', {}).get('text', '')
    if text:
        print(f'  OCR text found ({len(text)} chars):', flush=True)
        print(f'  {text[:500]}', flush=True)
    else:
        print('  No text detected by OCR', flush=True)
else:
    err = ds.get('error', '?') if ds else 'no response'
    print(f'  Describe screen FAILED: {str(err)[:200]}', flush=True)

# Also try extract_text
print('\n--- Extract Text ---', flush=True)
et = call('extract_text')
if et and et.get('ok'):
    text = et.get('result', {}).get('text', '')
    if text:
        print(f'  Text found ({len(text)} chars):', flush=True)
        print(f'  {text[:500]}', flush=True)
    else:
        print('  No text detected', flush=True)
else:
    err = et.get('error', '?') if et else 'no response'
    print(f'  Extract text FAILED: {str(err)[:200]}', flush=True)

p.terminate()
p.wait()
print('\nDone!', flush=True)
