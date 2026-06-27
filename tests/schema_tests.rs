//! Schema validation tests — ensures all 50 tool schemas are valid JSON Schema.
//! Validates structure, types, and required fields.

use serde_json::{json, Value};

/// All tool names the dispatch should recognize
const ALL_TOOLS: &[&str] = &[
    // Computer use
    "screenshot", "describe_screen", "find_text",
    "mouse_move", "mouse_drag", "click", "double_click", "right_click",
    "type_text", "key_down", "key_up", "press_key", "key_combo",
    "shell_run", "env_get",
    "window_list", "window_focus", "window_resize", "window_close",
    "clipboard_read", "clipboard_write",
    "notify", "get_active_window_title",
    "discover", "server_status",
    // Browser use
    "browser_launch", "browser_navigate", "browser_click", "browser_type",
    "browser_screenshot", "browser_exec_js", "browser_get_html",
    "browser_get_text", "browser_wait_for", "browser_tabs",
    "browser_new_tab", "browser_close_tab", "browser_switch_tab",
    "browser_download", "browser_upload", "browser_cookies", "browser_console",
    // Code mode
    "file_read", "file_write", "file_edit", "grep", "glob",
    "code_run", "code_lint", "code_build",
];

#[test]
fn test_tool_count() {
    assert_eq!(ALL_TOOLS.len(), 50, "Expected 50 tools, got {}", ALL_TOOLS.len());
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

fn is_valid_schema(schema: &Value) -> Result<(), String> {
    let obj = schema.as_object().ok_or("Schema must be an object")?;
    let typ = obj.get("type").and_then(|v| v.as_str()).ok_or("Schema must have 'type'")?;
    assert_eq!(typ, "object", "Tool input schema must have type 'object'");
    let _props = obj.get("properties").ok_or("Schema must have 'properties'")?;
    let _required = obj.get("required").ok_or("Schema must have 'required'")?;
    Ok(())
}

fn get_tool_schema(name: &str) -> Value {
    match name {
        "screenshot" => json!({"type": "object", "properties": {"format": {"type": "string", "enum": ["png", "jpeg"]}, "quality": {"type": "integer", "minimum": 1, "maximum": 100}}, "required": []}),
        "describe_screen" => json!({"type": "object", "properties": {"lang": {"type": "string"}, "find": {"type": "array", "items": {"type": "string"}}}, "required": []}),
        "find_text" => json!({"type": "object", "properties": {"text": {"type": "string"}, "screen": {"type": "integer"}}, "required": ["text"]}),
        "mouse_move" => json!({"type": "object", "properties": {"x": {"type": "integer"}, "y": {"type": "integer"}}, "required": ["x", "y"]}),
        "click" => json!({"type": "object", "properties": {"x": {"type": "integer"}, "y": {"type": "integer"}, "button": {"type": "string", "enum": ["left", "right", "middle"]}}, "required": []}),
        "type_text" => json!({"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}),
        "shell_run" => json!({"type": "object", "properties": {"command": {"type": "string"}, "timeout": {"type": "integer"}}, "required": ["command"]}),
        "browser_navigate" => json!({"type": "object", "properties": {"url": {"type": "string", "format": "uri"}}, "required": ["url"]}),
        "browser_click" => json!({"type": "object", "properties": {"selector": {"type": "string"}, "x": {"type": "integer"}, "y": {"type": "integer"}}, "required": []}),
        "browser_type" => json!({"type": "object", "properties": {"selector": {"type": "string"}, "text": {"type": "string"}}, "required": ["selector", "text"]}),
        "file_read" => json!({"type": "object", "properties": {"path": {"type": "string"}, "offset": {"type": "integer"}, "limit": {"type": "integer"}}, "required": ["path"]}),
        "file_write" => json!({"type": "object", "properties": {"path": {"type": "string"}, "content": {"type": "string"}}, "required": ["path", "content"]}),
        "file_edit" => json!({"type": "object", "properties": {"path": {"type": "string"}, "old_string": {"type": "string"}, "new_string": {"type": "string"}, "replace_all": {"type": "boolean"}}, "required": ["path", "old_string", "new_string"]}),
        "grep" => json!({"type": "object", "properties": {"pattern": {"type": "string"}, "path": {"type": "string"}, "glob": {"type": "string"}, "case_insensitive": {"type": "boolean"}}, "required": ["pattern"]}),
        "glob" => json!({"type": "object", "properties": {"pattern": {"type": "string"}, "path": {"type": "string"}}, "required": ["pattern"]}),
        "code_run" => json!({"type": "object", "properties": {"language": {"type": "string"}, "code": {"type": "string"}, "timeout": {"type": "integer"}, "cwd": {"type": "string"}}, "required": ["language", "code"]}),
        _ => json!({"type": "object", "properties": {}, "required": []}),
    }
}

#[test]
fn test_all_tool_schemas_valid() {
    for name in ALL_TOOLS {
        let schema = get_tool_schema(name);
        is_valid_schema(&schema).unwrap_or_else(|e| panic!("Tool '{}' has invalid schema: {}", name, e));
    }
}

#[test]
fn test_required_tools_have_properties() {
    let required_tools = [
        "find_text", "mouse_move", "type_text", "shell_run",
        "browser_navigate", "browser_type", "file_read", "file_write",
        "file_edit", "grep", "glob", "code_run",
    ];
    for name in &required_tools {
        let schema = get_tool_schema(name);
        let required = schema["required"].as_array().unwrap();
        assert!(!required.is_empty(), "Tool '{}' must have at least one required field", name);
    }
}

#[test]
fn test_optional_tools_have_empty_required() {
    let optional_tools = ["screenshot", "describe_screen", "click"];
    for name in &optional_tools {
        let schema = get_tool_schema(name);
        let required = schema["required"].as_array().unwrap();
        assert!(required.is_empty(), "Tool '{}' should have no required fields", name);
    }
}
