// desk-mcp Bridge — Chrome Extension Service Worker
//
// Opens a WebSocket-compatible TCP server on port 9224. The desk-mcp Rust
// provider connects as a WebSocket client, sends JSON commands, and this
// worker dispatches them using Chrome Extension / CDP APIs.
//
// Supported commands:
//   screenshot     → chrome.tabs.captureVisibleTab() → base64 PNG
//   get_screen_size → window.screen
//   click          → CDP Input.dispatchMouseEvent
//   mouse_move     → CDP Input.dispatchMouseEvent (mouseMoved)
//   scroll         → CDP Input.dispatchMouseEvent (mouseWheel)
//   drag           → CDP Input.dispatchMouseEvent sequence
//   type           → CDP Input.insertText
//   key_press      → CDP Input.dispatchKeyEvent
//   clipboard_get  → navigator.clipboard.readText()
//   clipboard_set  → navigator.clipboard.writeText()
//   list_windows   → chrome.windows.getAll()
//   focus_window   → chrome.windows.update(focused: true)
//   get_active_window → chrome.windows.getCurrent() / getLastFocused()
//   get_elements   → CDP Accessibility.getFullAXTree
//   notify         → chrome.notifications.create()

const LISTEN_PORT = 9224;
const LISTEN_HOST = '127.0.0.1';

// ── WebSocket constants ───────────────────────────────────────────────────
const WS_GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';
const OP_TEXT = 0x1;
const OP_CLOSE = 0x8;
const OP_PING = 0x9;
const OP_PONG = 0xA;

// ── Per-connection state ──────────────────────────────────────────────────
// socketId → { buffer: string, handshakeDone: bool }
const connections = new Map();

// ── CDP helper ────────────────────────────────────────────────────────────
let debuggerTabId = null;

async function ensureDebugger() {
  if (debuggerTabId !== null) {
    try {
      // Quick check if debugger is still attached
      await chrome.debugger.getTargets();
      return debuggerTabId;
    } catch (_) {
      debuggerTabId = null;
    }
  }

  // Find or create a debuggable tab
  const tabs = await chrome.tabs.query({ active: true, currentWindow: true });
  let target = tabs[0];
  if (!target) {
    const allWindows = await chrome.windows.getAll({ populate: true });
    for (const win of allWindows) {
      if (win.tabs && win.tabs.length > 0) {
        target = win.tabs[0];
        break;
      }
    }
  }
  if (!target) {
    throw new Error('No tabs available for debugging');
  }

  try {
    await chrome.debugger.attach({ tabId: target.id }, '1.3');
    debuggerTabId = target.id;
    console.log(`[desk-mcp] debugger attached to tab ${target.id}`);
  } catch (e) {
    console.error(`[desk-mcp] debugger attach failed: ${e.message}`);
    throw e;
  }
  return debuggerTabId;
}

// ── SHA-1 (minimal implementation) ────────────────────────────────────────
// For Sec-WebSocket-Accept header. Chrome extensions don't have crypto.subtle
// in service workers for SHA-1, so we implement it inline.

function sha1(str) {
  function rotateLeft(n, s) { return (n << s) | (n >>> (32 - s)); }

  const utf8 = unescape(encodeURIComponent(str));
  const msg = [];
  for (let i = 0; i < utf8.length; i++) {
    msg.push(utf8.charCodeAt(i));
  }

  // Padding
  const ml = msg.length * 8;
  msg.push(0x80);
  while ((msg.length % 64) !== 56) msg.push(0);
  for (let i = 0; i < 4; i++) {
    msg.push((ml >>> (24 - i * 8)) & 0xff);
  }

  // Process
  let h0 = 0x67452301, h1 = 0xEFCDAB89, h2 = 0x98BADCFE, h3 = 0x10325476, h4 = 0xC3D2E1F0;

  for (let i = 0; i < msg.length; i += 64) {
    const w = new Array(80);
    for (let t = 0; t < 16; t++) {
      w[t] = (msg[i + t * 4] << 24) | (msg[i + t * 4 + 1] << 16) |
             (msg[i + t * 4 + 2] << 8) | msg[i + t * 4 + 3];
    }
    for (let t = 16; t < 80; t++) {
      w[t] = rotateLeft(w[t - 3] ^ w[t - 8] ^ w[t - 14] ^ w[t - 16], 1);
    }

    let a = h0, b = h1, c = h2, d = h3, e = h4;

    for (let t = 0; t < 80; t++) {
      let f, k;
      if (t < 20) { f = (b & c) | (~b & d); k = 0x5A827999; }
      else if (t < 40) { f = b ^ c ^ d; k = 0x6ED9EBA1; }
      else if (t < 60) { f = (b & c) | (b & d) | (c & d); k = 0x8F1BBCDC; }
      else { f = b ^ c ^ d; k = 0xCA62C1D6; }

      const temp = (rotateLeft(a, 5) + f + e + k + w[t]) >>> 0;
      e = d; d = c; c = rotateLeft(b, 30) >>> 0; b = a; a = temp >>> 0;
    }

    h0 = (h0 + a) >>> 0; h1 = (h1 + b) >>> 0; h2 = (h2 + c) >>> 0;
    h3 = (h3 + d) >>> 0; h4 = (h4 + e) >>> 0;
  }

  // Output as hex
  const hex = (n) => ('0000000' + n.toString(16)).slice(-8);
  return hex(h0) + hex(h1) + hex(h2) + hex(h3) + hex(h4);
}

function base64Encode(str) {
  // Browser-compatible base64
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  let result = '';
  let padding = '';
  const bytes = [];
  for (let i = 0; i < str.length; i++) {
    bytes.push(str.charCodeAt(i) & 0xff);
  }
  const rem = bytes.length % 3;
  if (rem === 1) { bytes.push(0, 0); padding = '=='; }
  else if (rem === 2) { bytes.push(0); padding = '='; }

  for (let i = 0; i < bytes.length; i += 3) {
    const a = bytes[i], b = bytes[i + 1], c = bytes[i + 2];
    result += chars[a >> 2];
    result += chars[((a & 3) << 4) | (b >> 4)];
    result += chars[((b & 15) << 2) | (c >> 6)];
    result += chars[c & 63];
  }
  return result.substring(0, result.length - padding.length) + padding;
}

// ── WebSocket frame helpers ───────────────────────────────────────────────

function buildFrame(payload, isBinary) {
  const opcode = isBinary ? 0x2 : OP_TEXT;
  const bytes = [];
  bytes.push(0x80 | opcode); // FIN + opcode (server → client, unmasked)

  const len = payload.length;
  if (len <= 125) {
    bytes.push(len);
  } else if (len <= 65535) {
    bytes.push(126);
    bytes.push((len >> 8) & 0xff);
    bytes.push(len & 0xff);
  } else {
    bytes.push(127);
    for (let i = 7; i >= 0; i--) {
      bytes.push((len >> (i * 8)) & 0xff);
    }
  }

  // Append payload
  const payloadBytes = typeof payload === 'string'
    ? new TextEncoder().encode(payload)
    : new Uint8Array(payload);
  const combined = new Uint8Array(bytes.length + payloadBytes.length);
  combined.set(bytes, 0);
  combined.set(payloadBytes, bytes.length);
  return combined;
}

// ── Command handlers ──────────────────────────────────────────────────────

async function handleCommand(cmd) {
  const { id, method, params } = cmd;

  try {
    let result;
    switch (method) {
      // ── Screenshot ──────────────────────────────────────────
      case 'screenshot': {
        const dataUrl = await chrome.tabs.captureVisibleTab(null, { format: 'png' });
        // dataUrl format: "data:image/png;base64,XXXX"
        const b64 = dataUrl.split(',')[1] || '';
        result = { data: b64, format: 'png' };
        break;
      }

      case 'get_screen_size': {
        const tabs = await chrome.tabs.query({ active: true, currentWindow: true });
        result = {
          width: window.screen?.width || 1920,
          height: window.screen?.height || 1080,
          tabId: tabs[0]?.id || null,
        };
        break;
      }

      // ── Mouse ──────────────────────────────────────────────
      case 'click': {
        const tabId = await ensureDebugger();
        const button = params?.button || 'left';
        const clicks = params?.clicks || 1;
        const x = params?.x || 0;
        const y = params?.y || 0;

        for (let i = 0; i < clicks; i++) {
          await chrome.debugger.sendCommand(
            { tabId },
            'Input.dispatchMouseEvent',
            { type: 'mousePressed', x, y, button, clickCount: 1 }
          );
          await chrome.debugger.sendCommand(
            { tabId },
            'Input.dispatchMouseEvent',
            { type: 'mouseReleased', x, y, button, clickCount: 1 }
          );
        }
        result = { success: true, x, y, button, clicks };
        break;
      }

      case 'mouse_move': {
        const tabId = await ensureDebugger();
        await chrome.debugger.sendCommand(
          { tabId },
          'Input.dispatchMouseEvent',
          { type: 'mouseMoved', x: params?.x || 0, y: params?.y || 0 }
        );
        result = { success: true };
        break;
      }

      case 'scroll': {
        const tabId = await ensureDebugger();
        await chrome.debugger.sendCommand(
          { tabId },
          'Input.dispatchMouseEvent',
          {
            type: 'mouseWheel',
            x: params?.x || 0,
            y: params?.y || 0,
            deltaX: params?.dx || 0,
            deltaY: params?.dy || 0,
          }
        );
        result = { success: true };
        break;
      }

      case 'drag': {
        const tabId = await ensureDebugger();
        const button = params?.button || 'left';
        // Press
        await chrome.debugger.sendCommand(
          { tabId }, 'Input.dispatchMouseEvent',
          { type: 'mousePressed', x: params.x1, y: params.y1, button, clickCount: 1 }
        );
        // Move (with small steps for smoothness)
        await chrome.debugger.sendCommand(
          { tabId }, 'Input.dispatchMouseEvent',
          { type: 'mouseMoved', x: params.x2, y: params.y2 }
        );
        // Release
        await chrome.debugger.sendCommand(
          { tabId }, 'Input.dispatchMouseEvent',
          { type: 'mouseReleased', x: params.x2, y: params.y2, button, clickCount: 1 }
        );
        result = { success: true };
        break;
      }

      // ── Keyboard ───────────────────────────────────────────
      case 'type': {
        const tabId = await ensureDebugger();
        const text = params?.text || '';
        await chrome.debugger.sendCommand(
          { tabId },
          'Input.insertText',
          { text }
        );
        result = { chars_typed: text.length };
        break;
      }

      case 'key_press': {
        const tabId = await ensureDebugger();
        const key = params?.key || '';
        // Try to send as rawKeyDown + keyUp
        await chrome.debugger.sendCommand(
          { tabId }, 'Input.dispatchKeyEvent',
          { type: 'rawKeyDown', key, windowsVirtualKeyCode: keyCodeFor(key) }
        );
        await chrome.debugger.sendCommand(
          { tabId }, 'Input.dispatchKeyEvent',
          { type: 'keyUp', key, windowsVirtualKeyCode: keyCodeFor(key) }
        );
        result = { key };
        break;
      }

      // ── Clipboard ──────────────────────────────────────────
      case 'clipboard_get': {
        // Requires the page to be focused and have clipboard permission
        const text = ''; // Limited in service worker context
        result = { text };
        break;
      }

      case 'clipboard_set': {
        // Limited in service worker — try via offscreen document
        result = { success: false, error: 'clipboard_set requires active page context' };
        break;
      }

      // ── Windows ───────────────────────────────────────────
      case 'list_windows': {
        const windows = await chrome.windows.getAll({ populate: true });
        const winList = windows.map((w, idx) => ({
          id: w.id,
          title: w.tabs?.[0]?.title || `Window ${idx + 1}`,
          app: 'chrome',
          x: w.left || 0,
          y: w.top || 0,
          width: w.width || 1920,
          height: w.height || 1080,
          focused: w.focused || false,
          tabCount: w.tabs?.length || 0,
        }));
        result = { windows: winList };
        break;
      }

      case 'focus_window': {
        const title = params?.title || '';
        const allWindows = await chrome.windows.getAll({ populate: true });
        let matched = null;
        const candidates = [];

        for (const w of allWindows) {
          for (const tab of (w.tabs || [])) {
            if (tab.title && tab.title.toLowerCase().includes(title.toLowerCase())) {
              candidates.push(tab.title);
              if (!matched) matched = { windowId: w.id, tabId: tab.id, title: tab.title };
            }
          }
        }

        if (matched) {
          await chrome.windows.update(matched.windowId, { focused: true });
          await chrome.tabs.update(matched.tabId, { active: true });
          result = {
            matched: true,
            id: matched.tabId,
            title: matched.title,
            app: 'chrome',
            candidates,
          };
        } else {
          result = { matched: false, candidates };
        }
        break;
      }

      case 'get_active_window': {
        const tabs = await chrome.tabs.query({ active: true, currentWindow: true });
        const currentWindow = await chrome.windows.getCurrent();
        const tab = tabs[0];
        result = {
          found: !!tab,
          id: tab?.id,
          title: tab?.title || '',
          app: 'chrome',
          x: currentWindow?.left || 0,
          y: currentWindow?.top || 0,
          width: currentWindow?.width || 1920,
          height: currentWindow?.height || 1080,
        };
        break;
      }

      // ── Accessibility ──────────────────────────────────────
      case 'get_elements': {
        const tabId = await ensureDebugger();
        const axTree = await chrome.debugger.sendCommand(
          { tabId },
          'Accessibility.getFullAXTree'
        );

        // Flatten the AX tree into UiElement list
        const elements = [];
        const childMap = new Map(); // nodeId → [childNodeIds]

        function walk(node, depth) {
          if (depth > 200) return;
          const index = elements.length;
          const el = {
            index,
            role: node.role?.value || 'unknown',
            name: node.name?.value || '',
            actions: [],
            enabled: !(node.disabled || false),
            focused: node.focused || false,
            children: [],
          };

          if (node.value) el.value = node.value.value;
          if (node.description) el.description = node.description.value;
          if (node.properties) {
            for (const prop of node.properties) {
              if (prop.name === 'focusable' && prop.value?.value === true) {
                el.actions.push('focus');
              }
            }
          }
          if (node.bounds) {
            el.bounds = {
              x: node.bounds.x || 0,
              y: node.bounds.y || 0,
              width: node.bounds.width || 0,
              height: node.bounds.height || 0,
            };
          }

          elements.push(el);

          if (node.childIds) {
            childMap.set(index, node.childIds);
          }

          if (node.children) {
            for (const child of node.children) {
              walk(child, depth + 1);
            }
          }
        }

        for (const node of (axTree?.nodes || [])) {
          walk(node, 0);
        }

        // Resolve child indices
        for (let i = 0; i < elements.length; i++) {
          const childNodeIds = childMap.get(i);
          if (childNodeIds) {
            // Map node backend IDs to our indices
            // For now, children are resolved by the tree walk already
          }
        }

        result = {
          elements,
          width: window.screen?.width || 1920,
          height: window.screen?.height || 1080,
        };
        break;
      }

      // ── Notifications ─────────────────────────────────────
      case 'notify': {
        const notifId = await chrome.notifications.create({
          type: 'basic',
          iconUrl: 'icon.png',
          title: params?.title || 'desk-mcp',
          message: params?.message || '',
        });
        result = { id: notifId };
        break;
      }

      default:
        result = { error: `unknown method: ${method}` };
    }

    return { id, result };
  } catch (e) {
    return { id, error: e.message || String(e) };
  }
}

// ── Simple virtual keycode mapping ────────────────────────────────────────
function keyCodeFor(key) {
  const map = {
    'Enter': 13, 'Return': 13,
    'Tab': 9,
    'Escape': 27, 'Esc': 27,
    'Backspace': 8,
    'Delete': 46,
    'ArrowUp': 38, 'Up': 38,
    'ArrowDown': 40, 'Down': 40,
    'ArrowLeft': 37, 'Left': 37,
    'ArrowRight': 39, 'Right': 39,
    'Home': 36,
    'End': 35,
    'PageUp': 33,
    'PageDown': 34,
    'Control': 17, 'Ctrl': 17,
    'Shift': 16,
    'Alt': 18,
    'Meta': 91, 'Super': 91, 'Win': 91,
    ' ': 32, 'Space': 32,
  };
  if (map[key]) return map[key];
  if (key.length === 1) return key.toUpperCase().charCodeAt(0);
  return 0;
}

// ── TCP server ────────────────────────────────────────────────────────────

let serverSocketId = null;

function startServer() {
  chrome.sockets.tcpServer.create({}, (createInfo) => {
    if (chrome.runtime.lastError) {
      console.error('[desk-mcp] tcpServer.create failed:', chrome.runtime.lastError.message);
      return;
    }
    serverSocketId = createInfo.socketId;
    console.log(`[desk-mcp] TCP server socket created: ${serverSocketId}`);

    chrome.sockets.tcpServer.listen(serverSocketId, LISTEN_HOST, LISTEN_PORT, (result) => {
      if (chrome.runtime.lastError) {
        console.error('[desk-mcp] listen failed:', chrome.runtime.lastError.message);
        return;
      }
      console.log(`[desk-mcp] listening on ${LISTEN_HOST}:${LISTEN_PORT} (result=${result})`);
    });
  });
}

// Handle incoming connections
chrome.sockets.tcpServer.onAccept.addListener((info) => {
  if (info.socketId !== serverSocketId) return;

  const clientSocketId = info.clientSocketId;
  console.log(`[desk-mcp] new connection: ${clientSocketId}`);

  // Set socket to no-delay
  chrome.sockets.tcp.setNoDelay(clientSocketId, true, () => {});
  chrome.sockets.tcp.setPaused(clientSocketId, false);

  connections.set(clientSocketId, { buffer: '', handshakeDone: false });
});

// Handle incoming data
chrome.sockets.tcp.onReceive.addListener((info) => {
  const { socketId, data } = info;
  const conn = connections.get(socketId);
  if (!conn) return;

  // Accumulate data
  const chunk = new TextDecoder().decode(new Uint8Array(data));
  conn.buffer += chunk;

  // ── Phase 1: WebSocket handshake ───────────────────────────
  if (!conn.handshakeDone) {
    const delim = '\r\n\r\n';
    const idx = conn.buffer.indexOf(delim);
    if (idx === -1) return; // wait for complete headers

    const headerPart = conn.buffer.substring(0, idx);
    conn.buffer = conn.buffer.substring(idx + delim.length);

    // Extract Sec-WebSocket-Key
    const keyMatch = headerPart.match(/Sec-WebSocket-Key:\s*(.+)/i);
    if (!keyMatch) {
      console.error('[desk-mcp] no Sec-WebSocket-Key in handshake');
      closeSocket(socketId);
      return;
    }

    const key = keyMatch[1].trim();
    const acceptKey = base64Encode(
      String.fromCharCode(...sha1HexToBytes(sha1(key + WS_GUID)))
    );

    const response = [
      'HTTP/1.1 101 Switching Protocols',
      'Upgrade: websocket',
      'Connection: Upgrade',
      `Sec-WebSocket-Accept: ${acceptKey}`,
      '',
      '',
    ].join('\r\n');

    chrome.sockets.tcp.send(socketId, stringToArrayBuffer(response), () => {
      conn.handshakeDone = true;
      console.log('[desk-mcp] handshake complete');
    });
    return;
  }

  // ── Phase 2: Read WebSocket frames ─────────────────────────
  while (conn.buffer.length >= 2) {
    const bytes = new TextEncoder().encode(conn.buffer);
    if (bytes.length < 2) break;

    const b0 = bytes[0];
    const b1 = bytes[1];

    // Check for close frame
    const opcode = b0 & 0x0F;
    if (opcode === OP_CLOSE) {
      console.log('[desk-mcp] client sent close');
      closeSocket(socketId);
      return;
    }

    // Handle ping
    if (opcode === OP_PING) {
      const mask = (b1 & 0x80) !== 0;
      let payloadLen = b1 & 0x7F;
      let offset = 2;

      if (payloadLen === 126) { offset += 2; }
      else if (payloadLen === 127) { offset += 8; }
      if (mask) offset += 4;

      // Read ping payload
      if (bytes.length < offset + payloadLen) break;

      const pongPayload = bytes.slice(offset, offset + payloadLen);
      const pongFrame = buildFrameFromRaw(OP_PONG, pongPayload, false);

      chrome.sockets.tcp.send(socketId, pongFrame.buffer, () => {});
      // Remove the ping frame from buffer
      conn.buffer = conn.buffer.substring(offset + payloadLen);
      continue;
    }

    // Only handle text frames
    if (opcode !== OP_TEXT) {
      // Skip unknown frames — best effort
      break;
    }

    const mask = (b1 & 0x80) !== 0;
    let payloadLen = b1 & 0x7F;
    let offset = 2;

    if (payloadLen === 126) {
      if (bytes.length < 4) break;
      payloadLen = (bytes[2] << 8) | bytes[3];
      offset = 4;
    } else if (payloadLen === 127) {
      if (bytes.length < 10) break;
      // Read 64-bit length (big-endian)
      payloadLen = 0;
      for (let i = 0; i < 8; i++) {
        payloadLen = (payloadLen << 8) | bytes[2 + i];
      }
      offset = 10;
    }

    let maskKey = null;
    if (mask) {
      maskKey = [bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]];
      offset += 4;
    }

    if (bytes.length < offset + payloadLen) break; // incomplete frame

    // Extract payload
    const payloadBytes = bytes.slice(offset, offset + payloadLen);

    // Unmask if needed
    if (mask && maskKey) {
      for (let i = 0; i < payloadBytes.length; i++) {
        payloadBytes[i] ^= maskKey[i % 4];
      }
    }

    // Remove this frame from buffer
    conn.buffer = conn.buffer.substring(offset + payloadLen);

    // Parse JSON command
    let cmd;
    try {
      const json = new TextDecoder().decode(payloadBytes);
      cmd = JSON.parse(json);
    } catch (e) {
      console.error('[desk-mcp] JSON parse error:', e.message);
      continue;
    }

    // Handle command and send response
    handleCommand(cmd).then((response) => {
      const respJson = JSON.stringify(response);
      const frame = buildFrame(respJson, false);
      chrome.sockets.tcp.send(socketId, frame.buffer, (sendResult) => {
        if (chrome.runtime.lastError) {
          console.error('[desk-mcp] send error:', chrome.runtime.lastError.message);
        }
      });
    }).catch((err) => {
      console.error('[desk-mcp] command error:', err);
      const errorResp = JSON.stringify({ id: cmd.id, error: err.message || String(err) });
      const frame = buildFrame(errorResp, false);
      chrome.sockets.tcp.send(socketId, frame.buffer, () => {});
    });
  }
});

// ── Helpers ───────────────────────────────────────────────────────────────

function sha1HexToBytes(hex) {
  const bytes = [];
  for (let i = 0; i < hex.length; i += 2) {
    bytes.push(parseInt(hex.substring(i, i + 2), 16));
  }
  return new Uint8Array(bytes);
}

function stringToArrayBuffer(str) {
  const buf = new ArrayBuffer(str.length);
  const view = new Uint8Array(buf);
  for (let i = 0; i < str.length; i++) {
    view[i] = str.charCodeAt(i) & 0xff;
  }
  return buf;
}

function buildFrameFromRaw(opcode, payload, mask) {
  const bytes = [];
  bytes.push(0x80 | opcode); // FIN

  const len = payload.length;
  if (mask) {
    if (len <= 125) bytes.push(0x80 | len);
    else if (len <= 65535) { bytes.push(0x80 | 126); bytes.push((len >> 8) & 0xff, len & 0xff); }
    else { bytes.push(0x80 | 127); for (let i = 7; i >= 0; i--) bytes.push((len >> (i * 8)) & 0xff); }

    // Mask key (zeros — pong frames don't need real masking)
    bytes.push(0, 0, 0, 0);
    for (let i = 0; i < payload.length; i++) {
      bytes.push(payload[i]); // unmasked since mask key is zeros
    }
  } else {
    if (len <= 125) bytes.push(len);
    else if (len <= 65535) { bytes.push(126); bytes.push((len >> 8) & 0xff, len & 0xff); }
    else { bytes.push(127); for (let i = 7; i >= 0; i--) bytes.push((len >> (i * 8)) & 0xff); }

    for (let i = 0; i < payload.length; i++) {
      bytes.push(payload[i] ^ 0); // no mask
    }
  }

  return new Uint8Array(bytes);
}

function closeSocket(socketId) {
  connections.delete(socketId);
  chrome.sockets.tcp.close(socketId, () => {
    if (chrome.runtime.lastError) {
      // Socket might already be closed
    }
  });
}

// Handle disconnection
chrome.sockets.tcp.onClose.addListener((info) => {
  connections.delete(info.socketId);
});

// ── Startup ───────────────────────────────────────────────────────────────
console.log('[desk-mcp] service worker starting, initializing TCP server...');
startServer();

// Keep service worker alive by listening for messages
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (msg.type === 'ping') {
    sendResponse({ type: 'pong' });
  }
  return true;
});
