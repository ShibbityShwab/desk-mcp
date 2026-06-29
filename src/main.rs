//! DeskMCP — Entry point
//!
//! Implements the MCP JSON-RPC 2.0 protocol over stdio (default) or
//! HTTP / SSE (when `--http` or `DESKMCP_HTTP=1` is set).
//!
//! Stdio protocol: Content-Length header followed by JSON-RPC 2.0 message.
//! HTTP protocol: `POST /mcp` with JSON-RPC body → JSON-RPC response.

use desk_mcp::transport::{self, JsonRpcRequest, JsonRpcResponse};
use std::io::{BufRead, Write};
use std::path::PathBuf;

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
        if let Some(len) = line
            .strip_prefix("Content-Length:")
            .or_else(|| line.strip_prefix("content-length:"))
        {
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

/// Write a JSON-RPC response to stdout (stdio transport).
fn write_message(msg: &JsonRpcResponse) {
    let json = serde_json::to_string(msg).unwrap_or_default();
    let output = format!("Content-Length: {}\r\n\r\n{json}", json.len());
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = handle.write_all(output.as_bytes());
    let _ = handle.flush();
}

/// Stdio transport — read JSON-RPC from stdin, write to stdout.
async fn run_stdio() {
    // Start stdin reader IMMEDIATELY — before any environment detection.
    // aion_mcp sends `initialize` right after spawn; we must be ready.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(256);

    tokio::task::spawn_blocking(move || {
        let stdin = std::io::stdin();
        let mut reader = std::io::BufReader::new(stdin.lock());
        while let Some(msg) = read_message(&mut reader) {
            if tx.blocking_send(msg).is_err() {
                break;
            }
        }
        tracing::info!("stdin reader thread exiting");
    });

    // Run environment detection in background while processing messages.
    let _caps_handle = tokio::task::spawn_blocking(|| {
        let caps = desk_mcp::discovery::detect();
        tracing::info!(
            display = caps.display_type,
            desktop = caps.desktop,
            provider = caps.provider,
            browsers = caps.discovered_browsers.len(),
            "environment detected"
        );
        caps
    });

    // ── Create stdio session ──────────────────────────────────────────
    let stdio_sid = "stdio-session".to_string();
    if transport::SESSIONS.get_session(&stdio_sid).is_none() {
        transport::SESSIONS.create_deterministic(
            &stdio_sid,
            desk_mcp::session::SessionCapabilities::default(),
        );
    }

    // Process messages from stdin
    while let Some(msg) = rx.recv().await {
        let request: JsonRpcRequest = match serde_json::from_str(&msg) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("invalid JSON-RPC: {e}");
                continue;
            }
        };

        tracing::info!(method = request.method.as_str(), id = ?request.id, "received");

        let response = transport::handle_request(request, Some("stdio-session")).await;
        if let Some(resp) = response {
            write_message(&resp);
        }
    }

    tracing::info!("stdin closed, exiting");
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    // ── --init-policy: generate policy.yaml and exit ────────────
    if std::env::args().any(|a| a == "--init-policy") {
        init_policy();
        return;
    }

    // ── Auto-generate policy on first run ───────────────────────
    auto_init_policy();

    // ── Initialize tracing with optional OTLP layer ─────────────
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let env_filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive("desk_mcp=info".parse().unwrap());

    if std::env::var("DESKMCP_OTLP")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        // ── OpenTelemetry OTLP exporter ─────────────────────────
        use opentelemetry::trace::TracerProvider as _;
        use opentelemetry_sdk::trace::TracerProvider;
        use tracing_opentelemetry::OpenTelemetryLayer;

        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .build()
            .expect("failed to create OTLP exporter");

        let provider = TracerProvider::builder()
            .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
            .build();

        let tracer = provider.tracer("desk-mcp");
        let otel_layer = OpenTelemetryLayer::new(tracer);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .with(otel_layer)
            .init();

        tracing::info!("OpenTelemetry: OTLP export enabled");
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .init();
    }

    tracing::info!(
        server = desk_mcp::SERVER_NAME,
        version = desk_mcp::SERVER_VERSION,
        "starting MCP server"
    );

    // ── Transport selection ────────────────────────────────────────
    if transport::is_http_mode() {
        let host = transport::resolve_host();
        let port = transport::resolve_port();
        let addr: std::net::SocketAddr =
            format!("{host}:{port}").parse().expect("invalid host:port");

        tracing::info!(%addr, "HTTP mode selected");

        if let Err(e) = transport::run_http_server(addr).await {
            tracing::error!(error = %e, "HTTP server crashed");
            std::process::exit(1);
        }
    } else {
        tracing::info!("stdio mode selected");
        run_stdio().await;
    }
}

// ── Policy helpers ──────────────────────────────────────────────────

fn policy_path() -> PathBuf {
    let mut p = dirs_next::config_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    p.push("desk-mcp");
    p.push("policy.yaml");
    p
}

fn init_policy() {
    let path = policy_path();
    if path.exists() {
        println!("Policy already exists at {}", path.display());
    } else {
        let default_yaml = serde_yaml::to_string(&desk_mcp::policy::default_config())
            .expect("failed to serialize default policy");
        std::fs::create_dir_all(path.parent().unwrap()).ok();
        std::fs::write(&path, &default_yaml).expect("failed to write policy file");
        println!("Default policy written to {}", path.display());
    }
}

fn auto_init_policy() {
    let path = policy_path();
    if !path.exists() {
        let default_yaml = serde_yaml::to_string(&desk_mcp::policy::default_config())
            .expect("failed to serialize default policy");
        std::fs::create_dir_all(path.parent().unwrap()).ok();
        if std::fs::write(&path, &default_yaml).is_ok() {
            tracing::info!("Default policy written to {}", path.display());
        }
    }
}
