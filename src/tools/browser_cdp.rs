//! CDP (Chrome DevTools Protocol) connection helpers.
//!
//! Extracted from `browser.rs` to keep the tool-handler file focused
//! on the 17 browser-use tools.

use anyhow::Result;
use chromiumoxide::browser::Browser;
use tokio::time::Duration;

/// Find a free TCP port for Chrome DevTools.
pub(crate) fn find_free_port() -> Result<u16, String> {
    use std::net::TcpListener;
    TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("Cannot find free port: {e}"))
        .and_then(|l| l.local_addr().map(|a| a.port()).map_err(|e| format!("{e}")))
}

/// Connect to a running Chrome DevTools endpoint on localhost.
///
/// Uses a custom reqwest client with timeouts to fetch `/json/version`,
/// then passes the WebSocket URL directly to `Browser::connect()` wrapped
/// in a 10s timeout. This avoids chromiumoxide's internal HTTP client
/// (reqwest 0.13) which can hang indefinitely on some systems.
pub(crate) async fn connect_to_cdp_port(
    port: u16,
) -> Result<(Browser, chromiumoxide::handler::Handler), String> {
    let version_url = format!("http://localhost:{port}/json/version");

    // Custom reqwest client with timeouts
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let version_body = client
        .get(&version_url)
        .send()
        .await
        .map_err(|e| format!("Failed to reach Chrome DevTools on port {port}: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Failed to read /json/version on port {port}: {e}"))?;

    let version_json: serde_json::Value = serde_json::from_str(&version_body)
        .map_err(|e| format!("Failed to parse /json/version on port {port}: {e}"))?;

    let ws_url_raw = version_json["webSocketDebuggerUrl"]
        .as_str()
        .ok_or_else(|| {
            format!("No webSocketDebuggerUrl in /json/version response on port {port}")
        })?;

    // Replace localhost with 127.0.0.1 — some async-tungstenite builds
    // hang on localhost DNS resolution under tokio.
    let ws_url = ws_url_raw.replace("localhost", "127.0.0.1");

    tracing::info!(port = port, ws_url = %ws_url, "connecting to Chrome DevTools WebSocket");

    let connect_result =
        tokio::time::timeout(Duration::from_secs(10), Browser::connect(ws_url)).await;

    match connect_result {
        Ok(Ok(pair)) => Ok(pair),
        Ok(Err(e)) => Err(format!("CDP handshake failed on port {port}: {e}")),
        Err(_elapsed) => Err(format!(
            "CDP handshake timed out after 10s on port {port}. Chrome may be hung."
        )),
    }
}
