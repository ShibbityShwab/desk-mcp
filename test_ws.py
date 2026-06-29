import asyncio, json
# Use raw TCP + WebSocket handshake manually since no websockets library
import socket, struct, os, hashlib, base64

def ws_connect(url):
    """Minimal async WebSocket connect for testing."""
    from urllib.parse import urlparse
    p = urlparse(url)
    host = p.hostname
    port = p.port or 80
    path = p.path + ('?' + p.query if p.query else '')
    
    sock = socket.create_connection((host, port), timeout=5)
    
    # WebSocket handshake
    key = base64.b64encode(os.urandom(16)).decode()
    req = (
        f"GET {path} HTTP/1.1\r\n"
        f"Host: {host}:{port}\r\n"
        f"Upgrade: websocket\r\n"
        f"Connection: Upgrade\r\n"
        f"Sec-WebSocket-Key: {key}\r\n"
        f"Sec-WebSocket-Version: 13\r\n"
        f"\r\n"
    )
    sock.sendall(req.encode())
    
    resp = b""
    while b"\r\n\r\n" not in resp:
        resp += sock.recv(4096)
    
    if b"101" not in resp.split(b"\r\n")[0]:
        raise Exception(f"Handshake failed: {resp.split(b'\r\n')[0]}")
    
    print("WebSocket handshake OK!")
    
    # Send a CDP command
    msg = json.dumps({"id": 1, "method": "Browser.getVersion"})
    frame = bytearray()
    frame.append(0x81)  # FIN + text
    frame.append(0x80 | len(msg))  # MASK + length
    mask = os.urandom(4)
    frame.extend(mask)
    frame.extend(bytes(b ^ mask[i % 4] for i, b in enumerate(msg.encode())))
    sock.sendall(frame)
    
    # Read response
    data = sock.recv(4096)
    print(f"Raw response ({len(data)} bytes): {data[:200].hex()}")
    
    # Parse WebSocket frame
    if len(data) >= 2:
        opcode = data[0] & 0x0F
        masked = (data[1] & 0x80) != 0
        plen = data[1] & 0x7F
        offset = 2
        if plen == 126:
            plen = struct.unpack('>H', data[2:4])[0]
            offset = 4
        elif plen == 127:
            plen = struct.unpack('>Q', data[2:10])[0]
            offset = 10
        if masked:
            mask = data[offset:offset+4]
            offset += 4
            payload = bytes(b ^ mask[i % 4] for i, b in enumerate(data[offset:offset+plen]))
        else:
            payload = data[offset:offset+plen]
        
        if opcode == 0x01:
            print(f"CDP text response: {payload.decode()}")
        else:
            print(f"Opcode: {opcode}, payload: {payload[:200]}")
    
    sock.close()

ws_connect("ws://localhost:41555/devtools/browser/7febef79-1ef2-4237-90a7-5743d4b53bc9")
