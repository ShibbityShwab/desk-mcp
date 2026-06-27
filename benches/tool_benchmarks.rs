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
    let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
               5\t1\t1\t1\t1\t1\t0\t0\t80\t20\t99\tbenchmark\n\
               5\t1\t1\t1\t2\t1\t90\t0\t40\t20\t95\ttext\n\
               5\t1\t1\t1\t3\t1\t140\t0\t70\t20\t90\tparsing\n\
               5\t1\t1\t1\t4\t1\t220\t0\t60\t20\t85\troutine\n\
               5\t1\t1\t1\t5\t1\t290\t0\t45\t20\t80\tspeed\n\
               5\t1\t1\t1\t6\t1\t340\t0\t40\t20\t75\ttest\n\
               5\t1\t1\t1\t7\t1\t390\t0\t110\t20\t70\tperformance\n\
               5\t1\t1\t1\t8\t1\t510\t0\t70\t20\t65\tanalysis\n\
               5\t1\t1\t1\t9\t1\t590\t0\t50\t20\t60\tresult\n\
               5\t1\t1\t1\t10\t1\t650\t0\t55\t20\t55\toutput\n\
               5\t1\t1\t1\t11\t1\t715\t0\t45\t20\t50\tinput\n\
               5\t1\t1\t1\t12\t1\t770\t0\t60\t20\t45\tsample\n\
               5\t1\t1\t1\t13\t1\t840\t0\t50\t20\t40\tvalue\n\
               5\t1\t1\t1\t14\t1\t900\t0\t40\t20\t35\tdata\n\
               5\t1\t1\t1\t15\t1\t950\t0\t50\t20\t30\tcheck";

    c.bench_function("ocr_parse_15_words", |b| {
        b.iter(|| {
            let results = desk_mcp::ocr::parse_tsv(black_box(tsv)).unwrap();
            black_box(results.len());
        })
    });

    c.bench_function("ocr_find_text", |b| {
        let results = desk_mcp::ocr::parse_tsv(tsv).unwrap();
        b.iter(|| {
            let found = desk_mcp::ocr::find_text(black_box(&results), "bench", false);
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
        let err = McpError::Timeout { tool: "test".into(), seconds: 30.0 };
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
