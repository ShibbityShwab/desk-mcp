//! Integration tests for desk-mcp.
//! Tests tool dispatch routing and error handling.

use serde_json::json;

#[test]
fn test_discovery_is_cached() {
    let caps1 = desk_mcp::discovery::detect();
    let caps2 = desk_mcp::discovery::detect();
    let ptr1 = caps1 as *const _;
    let ptr2 = caps2 as *const _;
    assert_eq!(
        ptr1, ptr2,
        "Discovery results should be cached (same pointer)"
    );
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
fn test_discovery_refresh_browsers() {
    // refresh_browsers() should return a Vec (possibly empty)
    let browsers = desk_mcp::discovery::refresh_browsers();
    // Just verify it doesn't panic and returns a valid type
    let _count = browsers.len();
}

#[test]
fn test_discovery_detect_has_fields() {
    let caps = desk_mcp::discovery::detect();
    assert!(!caps.display_type.is_empty());
    assert!(!caps.desktop.is_empty());
    assert!(!caps.provider.is_empty());
    assert!(!caps.screenshot_tool.is_empty());
    assert!(!caps.input_tool.is_empty());
    assert!(!caps.window_tool.is_empty());
    assert!(!caps.browser_automation.is_empty());
    assert!(!caps.home_dir.is_empty());
    assert!(!caps.xdg_runtime_dir.is_empty());
}

#[test]
fn test_provider_exists() {
    let provider = &desk_mcp::PROVIDER;
    let name = provider.name();
    assert!(!name.is_empty(), "Provider must have a name");
}

#[test]
fn test_tool_list_not_empty() {
    let tools = desk_mcp::tools::all_tools();
    assert!(!tools.is_empty(), "Tool list should not be empty");
}

#[test]
fn test_all_tools_have_names() {
    let tools = desk_mcp::tools::all_tools();
    for tool in &tools {
        assert!(!tool.name.is_empty(), "Tool has empty name");
        assert!(
            !tool.description.is_empty(),
            "Tool '{}' has empty description",
            tool.name
        );
    }
}

#[test]
fn test_server_constants() {
    assert_eq!(desk_mcp::SERVER_NAME, "desk-mcp");
    assert!(!desk_mcp::SERVER_VERSION.is_empty());
}
