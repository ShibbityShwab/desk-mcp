//! Performance benchmarks for desk-mcp.
//!
//! Run with: cargo bench
//!
//! Benchmarks are organized by module. Each benchmark group measures
//! a specific subsystem. Targets are documented in BENCHMARKING.md.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use desk_mcp::providers::mock::MockProvider;
use desk_mcp::providers::{ComputerProvider, ElementBounds, UiElement, WindowState};
// ═══════════════════════════════════════════════════════════════════════════
// Policy benchmarks
// ═══════════════════════════════════════════════════════════════════════════

fn bench_policy(c: &mut Criterion) {
    c.bench_function("policy_single_tool_allow", |b| {
        b.iter(|| {
            let decision = desk_mcp::policy::evaluate("screenshot", &serde_json::json!({}));
            black_box(decision);
        })
    });

    c.bench_function("policy_single_tool_deny_dangerous", |b| {
        b.iter(|| {
            let decision = desk_mcp::policy::evaluate(
                "shell_run",
                &serde_json::json!({"command": "rm -rf /"}),
            );
            black_box(decision);
        })
    });

    c.bench_function("policy_require_confirmation", |b| {
        b.iter(|| {
            let decision =
                desk_mcp::policy::evaluate("shell_run", &serde_json::json!({"command": "ls"}));
            black_box(decision);
        })
    });
}

// ═══════════════════════════════════════════════════════════════════════════
// Session benchmarks
// ═══════════════════════════════════════════════════════════════════════════

fn bench_session(c: &mut Criterion) {
    let caps = desk_mcp::session::SessionCapabilities::default();

    c.bench_function("session_create_and_lookup", |b| {
        b.iter(|| {
            let id = desk_mcp::session::SESSIONS.create_session(caps.clone());
            let session = desk_mcp::session::SESSIONS.get_session(&id);
            black_box(session);
            desk_mcp::session::SESSIONS.destroy_session(&id);
        })
    });

    c.bench_function("session_stats_build", |b| {
        // Pre-create 10 sessions
        let ids: Vec<_> = (0..10)
            .map(|_| desk_mcp::session::SESSIONS.create_session(caps.clone()))
            .collect();

        b.iter(|| {
            let stats = desk_mcp::session::SESSIONS.session_stats();
            black_box(stats);
        });

        // Cleanup
        for id in &ids {
            desk_mcp::session::SESSIONS.destroy_session(id);
        }
    });
}

// ═══════════════════════════════════════════════════════════════════════════
// Mock provider benchmarks
// ═══════════════════════════════════════════════════════════════════════════

fn bench_mock_provider(c: &mut Criterion) {
    let mock = MockProvider::new();

    c.bench_function("mock_screenshot_full", |b| {
        b.iter(|| {
            let data = mock.screenshot(None).unwrap();
            black_box(data);
        })
    });

    c.bench_function("mock_screenshot_crop", |b| {
        b.iter(|| {
            let data = mock.screenshot(Some((0, 0, 200, 200))).unwrap();
            black_box(data);
        })
    });

    c.bench_function("mock_mouse_click", |b| {
        b.iter(|| {
            mock.mouse_click("left", Some(100), Some(200), 1).unwrap();
        })
    });

    c.bench_function("mock_keyboard_type_100chars", |b| {
        let text = "a".repeat(100);
        b.iter(|| {
            mock.keyboard_type(&text, 0).unwrap();
        })
    });

    // Element tree with 100 elements
    let elements: Vec<UiElement> = (0..100)
        .map(|i| UiElement {
            index: i,
            role: "push button".into(),
            name: format!("Button {i}"),
            value: None,
            description: None,
            actions: vec!["click".into()],
            bounds: Some(ElementBounds {
                x: (i * 20) as i32,
                y: 100,
                width: 80,
                height: 30,
            }),
            enabled: true,
            focused: i == 0,
            children: vec![],
        })
        .collect();
    let mock = MockProvider::new().with_element_tree(elements);

    c.bench_function("mock_get_window_state_100elements", |b| {
        b.iter(|| {
            let state = mock.get_window_state().unwrap();
            black_box(state.element_count);
        })
    });
}

// ═══════════════════════════════════════════════════════════════════════════
// Resolution benchmarks
// ═══════════════════════════════════════════════════════════════════════════

fn bench_resolution(c: &mut Criterion) {
    use desk_mcp::resolution::Target;

    // Tier 1: AT-SPI tree search (synthetic)
    let elements: Vec<UiElement> = (0..100)
        .map(|i| UiElement {
            index: i,
            role: if i % 3 == 0 {
                "push button"
            } else if i % 3 == 1 {
                "text"
            } else {
                "menu item"
            }
            .into(),
            name: format!("Element {i}"),
            value: None,
            description: None,
            actions: vec!["click".into()],
            bounds: Some(ElementBounds {
                x: (i * 20) as i32,
                y: 100,
                width: 80,
                height: 30,
            }),
            enabled: true,
            focused: false,
            children: vec![],
        })
        .collect();

    let state = WindowState {
        window: desk_mcp::providers::WindowInfo {
            id: "test-1".into(),
            title: "Test Window".into(),
            app: "test".into(),
            pid: None,
            geometry: desk_mcp::providers::WindowGeometry {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
        },
        element_count: elements.len(),
        elements,
    };

    c.bench_function("resolution_tier1_find_by_name", |b| {
        b.iter(|| {
            // Simulate what find_in_tree does — search for an element by name
            let target = Target::ByName {
                name: "Element 99".into(),
            };
            let name_lower = match &target {
                Target::ByName { name } => name.to_lowercase(),
                _ => unreachable!(),
            };
            let found = state
                .elements
                .iter()
                .find(|e| e.name.to_lowercase() == name_lower);
            black_box(found.is_some());
        })
    });

    // Tier 3: OCR text search (synthetic items)
    let ocr_items: Vec<desk_mcp::ocr::OcrItem> = (0..200)
        .map(|i| desk_mcp::ocr::OcrItem {
            text: format!("word_{i:03}"),
            bounds: None,
            confidence: 1.0,
        })
        .collect();

    c.bench_function("resolution_tier3_ocr_find_text", |b| {
        b.iter(|| {
            let found = desk_mcp::ocr::find_text(black_box(&ocr_items), "word_150", false);
            black_box(found.is_some());
        })
    });
}

// ═══════════════════════════════════════════════════════════════════════════
// Original benchmarks (preserved)
// ═══════════════════════════════════════════════════════════════════════════

fn bench_tool_dispatch(c: &mut Criterion) {
    c.bench_function("discovery_cached", |b| {
        b.iter(|| {
            let caps = desk_mcp::discovery::detect();
            black_box(caps.provider.as_str());
        })
    });

    c.bench_function("response_ok_build", |b| {
        b.iter(|| {
            let resp = desk_mcp::response::ok(serde_json::json!({"result": true}));
            black_box(resp);
        })
    });

    c.bench_function("response_err_build", |b| {
        b.iter(|| {
            let resp = desk_mcp::response::err("TEST_CODE", "test message");
            black_box(resp);
        })
    });

    c.bench_function("response_serialize", |b| {
        let resp = desk_mcp::response::ok(serde_json::json!({"data": "benchmark test value"}));
        b.iter(|| {
            let json = serde_json::to_string(black_box(&resp)).unwrap();
            black_box(json);
        })
    });
}

fn bench_ocr_parse(c: &mut Criterion) {
    let items: Vec<desk_mcp::ocr::OcrItem> = [
        "benchmark",
        "text",
        "parsing",
        "routine",
        "speed",
        "test",
        "performance",
        "analysis",
        "result",
        "output",
        "input",
        "sample",
        "value",
        "data",
        "check",
    ]
    .iter()
    .map(|t| desk_mcp::ocr::OcrItem {
        text: t.to_string(),
        bounds: None,
        confidence: 1.0,
    })
    .collect();

    c.bench_function("ocr_find_text_exact", |b| {
        b.iter(|| {
            let found = desk_mcp::ocr::find_text(black_box(&items), "benchmark", false);
            black_box(found.is_some());
        })
    });

    c.bench_function("ocr_find_text_partial", |b| {
        b.iter(|| {
            let found = desk_mcp::ocr::find_text(black_box(&items), "perf", true);
            black_box(found.is_some());
        })
    });
}

fn bench_error_types(c: &mut Criterion) {
    use desk_mcp::error::McpError;

    c.bench_function("error_create_dep_missing", |b| {
        b.iter(|| {
            let err = McpError::DependencyMissing {
                tool: "test".into(),
                dep: "test-dep".into(),
                hint: "install it".into(),
            };
            black_box(err.code());
        })
    });

    c.bench_function("error_display", |b| {
        let err = McpError::PathOutsideWorkspace {
            path: "/tmp/../etc/passwd".into(),
            root: "/home/user/Projects".into(),
        };
        b.iter(|| {
            let msg = err.to_string();
            black_box(msg);
        })
    });

    c.bench_function("error_code", |b| {
        let err = McpError::Timeout {
            tool: "test".into(),
            seconds: 30.0,
        };
        b.iter(|| {
            let code = err.code();
            black_box(code);
        })
    });
}

// ═══════════════════════════════════════════════════════════════════════════
// Audit + recipes benchmarks
// ═══════════════════════════════════════════════════════════════════════════

fn bench_audit_recipes(c: &mut Criterion) {
    c.bench_function("audit_sanitize_text", |b| {
        let args = serde_json::json!({"text": "secret password here", "other": "visible"});
        b.iter(|| {
            // audit::log is what we want, but we can't easily call it without filesystem.
            // Instead test the sanitization path: rebuild the args.
            let obj = args.as_object().unwrap();
            let mut m = serde_json::Map::new();
            for (k, v) in obj {
                if k == "text" {
                    m.insert(
                        k.clone(),
                        serde_json::json!({"text_len": v.as_str().unwrap().len()}),
                    );
                } else {
                    m.insert(k.clone(), v.clone());
                }
            }
            black_box(serde_json::Value::Object(m));
        })
    });

    c.bench_function("recipe_substitute_params", |b| {
        let template = serde_json::json!({
            "url": "https://github.com/{repo}/issues/new",
            "title": "{title}",
            "body": "Reported by {user}"
        });
        let mut params = std::collections::HashMap::new();
        params.insert("repo".into(), "org/repo".into());
        params.insert("title".into(), "bug: crash on startup".into());
        params.insert("user".into(), "benchmark-bot".into());

        b.iter(|| {
            let result =
                desk_mcp::recipes::substitute_params(black_box(&template), black_box(&params));
            black_box(result);
        })
    });
}

// ═══════════════════════════════════════════════════════════════════════════
// Registration
// ═══════════════════════════════════════════════════════════════════════════

criterion_group!(
    benches,
    bench_policy,
    bench_session,
    bench_mock_provider,
    bench_resolution,
    bench_tool_dispatch,
    bench_ocr_parse,
    bench_error_types,
    bench_audit_recipes,
);
criterion_main!(benches);
