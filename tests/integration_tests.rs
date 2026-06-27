//! Integration tests for desk-mcp.
//! Tests tool dispatch routing and error handling.

use serde_json::json;

#[test]
fn test_discovery_is_cached() {
    let caps1 = desk_mcp::discovery::detect();
    let caps2 = desk_mcp::discovery::detect();
    let ptr1 = caps1 as *const _;
    let ptr2 = caps2 as *const _;
    assert_eq!(ptr1, ptr2, "Discovery results should be cached (same pointer)");
}

#[test]
fn test_discovery_has_browser_automation() {
    let caps = desk_mcp::discovery::detect();
    assert_eq!(caps.browser_automation, "chromiumoxide");
}

#[test]
fn test_response_ok_roundtrip() {
    let resp = desk_mcp::response::ok(json!({"value": 42}));
    let serialized = serde_json::to_string(&resp).unwrap();
    let deserialized: desk_mcp::response::ToolResponse = serde_json::from_str(&serialized).unwrap();
    assert!(deserialized.ok);
    assert_eq!(deserialized.result, Some(json!({"value": 42})));
    assert!(deserialized.error.is_none());
}

#[test]
fn test_response_err_roundtrip() {
    let resp = desk_mcp::response::err("SOME_CODE", "message here");
    let serialized = serde_json::to_string(&resp).unwrap();
    let deserialized: desk_mcp::response::ToolResponse = serde_json::from_str(&serialized).unwrap();
    assert!(!deserialized.ok);
    assert!(deserialized.result.is_none());
    assert_eq!(deserialized.error.as_ref().unwrap().code, "SOME_CODE");
}

#[test]
fn test_mcp_error_roundtrip() {
    use desk_mcp::error::McpError;
    let err = McpError::BrowserNotLaunched;
    let serialized = serde_json::to_string(&err.to_string()).unwrap();
    assert!(serialized.contains("Browser"));
    assert!(serialized.contains("launched"));
}

#[test]
fn test_error_code_consistency() {
    use desk_mcp::error::McpError;

    let variants = [
        McpError::DependencyMissing { tool: "t".into(), dep: "d".into(), hint: "h".into() },
        McpError::NotAvailable { tool: "t".into() },
        McpError::Timeout { tool: "t".into(), seconds: 1.0 },
        McpError::BrowserNotLaunched,
        McpError::BrowserLaunchFailed("msg".into()),
        McpError::Io { path: "p".into(), source: std::io::Error::new(std::io::ErrorKind::Other, "e") },
        McpError::FileOp { op: "read".into(), path: "p".into(), detail: "d".into() },
        McpError::PathOutsideWorkspace { path: "p".into(), root: "r".into() },
        McpError::ShellNotAllowed,
        McpError::CodeNotAllowed,
        McpError::UnknownTool { name: "n".into() },
        McpError::ToolError("msg".into()),
        McpError::JsonError(serde_json::from_str::<serde_json::Value>("invalid").unwrap_err()),
    ];

    for variant in &variants {
        let code = variant.code();
        assert!(!code.is_empty(), "Error code for variant is empty");
        assert!(
            code.chars().all(|c| c.is_uppercase() || c == '_'),
            "Error code '{}' should be UPPER_SNAKE_CASE", code
        );
    }
}

#[test]
fn test_mcp_error_from_serde_json() {
    use desk_mcp::error::McpError;
    let err: Result<serde_json::Value, _> = serde_json::from_str("{invalid");
    let mcp_err: McpError = err.unwrap_err().into();
    assert_eq!(mcp_err.code(), "JSON_ERROR");
}

#[test]
fn test_mcp_error_from_anyhow() {
    use desk_mcp::error::McpError;
    let anyhow_err = anyhow::anyhow!("test error");
    let mcp_err: McpError = anyhow_err.into();
    assert_eq!(mcp_err.code(), "TOOL_ERROR");
    assert!(mcp_err.to_string().contains("test error"));
}

#[test]
fn test_ocr_find_text() {
    // Proper tesseract TSV: header line + one line per word
    // level	page_num	block_num	par_num	line_num	word_num	left	top	width	height	conf	text
    let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
               5\t1\t1\t1\t1\t1\t0\t0\t100\t20\t99\tusername\n\
               5\t1\t1\t1\t2\t1\t120\t0\t80\t20\t95\tpassword\n\
               5\t1\t1\t1\t3\t1\t220\t0\t60\t20\t80\tlogin";
    let results = desk_mcp::ocr::parse_tsv(tsv).unwrap();
    assert_eq!(results.len(), 3);

    let user = desk_mcp::ocr::find_text(&results, "user", true);
    assert!(user.is_some());

    let pass = desk_mcp::ocr::find_text(&results, "pass", true);
    assert!(pass.is_some());

    let nonexistent = desk_mcp::ocr::find_text(&results, "zzzzzzz", false);
    assert!(nonexistent.is_none());
}

#[test]
fn test_server_name_and_version() {
    assert!(!desk_mcp::SERVER_NAME.is_empty());
    assert!(!desk_mcp::SERVER_VERSION.is_empty());
    assert!(
        desk_mcp::SERVER_VERSION
            .split('.')
            .all(|part| part.parse::<u32>().is_ok()),
        "Version '{}' is not semver",
        desk_mcp::SERVER_VERSION
    );
}
