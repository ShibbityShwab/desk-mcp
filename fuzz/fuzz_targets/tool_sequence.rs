#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Try to interpret fuzz input as a JSON-RPC tool call
    if let Ok(json_str) = std::str::from_utf8(data) {
        if let Ok(request) = serde_json::from_str::<serde_json::Value>(json_str) {
            // Extract tool name and arguments
            let name = request
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = request
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            // Don't actually execute — just verify dispatch doesn't panic
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _ = rt.block_on(async {
                desk_mcp::tools::dispatch(name, args).await
            });
        }
    }
});
