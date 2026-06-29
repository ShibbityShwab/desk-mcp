//! Embedded web dashboard — Pillar II.4 of the Kowloon Manifesto.
//!
//! Serves a live observability page at `GET /dashboard` with auto-refreshing
//! session stats.  The stats endpoint `GET /dashboard/stats` returns a JSON
//! snapshot of the session registry.
//!
//! Everything is self-contained: HTML, CSS, and JS are inlined so the page
//! needs zero external dependencies.

use std::sync::LazyLock;
use std::time::Instant;

use crate::transport::SESSIONS;

// ── Uptime ──────────────────────────────────────────────────────────────

/// Process start time — captured once at first access.
pub static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

// ── Dashboard HTML ──────────────────────────────────────────────────────

/// Full inline HTML page — dark theme, stats cards, session table,
/// auto-refresh every 3 s via `fetch('/dashboard/stats')`.
const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>desk-mcp dashboard</title>
<style>
*,*::before,*::after{box-sizing:border-box;margin:0;padding:0}
body{font-family:system-ui,-apple-system,Segoe UI,Roboto,sans-serif;background:#0d1117;color:#c9d1d9;min-height:100vh}
header{padding:20px 28px;border-bottom:1px solid #21262d}
header h1{font-size:1.3rem;font-weight:600;color:#f0f6fc}
header span{font-size:0.8rem;color:#8b949e;margin-left:8px}
main{padding:24px 28px;max-width:1100px;margin:0 auto}
.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:14px;margin-bottom:28px}
.card{background:#161b22;border:1px solid #21262d;border-radius:8px;padding:18px 20px}
.card .label{font-size:0.7rem;text-transform:uppercase;letter-spacing:0.06em;color:#8b949e;margin-bottom:6px}
.card .value{font-size:1.7rem;font-weight:600;color:#58a6ff}
.card .sub{font-size:0.75rem;color:#8b949e;margin-top:4px}
h2{margin-bottom:14px;font-size:1rem;color:#f0f6fc}
table{width:100%;border-collapse:collapse;font-size:0.85rem}
thead th{text-align:left;padding:10px 14px;border-bottom:2px solid #21262d;color:#8b949e;font-weight:500}
tbody td{padding:10px 14px;border-bottom:1px solid #21262d}
tbody tr:hover{background:#161b22}
.empty{text-align:center;color:#484f58;padding:40px 0}
#error{background:#490202;color:#ff7b72;padding:12px 16px;border-radius:6px;margin-bottom:16px;display:none}
footer{text-align:center;color:#30363d;font-size:0.7rem;padding:20px}
</style></head>
<body>
<header><h1>&#9670; desk-mcp<span>dashboard</span></h1></header>
<main>
<div id="error"></div>
<div class="cards">
<div class="card"><div class="label">Sessions</div><div class="value" id="sessions">—</div><div class="sub">active</div></div>
<div class="card"><div class="label">Actions</div><div class="value" id="actions">—</div><div class="sub">total</div></div>
<div class="card"><div class="label">Uptime</div><div class="value" id="uptime">—</div><div class="sub" id="uptime_sub"></div></div>
<div class="card"><div class="label">Version</div><div class="value" id="version">—</div><div class="sub" id="server_sub"></div></div>
</div>
<h2>Active Sessions</h2>
<table>
<thead><tr><th>Session ID</th><th>Created</th><th>Actions</th><th>Last Active</th></tr></thead>
<tbody id="sessions_body"><tr class="empty"><td colspan="4">No active sessions</td></tr></tbody>
</table>
</main>
<footer>desk-mcp &bull; observability dashboard</footer>
<script>
function fmtTime(sec){var h=Math.floor(sec/3600),m=Math.floor((sec%3600)/60),s=Math.floor(sec%60);return h>0?h+'h '+m+'m '+s+'s':m>0?m+'m '+s+'s':s+'s'}
function shortId(id){return id.length>12?id.slice(0,12)+'…':id}
function fmtDate(iso){var d=new Date(iso);return d.toLocaleString()}
var token = new URLSearchParams(window.location.search).get('token') || '';
async function refresh(){try{var r=await fetch('/dashboard/stats' + (token ? '?token=' + token : ''));if(!r.ok)throw new Error(r.status+' '+r.statusText);
var d=await r.json();document.getElementById('error').style.display='none';
document.getElementById('sessions').textContent=d.active_sessions;
document.getElementById('actions').textContent=d.total_actions;
document.getElementById('uptime').textContent=fmtTime(d.uptime_seconds);
document.getElementById('uptime_sub').textContent='since launch';
document.getElementById('version').textContent=d.version;
document.getElementById('server_sub').textContent=d.server;
var tb=document.getElementById('sessions_body');
if(!d.sessions||d.sessions.length===0){tb.innerHTML='<tr class="empty"><td colspan="4">No active sessions</td></tr>';return}
tb.innerHTML='';
d.sessions.forEach(function(s){var tr=document.createElement('tr');
tr.innerHTML='<td><code>'+shortId(s.id)+'</code></td><td>'+fmtDate(s.created)+'</td><td>'+s.actions+'</td><td>'+fmtDate(s.last_active)+'</td>';
tb.appendChild(tr)})}catch(e){var er=document.getElementById('error');er.textContent='Stats fetch failed: '+e.message;er.style.display='block'}}
refresh();setInterval(refresh,3000)
</script></body>
</html>"##;

// ── Handlers ────────────────────────────────────────────────────────────

/// `GET /dashboard` — serve the observability page.
pub async fn dashboard_handler(
    headers: axum::http::HeaderMap,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> axum::response::Response {
    if !check_dashboard_auth(&headers, &query) {
        return axum::response::Response::builder()
            .status(401)
            .header("content-type", "text/plain")
            .body(axum::body::Body::from("Unauthorized"))
            .unwrap();
    }
    axum::response::Response::builder()
        .header("content-type", "text/html")
        .body(axum::body::Body::from(DASHBOARD_HTML))
        .unwrap()
}

/// `GET /dashboard/stats` — JSON snapshot of server and session state.
pub async fn stats_handler(
    headers: axum::http::HeaderMap,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> axum::response::Response {
    if !check_dashboard_auth(&headers, &query) {
        return axum::response::Response::builder()
            .status(401)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"error":"Unauthorized"}"#))
            .unwrap();
    }

    let uptime = START_TIME.elapsed().as_secs_f64().round() as u64;

    let mut stats = SESSIONS.session_stats();
    // Inject server metadata on top of session_stats() fields.
    if let Some(obj) = stats.as_object_mut() {
        obj.insert("server".into(), serde_json::json!(crate::SERVER_NAME));
        obj.insert("version".into(), serde_json::json!(crate::SERVER_VERSION));
        obj.insert("uptime_seconds".into(), serde_json::json!(uptime));
    }

    let body = serde_json::to_vec(&stats).unwrap_or_default();
    axum::response::Response::builder()
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap()
}

/// Check dashboard auth from Authorization header or ?token= query parameter.
fn check_dashboard_auth(
    headers: &axum::http::HeaderMap,
    query: &std::collections::HashMap<String, String>,
) -> bool {
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| crate::auth::from_header(v));
    let token_param = query.get("token").map(|s| s.as_str());
    crate::auth::validate(bearer.or(token_param))
}
