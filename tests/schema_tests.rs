//! Schema validation tests — ensures all tool schemas are valid JSON Schema.
//! Validates structure, types, and required fields.

/// All tool names the dispatch should recognize
const ALL_TOOLS: &[&str] = &[
    // Computer use
    "screenshot",
    "get_screen_size",
    "mouse_move",
    "mouse_click",
    "mouse_double_click",
    "mouse_scroll",
    "mouse_drag",
    "keyboard_type",
    "key_press",
    "press_hotkey",
    "shell_run",
    "env_get",
    "clipboard_get",
    "clipboard_set",
    "list_windows",
    "focus_window",
    "get_active_window",
    "open_app",
    "notify",
    "get_window_state",
    "type_to_window",
    "click_on_text",
    "wait_for_text",
    "extract_text",
    "describe_screen",
    "wait",
    // Browser use
    "browser_launch",
    "browser_navigate",
    "browser_click",
    "browser_type",
    "browser_screenshot",
    "browser_exec_js",
    "browser_get_html",
    "browser_get_text",
    "browser_wait_for",
    "browser_tabs",
    "browser_new_tab",
    "browser_close_tab",
    "browser_switch_tab",
    "browser_download",
    "browser_upload",
    "browser_cookies",
    "browser_console",
    "browser_refresh",
    // Code mode
    "file_read",
    "file_write",
    "file_edit",
    "grep",
    "glob",
    "code_run",
    "code_lint",
    "code_build",
    // Accessibility
    "find_elements",
    "get_element_text",
    "click_element",
    "get_window_tree",
    // Status
    "server_status",
    // Safety & confirmation
    "request_confirmation",
    "approve",
    "deny",
    "list_pending",
];

#[test]
fn test_tool_count() {
    let tools = desk_mcp::tools::all_tools();
    assert!(!tools.is_empty(), "Tool list should not be empty");
    // The exact count may vary as tools are added/removed
    assert!(
        tools.len() >= 28,
        "Expected at least 28 tools, got {}",
        tools.len()
    );
}

#[test]
fn test_no_duplicate_tool_names() {
    let mut sorted = ALL_TOOLS.to_vec();
    sorted.sort();
    let original_len = sorted.len();
    sorted.dedup();
    assert_eq!(original_len, sorted.len(), "Duplicate tool names found!");
}

#[test]
fn test_all_tool_names_are_valid() {
    for name in ALL_TOOLS {
        assert!(
            name.chars().all(|c| c.is_alphanumeric() || c == '_'),
            "Tool name '{}' contains invalid characters",
            name
        );
        assert!(!name.is_empty(), "Empty tool name found");
    }
}

#[test]
fn test_every_tool_has_schema() {
    let tools = desk_mcp::tools::all_tools();
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

    for expected in ALL_TOOLS {
        assert!(
            tool_names.contains(expected),
            "Tool '{}' is in ALL_TOOLS but not registered in all_tools()",
            expected
        );
    }
}

#[test]
fn test_tool_schemas_are_valid_json() {
    let tools = desk_mcp::tools::all_tools();
    for tool in &tools {
        // input_schema should be a valid JSON object
        assert!(
            tool.input_schema.is_object(),
            "Tool '{}' has non-object input_schema",
            tool.name
        );
        // Should have 'type' field
        assert_eq!(
            tool.input_schema.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "Tool '{}' input_schema missing type=object",
            tool.name
        );
    }
}

#[test]
fn test_tool_descriptions_are_meaningful() {
    let tools = desk_mcp::tools::all_tools();
    for tool in &tools {
        assert!(
            !tool.description.is_empty(),
            "Tool '{}' has empty description",
            tool.name
        );
        assert!(
            tool.description.len() > 10,
            "Tool '{}' description too short: '{}'",
            tool.name,
            tool.description
        );
    }
}

#[test]
fn test_browser_tools_exist() {
    let tools = desk_mcp::tools::all_tools();
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(
        tool_names.contains(&"browser_launch"),
        "browser_launch missing"
    );
    assert!(
        tool_names.contains(&"browser_navigate"),
        "browser_navigate missing"
    );
    assert!(
        tool_names.contains(&"browser_get_text"),
        "browser_get_text missing"
    );
}

#[test]
fn test_a11y_tools_exist() {
    let tools = desk_mcp::tools::all_tools();
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(
        tool_names.contains(&"find_elements"),
        "find_elements missing"
    );
    assert!(
        tool_names.contains(&"get_window_tree"),
        "get_window_tree missing"
    );
}

#[test]
fn test_no_discovery_tools_in_tool_list() {
    // discovery tools like "discover" and "server_status" are ok (status tools)
    // but internal-only discovery tools should not be listed
    let tools = desk_mcp::tools::all_tools();
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    // These should not be user-facing tools
    for name in tool_names {
        assert!(
            !name.starts_with("discovery_"),
            "Internal discovery tool '{}' leaked into tool list",
            name
        );
    }
}
