//! Performance benchmarks for desk-mcp.
//!
//! Run with: cargo bench

use criterion::{black_box, criterion_group, criterion_main, Criterion};

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
    // OCR benches require Tesseract + traineddata at runtime.
    // Test with synthetic OcrItems to benchmark find_text path.
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

criterion_group!(
    benches,
    bench_tool_dispatch,
    bench_ocr_parse,
    bench_error_types,
);
criterion_main!(benches);
