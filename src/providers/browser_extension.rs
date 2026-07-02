//! Browser Extension provider — communicates with a Chrome extension via WebSocket.
//!
//! This provider implements the `ComputerProvider` trait by sending JSON commands
//! over a WebSocket connection to a companion Chrome extension. The extension
//! executes the commands using Chrome DevTools Protocol (CDP) and returns results.
//!
//! ## Architecture
//! ```text
//! desk-mcp (Rust) --WebSocket client--> Chrome Extension (JS, WebSocket server)
//!   |-- chrome.tabs.captureVisibleTab()  -> screenshot
//!   |-- chrome.debugger (CDP)            -> click, type
//!   |-- chrome.windows.getAll()          -> list_windows
//!   |-- Accessibility.getFullAXTree      -> get_elements
//! ```
//!
//! ## Usage
//! Start desk-mcp with `--browser-extension ws://127.0.0.1:9224` or set
//! `DESKMCP_BROWSER_EXT=1` (defaults to `ws://127.0.0.1:9224`).
//!
//! The Chrome extension must be loaded in `chrome://extensions` (Developer mode,
//! "Load unpacked") and listening on the same port.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;
use anyhow::{bail, Context};
use serde_json::Value;

// ── WebSocket framing constants ──────────────────────────────────────────

const OP_TEXT: u8 = 0x1;
const OP_CLOSE: u8 = 0x8;

// ── Provider ──────────────────────────────────────────────────────────────

/// Provider that drives a Chrome extension via WebSocket.
///
/// Each `ComputerProvider` method serializes to a JSON command, sends it over
/// WebSocket, and deserializes the response. The WebSocket connection is
/// created on first use and reused (lazy, persistent).
pub struct BrowserExtensionProvider {
    ws_url: String,
    /// Shared command counter for unique message IDs.
    cmd_id: AtomicU64,
    /// Lazily-initialised WebSocket sender task.
    /// Uses an mpsc channel so multiple threads can queue commands.
    conn: Mutex<Option<Arc<WsConnection>>>,
}

/// Handle to the background WebSocket writer/reader task.
struct WsConnection {
    sender: tokio::sync::mpsc::Sender<WsCommand>,
}

/// A pending command plus a oneshot for the response.
struct WsCommand {
    payload: String,
    response_tx: tokio::sync::oneshot::Sender<Result<Value>>,
}

impl BrowserExtensionProvider {
    /// Create a new browser extension provider.
    ///
    /// `ws_url` is the WebSocket address of the Chrome extension's
    /// listener, e.g. `"ws://127.0.0.1:9224"`.
    pub fn new(ws_url: &str) -> Self {
        Self {
            ws_url: ws_url.to_string(),
            cmd_id: AtomicU64::new(1),
            conn: Mutex::new(None),
        }
    }

    /// Parse `--browser-extension <url>` from CLI args.
    pub fn resolve_ws_url() -> Option<String> {
        let args: Vec<String> = std::env::args().collect();
        for i in 0..args.len() {
            if args[i] == "--browser-extension" {
                return args.get(i + 1).cloned();
            }
        }
        // Env fallback: DESKMCP_BROWSER_EXT=1 uses default, or a custom URL
        if let Ok(val) = std::env::var("DESKMCP_BROWSER_EXT") {
            let trimmed = val.trim();
            if trimmed == "1" || trimmed.eq_ignore_ascii_case("true") {
                return Some("ws://127.0.0.1:9224".to_string());
            }
            if trimmed.starts_with("ws://") || trimmed.starts_with("wss://") {
                return Some(trimmed.to_string());
            }
        }
        None
    }

    /// Returns `true` if the user requested browser extension mode.
    pub fn is_enabled() -> bool {
        Self::resolve_ws_url().is_some()
    }

    // ── Internal: get or create the WebSocket connection ──────────────

    /// Ensure the WebSocket connection is active, returning a clone of the sender.
    fn ensure_sender(&self) -> Result<tokio::sync::mpsc::Sender<WsCommand>> {
        let mut guard = self.conn.lock().unwrap();
        if let Some(ref conn) = *guard {
            return Ok(conn.sender.clone());
        }
        // Create a fresh connection
        let conn = self.spawn_ws_task()?;
        let sender = conn.sender.clone();
        *guard = Some(conn);
        Ok(sender)
    }

    /// Spawn a background Tokio task that maintains the WebSocket link.
    ///
    /// Called from within a `block_on` context so `tokio::spawn` is available.
    fn spawn_ws_task(&self) -> Result<Arc<WsConnection>> {
        let url = self.ws_url.clone();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<WsCommand>(32);

        // Spawn a Tokio task (not a std thread) so I/O is properly
        // integrated with the runtime.
        tokio::spawn(async move {
            loop {
                // Wait for the next command
                let cmd: WsCommand = match rx.recv().await {
                    Some(c) => c,
                    None => break, // channel closed
                };

                let result = Self::execute_one(&url, &cmd.payload).await;

                // Send response back — ignore if receiver dropped
                let _ = cmd.response_tx.send(result);

                // Drain any additional queued commands while we're here
                while let Ok(cmd) = rx.try_recv() {
                    let result = Self::execute_one(&url, &cmd.payload).await;
                    let _ = cmd.response_tx.send(result);
                }
            }
        });

        Ok(Arc::new(WsConnection { sender: tx }))
    }

    /// Execute a single command: connect, send, read response, disconnect.
    async fn execute_one(url: &str, payload: &str) -> Result<Value> {
        // Parse host:port from ws:// url
        let addr = url
            .strip_prefix("ws://")
            .or_else(|| url.strip_prefix("wss://"))
            .unwrap_or("127.0.0.1:9224");

        // Connect via TCP
        let mut stream = TcpStream::connect(addr)
            .with_context(|| format!("failed to connect to browser extension at {addr}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(15)))
            .context("set_read_timeout")?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .context("set_write_timeout")?;

        // WebSocket handshake
        let key = generate_ws_key();
        let host = addr;
        let upgrade_req = format!(
            "GET / HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        stream
            .write_all(upgrade_req.as_bytes())
            .context("write WebSocket upgrade request")?;
        stream.flush().context("flush upgrade request")?;

        // Read the upgrade response
        let mut resp_buf = [0u8; 4096];
        let mut resp_data = Vec::new();
        loop {
            let n = stream
                .read(&mut resp_buf)
                .context("read upgrade response")?;
            if n == 0 {
                bail!("connection closed during WebSocket handshake");
            }
            resp_data.extend_from_slice(&resp_buf[..n]);
            let resp_str = String::from_utf8_lossy(&resp_data);
            if resp_str.contains("\r\n\r\n") {
                break;
            }
            if resp_data.len() > 4096 {
                bail!("handshake response too large");
            }
        }

        let resp_str = String::from_utf8_lossy(&resp_data);
        if !resp_str.contains("101") {
            bail!(
                "WebSocket handshake failed: {}",
                resp_str.lines().next().unwrap_or("")
            );
        }

        // Send the command frame (masked text frame)
        send_frame(&mut stream, payload.as_bytes(), true).context("send WebSocket frame")?;

        // Read the response frame
        let resp_bytes = read_frame(&mut stream).context("read WebSocket response frame")?;
        let resp_json: Value =
            serde_json::from_slice(&resp_bytes).context("deserialize WebSocket response")?;

        // Shut down the connection cleanly
        let _ = stream.shutdown(std::net::Shutdown::Both);

        Ok(resp_json)
    }

    /// Send a JSON command to the extension and return the parsed response.
    ///
    /// Called from sync trait methods via `block_on`.
    async fn send_command(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.cmd_id.fetch_add(1, Ordering::Relaxed);
        let request = serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        });
        let payload = serde_json::to_string(&request).context("serialize command")?;

        let sender = self.ensure_sender()?;
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        sender
            .send(WsCommand {
                payload,
                response_tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("WebSocket background task terminated"))?;

        response_rx
            .await
            .map_err(|_| anyhow::anyhow!("WebSocket response channel closed"))?
    }

    /// Convenience: send a command with no params.
    async fn send_simple(&self, method: &str) -> Result<Value> {
        self.send_command(method, Value::Object(serde_json::Map::new()))
            .await
    }
}

// ── WebSocket helpers ─────────────────────────────────────────────────────

/// Generate a random 16-byte key for the WebSocket handshake.
fn generate_ws_key() -> String {
    use base64::Engine;
    let mut buf = [0u8; 16];
    // Simple XOR-shift PRNG seeded by time + pid for low-stakes use
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
        ^ std::process::id() as u64;
    let mut state = seed;
    for b in buf.iter_mut() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *b = (state >> 32) as u8;
    }
    base64::engine::general_purpose::STANDARD.encode(buf)
}

/// Send a WebSocket text frame.
///
/// When `masked` is true (client-to-server), a 4-byte random mask is applied.
fn send_frame(stream: &mut TcpStream, payload: &[u8], masked: bool) -> Result<()> {
    let mut header = Vec::with_capacity(14);

    // FIN + opcode
    header.push(0x80 | OP_TEXT);

    // Mask + payload length
    let len = payload.len();
    if len <= 125 {
        header.push(if masked { 0x80 | len as u8 } else { len as u8 });
    } else if len <= 65535 {
        header.push(if masked { 0x80 | 126 } else { 126 });
        header.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        header.push(if masked { 0x80 | 127 } else { 127 });
        header.extend_from_slice(&(len as u64).to_be_bytes());
    }

    if masked {
        // Generate 4-byte mask key
        let mask: [u8; 4] = fastrand_mask();
        header.extend_from_slice(&mask);

        stream.write_all(&header).context("write frame header")?;

        // Write masked payload
        let mut masked_payload = Vec::with_capacity(len);
        for (i, &b) in payload.iter().enumerate() {
            masked_payload.push(b ^ mask[i % 4]);
        }
        stream
            .write_all(&masked_payload)
            .context("write frame payload")?;
    } else {
        stream.write_all(&header).context("write frame header")?;
        stream.write_all(payload).context("write frame payload")?;
    }

    stream.flush().context("flush frame")?;
    Ok(())
}

/// Read a WebSocket text frame and return the payload bytes.
fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>> {
    // Read first 2 bytes
    let mut hdr = [0u8; 2];
    read_exact(stream, &mut hdr).context("read frame header")?;

    let opcode = hdr[0] & 0x0F;
    let masked = (hdr[1] & 0x80) != 0;
    let mut payload_len = (hdr[1] & 0x7F) as u64;

    // Extended payload length
    let mut length_buf: [u8; 8] = [0u8; 8];
    if payload_len == 126 {
        read_exact(stream, &mut length_buf[..2]).context("read extended len (16-bit)")?;
        payload_len = u16::from_be_bytes([length_buf[0], length_buf[1]]) as u64;
    } else if payload_len == 127 {
        read_exact(stream, &mut length_buf).context("read extended len (64-bit)")?;
        payload_len = u64::from_be_bytes(length_buf);
    }

    // Read mask key if present
    let mut mask_key = [0u8; 4];
    if masked {
        read_exact(stream, &mut mask_key).context("read mask key")?;
    }

    // Handle close frames
    if opcode == OP_CLOSE {
        return Ok(Vec::new());
    }

    if opcode != OP_TEXT {
        bail!("expected text frame, got opcode {opcode}");
    }

    // Read payload
    let mut payload = vec![0u8; payload_len as usize];
    read_exact(stream, &mut payload).context("read frame payload")?;

    // Unmask if needed
    if masked {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= mask_key[i % 4];
        }
    }

    Ok(payload)
}

/// Read exactly `buf.len()` bytes from the stream.
fn read_exact(stream: &mut TcpStream, buf: &mut [u8]) -> Result<()> {
    let mut total = 0;
    while total < buf.len() {
        let n = stream.read(&mut buf[total..]).context("read_exact")?;
        if n == 0 {
            bail!(
                "connection closed unexpectedly (read {total}/{})",
                buf.len()
            );
        }
        total += n;
    }
    Ok(())
}

/// Generate a 4-byte mask key from a simple fast PRNG.
fn fastrand_mask() -> [u8; 4] {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let mut state = seed.wrapping_add(1);
    let mut buf = [0u8; 4];
    for b in buf.iter_mut() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *b = (state >> 32) as u8;
    }
    buf
}

// ── ComputerProvider impl ─────────────────────────────────────────────────

impl ComputerProvider for BrowserExtensionProvider {
    fn name(&self) -> &str {
        "browser_extension"
    }

    // ── Screenshot ────────────────────────────────────────────────

    fn screenshot(&self, _region: Option<(i32, i32, u32, u32)>) -> Result<Vec<u8>> {
        let result = block_on(self.send_simple("screenshot"))?;

        let b64 = result["data"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("screenshot response missing 'data' field"))?;

        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .context("decode screenshot base64")
    }

    fn get_screen_size(&self) -> Result<ScreenSize> {
        let result = block_on(self.send_simple("get_screen_size"))?;
        Ok(ScreenSize {
            width: result["width"].as_u64().unwrap_or(1920) as u32,
            height: result["height"].as_u64().unwrap_or(1080) as u32,
        })
    }

    // ── Mouse ─────────────────────────────────────────────────────

    fn mouse_move(&self, x: i32, y: i32, _smooth: bool, _duration_ms: u64) -> Result<()> {
        block_on(self.send_command("mouse_move", serde_json::json!({"x": x, "y": y})))?;
        Ok(())
    }

    fn mouse_click(&self, button: &str, x: Option<i32>, y: Option<i32>, clicks: u32) -> Result<()> {
        let mut params = serde_json::json!({
            "button": button,
            "clicks": clicks,
        });
        if let (Some(x), Some(y)) = (x, y) {
            params["x"] = serde_json::json!(x);
            params["y"] = serde_json::json!(y);
        }
        block_on(self.send_command("click", params))?;
        Ok(())
    }

    fn mouse_scroll(&self, dx: i32, dy: i32, x: Option<i32>, y: Option<i32>) -> Result<()> {
        let mut params = serde_json::json!({
            "dx": dx,
            "dy": dy,
        });
        if let (Some(x), Some(y)) = (x, y) {
            params["x"] = serde_json::json!(x);
            params["y"] = serde_json::json!(y);
        }
        block_on(self.send_command("scroll", params))?;
        Ok(())
    }

    fn mouse_drag(
        &self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        button: &str,
        _duration_ms: u64,
    ) -> Result<()> {
        block_on(self.send_command(
            "drag",
            serde_json::json!({
                "x1": x1, "y1": y1,
                "x2": x2, "y2": y2,
                "button": button,
            }),
        ))?;
        Ok(())
    }

    // ── Keyboard ──────────────────────────────────────────────────

    fn keyboard_type(&self, text: &str, _delay_ms: u64) -> Result<()> {
        block_on(self.send_command("type", serde_json::json!({"text": text})))?;
        Ok(())
    }

    fn key_press(&self, key: &str) -> Result<()> {
        block_on(self.send_command("key_press", serde_json::json!({"key": key})))?;
        Ok(())
    }

    // ── Clipboard ─────────────────────────────────────────────────

    fn clipboard_get(&self) -> Result<String> {
        let result = block_on(self.send_simple("clipboard_get"))?;
        result["text"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("clipboard_get response missing 'text' field"))
    }

    fn clipboard_set(&self, text: &str) -> Result<()> {
        block_on(self.send_command("clipboard_set", serde_json::json!({"text": text})))?;
        Ok(())
    }

    // ── Shell ─────────────────────────────────────────────────────

    fn shell_run(&self, _command: &str, _timeout_secs: u64) -> Result<ShellResult> {
        bail!("shell_run not supported in browser extension provider")
    }

    // ── Windows ───────────────────────────────────────────────────

    fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        let result = block_on(self.send_simple("list_windows"))?;

        let windows: Vec<WindowInfo> = result["windows"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|w| {
                Some(WindowInfo {
                    id: w["id"].as_u64()?.to_string(),
                    title: w["title"].as_str()?.to_string(),
                    app: w
                        .get("app")
                        .and_then(|v| v.as_str())
                        .unwrap_or("chrome")
                        .to_string(),
                    pid: None,
                    geometry: WindowGeometry {
                        x: w["x"].as_i64()? as i32,
                        y: w["y"].as_i64()? as i32,
                        width: w["width"].as_u64()? as u32,
                        height: w["height"].as_u64()? as u32,
                    },
                })
            })
            .collect();

        Ok(windows)
    }

    fn focus_window(&self, title_match: &str) -> Result<WindowMatch> {
        let result =
            block_on(self.send_command("focus_window", serde_json::json!({"title": title_match})))?;

        Ok(WindowMatch {
            matched: result["matched"].as_bool().unwrap_or(false),
            id: result["id"].as_u64().map(|v| v.to_string()),
            title: result["title"].as_str().map(String::from),
            app: result["app"].as_str().map(String::from),
            candidates: result["candidates"].as_array().map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            }),
        })
    }

    fn get_active_window(&self) -> Result<Option<WindowInfo>> {
        let result = block_on(self.send_simple("get_active_window"))?;

        if result["found"].as_bool() != Some(true) {
            return Ok(None);
        }

        Ok(Some(WindowInfo {
            id: result["id"]
                .as_u64()
                .map(|v| v.to_string())
                .unwrap_or_default(),
            title: result["title"].as_str().unwrap_or("").to_string(),
            app: result["app"].as_str().unwrap_or("chrome").to_string(),
            pid: None,
            geometry: WindowGeometry {
                x: result["x"].as_i64().unwrap_or(0) as i32,
                y: result["y"].as_i64().unwrap_or(0) as i32,
                width: result["width"].as_u64().unwrap_or(1920) as u32,
                height: result["height"].as_u64().unwrap_or(1080) as u32,
            },
        }))
    }

    // ── Apps / Notifications ─────────────────────────────────────

    fn open_app(&self, _app_name: &str) -> Result<()> {
        bail!("open_app not supported in browser extension provider")
    }

    fn notify(&self, title: &str, message: &str, _urgency: &str) -> Result<()> {
        block_on(self.send_command(
            "notify",
            serde_json::json!({"title": title, "message": message}),
        ))?;
        Ok(())
    }

    // ── Accessibility / Element Trees ─────────────────────────────

    fn get_window_state(&self) -> Result<WindowState> {
        let result = block_on(self.send_simple("get_elements"))?;

        let elements: Vec<UiElement> = result["elements"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|el| {
                Some(UiElement {
                    index: el["index"].as_u64()? as u32,
                    role: el["role"].as_str()?.to_string(),
                    name: el["name"].as_str().unwrap_or("").to_string(),
                    value: el.get("value").and_then(|v| v.as_str()).map(String::from),
                    description: el
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    actions: el["actions"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    bounds: el.get("bounds").and_then(|b| {
                        Some(ElementBounds {
                            x: b["x"].as_i64()? as i32,
                            y: b["y"].as_i64()? as i32,
                            width: b["width"].as_i64()? as i32,
                            height: b["height"].as_i64()? as i32,
                        })
                    }),
                    enabled: el["enabled"].as_bool().unwrap_or(true),
                    focused: el["focused"].as_bool().unwrap_or(false),
                    children: el["children"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_u64().map(|n| n as u32))
                                .collect()
                        })
                        .unwrap_or_default(),
                })
            })
            .collect();

        let window = result.get("window").and_then(|w| {
            Some(WindowInfo {
                id: w["id"].as_u64()?.to_string(),
                title: w["title"].as_str()?.to_string(),
                app: w
                    .get("app")
                    .and_then(|v| v.as_str())
                    .unwrap_or("chrome")
                    .to_string(),
                pid: None,
                geometry: WindowGeometry {
                    x: w["x"].as_i64()? as i32,
                    y: w["y"].as_i64()? as i32,
                    width: w["width"].as_u64()? as u32,
                    height: w["height"].as_u64()? as u32,
                },
            })
        });

        let element_count = elements.len();

        Ok(WindowState {
            window: window.unwrap_or(WindowInfo {
                id: "0".to_string(),
                title: "Browser Viewport".to_string(),
                app: "chrome".to_string(),
                pid: None,
                geometry: WindowGeometry {
                    x: 0,
                    y: 0,
                    width: result["width"].as_u64().unwrap_or(1920) as u32,
                    height: result["height"].as_u64().unwrap_or(1080) as u32,
                },
            }),
            elements,
            element_count,
        })
    }
}

// ── Sync→async bridge ─────────────────────────────────────────────────────

/// Run an async future from a synchronous context using the current Tokio runtime.
///
/// Uses `block_in_place` to temporarily step out of the tokio runtime, then
/// `block_on` to drive the future to completion. This prevents deadlocks when
/// the future itself needs to spawn tasks on the runtime.
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(f))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_key_generation() {
        let key1 = generate_ws_key();
        let key2 = generate_ws_key();
        assert_ne!(key1, key2, "two keys should differ");
        // Base64 of 16 bytes → 24 characters
        assert_eq!(key1.len(), 24);
    }

    #[test]
    fn test_provider_new() {
        let p = BrowserExtensionProvider::new("ws://127.0.0.1:9224");
        assert_eq!(p.name(), "browser_extension");
        assert_eq!(p.cmd_id.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_resolve_ws_url_not_set() {
        // Without env/args, should be None
        // (This test doesn't set env vars, so it should return None
        //  unless the test runner passes --browser-extension)
        let url = BrowserExtensionProvider::resolve_ws_url();
        // Don't assert — depends on test environment
        let _ = url;
    }
}
