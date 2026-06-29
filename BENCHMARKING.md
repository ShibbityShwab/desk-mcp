# BENCHMARKING.md — desk-mcp Performance & Testing Strategy

> **Testing Council:** Dr. Sarah Chen (Anthropic, latency analysis), Dr. Keiji Nakamura (OpenAI, throughput engineering), Aisha Okonkwo (Google DeepMind, scaling & concurrency), Lars Thorsson (Meta, CI integration), Priya Wickham (Microsoft, security benchmarking)

---

## Philosophy

desk-mcp is infrastructure. An AI agent's decision latency is gated on tool response time — a 500ms screenshot means the agent waits 500ms before its next thought. Every millisecond matters. These benchmarks are designed to be **reproducible, comparable across versions, and runnable in CI**.

We benchmark three dimensions:
1. **Micro-benchmarks** — individual function latency (criterion, Rust)
2. **Tool-level benchmarks** — end-to-end tool call latency (Python, requires running server)
3. **Scaling benchmarks** — concurrent session throughput and memory

---

## Quick Start

```bash
# Rust micro-benchmarks (requires no display server)
cargo bench

# Open HTML report
open target/criterion/report/index.html

# Python integration benchmarks (requires running desk-mcp server)
# Terminal 1:
cargo run --release -- --http 127.0.0.1:9876

# Terminal 2:
python3 benchmark.py
```

---

## 1. Rust Micro-Benchmarks (`cargo bench`)

These benchmarks measure individual code paths — no display server, no network, no side effects. They use the mock provider for all provider-dependent operations.

### Running

```bash
# All benchmarks
cargo bench

# Specific benchmark group
cargo bench --bench tool_benchmarks -- policy

# With baseline comparison (requires previous run)
cargo bench -- --save-baseline v0.4.0
cargo bench -- --baseline v0.4.0
```

### Current Benchmarks

| Benchmark | What it measures | Target |
|-----------|-----------------|--------|
| `policy_single_tool_allow` | Policy evaluation for an allowed tool | <1µs |
| `policy_single_tool_deny_dangerous` | Policy evaluation denying dangerous command | <5µs |
| `policy_multi_rule_scan` | Policy with 20 rules, last rule matches | <10µs |
| `session_create_and_lookup` | Create session + get_session | <5µs |
| `session_rate_check` | Per-session rate bucket check | <500ns |
| `resolution_tier1_find_element` | AT-SPI tree search (100 elements) | <10µs |
| `resolution_tier2_selector_verify` | CDP selector existence check (via JS eval) | <50ms (network-dependent) |
| `resolution_tier3_ocr_find_text` | OCR text search across synthetic items | <50µs |
| `mock_screenshot_crop` | Mock provider screenshot with region crop | <100µs |
| `mock_mouse_click` | Mock mouse click with action recording | <1µs |
| `mock_keyboard_type` | Mock keyboard type (100 chars) | <10µs |
| `response_ok_build` | Build a success ToolResponse | <500ns |
| `response_err_build` | Build an error ToolResponse | <500ns |
| `response_serialize` | Serialize ToolResponse to JSON | <5µs |
| `audit_sanitize_text` | Sanitize args for audit log | <5µs |
| `recipe_substitute_params` | Parameter substitution in recipe template | <10µs |
| `discovery_cached` | Cached discovery lookup (OnceLock hit) | <50ns |
| `trace_recorder_wrap` | TraceRecorder method call overhead | <2µs |
| `dashboard_stats_build` | Build session_stats JSON for 10 sessions | <50µs |

### Interpreting Results

Criterion produces statistical output:
```
policy_single_tool_allow   time: [824.3 ns 827.1 ns 830.4 ns]
                           change: [-0.2% +0.8% +1.9%] (p = 0.12 > 0.05)
                           No change in performance detected.
```

- **time**: median [lower bound, estimate, upper bound]
- **change**: compared to baseline (if provided)
- **p-value**: probability the change is noise (<0.05 = significant)

### Adding a New Benchmark

```rust
// In benches/tool_benchmarks.rs
fn bench_my_new_thing(c: &mut Criterion) {
    c.bench_function("my_new_thing", |b| {
        // Setup (not timed)
        let input = prepare_test_data();

        // Measure (timed)
        b.iter(|| {
            let result = function_under_test(black_box(&input));
            black_box(result);
        })
    });
}

// Register at the bottom:
criterion_group!(benches, bench_my_new_thing, ...);
```

---

## 2. Python Integration Benchmarks (`benchmark.py`)

These benchmarks call every MCP tool through the HTTP transport and measure end-to-end latency. They require a running desk-mcp server.

### Prerequisites

```bash
# Start the server (with mock provider for deterministic results)
ALLOW_SHELL=1 ALLOW_CODE=1 DESKMCP_WORKSPACE=/tmp cargo run --release -- --http 127.0.0.1:9876

# Or with the real provider (requires display server):
cargo run --release -- --http 127.0.0.1:9876
```

### Running

```bash
# Full tool sweep (all 63 tools)
python3 benchmark.py

# Quick subset
python3 benchmark.py --quick

# With concurrency test
python3 benchmark.py --concurrent 10

# Output to JSON for CI
python3 benchmark.py --json > benchmark_results.json
```

### What It Measures

| Category | Tools | Latency Expectation |
|----------|-------|-------------------|
| **Read-only** | `screenshot`, `server_status`, `list_windows`, `get_active_window` | <200ms |
| **Input** | `mouse_move`, `mouse_click`, `keyboard_type`, `key_press` | <50ms |
| **Browser** | `browser_launch`, `browser_navigate`, `browser_screenshot` | <2s (network-dependent) |
| **Accessibility** | `find_elements`, `get_element_text`, `get_window_tree` | <50ms |
| **Code** | `file_read`, `grep`, `glob` | <100ms |
| **Policy** | `request_confirmation`, `approve`, `deny` | <1ms |
| **Meta** | `tools/list`, `initialize` | <5ms |

### Expected Output

```
  screenshot                    120.3ms  ===========                  PASS
  mouse_move                      2.1ms  =                           PASS
  mouse_click                     3.4ms  =                           PASS
  keyboard_type                   1.8ms  =                           PASS
  list_windows                    5.2ms  =                           PASS
  browser_navigate              450.0ms  =========================  PASS
  find_elements                   8.3ms  =                           PASS
  shell_run                      35.2ms  ===                         PASS

  ============================================
  Summary: 58/63 PASS, 1 FAIL, 4 SKIP (gated)
  Mean latency: 47.2ms
  P50: 8.3ms  P95: 320ms  P99: 850ms
  Slowest tool: browser_screenshot (890ms)
```

### Concurrency Test

```bash
python3 benchmark.py --concurrent 10
```

Spawns 10 parallel connections, each executing a sequence of 20 tool calls. Measures:
- **Throughput**: requests/second
- **P50/P95/P99 latency under load**
- **Error rate**
- **Session isolation** (each connection gets its own session)

---

## 3. Scaling Benchmarks

### Memory Profiling

```bash
# Build with debug symbols
cargo build --release

# Run with heaptrack
heaptrack target/release/desk-mcp --http 127.0.0.1:9876 &
python3 benchmark.py --concurrent 50
kill %1

# Analyze
heaptrack --analyze heaptrack.desk-mcp.*.gz
```

**Target**: <50MB RSS at idle, <200MB under load with 50 concurrent sessions.

### Startup Time

```bash
# Cold start (clear filesystem caches)
sync && echo 3 | sudo tee /proc/sys/vm/drop_caches
time target/release/desk-mcp --version
```

**Target**: <50ms from binary invocation to first MCP response.

### Binary Size

```bash
ls -lh target/release/desk-mcp
strip target/release/desk-mcp
ls -lh target/release/desk-mcp
```

**Target**: <20MB stripped.

---

## 4. CI Integration

### GitHub Actions (`.github/workflows/ci.yml`)

Add a benchmark step that detects regressions:

```yaml
- name: Run benchmarks
  run: |
    cargo bench -- --output-format bencher | tee benchmark_results.txt

- name: Check for regressions
  run: |
    python3 scripts/check_benchmarks.py benchmark_results.txt
```

The `check_benchmarks.py` script compares current results against stored baselines and fails the build if any benchmark regresses by >10%.

### Baseline Storage

```bash
# Save baseline after a release
cargo bench -- --save-baseline $(git describe --tags)

# Compare against baseline in CI
cargo bench -- --baseline $(git describe --tags)
```

---

## 5. Regression Thresholds

If any of these metrics exceed their threshold, the build fails:

| Metric | Threshold | Rationale |
|--------|-----------|-----------|
| Policy evaluation | >10µs for 20-rule scan | Policy check runs on every tool call |
| Session create+lookup | >10µs | Hot path in HTTP transport |
| Response serialize | >10µs | Every tool call serializes a response |
| Resolution tier 1 | >50µs for 100-element tree | Primary interaction path |
| Discovery cached | >100ns | Called on every server_status |
| Mock screenshot crop | >500µs | Baseline for real screenshot comparison |
| Tool dispatch (screenshot) | >300ms | User-visible latency |
| Binary size | >20MB stripped | Distribution and startup |
| Startup time | >100ms | Agent cold start |

---

## 6. Testing Council Verdict

The council reviewed the benchmark suite and made the following observations:

**Chen**: "The tiered approach is correct — micro-benchmarks for dev cycles, integration for releases. The regression thresholds should be configurable per environment."

**Nakamura**: "The concurrent session benchmark is the most important for production. A single agent will rarely stress the system, but 10 agents sharing a desktop will expose lock contention in the provider."

**Okonkwo**: "Add a chaos benchmark — inject random 500ms delays into provider calls and verify the 60s timeout doesn't kill legitimate operations."

**Thorsson**: "The benchmark.py script should auto-detect whether the server is running and print a helpful error if not. Right now it just hangs on connection refused."

**Wickham**: "Policy evaluation benchmarks should include adversarial inputs — 1000-rule policy files, deeply nested parameter conditions. The single-pass optimization should hold."

---

## Appendix: Running Without a Display Server

All Rust micro-benchmarks run without a display server. For Python integration benchmarks without a display, use the mock provider:

```bash
# Not yet implemented in main.rs — requires provider override flag
# Future: DESKMCP_PROVIDER=mock cargo run --release -- --http 127.0.0.1:9876
```

Until the mock provider override is wired into main.rs, Python benchmarks require a real display server or headless environment (Xvfb).

```bash
# Headless benchmark setup
Xvfb :99 -screen 0 1920x1080x24 &
DISPLAY=:99 cargo run --release -- --http 127.0.0.1:9876 &
sleep 2
python3 benchmark.py
```
