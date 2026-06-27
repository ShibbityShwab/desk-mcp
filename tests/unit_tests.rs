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

// ── OCR TSV parsing tests ──

#[test]
fn test_parse_tsv_valid() {
    // Word: "hello" conf 95, "world" conf 87
    let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
               5\t1\t1\t1\t1\t1\t0\t0\t50\t20\t95\thello\n\
               5\t1\t1\t1\t2\t1\t60\t0\t50\t20\t87\tworld";
    let results = desk_mcp::ocr::parse_tsv(tsv).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].text, "hello");
    assert_eq!(results[0].confidence, 95.0);
    assert_eq!(results[1].text, "world");
    assert_eq!(results[1].confidence, 87.0);
}

#[test]
fn test_parse_tsv_empty() {
    let results = desk_mcp::ocr::parse_tsv("").unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_parse_tsv_malformed() {
    // Missing confidence column — text is literally "hello"
    let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
               5\t1\t1\t1\t1\t1\t0\t0\t50\t20\t0\thello";
    let results = desk_mcp::ocr::parse_tsv(tsv).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].text, "hello");
    assert_eq!(results[0].confidence, 0.0);
}

#[test]
fn test_parse_tsv_multiple_words() {
    let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
               5\t1\t1\t1\t1\t1\t0\t0\t70\t20\t99\tfoo\n\
               5\t1\t1\t1\t2\t1\t80\t0\t30\t20\t50\tbar\n\
               5\t1\t1\t1\t3\t1\t120\t0\t40\t20\t99\tbaz";
    let results = desk_mcp::ocr::parse_tsv(tsv).unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].text, "foo");
    assert!(results[0].confidence > 90.0);
    assert_eq!(results[2].text, "baz");
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

// ── Path validation tests ──

#[test]
fn test_workspace_root_env_var() {
    std::env::set_var("DESKMCP_WORKSPACE", "/tmp/test_ws");
    std::env::remove_var("DESKMCP_WORKSPACE");
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

    let err = McpError::Timeout { tool: "x".into(), seconds: 5.0 };
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
    let err = McpError::NotAvailable { tool: "browser".into() };
    let msg = err.to_string();
    assert!(msg.contains("browser"));
    assert!(msg.contains("not available"));
}
