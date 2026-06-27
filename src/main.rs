//! DeskMCP — Entry point
//!
//! Implements the MCP JSON-RPC 2.0 stdio protocol.
//! Reads messages from stdin, dispatches to tool handlers, writes responses to stdout.
//!
//! Protocol: Content-Length header followed by JSON-RPC 2.0 message.

use desk_mcp::tools;
use serde_json::Value;
use std::io::{BufRead, Write};
use tokio::sync::mpsc;

/// Represents a JSON-RPC 2.0 request or notification
#[derive(Debug, serde::Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

/// JSON-RPC 2.0 response
#[derive(Debug, serde::Serialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, serde::Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

/// Read a complete JSON-RPC message from a buffered reader.
/// Returns the raw JSON string.
fn read_message(reader: &mut impl BufRead) -> Option<String> {
    // Read Content-Length header
    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None; // EOF
        }
        let line = line.trim().to_string();
        if line.is_empty() {
            break; // End of headers
        }
        if let Some(len) = line.strip_prefix("Content-Length:").or_else(|| line.strip_prefix("content-length:")) {
            content_length = len.trim().parse().unwrap_or(0);
        }
    }

    if content_length == 0 {
        return None;
    }

    // Read body
    let mut body = vec![0u8; content_length];
    if let Ok(_buf) = reader.read_exact(&mut body) {
        String::from_utf8(body).ok()
    } else {
        None
    }
}

/// Write a JSON-RPC response to stdout
fn write_message(msg: &JsonRpcResponse) {
    let json = serde_json::to_string(msg).unwrap_or_default();
    let output = format!("Content-Length: {}\r\n\r\n{json}", json.len());
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = handle.write_all(output.as_bytes());
    let _ = handle.flush();
}

/// Handle a JSON-RPC request, returning the response (None for notifications)
async fn handle_request(req: JsonRpcRequest) -> Option<JsonRpcResponse> {
    let id = req.id;

    let result = match req.method.as_str() {
        "initialize" => {
            Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": {
                    "name": desk_mcp::SERVER_NAME,
                    "version": desk_mcp::SERVER_VERSION,
                },
                "capabilities": {
                    "tools": {}
                }
            }))
        }

        "tools/list" => {
            let tools = tools::all_tools();
            let tools_json: Vec<Value> = tools.iter()
                .map(|t| serde_json::to_value(t).unwrap_or_default())
                .collect();
            Some(serde_json::json!({ "tools": tools_json }))
        }

        "tools/call" => {
            let params = req.params.unwrap_or_default();
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

            let response = tools::dispatch(name, arguments).await;
            let result_value = serde_json::to_value(response).unwrap_or_default();

            Some(serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string(&result_value).unwrap_or_default(),
                }]
            }))
        }

        "ping" => {
            Some(serde_json::json!({}))
        }

        _method if _method.starts_with("notifications/") => {
            // Notifications don't need responses
            tracing::debug!("notification: {}", _method);
            None
        }

        _ => {
            // Unknown method
            if id.is_none() {
                None
            } else {
                return Some(JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32601,
                        message: format!("Method not found: {}", req.method),
                    }),
                });
            }
        }
    };

    // Only respond if there's an id (not a notification)
    match (id, result) {
        (Some(id), Some(result)) => Some(JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: Some(id),
            result: Some(result),
            error: None,
        }),
        _ => None,
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("desk_mcp=info".parse().unwrap())
        )
        .with_writer(std::io::stderr) // stderr for logs (stdin/stdout is MCP protocol)
        .init();

    tracing::info!(
        server = desk_mcp::SERVER_NAME,
        version = desk_mcp::SERVER_VERSION,
        "starting MCP server"
    );

    // Discover environment (triggers lazy provider init)
    let caps = desk_mcp::discovery::detect();
    tracing::info!(
        display = caps.display_type,
        desktop = caps.desktop,
        provider = caps.provider,
        browsers = caps.discovered_browsers.len(),
        "environment detected"
    );

    // Main loop: re-establish stdin reader when parent reconnects
    loop {
        let (tx, mut rx) = mpsc::channel::<String>(256);

        // Spawn stdin reader
        let stdin = std::io::stdin();
        std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(stdin.lock());
            while let Some(msg) = read_message(&mut reader) {
                if tx.blocking_send(msg).is_err() {
                    break; // Receiver closed
                }
            }
        });

        // Process messages
        while let Some(msg) = rx.recv().await {
            let request: JsonRpcRequest = match serde_json::from_str(&msg) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("invalid JSON-RPC: {e}");
                    continue;
                }
            };

            tracing::debug!(method = request.method, "received request");

            let response = handle_request(request).await;
            if let Some(resp) = response {
                write_message(&resp);
            }
        }

        // stdin EOF — parent may reconnect; short sleep then retry
        tracing::debug!("stdin closed, waiting for reconnect...");
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}
