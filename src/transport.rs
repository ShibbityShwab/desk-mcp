//! Transport layer — shared JSON-RPC types, request dispatch, and HTTP/SSE server.
//!
//! Both the stdio transport (main.rs) and the HTTP transport use the same
//! `handle_request` function defined here.
//!
//! ## Sessions
//! Each connection gets an isolated session via `SESSIONS`. HTTP requests
//! map the Bearer token → deterministic session id; stdio uses a single
//! `"stdio-session"` id.

use axum::{routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::SocketAddr;
use crate::session::{self, SessionCapabilities};

// ── JSON-RPC 2.0 types ────────────────────────────────────────────────

/// Incoming JSON-RPC request (deserialised from stdin or HTTP body).
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

/// Outgoing JSON-RPC success response.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// Structured JSON-RPC error object.
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

// ── Session registry ─────────────────────────────────────────────────

/// Global session manager — one per process (defined in session module, re-exported here for convenience).
pub use crate::session::SESSIONS;

// ── Shared request handler ────────────────────────────────────────────

/// Dispatch a JSON-RPC request and return the response (if any).
///
/// `session_id` identifies the caller's session. For HTTP this is
/// derived from the Bearer token; for stdio it is `"stdio-session"`.
///
/// Returns `None` for *notifications* (requests without an `id`).
pub async fn handle_request(
    req: JsonRpcRequest,
    session_id: Option<&str>,
) -> Option<JsonRpcResponse> {
    // ── Resolve (or create) session ──────────────────────────────────
    let session = session_id.and_then(|sid| SESSIONS.get_session(&sid.to_string()));

    // Rate limiting is now handled by dispatch() — see tools/mod.rs for
    // per-session and global rate check logic.
    let id = req.id;

    let result = match req.method.as_str() {
        "initialize" => {
            tracing::info!("MCP initialize — protocol version: {:?}", req.params);
            Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": {
                    "name": crate::SERVER_NAME,
                    "version": crate::SERVER_VERSION,
                },
                "capabilities": {
                    "tools": {}
                }
            }))
        }

        "tools/list" => {
            let tools = crate::tools::all_tools();
            let tools_json: Vec<Value> = tools
                .iter()
                .map(|t| serde_json::to_value(t).unwrap_or_default())
                .collect();
            Some(serde_json::json!({ "tools": tools_json }))
        }

        "tools/call" => {
            let params = req.params.unwrap_or_default();
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

            tracing::info!(tool = %tool_name, "tools/call");

            let tool_response = crate::tools::dispatch(tool_name, arguments, session_id).await;
            let result_value = serde_json::to_value(&tool_response).unwrap_or_default();

            Some(serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string(&result_value).unwrap_or_default(),
                }]
            }))
        }

        "ping" => Some(serde_json::json!({})),

        method if method.starts_with("notifications/") => {
            tracing::debug!("notification: {}", method);
            None
        }

        _ => {
            // Unknown method — only respond if it has an id
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
                        data: None,
                    }),
                });
            }
        }
    };

    // ── Record action in session (after dispatch, before response) ────
    if let Some(ref s) = session {
        s.record_action();
        crate::session::SESSIONS.increment_total_actions();
    }

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

// ── HTTP / SSE server ─────────────────────────────────────────────────

/// POST /mcp  —  JSON-RPC request → JSON-RPC response
async fn mcp_handler(
    headers: axum::http::HeaderMap,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
    Json(request): Json<JsonRpcRequest>,
) -> axum::response::Response {
    // Auth check
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| crate::auth::from_header(v));
    let token_param = query.get("token").map(|s| s.as_str());
    let provided = bearer.or(token_param);

    if !crate::auth::validate(provided) {
        return axum::response::Response::builder()
            .status(401)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                r#"{"jsonrpc":"2.0","error":{"code":-32001,"message":"Unauthorized: provide a valid Bearer token or ?token= parameter"}}"#
            ))
            .unwrap();
    }

    // ── Session lookup / creation ──────────────────────────────────────
    // Each unique auth token maps to its own deterministic session.
    let token = provided.unwrap_or("anonymous");
    let session_id = session::hash_to_session_id(token.as_bytes());

    // Create session if it doesn't exist yet.
    if SESSIONS.get_session(&session_id).is_none() {
        SESSIONS.create_deterministic(&session_id, SessionCapabilities::default());
    }

    match handle_request(request, Some(&session_id)).await {
        Some(response) => {
            let body = serde_json::to_vec(&response).unwrap_or_default();
            axum::response::Response::builder()
                .header("content-type", "application/json")
                .body(axum::body::Body::from(body))
                .unwrap()
        }
        None => {
            // Notification — 204 No Content
            axum::response::Response::builder()
                .status(204)
                .body(axum::body::Body::empty())
                .unwrap()
        }
    }
}

/// GET /health  —  simple liveness check
async fn health_handler() -> &'static str {
    "ok"
}

/// Start the HTTP / SSE MCP server on the given address.
///
/// The server listens on `POST /mcp` for JSON-RPC and `GET /health`
/// for liveness probes.  Ctrl‑C (or SIGTERM) shuts it down cleanly.
pub async fn run_http_server(addr: SocketAddr) -> anyhow::Result<()> {
    use tower_http::cors::CorsLayer;

    let app = Router::new()
        .route("/health", axum::routing::get(health_handler))
        .route("/mcp", post(mcp_handler))
        .route("/dashboard", axum::routing::get(crate::dashboard::dashboard_handler))
        .route("/dashboard/stats", axum::routing::get(crate::dashboard::stats_handler))
        .layer(CorsLayer::permissive());

    tracing::info!(
        server = crate::SERVER_NAME,
        version = crate::SERVER_VERSION,
        transport = "streamable-http",
        address = %addr,
        "DeskMCP HTTP server starting"
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("HTTP server shut down");
    Ok(())
}

/// Resolves to `()` when the process receives SIGINT or SIGTERM.
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    tracing::info!("Shutdown signal received");
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Parse `--port` / `DESKMCP_PORT` (default 9876).
pub fn resolve_port() -> u16 {
    // CLI: `--port N`
    let args: Vec<String> = std::env::args().collect();
    for i in 0..args.len() {
        if args[i] == "--port" {
            if let Some(port_str) = args.get(i + 1) {
                if let Ok(p) = port_str.parse::<u16>() {
                    return p;
                }
            }
        }
    }
    // Env: DESKMCP_PORT
    if let Ok(val) = std::env::var("DESKMCP_PORT") {
        if let Ok(p) = val.trim().parse::<u16>() {
            return p;
        }
    }
    8765
}

/// Parse `--host` / `DESKMCP_HOST` (default 127.0.0.1).
pub fn resolve_host() -> String {
    let args: Vec<String> = std::env::args().collect();
    for i in 0..args.len() {
        if args[i] == "--host" {
            if let Some(h) = args.get(i + 1) {
                return h.clone();
            }
        }
    }
    if let Ok(val) = std::env::var("DESKMCP_HOST") {
        return val.trim().to_string();
    }
    "127.0.0.1".into()
}

/// Returns `true` if the user requested HTTP mode via `--http` or
/// `DESKMCP_HTTP=1`.
pub fn is_http_mode() -> bool {
    if std::env::args().any(|a| a == "--http") {
        return true;
    }
    if let Ok(val) = std::env::var("DESKMCP_HTTP") {
        if val.trim() == "1" || val.trim().eq_ignore_ascii_case("true") {
            return true;
        }
    }
    false
}
