//! Unit tests for desk-mcp core functionality.
//! Tests response builders, OCR TSV parsing, and path validation.

use desk_mcp::response;
use serde_json::json;

#[test]
fn test_ok_response() {
    let resp = response::ok(json!({"status": "ready"}));
    assert!(resp.ok);
    assert_eq!(resp.result, Some(json!({"status": "ready"})));
    assert!(resp.error.is_none());
}

#[test]
fn test_err_response() {
    let resp = response::err("TEST_ERROR", "something went wrong");
    assert!(!resp.ok);
    assert!(resp.result.is_none());
    assert_eq!(resp.error.as_ref().unwrap().code, "TEST_ERROR");
    assert_eq!(resp.error.as_ref().unwrap().message, "something went wrong");
}

#[test]
fn test_err_detail_response() {
    let resp = response::err_detail("IO_ERROR", "read failed", "/tmp/test.txt");
    assert!(!resp.ok);
    let err = resp.error.unwrap();
    assert_eq!(err.code, "IO_ERROR");
    assert_eq!(err.detail.unwrap(), "/tmp/test.txt");
}

#[test]
fn test_dep_missing_response() {
    let resp = response::dep_missing("screenshot", "spectacle", "pacman -S spectacle");
    assert!(!resp.ok);
    let err = resp.error.unwrap();
    assert_eq!(err.code, "DEPENDENCY_MISSING");
    assert!(err.message.contains("spectacle"));
    assert!(err.message.contains("screenshot"));
}

#[test]
fn test_not_available_response() {
    let resp = response::not_available("mouse_move");
    assert!(!resp.ok);
    let err = resp.error.unwrap();
    assert_eq!(err.code, "NOT_IMPLEMENTED");
    assert!(err.message.contains("mouse_move"));
}

#[test]
fn test_timeout_response() {
    let resp = response::timeout("browser_navigate", 30.0);
    assert!(!resp.ok);
    let err = resp.error.unwrap();
    assert_eq!(err.code, "TIMEOUT");
    assert!(err.message.contains("30s"));
    assert!(err.message.contains("browser_navigate"));
}

// ── Response serialization tests ──

#[test]
fn test_tool_response_serialization_ok() {
    let resp = response::ok(json!({"done": true}));
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"ok\":true"));
    assert!(json.contains("\"done\":true"));
    assert!(!json.contains("\"error\":"));
}

#[test]
fn test_tool_response_serialization_err() {
    let resp = response::err("FAIL", "bad");
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"ok\":false"));
    assert!(json.contains("\"code\":\"FAIL\""));
    assert!(!json.contains("\"result\":"));
}

// ── Error type tests ──

#[test]
fn test_mcp_error_codes() {
    use desk_mcp::error::McpError;

    let err = McpError::DependencyMissing {
        tool: "test".into(),
        dep: "test-dep".into(),
        hint: "install it".into(),
    };
    assert_eq!(err.code(), "DEPENDENCY_MISSING");

    let err = McpError::Timeout {
        tool: "x".into(),
        seconds: 5.0,
    };
    assert_eq!(err.code(), "TIMEOUT");

    let err = McpError::BrowserNotLaunched;
    assert_eq!(err.code(), "BROWSER_NOT_LAUNCHED");

    let err = McpError::ShellNotAllowed;
    assert_eq!(err.code(), "SHELL_NOT_ALLOWED");
}

#[test]
fn test_mcp_error_from_io_error() {
    use desk_mcp::error::McpError;
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let mcp_err: McpError = io_err.into();
    assert_eq!(mcp_err.code(), "TOOL_ERROR");
}

#[test]
fn test_mcp_error_display() {
    use desk_mcp::error::McpError;
    let err = McpError::NotAvailable {
        tool: "browser".into(),
    };
    let msg = err.to_string();
    assert!(msg.contains("browser"));
    assert!(msg.contains("not available"));
}

// ── Discovery tests ──

#[test]
fn test_discovery_ocr_flag() {
    let caps = desk_mcp::discovery::detect();
    assert!(
        caps.ocr,
        "OCR should be listed as available (pure-Rust ocrs)"
    );
}

#[test]
fn test_discovery_browser_automation_string() {
    let caps = desk_mcp::discovery::detect();
    assert_eq!(caps.browser_automation, "chromiumoxide");
}

#[test]
fn test_server_name_and_version() {
    assert!(!desk_mcp::SERVER_NAME.is_empty());
    assert!(!desk_mcp::SERVER_VERSION.is_empty());
}
